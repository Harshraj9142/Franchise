//! End-to-end tests on a local NEAR sandbox. Each test maps to a row of THREAT_MODEL.md.
//! Run: ./scripts/build.sh && cargo test -p integration-tests
mod common;

use common::*;
use near_workspaces::types::{AccessKeyPermission, Gas, NearToken};
use serde_json::json;

#[tokio::test]
async fn mint_creates_vault_and_nft() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    assert_eq!(token, "1");

    let nft = env.view("nft_token", json!({ "token_id": token })).await?;
    assert_eq!(nft["owner_id"], env.creator.id().as_str());

    let state = env.vault_state(&token).await?;
    assert_eq!(state["owner"], env.creator.id().as_str());
    assert_eq!(state["controller"], env.id().as_str());
    assert_eq!(state["paused"], true);
    assert!(env.vault_keys(&token).await?.is_empty(), "a new vault must have no access keys at all");

    let agent = env.agent(&token).await?;
    assert_eq!(agent["vault"], env.vault_id(&token).as_str());
    assert_eq!(agent["creator"], env.creator.id().as_str());
    Ok(())
}

#[tokio::test]
async fn vault_has_only_function_call_keys() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    env.set_operator(&env.creator, &token).await?;

    let keys = env.vault_keys(&token).await?;
    assert_eq!(keys.len(), 1);
    assert!(!keys.iter().any(is_full_access), "vault must never hold a full-access key");
    match &keys[0].access_key.permission {
        AccessKeyPermission::FunctionCall(p) => {
            assert_eq!(p.receiver_id, env.vault_id(&token).as_str());
            assert_eq!(p.method_names, vec!["complete_job".to_string(), "hire_agent".to_string()]);
            assert!(p.allowance.is_some(), "operator key must have a capped gas allowance");
        }
        AccessKeyPermission::FullAccess => unreachable!(),
    }
    Ok(())
}

#[tokio::test]
async fn full_job_flow_pays_vault_and_creator_royalty() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let op = env.set_operator(&env.creator, &token).await?;
    let creator_before = env.balance(&env.creator).await?;

    let job = env.hire(&env.client, &token, "Write a haiku about NEAR").await?;
    let open = env.view("get_open_jobs", json!({ "token_id": token })).await?;
    assert_eq!(open.as_array().unwrap().len(), 1);

    env.complete(&op, job, "Blocks fall like soft snow").await?.into_result()?;

    let j = env.view("get_job", json!({ "job_id": job })).await?;
    assert_eq!(j["status"], "Done");
    assert_eq!(j["result"], "Blocks fall like soft snow");
    assert_eq!(j["holder_at_completion"], env.creator.id().as_str());

    let agent = env.agent(&token).await?;
    assert_eq!(agent["jobs_completed"], 1);
    assert_eq!(yocto(&agent["total_earned"]), milli(95).as_yoctonear());
    assert_eq!(yocto(&agent["royalties_paid"]), milli(5).as_yoctonear());

    // The creator received the 5% royalty and spent nothing in between.
    let creator_gain = env.balance(&env.creator).await? - creator_before;
    assert!(creator_gain >= milli(5).as_yoctonear() && creator_gain < milli(6).as_yoctonear(), "creator gain {creator_gain}");
    Ok(())
}

#[tokio::test]
async fn transfer_revokes_operator_key() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let op = env.set_operator(&env.creator, &token).await?;
    let job = env.hire(&env.client, &token, "one").await?;
    env.complete(&op, job, "done").await?.into_result()?;
    let balance_before = yocto(&env.vault_state(&token).await?["balance"]);

    let res = env.transfer(&env.creator, &env.buyer, &token).await?;
    assert!(res.receipt_failures().is_empty(), "on_transfer must succeed: {:?}", res.receipt_failures());

    assert!(env.vault_keys(&token).await?.is_empty(), "the seller's operator key must be deleted");
    let state = env.vault_state(&token).await?;
    assert_eq!(state["owner"], env.buyer.id().as_str());
    assert_eq!(state["epoch"], 1);
    assert_eq!(state["paused"], true);
    // NEAR credits part of the burnt gas to the executing account, so it can only grow.
    assert!(yocto(&state["balance"]) >= balance_before, "earnings stay in the vault for the buyer");

    let agent = env.agent(&token).await?;
    assert_eq!(agent["owner"], env.buyer.id().as_str());
    assert_eq!(agent["paused"], true);
    assert_eq!(agent["operator_set"], false);

    // The old key no longer exists, so the seller cannot even sign as the vault.
    let attempt = env.complete(&op, job, "still here?").await;
    assert!(attempt.is_err() || attempt.unwrap().is_failure(), "old operator key must be unusable");
    Ok(())
}

#[tokio::test]
async fn buyer_resumes_and_withdraws_everything() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let seller_op = env.set_operator(&env.creator, &token).await?;
    for task in ["a", "b"] {
        let job = env.hire(&env.client, &token, task).await?;
        env.complete(&seller_op, job, "ok").await?.into_result()?;
    }
    env.transfer(&env.creator, &env.buyer, &token).await?.into_result()?;

    // Paused agents can't be hired until the new holder sets an operator.
    let agent = env.agent(&token).await?;
    let price = yocto(&agent["price"]);
    let paused_hire = env
        .client
        .call(env.id(), "hire")
        .args_json(json!({ "token_id": token, "task": "c" }))
        .deposit(NearToken::from_yoctonear(price + milli(30).as_yoctonear()))
        .transact()
        .await?;
    assert_fails_with(&paused_hire, "agent is paused");

    let buyer_op = env.set_operator(&env.buyer, &token).await?;
    let job = env.hire(&env.client, &token, "c").await?;
    env.complete(&buyer_op, job, "ok").await?.into_result()?;

    let available = yocto(&env.vault_state(&token).await?["available"]);
    assert!(available > milli(250).as_yoctonear(), "three jobs of earnings should be withdrawable");

    let seller_try = env.withdraw(&env.creator, &token).await?;
    assert_fails_with(&seller_try, "only the current holder");

    let before = env.balance(&env.buyer).await?;
    let res = env.withdraw(&env.buyer, &token).await?.into_result()?;
    let sent: String = res.json()?;
    assert!(sent.parse::<u128>()? >= available, "withdraw sends everything available");
    env.worker.fast_forward(5).await?; // let gas refund receipts land
    let after = env.balance(&env.buyer).await?;
    let gain = after as i128 - before as i128;
    let vault_after = env.vault_state(&token).await?;
    assert_eq!(yocto(&vault_after["total_withdrawn"]), sent.parse::<u128>()?);
    assert!(gain > (available - milli(15).as_yoctonear()) as i128, "buyer gain {gain} vs available {available}");

    let history = env.view("get_history", json!({ "token_id": token })).await?;
    let kinds: Vec<&str> = history.as_array().unwrap().iter().map(|h| h["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"Withdrawn"));
    assert!(kinds.contains(&"Transferred"));
    assert!(kinds.contains(&"RoyaltyPaid"));
    Ok(())
}

#[tokio::test]
async fn withdraw_racing_a_sale() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let op = env.set_operator(&env.creator, &token).await?;
    let job = env.hire(&env.client, &token, "one").await?;
    env.complete(&op, job, "done").await?.into_result()?;
    let available_before = yocto(&env.vault_state(&token).await?["available"]);

    // The seller fires a withdrawal and a sale at the same time.
    let (w, t) = tokio::join!(env.withdraw(&env.creator, &token), env.transfer(&env.creator, &env.buyer, &token));
    t?.into_result()?;
    let _ = w?; // may succeed or fail depending on ordering: both outcomes are safe

    let state = env.vault_state(&token).await?;
    assert_eq!(state["owner"], env.buyer.id().as_str());
    let withdrawn = yocto(&state["total_withdrawn"]);
    let available_after = yocto(&state["available"]);
    if withdrawn == 0 {
        assert!(available_after >= available_before, "if the payout lost the race, all money stays for the buyer");
    } else {
        // The payout won the race: it was requested and executed while the seller still held the NFT.
        assert!(withdrawn >= available_before);
        let history = env.view("get_history", json!({ "token_id": token })).await?;
        let w = history.as_array().unwrap().iter().find(|h| h["kind"] == "Withdrawn").unwrap();
        assert_eq!(w["actor"], env.creator.id().as_str());
        assert_eq!(w["epoch"], 0, "the seller's payout must belong to the seller's epoch");
    }

    // After the sale the seller can never withdraw again.
    assert_fails_with(&env.withdraw(&env.creator, &token).await?, "only the current holder");
    Ok(())
}

#[tokio::test]
async fn hire_agent_respects_caps() -> anyhow::Result<()> {
    let env = setup().await?;
    let manager = env.mint(&env.creator, "Manager", 100).await?;
    let translator = env.mint(&env.creator, "Translator", 100).await?;
    let op = env.set_operator(&env.creator, &manager).await?;
    env.set_operator(&env.creator, &translator).await?;

    // Give the manager agent some working capital.
    env.worker
        .root_account()?
        .transfer_near(&env.vault_id(&manager), NearToken::from_near(2))
        .await?
        .into_result()?;

    let hire = |amount: u128, token: String| {
        op.call(op.id(), "hire_agent")
            .args_json(json!({ "token_id": token, "task": "Translate 'hello' to French", "amount": milli(amount).as_yoctonear().to_string() }))
            .gas(Gas::from_tgas(100))
            .transact()
    };

    assert_fails_with(&hire(600, translator.clone()).await?, "max_per_tx");
    assert_fails_with(&hire(130, manager.clone()).await?, "an agent cannot hire itself");

    let ok: Option<String> = hire(130, translator.clone()).await?.into_result()?.json()?;
    let job_id: u64 = ok.expect("hire should return a job id").parse()?;
    let job = env.view("get_job", json!({ "job_id": job_id })).await?;
    assert_eq!(job["client"], env.vault_id(&manager).as_str(), "the agent itself is the client");
    assert_eq!(job["token_id"], translator);

    // The holder tightens the daily budget; the third hire breaks it.
    env.creator
        .call(env.id(), "set_limits")
        .args_json(json!({ "token_id": manager, "max_per_tx": milli(200).as_yoctonear().to_string(), "max_per_day": milli(300).as_yoctonear().to_string() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    hire(130, translator.clone()).await?.into_result()?;
    assert_fails_with(&hire(130, translator.clone()).await?, "max_per_day");

    let state = env.vault_state(&manager).await?;
    assert_eq!(yocto(&state["spent_today"]), milli(260).as_yoctonear());
    Ok(())
}

#[tokio::test]
async fn approvals_cleared_on_transfer_and_marketplace_sale_syncs_vault() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    env.set_operator(&env.creator, &token).await?;
    let market = &env.client;

    env.creator
        .call(env.id(), "nft_approve")
        .args_json(json!({ "token_id": token, "account_id": market.id() }))
        .deposit(milli(10))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;

    let nft = env.view("nft_token", json!({ "token_id": token })).await?;
    let approval_id = nft["approved_account_ids"][market.id().as_str()].as_u64().unwrap();

    // A marketplace completes a sale using its approval.
    market
        .call(env.id(), "nft_transfer")
        .args_json(json!({ "receiver_id": env.buyer.id(), "token_id": token, "approval_id": approval_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?
        .into_result()?;

    let approved: bool = env
        .franchise
        .view("nft_is_approved")
        .args_json(json!({ "token_id": token, "approved_account_id": market.id() }))
        .await?
        .json()?;
    assert!(!approved, "approvals must not survive a transfer");
    assert_eq!(env.vault_state(&token).await?["owner"], env.buyer.id().as_str());
    assert!(env.vault_keys(&token).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn holder_methods_need_one_yocto() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;

    let no_deposit = env.creator.call(env.id(), "withdraw").args_json(json!({ "token_id": token })).transact().await?;
    assert_fails_with(&no_deposit, "Requires attached deposit of exactly 1 yoctoNEAR");

    let not_holder = env.withdraw(&env.buyer, &token).await?;
    assert_fails_with(&not_holder, "only the current holder");
    Ok(())
}

#[tokio::test]
async fn only_own_vault_can_settle() -> anyhow::Result<()> {
    let env = setup().await?;
    let a = env.mint(&env.creator, "Agent A", 100).await?;
    let b = env.mint(&env.creator, "Agent B", 100).await?;
    env.set_operator(&env.creator, &a).await?;
    let op_b = env.set_operator(&env.creator, &b).await?;
    let job_a = env.hire(&env.client, &a, "for A only").await?;

    // B's vault tries to settle A's job.
    assert_fails_with(&env.complete(&op_b, job_a, "stolen").await?, "job belongs to another agent");

    // A plain account calls settle_job directly.
    let direct = env
        .client
        .call(env.id(), "settle_job")
        .args_json(json!({ "token_id": a, "job_id": job_a, "result": "fake", "vault_epoch": 0 }))
        .transact()
        .await?;
    assert_fails_with(&direct, "only this agent's vault");
    Ok(())
}

#[tokio::test]
async fn cancel_after_timeout_and_no_double_payout() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let op = env.set_operator(&env.creator, &token).await?;
    let job = env.hire(&env.client, &token, "slow job").await?;

    let early = env.client.call(env.id(), "cancel_job").args_json(json!({ "job_id": job })).transact().await?;
    assert_fails_with(&early, "job has not timed out yet");

    env.worker.fast_forward(200).await?;
    let before = env.balance(&env.client).await?;
    env.client.call(env.id(), "cancel_job").args_json(json!({ "job_id": job })).transact().await?.into_result()?;
    let refunded = env.balance(&env.client).await? + milli(2).as_yoctonear() - before;
    assert!(refunded >= milli(100).as_yoctonear(), "client gets the price back (minus gas)");

    assert_fails_with(&env.complete(&op, job, "too late").await?, "job is not open");
    assert_eq!(env.agent(&token).await?["jobs_completed"], 0);
    Ok(())
}

#[tokio::test]
async fn vault_rejects_direct_calls() -> anyhow::Result<()> {
    let env = setup().await?;
    let token = env.mint(&env.creator, "Haiku Bot", 100).await?;
    let vault = env.vault_id(&token);

    let w = env
        .creator
        .call(&vault, "withdraw")
        .args_json(json!({ "to": env.creator.id(), "epoch": 0 }))
        .transact()
        .await?;
    assert_fails_with(&w, "only the franchise contract");

    let c = env.creator.call(&vault, "complete_job").args_json(json!({ "job_id": 0, "result": "x" })).transact().await?;
    assert_fails_with(&c, "only the operator key");

    let t = env
        .creator
        .call(&vault, "on_transfer")
        .args_json(json!({ "new_owner": env.creator.id(), "epoch": 99 }))
        .transact()
        .await?;
    assert_fails_with(&t, "only the franchise contract");
    Ok(())
}

#[tokio::test]
async fn combined_views_match_individual_views() -> anyhow::Result<()> {
    let env = setup().await?;
    let a = env.mint(&env.creator, "Agent A", 100).await?;
    let b = env.mint(&env.creator, "Agent B", 100).await?;
    let op_a = env.set_operator(&env.creator, &a).await?;
    env.set_operator(&env.creator, &b).await?;
    let done = env.hire(&env.client, &a, "first").await?;
    env.complete(&op_a, done, "ok").await?.into_result()?;
    let open_b = env.hire(&env.client, &b, "for B").await?;
    let open_a = env.hire(&env.client, &a, "second for A").await?;

    let market = env.view("get_market", json!({})).await?;
    assert_eq!(market["agents"], env.view("list_agents", json!({})).await?);
    assert_eq!(market["config"], env.view("get_config", json!({})).await?);
    assert_eq!(market["recent_jobs"].as_array().unwrap().len(), 3);

    let page = env.view("get_agent_page", json!({ "token_id": a })).await?;
    assert_eq!(page["agent"], env.agent(&a).await?);
    assert_eq!(page["jobs"], env.view("get_jobs", json!({ "token_id": a })).await?);
    assert_eq!(page["history"], env.view("get_history", json!({ "token_id": a })).await?);
    assert!(env.view("get_agent_page", json!({ "token_id": "999" })).await?.is_null());

    // Open work across all agents, oldest first, each carrying its agent.
    let work = env.view("get_open_work", json!({})).await?;
    let ids: Vec<u64> = work.as_array().unwrap().iter().map(|w| w["job"]["id"].as_u64().unwrap()).collect();
    assert_eq!(ids, vec![open_b, open_a]);
    assert_eq!(work[0]["agent"]["token_id"], b);

    // A paused agent's jobs are not offered as work.
    env.creator
        .call(env.id(), "set_paused")
        .args_json(json!({ "token_id": b, "paused": true }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    let work = env.view("get_open_work", json!({})).await?;
    assert_eq!(work.as_array().unwrap().len(), 1);
    assert_eq!(work[0]["job"]["id"], open_a);
    Ok(())
}
