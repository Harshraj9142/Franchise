#![allow(dead_code)]
//! Shared sandbox setup. Build the contracts first: ./scripts/build.sh
use near_workspaces::network::Sandbox;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{AccessKeyPermission, Gas, KeyType, NearToken, SecretKey};
use near_workspaces::{Account, AccountId, Contract, Worker};
use serde_json::{json, Value};

pub const TGAS_300: Gas = Gas::from_tgas(300);

pub fn milli(n: u128) -> NearToken {
    NearToken::from_millinear(n)
}

pub struct Env {
    pub worker: Worker<Sandbox>,
    pub franchise: Contract,
    pub creator: Account,
    pub buyer: Account,
    pub client: Account,
}

pub async fn setup() -> anyhow::Result<Env> {
    let worker = near_workspaces::sandbox().await?;
    let wasm = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../res/franchise.wasm"))
        .expect("res/franchise.wasm missing: run ./scripts/build.sh first");
    let franchise = worker.dev_deploy(&wasm).await?;
    franchise
        .call("new")
        .args_json(json!({ "job_timeout_sec": 5 }))
        .transact()
        .await?
        .into_result()?;
    let root = worker.root_account()?;
    let mut accounts = vec![];
    for name in ["creator", "buyer", "client"] {
        accounts.push(
            root.create_subaccount(name)
                .initial_balance(NearToken::from_near(50))
                .transact()
                .await?
                .into_result()?,
        );
    }
    let client = accounts.pop().unwrap();
    let buyer = accounts.pop().unwrap();
    let creator = accounts.pop().unwrap();
    Ok(Env { worker, franchise, creator, buyer, client })
}

impl Env {
    pub fn id(&self) -> &AccountId {
        self.franchise.id()
    }

    pub async fn view(&self, method: &str, args: Value) -> anyhow::Result<Value> {
        Ok(self.franchise.view(method).args_json(args).await?.json()?)
    }

    pub async fn mint(&self, who: &Account, name: &str, price_milli: u128) -> anyhow::Result<String> {
        let cost: String = self.view("mint_cost", json!({})).await?.as_str().unwrap().to_string();
        let res = who
            .call(self.id(), "mint")
            .args_json(json!({
                "name": name,
                "skill": format!("You are {name}. Answer in two sentences."),
                "price": milli(price_milli).as_yoctonear().to_string(),
            }))
            .deposit(NearToken::from_yoctonear(cost.parse()?))
            .gas(TGAS_300)
            .transact()
            .await?;
        let token: Option<String> = res.clone().into_result()?.json()?;
        Ok(token.expect("mint returned no token: vault creation failed"))
    }

    pub async fn agent(&self, token_id: &str) -> anyhow::Result<Value> {
        self.view("get_agent", json!({ "token_id": token_id })).await
    }

    pub fn vault_id(&self, token_id: &str) -> AccountId {
        format!("agent-{token_id}.{}", self.id()).parse().unwrap()
    }

    pub async fn vault_state(&self, token_id: &str) -> anyhow::Result<Value> {
        Ok(self.worker.view(&self.vault_id(token_id), "get_state").await?.json()?)
    }

    pub async fn vault_keys(&self, token_id: &str) -> anyhow::Result<Vec<near_workspaces::types::AccessKeyInfo>> {
        Ok(self.worker.view_access_keys(&self.vault_id(token_id)).await?)
    }

    /// The holder registers a fresh operator key. Returns a signer that acts as the
    /// vault account using that function-call key, exactly like the agent runtime.
    pub async fn set_operator(&self, holder: &Account, token_id: &str) -> anyhow::Result<Account> {
        let sk = SecretKey::from_random(KeyType::ED25519);
        holder
            .call(self.id(), "set_operator")
            .args_json(json!({ "token_id": token_id, "public_key": sk.public_key().to_string() }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?
            .into_result()?;
        Ok(Account::from_secret_key(self.vault_id(token_id), sk, &self.worker))
    }

    pub async fn hire(&self, client: &Account, token_id: &str, task: &str) -> anyhow::Result<u64> {
        let agent = self.agent(token_id).await?;
        let price: u128 = agent["price"].as_str().unwrap().parse()?;
        let res = client
            .call(self.id(), "hire")
            .args_json(json!({ "token_id": token_id, "task": task }))
            .deposit(NearToken::from_yoctonear(price + milli(30).as_yoctonear()))
            .gas(Gas::from_tgas(50))
            .transact()
            .await?
            .into_result()?;
        let id: String = res.json()?;
        Ok(id.parse()?)
    }

    pub async fn complete(&self, operator: &Account, job_id: u64, result: &str) -> anyhow::Result<ExecutionFinalResult> {
        Ok(operator
            .call(operator.id(), "complete_job")
            .args_json(json!({ "job_id": job_id, "result": result }))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?)
    }

    pub async fn transfer(&self, from: &Account, to: &Account, token_id: &str) -> anyhow::Result<ExecutionFinalResult> {
        Ok(from
            .call(self.id(), "nft_transfer")
            .args_json(json!({ "receiver_id": to.id(), "token_id": token_id }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?)
    }

    pub async fn withdraw(&self, holder: &Account, token_id: &str) -> anyhow::Result<ExecutionFinalResult> {
        Ok(holder
            .call(self.id(), "withdraw")
            .args_json(json!({ "token_id": token_id }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?)
    }

    pub async fn balance(&self, account: &Account) -> anyhow::Result<u128> {
        Ok(account.view_account().await?.balance.as_yoctonear())
    }
}

pub fn yocto(v: &Value) -> u128 {
    v.as_str().expect("expected a U128 string").parse().unwrap()
}

pub fn is_full_access(k: &near_workspaces::types::AccessKeyInfo) -> bool {
    matches!(k.access_key.permission, AccessKeyPermission::FullAccess)
}

/// Asserts the transaction failed and its error mentions `needle`.
pub fn assert_fails_with(res: &ExecutionFinalResult, needle: &str) {
    assert!(res.is_failure(), "expected failure containing {needle:?}, but it succeeded");
    let text = format!("{:?}", res.failures());
    assert!(text.contains(needle), "expected failure containing {needle:?}, got: {text}");
}
