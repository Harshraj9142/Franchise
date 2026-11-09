//! Unit tests for checks the sandbox can't easily reach, such as a vault whose
//! operator key survived a sale because the key deletion failed (THREAT_MODEL.md #2).
use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn as_caller(predecessor: &str, deposit: NearToken) {
    testing_env!(VMContextBuilder::new()
        .current_account_id(id("franchise.testnet"))
        .predecessor_account_id(id(predecessor))
        .attached_deposit(deposit)
        .account_balance(NearToken::from_near(100))
        .build());
}

fn vault(token: &str) -> String {
    format!("agent-{token}.franchise.testnet")
}

/// A contract with agents already minted (bypassing the async vault creation).
fn setup(tokens: &[&str]) -> Contract {
    as_caller("franchise.testnet", NearToken::from_yoctonear(0));
    let mut c = Contract::new(None, Some(60));
    for t in tokens {
        let meta = TokenMetadata {
            title: Some(format!("Agent {t}")),
            description: None,
            media: None,
            media_hash: None,
            copies: None,
            issued_at: None,
            expires_at: None,
            starts_at: None,
            updated_at: None,
            extra: None,
            reference: None,
            reference_hash: None,
        };
        c.tokens.internal_mint_with_refund(t.to_string(), id("alice.testnet"), Some(meta), None);
        c.agents.insert(
            t.to_string(),
            Agent {
                name: format!("Agent {t}"),
                skill: "test".into(),
                price: NearToken::from_millinear(100),
                creator: id("alice.testnet"),
                vault: Contract::vault_account_for(t),
                epoch: 0,
                synced_owner: id("alice.testnet"),
                operator_set: true,
                paused: false,
                jobs_completed: 0,
                total_earned: NearToken::from_yoctonear(0),
                royalties_paid: NearToken::from_yoctonear(0),
                total_withdrawn: NearToken::from_yoctonear(0),
                job_ids: vec![],
                history_ids: vec![],
                created_at: 0,
            },
        );
    }
    c
}

fn hire_as(c: &mut Contract, client: &str, token: &str, epoch: Option<u64>) -> u64 {
    as_caller(client, NearToken::from_millinear(130));
    c.hire(token.to_string(), "task".into(), epoch).0
}

#[test]
fn transfer_bumps_epoch_and_pauses() {
    let mut c = setup(&["1"]);
    as_caller("alice.testnet", NearToken::from_yoctonear(1));
    c.nft_transfer(id("bob.testnet"), "1".into(), None, None);
    let a = c.agents.get("1").unwrap();
    assert_eq!(a.epoch, 1);
    assert!(a.paused);
    assert!(!a.operator_set);
    assert_eq!(a.synced_owner, id("bob.testnet"));
}

#[test]
fn settle_from_current_epoch_pays_vault_and_creator() {
    let mut c = setup(&["1"]);
    let job = hire_as(&mut c, "carol.testnet", "1", None);
    as_caller(&vault("1"), NearToken::from_yoctonear(0));
    c.settle_job("1".into(), job, "answer".into(), 0);
    let a = c.agents.get("1").unwrap();
    assert_eq!(a.jobs_completed, 1);
    assert_eq!(a.total_earned, NearToken::from_millinear(95));
    assert_eq!(a.royalties_paid, NearToken::from_millinear(5));
    assert_eq!(c.jobs.get(job as u32).unwrap().status, JobStatus::Done);
}

#[test]
#[should_panic(expected = "stale operator")]
fn settle_rejects_vault_with_stale_epoch() {
    let mut c = setup(&["1"]);
    let job = hire_as(&mut c, "carol.testnet", "1", None);
    // Sale happens. Pretend the vault never processed on_transfer (its epoch is still 0)
    // and the buyer already set a new operator (agent unpaused).
    as_caller("alice.testnet", NearToken::from_yoctonear(1));
    c.nft_transfer(id("bob.testnet"), "1".into(), None, None);
    c.agents.get_mut("1").unwrap().paused = false;
    // The seller's surviving key signs as the vault with the old epoch.
    as_caller(&vault("1"), NearToken::from_yoctonear(0));
    c.settle_job("1".into(), job, "seller still here".into(), 0);
}

#[test]
#[should_panic(expected = "stale operator: the hiring agent changed hands")]
fn hire_from_vault_with_stale_epoch_rejected() {
    let mut c = setup(&["1", "2"]);
    c.agents.get_mut("1").unwrap().epoch = 2;
    hire_as(&mut c, &vault("1"), "2", Some(1));
}

#[test]
#[should_panic(expected = "an agent cannot hire itself")]
fn vault_cannot_hire_its_own_agent() {
    let mut c = setup(&["1"]);
    hire_as(&mut c, &vault("1"), "1", Some(0));
}

#[test]
#[should_panic(expected = "only this agent's vault")]
fn other_vault_cannot_settle() {
    let mut c = setup(&["1", "2"]);
    let job = hire_as(&mut c, "carol.testnet", "1", None);
    as_caller(&vault("2"), NearToken::from_yoctonear(0));
    c.settle_job("1".into(), job, "not mine".into(), 0);
}

#[test]
fn lookalike_account_is_not_a_vault() {
    let c = setup(&["1"]);
    assert_eq!(c.token_of_vault(&id(&vault("1"))), Some("1".to_string()));
    assert_eq!(c.token_of_vault(&id("agent-1.franchise-evil.testnet")), None);
    assert_eq!(c.token_of_vault(&id("agent-9.franchise.testnet")), None);
}
