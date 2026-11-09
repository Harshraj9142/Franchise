//! Franchise: NFTs that own an AI agent's NEAR account.
//!
//! Each NFT `N` is tied to a real sub-account `agent-N.<this contract>` running the
//! `vault` contract. The vault holds the agent's earnings and has no full-access key.
//! Whoever holds the NFT controls the vault: they set the operator key the agent runtime
//! signs with, pause the agent, and withdraw. Selling the NFT moves all of it, and every
//! transfer bumps an epoch that invalidates the previous holder's operator key.
//!
//! This contract is also the job escrow: clients `hire` an agent, the agent's vault calls
//! `settle_job` with the result, and the price is split between the vault and the agent's
//! original creator (a royalty that survives every resale).

mod events;
mod views;

use near_contract_standards::non_fungible_token::approval::NonFungibleTokenApproval;
use near_contract_standards::non_fungible_token::core::{NonFungibleTokenCore, NonFungibleTokenResolver};
use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
use near_contract_standards::non_fungible_token::events::NftMint;
use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{NonFungibleToken, Token, TokenId};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{Base58CryptoHash, U128, U64};
use near_sdk::store::{IterableMap, Vector};
use near_sdk::{
    assert_one_yocto, env, ext_contract, near, require, AccountId, BorshStorageKey, CryptoHash, Gas,
    NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue, PublicKey,
};
use std::collections::HashMap;

pub use views::*;

/// Vault code embedded at build time (see scripts/build.sh). Used when no NEP-591
/// global contract hash is configured.
const VAULT_WASM: &[u8] = include_bytes!("../../../res/vault.wasm");

pub const JOB_STORAGE_FEE: NearToken = NearToken::from_millinear(30);
pub const MIN_PRICE: NearToken = NearToken::from_millinear(10);
pub const MAX_TASK_LEN: usize = 500;
pub const MAX_RESULT_LEN: usize = 1500;
pub const MAX_NAME_LEN: usize = 48;
pub const MAX_SKILL_LEN: usize = 600;
/// Balance a new vault starts with on top of its code storage: state storage,
/// the 0.05 NEAR reserve and room for the operator key's gas allowance.
const VAULT_BASE_FUNDING: NearToken = NearToken::from_millinear(300);
/// Covers this contract's own storage for the token, metadata and agent record.
const MINT_STORAGE_BUFFER: NearToken = NearToken::from_millinear(100);

const GAS_VAULT_INIT: Gas = Gas::from_tgas(10);
const GAS_ON_CREATED: Gas = Gas::from_tgas(40);
const GAS_VAULT_CALL: Gas = Gas::from_tgas(10);
const GAS_ON_WITHDRAWN: Gas = Gas::from_tgas(10);

#[ext_contract(ext_vault)]
#[allow(dead_code)]
trait Vault {
    fn on_transfer(&mut self, new_owner: AccountId, epoch: u64);
    fn set_operator(&mut self, public_key: PublicKey, epoch: u64);
    fn set_paused(&mut self, paused: bool, epoch: u64);
    fn set_limits(&mut self, max_per_tx: U128, max_per_day: U128, epoch: u64);
    fn withdraw(&mut self, to: AccountId, amount: Option<U128>, epoch: u64) -> U128;
}

#[derive(BorshStorageKey)]
#[near]
enum StorageKey {
    NonFungibleToken,
    Metadata,
    TokenMetadata,
    Enumeration,
    Approval,
    Agents,
    Jobs,
    History,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Debug)]
pub enum JobStatus {
    Open,
    Done,
    Refunded,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Agent {
    pub name: String,
    pub skill: String,
    pub price: NearToken,
    pub creator: AccountId,
    pub vault: AccountId,
    /// Goes up on every change of holder. Vault calls must carry the current value.
    pub epoch: u64,
    /// The holder the vault was last told about.
    pub synced_owner: AccountId,
    pub operator_set: bool,
    pub paused: bool,
    pub jobs_completed: u64,
    pub total_earned: NearToken,
    pub royalties_paid: NearToken,
    pub total_withdrawn: NearToken,
    pub job_ids: Vec<u64>,
    pub history_ids: Vec<u64>,
    pub created_at: u64,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Job {
    pub id: u64,
    pub token_id: TokenId,
    pub client: AccountId,
    pub task: String,
    pub price: NearToken,
    pub status: JobStatus,
    pub result: Option<String>,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    pub holder_at_completion: Option<AccountId>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Debug)]
pub enum HistoryKind {
    Minted,
    OperatorSet,
    Paused,
    Resumed,
    JobHired,
    JobPaid,
    RoyaltyPaid,
    JobRefunded,
    Withdrawn,
    Transferred,
    LimitsSet,
    PriceSet,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct HistoryEntry {
    pub kind: HistoryKind,
    pub token_id: TokenId,
    pub actor: AccountId,
    pub amount: Option<NearToken>,
    pub job_id: Option<u64>,
    pub epoch: u64,
    pub timestamp: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    tokens: NonFungibleToken,
    metadata: LazyOption<NFTContractMetadata>,
    agents: IterableMap<TokenId, Agent>,
    jobs: Vector<Job>,
    history: Vector<HistoryEntry>,
    next_token_id: u64,
    /// When set, vaults use this NEP-591 global contract instead of deploying code.
    vault_code_hash: Option<CryptoHash>,
    creator_royalty_bps: u16,
    job_timeout_ns: u64,
}

#[near]
impl Contract {
    /// `vault_code_hash`: optional NEP-591 global contract hash for the vault code.
    /// `job_timeout_sec`: how long a client waits before they can cancel an open job.
    #[init]
    pub fn new(vault_code_hash: Option<Base58CryptoHash>, job_timeout_sec: Option<u64>) -> Self {
        let metadata = NFTContractMetadata {
            spec: NFT_METADATA_SPEC.to_string(),
            name: "Franchise Agents".to_string(),
            symbol: "AGENT".to_string(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        };
        Self {
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                env::current_account_id(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            agents: IterableMap::new(StorageKey::Agents),
            jobs: Vector::new(StorageKey::Jobs),
            history: Vector::new(StorageKey::History),
            next_token_id: 1,
            vault_code_hash: vault_code_hash.map(Into::into),
            creator_royalty_bps: 500,
            job_timeout_ns: job_timeout_sec.unwrap_or(600) * 1_000_000_000,
        }
    }

    // ------------------------------------------------------------------ mint

    /// Launch a new agent business. Creates `agent-N.<this>` with the vault code, then
    /// mints NFT `N` to the caller. Attach `mint_cost()`; the unused part is refunded.
    #[payable]
    pub fn mint(&mut self, name: String, skill: String, price: U128) -> Promise {
        require!(!name.trim().is_empty() && name.len() <= MAX_NAME_LEN, "name must be 1..=48 bytes");
        require!(!skill.trim().is_empty() && skill.len() <= MAX_SKILL_LEN, "skill must be 1..=600 bytes");
        require!(price.0 >= MIN_PRICE.as_yoctonear(), "price must be at least 0.01 NEAR");
        let deposit = env::attached_deposit();
        require!(deposit >= self.mint_cost_internal(), "attach mint_cost() to mint");

        let id = self.next_token_id;
        self.next_token_id += 1;
        let token_id = id.to_string();
        let vault = Self::vault_account_for(&token_id);
        let owner = env::predecessor_account_id();

        let mut create = Promise::new(vault.clone()).create_account().transfer(self.vault_funding());
        create = match self.vault_code_hash {
            Some(hash) => create.use_global_contract(hash),
            None => create.deploy_contract(VAULT_WASM),
        };
        let init_args = near_sdk::serde_json::json!({ "token_id": token_id, "owner": owner });
        create
            .function_call("new", init_args.to_string().into_bytes(), NearToken::from_yoctonear(0), GAS_VAULT_INIT)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_CREATED)
                    .on_vault_created(token_id, owner, name, skill, price, U128(deposit.as_yoctonear())),
            )
    }

    /// Mints the NFT only once its vault account exists; otherwise refunds the deposit.
    #[private]
    pub fn on_vault_created(
        &mut self,
        token_id: TokenId,
        owner: AccountId,
        name: String,
        skill: String,
        price: U128,
        deposit: U128,
    ) -> Option<TokenId> {
        let deposit = NearToken::from_yoctonear(deposit.0);
        if !near_sdk::is_promise_success() {
            // The failed batch refunded the vault funding to this contract.
            Promise::new(owner).transfer(deposit).detach();
            return None;
        }
        let storage_before = env::storage_usage();
        let metadata = TokenMetadata {
            title: Some(name.clone()),
            description: Some(skill.clone()),
            media: None,
            media_hash: None,
            copies: Some(1),
            issued_at: Some(env::block_timestamp_ms().to_string()),
            expires_at: None,
            starts_at: None,
            updated_at: None,
            extra: Some(format!("{{\"vault\":\"{}\"}}", Self::vault_account_for(&token_id))),
            reference: None,
            reference_hash: None,
        };
        self.tokens.internal_mint_with_refund(token_id.clone(), owner.clone(), Some(metadata), None);
        NftMint { owner_id: &owner, token_ids: &[&token_id], memo: None }.emit();

        let agent = Agent {
            name,
            skill,
            price: NearToken::from_yoctonear(price.0),
            creator: owner.clone(),
            vault: Self::vault_account_for(&token_id),
            epoch: 0,
            synced_owner: owner.clone(),
            operator_set: false,
            paused: true,
            jobs_completed: 0,
            total_earned: NearToken::from_yoctonear(0),
            royalties_paid: NearToken::from_yoctonear(0),
            total_withdrawn: NearToken::from_yoctonear(0),
            job_ids: vec![],
            history_ids: vec![],
            created_at: env::block_timestamp(),
        };
        self.agents.insert(token_id.clone(), agent);
        self.record(&token_id, HistoryKind::Minted, owner.clone(), None, None);

        let used = env::storage_byte_cost().saturating_mul((env::storage_usage() - storage_before) as u128);
        let refund = deposit.saturating_sub(self.vault_funding()).saturating_sub(used);
        if refund.as_yoctonear() > 0 {
            Promise::new(owner).transfer(refund).detach();
        }
        Some(token_id)
    }

    // ------------------------------------------------------------ holder actions

    /// Register the key the agent runtime signs with. Resumes the agent.
    #[payable]
    pub fn set_operator(&mut self, token_id: TokenId, public_key: PublicKey) -> Promise {
        assert_one_yocto();
        let mut agent = self.assert_holder(&token_id);
        agent.operator_set = true;
        agent.paused = false;
        let (vault, epoch) = (agent.vault.clone(), agent.epoch);
        self.agents.insert(token_id.clone(), agent);
        self.record(&token_id, HistoryKind::OperatorSet, env::predecessor_account_id(), None, None);
        ext_vault::ext(vault).with_static_gas(GAS_VAULT_CALL).set_operator(public_key, epoch)
    }

    #[payable]
    pub fn set_paused(&mut self, token_id: TokenId, paused: bool) -> Promise {
        assert_one_yocto();
        let mut agent = self.assert_holder(&token_id);
        require!(paused || agent.operator_set, "set an operator key first");
        agent.paused = paused;
        let (vault, epoch) = (agent.vault.clone(), agent.epoch);
        self.agents.insert(token_id.clone(), agent);
        let kind = if paused { HistoryKind::Paused } else { HistoryKind::Resumed };
        self.record(&token_id, kind, env::predecessor_account_id(), None, None);
        ext_vault::ext(vault).with_static_gas(GAS_VAULT_CALL).set_paused(paused, epoch)
    }

    #[payable]
    pub fn set_limits(&mut self, token_id: TokenId, max_per_tx: U128, max_per_day: U128) -> Promise {
        assert_one_yocto();
        let agent = self.assert_holder(&token_id);
        self.record(&token_id, HistoryKind::LimitsSet, env::predecessor_account_id(), Some(max_per_day.0), None);
        ext_vault::ext(agent.vault).with_static_gas(GAS_VAULT_CALL).set_limits(max_per_tx, max_per_day, agent.epoch)
    }

    #[payable]
    pub fn set_price(&mut self, token_id: TokenId, price: U128) {
        assert_one_yocto();
        require!(price.0 >= MIN_PRICE.as_yoctonear(), "price must be at least 0.01 NEAR");
        let mut agent = self.assert_holder(&token_id);
        agent.price = NearToken::from_yoctonear(price.0);
        self.agents.insert(token_id.clone(), agent);
        self.record(&token_id, HistoryKind::PriceSet, env::predecessor_account_id(), Some(price.0), None);
    }

    /// Move earnings from the vault to the caller, who must hold the NFT now and still
    /// hold it (same epoch) when the vault executes the payout.
    #[payable]
    pub fn withdraw(&mut self, token_id: TokenId, amount: Option<U128>) -> Promise {
        assert_one_yocto();
        let agent = self.assert_holder(&token_id);
        let holder = env::predecessor_account_id();
        ext_vault::ext(agent.vault)
            .with_static_gas(GAS_VAULT_CALL)
            .withdraw(holder.clone(), amount, agent.epoch)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_WITHDRAWN)
                    .on_withdrawn(token_id, holder, agent.epoch),
            )
    }

    #[private]
    pub fn on_withdrawn(
        &mut self,
        #[callback_result] result: Result<U128, PromiseError>,
        token_id: TokenId,
        holder: AccountId,
        epoch: u64,
    ) -> U128 {
        let Ok(amount) = result else {
            return U128(0);
        };
        if let Some(agent) = self.agents.get_mut(&token_id) {
            agent.total_withdrawn = agent.total_withdrawn.saturating_add(NearToken::from_yoctonear(amount.0));
        }
        let id = self.history.len() as u64;
        self.history.push(HistoryEntry {
            kind: HistoryKind::Withdrawn,
            token_id: token_id.clone(),
            actor: holder.clone(),
            amount: Some(NearToken::from_yoctonear(amount.0)),
            job_id: None,
            epoch,
            timestamp: env::block_timestamp(),
        });
        if let Some(agent) = self.agents.get_mut(&token_id) {
            agent.history_ids.push(id);
        }
        events::emit("withdrawn", near_sdk::serde_json::json!({ "token_id": token_id, "holder": holder, "amount": amount }));
        amount
    }

    /// Anyone can push the current holder to the vault again (e.g. if an earlier
    /// `on_transfer` ran out of gas). Bumps the epoch, which also pauses the agent.
    pub fn resync(&mut self, token_id: TokenId) {
        let owner = self.tokens.owner_by_id.get(&token_id).expect("no such token");
        let mut agent = self.agents.get(&token_id).cloned().expect("no such agent");
        agent.synced_owner = owner.clone();
        self.push_new_owner(&token_id, agent, owner);
    }

    // ------------------------------------------------------------------ jobs

    /// Hire an agent. Attach `price + JOB_STORAGE_FEE`; any excess is refunded.
    /// Vaults hiring other agents pass their epoch; humans pass nothing.
    #[payable]
    pub fn hire(&mut self, token_id: TokenId, task: String, vault_epoch: Option<u64>) -> U64 {
        require!(!task.trim().is_empty() && task.len() <= MAX_TASK_LEN, "task must be 1..=500 bytes");
        let client = env::predecessor_account_id();
        let mut agent = self.agents.get(&token_id).cloned().expect("no such agent");
        require!(!agent.paused, "agent is paused: its holder has not set an operator");
        if let Some(hirer_token) = self.token_of_vault(&client) {
            require!(hirer_token != token_id, "an agent cannot hire itself");
            let hirer = self.agents.get(&hirer_token).expect("no such agent");
            require!(
                vault_epoch == Some(hirer.epoch) && !hirer.paused,
                "stale operator: the hiring agent changed hands"
            );
        }
        let cost = agent.price.saturating_add(JOB_STORAGE_FEE);
        let deposit = env::attached_deposit();
        require!(deposit >= cost, "attach price + 0.03 NEAR storage fee");

        let id = self.jobs.len() as u64;
        self.jobs.push(Job {
            id,
            token_id: token_id.clone(),
            client: client.clone(),
            task,
            price: agent.price,
            status: JobStatus::Open,
            result: None,
            created_at: env::block_timestamp(),
            completed_at: None,
            holder_at_completion: None,
        });
        agent.job_ids.push(id);
        let price = agent.price;
        self.agents.insert(token_id.clone(), agent);
        self.record(&token_id, HistoryKind::JobHired, client.clone(), Some(price.as_yoctonear()), Some(id));
        events::emit("job_hired", near_sdk::serde_json::json!({ "token_id": token_id, "job_id": id, "client": client }));

        let excess = deposit.saturating_sub(cost);
        if excess.as_yoctonear() > 0 {
            Promise::new(client).transfer(excess).detach();
        }
        U64(id)
    }

    /// Called by an agent's vault (signed by its operator key) with the job result.
    pub fn settle_job(&mut self, token_id: TokenId, job_id: u64, result: String, vault_epoch: u64) {
        let mut agent = self.agents.get(&token_id).cloned().expect("no such agent");
        require!(env::predecessor_account_id() == agent.vault, "only this agent's vault can settle its jobs");
        require!(vault_epoch == agent.epoch && !agent.paused, "stale operator: the NFT changed hands");
        require!(!result.is_empty() && result.len() <= MAX_RESULT_LEN, "result must be 1..=1500 bytes");
        let mut job = self.jobs.get(job_id as u32).cloned().expect("no such job");
        require!(job.token_id == token_id, "job belongs to another agent");
        require!(job.status == JobStatus::Open, "job is not open");

        let holder = self.tokens.owner_by_id.get(&token_id).expect("no such token");
        let royalty = NearToken::from_yoctonear(
            job.price.as_yoctonear() * self.creator_royalty_bps as u128 / 10_000,
        );
        let net = job.price.saturating_sub(royalty);

        job.status = JobStatus::Done;
        job.result = Some(result);
        job.completed_at = Some(env::block_timestamp());
        job.holder_at_completion = Some(holder.clone());
        self.jobs.replace(job_id as u32, job);

        agent.jobs_completed += 1;
        agent.total_earned = agent.total_earned.saturating_add(net);
        agent.royalties_paid = agent.royalties_paid.saturating_add(royalty);
        let (vault, creator) = (agent.vault.clone(), agent.creator.clone());
        self.agents.insert(token_id.clone(), agent);

        self.record(&token_id, HistoryKind::JobPaid, holder, Some(net.as_yoctonear()), Some(job_id));
        Promise::new(vault).transfer(net).detach();
        if royalty.as_yoctonear() > 0 {
            self.record(&token_id, HistoryKind::RoyaltyPaid, creator.clone(), Some(royalty.as_yoctonear()), Some(job_id));
            Promise::new(creator).transfer(royalty).detach();
        }
        events::emit(
            "job_paid",
            near_sdk::serde_json::json!({ "token_id": token_id, "job_id": job_id, "net": U128(net.as_yoctonear()), "royalty": U128(royalty.as_yoctonear()) }),
        );
    }

    /// The client takes their money back if an open job wasn't completed in time.
    pub fn cancel_job(&mut self, job_id: u64) {
        let mut job = self.jobs.get(job_id as u32).cloned().expect("no such job");
        require!(env::predecessor_account_id() == job.client, "only the client can cancel");
        require!(job.status == JobStatus::Open, "job is not open");
        require!(env::block_timestamp() >= job.created_at + self.job_timeout_ns, "job has not timed out yet");
        job.status = JobStatus::Refunded;
        let (client, price, token_id) = (job.client.clone(), job.price, job.token_id.clone());
        self.jobs.replace(job_id as u32, job);
        self.record(&token_id, HistoryKind::JobRefunded, client.clone(), Some(price.as_yoctonear()), Some(job_id));
        Promise::new(client).transfer(price).detach();
    }

    // -------------------------------------------------------------- internal

    fn vault_account_for(token_id: &str) -> AccountId {
        format!("agent-{}.{}", token_id, env::current_account_id()).parse().unwrap()
    }

    /// If `account` is one of our vaults, returns its token id.
    fn token_of_vault(&self, account: &AccountId) -> Option<TokenId> {
        let suffix = format!(".{}", env::current_account_id());
        let id = account.as_str().strip_suffix(&suffix)?.strip_prefix("agent-")?;
        let agent = self.agents.get(id)?;
        (agent.vault == *account).then(|| id.to_string())
    }

    fn vault_funding(&self) -> NearToken {
        match self.vault_code_hash {
            Some(_) => VAULT_BASE_FUNDING,
            None => env::storage_byte_cost()
                .saturating_mul(VAULT_WASM.len() as u128)
                .saturating_add(VAULT_BASE_FUNDING),
        }
    }

    fn mint_cost_internal(&self) -> NearToken {
        self.vault_funding().saturating_add(MINT_STORAGE_BUFFER)
    }

    fn assert_holder(&self, token_id: &TokenId) -> Agent {
        let owner = self.tokens.owner_by_id.get(token_id).expect("no such token");
        require!(env::predecessor_account_id() == owner, "only the current holder");
        self.agents.get(token_id).cloned().expect("no such agent")
    }

    /// Called after every transfer path. If the holder changed, bump the epoch, pause,
    /// and tell the vault to drop the old operator key.
    fn sync_owner(&mut self, token_id: &TokenId) {
        let owner = self.tokens.owner_by_id.get(token_id).expect("no such token");
        let mut agent = self.agents.get(token_id).cloned().expect("no such agent");
        if agent.synced_owner == owner {
            return;
        }
        agent.synced_owner = owner.clone();
        self.push_new_owner(token_id, agent, owner);
    }

    fn push_new_owner(&mut self, token_id: &TokenId, mut agent: Agent, owner: AccountId) {
        agent.epoch += 1;
        agent.operator_set = false;
        agent.paused = true;
        let (vault, epoch) = (agent.vault.clone(), agent.epoch);
        self.agents.insert(token_id.clone(), agent);
        self.record(token_id, HistoryKind::Transferred, owner.clone(), None, None);
        ext_vault::ext(vault).with_static_gas(GAS_VAULT_CALL).on_transfer(owner, epoch).detach();
    }

    fn record(&mut self, token_id: &TokenId, kind: HistoryKind, actor: AccountId, amount: Option<u128>, job_id: Option<u64>) {
        let id = self.history.len() as u64;
        let epoch = self.agents.get(token_id).map(|a| a.epoch).unwrap_or(0);
        self.history.push(HistoryEntry {
            kind,
            token_id: token_id.clone(),
            actor,
            amount: amount.map(NearToken::from_yoctonear),
            job_id,
            epoch,
            timestamp: env::block_timestamp(),
        });
        if let Some(agent) = self.agents.get_mut(token_id) {
            agent.history_ids.push(id);
        }
    }
}

// ------------------------------------------------------------- NEP-171 core

#[near]
impl NonFungibleTokenCore for Contract {
    #[payable]
    fn nft_transfer(&mut self, receiver_id: AccountId, token_id: TokenId, approval_id: Option<u64>, memo: Option<String>) {
        self.tokens.nft_transfer(receiver_id, token_id.clone(), approval_id, memo);
        self.sync_owner(&token_id);
    }

    #[payable]
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        let result = self.tokens.nft_transfer_call(receiver_id, token_id.clone(), approval_id, memo, msg);
        self.sync_owner(&token_id);
        result
    }

    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        self.tokens.nft_token(token_id)
    }
}

#[near]
impl NonFungibleTokenResolver for Contract {
    #[private]
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> bool {
        let transferred = self.tokens.nft_resolve_transfer(previous_owner_id, receiver_id, token_id.clone(), approved_account_ids);
        // If the receiver returned the token, the holder changed back: sync again.
        self.sync_owner(&token_id);
        transferred
    }
}

#[near]
impl NonFungibleTokenApproval for Contract {
    #[payable]
    fn nft_approve(&mut self, token_id: TokenId, account_id: AccountId, msg: Option<String>) -> Option<Promise> {
        self.tokens.nft_approve(token_id, account_id, msg)
    }

    #[payable]
    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) {
        self.tokens.nft_revoke(token_id, account_id);
    }

    #[payable]
    fn nft_revoke_all(&mut self, token_id: TokenId) {
        self.tokens.nft_revoke_all(token_id);
    }

    fn nft_is_approved(&self, token_id: TokenId, approved_account_id: AccountId, approval_id: Option<u64>) -> bool {
        self.tokens.nft_is_approved(token_id, approved_account_id, approval_id)
    }
}

#[near]
impl NonFungibleTokenEnumeration for Contract {
    fn nft_total_supply(&self) -> U128 {
        self.tokens.nft_total_supply()
    }

    fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token> {
        self.tokens.nft_tokens(from_index, limit)
    }

    fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        self.tokens.nft_supply_for_owner(account_id)
    }

    fn nft_tokens_for_owner(&self, account_id: AccountId, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token> {
        self.tokens.nft_tokens_for_owner(account_id, from_index, limit)
    }
}

#[near]
impl NonFungibleTokenMetadataProvider for Contract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[cfg(test)]
mod tests;
