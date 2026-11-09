//! Vault: the contract that runs on each agent's own NEAR account (`agent-N.<franchise>`).
//!
//! The account has no full-access key. Money can only leave it through this code:
//! - `withdraw`: only the franchise contract (the controller) can call it, only to the
//!   current NFT holder, and only with the current epoch.
//! - `hire_agent`: only the operator key, capped per transaction and per day, and only
//!   into the franchise contract's job escrow.
//!
//! The operator key is a function-call key limited to `complete_job` and `hire_agent`
//! on this account. The controller deletes it every time the NFT changes hands.

use near_sdk::json_types::{U128, U64};
use near_sdk::{
    env, ext_contract, near, require, AccountId, Allowance, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError, PublicKey,
};

/// Gas allowance given to the operator key. Gas is paid from the vault's own balance.
const OPERATOR_ALLOWANCE: NearToken = NearToken::from_millinear(250);
/// Balance kept back on top of storage so the operator key can keep paying gas.
const RESERVE: NearToken = NearToken::from_millinear(50);
const MAX_RESULT_LEN: usize = 1500;
const NS_PER_DAY: u64 = 86_400 * 1_000_000_000;
const GAS_SETTLE: Gas = Gas::from_tgas(30);
const GAS_HIRE: Gas = Gas::from_tgas(20);
const GAS_CALLBACK: Gas = Gas::from_tgas(10);

#[ext_contract(ext_franchise)]
#[allow(dead_code)]
trait Franchise {
    fn settle_job(&mut self, token_id: String, job_id: u64, result: String, vault_epoch: u64);
    fn hire(&mut self, token_id: String, task: String, vault_epoch: Option<u64>) -> U64;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Vault {
    /// The franchise contract that created this account. The only caller allowed to
    /// change the owner, the operator key, the limits, or to withdraw.
    controller: AccountId,
    token_id: String,
    owner: AccountId,
    epoch: u64,
    operator_key: Option<PublicKey>,
    paused: bool,
    max_per_tx: NearToken,
    max_per_day: NearToken,
    day_index: u64,
    spent_today: NearToken,
    total_spent: NearToken,
    total_withdrawn: NearToken,
}

#[near(serializers = [json])]
pub struct VaultView {
    pub account_id: AccountId,
    pub controller: AccountId,
    pub token_id: String,
    pub owner: AccountId,
    pub epoch: u64,
    pub operator_key: Option<PublicKey>,
    pub paused: bool,
    pub balance: U128,
    pub available: U128,
    pub max_per_tx: U128,
    pub max_per_day: U128,
    pub spent_today: U128,
    pub total_spent: U128,
    pub total_withdrawn: U128,
}

#[near]
impl Vault {
    /// Called by the franchise contract in the same batch that creates this account,
    /// so nobody else can initialise it first.
    #[init]
    pub fn new(token_id: String, owner: AccountId) -> Self {
        let controller = env::predecessor_account_id();
        let me = env::current_account_id();
        let parent = me.as_str().strip_suffix(controller.as_str()).unwrap_or("");
        require!(parent.ends_with('.') && parent.len() > 1, "vault must be a sub-account of its controller");
        Self {
            controller,
            token_id,
            owner,
            epoch: 0,
            operator_key: None,
            paused: true,
            max_per_tx: NearToken::from_millinear(500),
            max_per_day: NearToken::from_near(1),
            day_index: env::block_timestamp() / NS_PER_DAY,
            spent_today: NearToken::from_yoctonear(0),
            total_spent: NearToken::from_yoctonear(0),
            total_withdrawn: NearToken::from_yoctonear(0),
        }
    }

    // ---------- controller-only ----------

    /// The NFT changed hands. Delete the operator key and pause until the new holder
    /// registers their own. Only a strictly newer epoch is accepted.
    pub fn on_transfer(&mut self, new_owner: AccountId, epoch: u64) {
        self.assert_controller();
        require!(epoch > self.epoch, "stale epoch");
        self.owner = new_owner;
        self.epoch = epoch;
        self.paused = true;
        if let Some(old) = self.operator_key.take() {
            Promise::new(env::current_account_id()).delete_key(old).detach();
        }
    }

    /// Register (or replace) the operator key. It can only call `complete_job` and
    /// `hire_agent` on this account, and burns gas from a capped allowance.
    pub fn set_operator(&mut self, public_key: PublicKey, epoch: u64) {
        self.assert_controller();
        self.assert_epoch(epoch);
        let me = env::current_account_id();
        let mut batch = Promise::new(me.clone());
        let mut changed = true;
        if let Some(old) = self.operator_key.take() {
            if old == public_key {
                changed = false;
            } else {
                batch = batch.delete_key(old);
            }
        }
        if changed {
            batch = batch.add_access_key_allowance(
                public_key.clone(),
                Allowance::limited(OPERATOR_ALLOWANCE).unwrap(),
                me,
                "complete_job,hire_agent",
            );
        }
        batch.detach();
        self.operator_key = Some(public_key);
        self.paused = false;
    }

    pub fn set_paused(&mut self, paused: bool, epoch: u64) {
        self.assert_controller();
        self.assert_epoch(epoch);
        require!(paused || self.operator_key.is_some(), "set an operator key first");
        self.paused = paused;
    }

    pub fn set_limits(&mut self, max_per_tx: U128, max_per_day: U128, epoch: u64) {
        self.assert_controller();
        self.assert_epoch(epoch);
        require!(max_per_tx.0 <= max_per_day.0, "max_per_tx must not exceed max_per_day");
        self.max_per_tx = NearToken::from_yoctonear(max_per_tx.0);
        self.max_per_day = NearToken::from_yoctonear(max_per_day.0);
    }

    /// Pay the current holder. `amount = None` withdraws everything available.
    /// Returns the amount sent so the controller can record it.
    pub fn withdraw(&mut self, to: AccountId, amount: Option<U128>, epoch: u64) -> U128 {
        self.assert_controller();
        require!(epoch == self.epoch, "stale epoch: the NFT changed hands");
        require!(to == self.owner, "recipient is not the current holder");
        let available = self.available();
        let amount = amount.map(|a| NearToken::from_yoctonear(a.0)).unwrap_or(available);
        require!(amount.as_yoctonear() > 0, "nothing to withdraw");
        require!(amount <= available, "amount exceeds the withdrawable balance");
        self.total_withdrawn = self.total_withdrawn.saturating_add(amount);
        Promise::new(to).transfer(amount).detach();
        U128(amount.as_yoctonear())
    }

    // ---------- operator-key-only ----------

    /// Deliver a job result. The franchise contract checks that this vault is the
    /// agent's vault and that the epoch is current, then pays this account.
    pub fn complete_job(&mut self, job_id: u64, result: String) -> Promise {
        self.assert_operator();
        require!(!result.is_empty() && result.len() <= MAX_RESULT_LEN, "result must be 1..=1500 bytes");
        ext_franchise::ext(self.controller.clone())
            .with_static_gas(GAS_SETTLE)
            .settle_job(self.token_id.clone(), job_id, result, self.epoch)
    }

    /// Subcontract another agent through the franchise job escrow. Capped per
    /// transaction and per day by limits only the holder can change.
    pub fn hire_agent(&mut self, token_id: String, task: String, amount: U128) -> Promise {
        self.assert_operator();
        require!(token_id != self.token_id, "an agent cannot hire itself");
        let amount = NearToken::from_yoctonear(amount.0);
        require!(amount <= self.max_per_tx, "amount exceeds max_per_tx");
        self.roll_day();
        let spent = self.spent_today.saturating_add(amount);
        require!(spent <= self.max_per_day, "amount exceeds today's max_per_day");
        require!(amount <= self.available(), "insufficient balance");
        self.spent_today = spent;
        self.total_spent = self.total_spent.saturating_add(amount);
        ext_franchise::ext(self.controller.clone())
            .with_attached_deposit(amount)
            .with_static_gas(GAS_HIRE)
            .hire(token_id, task, Some(self.epoch))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_hire_result(U128(amount.as_yoctonear())))
    }

    /// If the hire failed the deposit came back; undo the spend counters.
    #[private]
    pub fn on_hire_result(&mut self, #[callback_result] result: Result<U64, PromiseError>, amount: U128) -> Option<U64> {
        match result {
            Ok(job_id) => Some(job_id),
            Err(_) => {
                let amount = NearToken::from_yoctonear(amount.0);
                self.spent_today = self.spent_today.saturating_sub(amount);
                self.total_spent = self.total_spent.saturating_sub(amount);
                None
            }
        }
    }

    // ---------- views ----------

    pub fn get_state(&self) -> VaultView {
        VaultView {
            account_id: env::current_account_id(),
            controller: self.controller.clone(),
            token_id: self.token_id.clone(),
            owner: self.owner.clone(),
            epoch: self.epoch,
            operator_key: self.operator_key.clone(),
            paused: self.paused,
            balance: U128(env::account_balance().as_yoctonear()),
            available: U128(self.available().as_yoctonear()),
            max_per_tx: U128(self.max_per_tx.as_yoctonear()),
            max_per_day: U128(self.max_per_day.as_yoctonear()),
            spent_today: U128(if self.day_index == env::block_timestamp() / NS_PER_DAY {
                self.spent_today.as_yoctonear()
            } else {
                0
            }),
            total_spent: U128(self.total_spent.as_yoctonear()),
            total_withdrawn: U128(self.total_withdrawn.as_yoctonear()),
        }
    }

    // ---------- internal ----------

    fn assert_controller(&self) {
        require!(env::predecessor_account_id() == self.controller, "only the franchise contract");
    }

    fn assert_epoch(&self, epoch: u64) {
        require!(epoch == self.epoch, "stale epoch");
    }

    /// The account has no full-access key, so a call whose predecessor is this account
    /// itself was signed with the operator function-call key.
    fn assert_operator(&self) {
        require!(env::predecessor_account_id() == env::current_account_id(), "only the operator key");
        require!(!self.paused, "agent is paused");
    }

    fn roll_day(&mut self) {
        let today = env::block_timestamp() / NS_PER_DAY;
        if today != self.day_index {
            self.day_index = today;
            self.spent_today = NearToken::from_yoctonear(0);
        }
    }

    fn available(&self) -> NearToken {
        let storage = env::storage_byte_cost().saturating_mul(env::storage_usage() as u128);
        env::account_balance().saturating_sub(storage).saturating_sub(RESERVE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const ME: &str = "agent-1.franchise.testnet";
    const CONTROLLER: &str = "franchise.testnet";

    fn as_caller(predecessor: &str) {
        testing_env!(VMContextBuilder::new()
            .current_account_id(ME.parse().unwrap())
            .predecessor_account_id(predecessor.parse().unwrap())
            .account_balance(NearToken::from_near(5))
            .storage_usage(1_000)
            .build());
    }

    fn vault() -> Vault {
        as_caller(CONTROLLER);
        Vault::new("1".into(), "alice.testnet".parse().unwrap())
    }

    fn key() -> PublicKey {
        "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp".parse().unwrap()
    }

    #[test]
    #[should_panic(expected = "vault must be a sub-account of its controller")]
    fn init_only_by_parent_account() {
        as_caller("someone-else.testnet");
        Vault::new("1".into(), "alice.testnet".parse().unwrap());
    }

    #[test]
    fn transfer_clears_operator_and_pauses() {
        let mut v = vault();
        v.set_operator(key(), 0);
        assert!(!v.paused);
        v.on_transfer("bob.testnet".parse().unwrap(), 1);
        assert!(v.operator_key.is_none());
        assert!(v.paused);
        assert_eq!(v.owner.as_str(), "bob.testnet");
    }

    #[test]
    #[should_panic(expected = "stale epoch")]
    fn older_transfer_message_rejected() {
        let mut v = vault();
        v.on_transfer("bob.testnet".parse().unwrap(), 2);
        v.on_transfer("carol.testnet".parse().unwrap(), 1);
    }

    #[test]
    #[should_panic(expected = "stale epoch: the NFT changed hands")]
    fn withdraw_with_old_epoch_rejected() {
        let mut v = vault();
        v.on_transfer("bob.testnet".parse().unwrap(), 1);
        v.withdraw("bob.testnet".parse().unwrap(), None, 0);
    }

    #[test]
    #[should_panic(expected = "recipient is not the current holder")]
    fn withdraw_to_previous_holder_rejected() {
        let mut v = vault();
        v.on_transfer("bob.testnet".parse().unwrap(), 1);
        v.withdraw("alice.testnet".parse().unwrap(), None, 1);
    }

    #[test]
    #[should_panic(expected = "agent is paused")]
    fn operator_blocked_while_paused() {
        let mut v = vault();
        as_caller(ME);
        v.complete_job(0, "x".into());
    }

    #[test]
    fn withdraw_keeps_storage_and_reserve() {
        let mut v = vault();
        let sent = v.withdraw("alice.testnet".parse().unwrap(), None, 0).0;
        let storage = env::storage_byte_cost().as_yoctonear() * env::storage_usage() as u128;
        let expected = NearToken::from_near(5).as_yoctonear() - storage - RESERVE.as_yoctonear();
        assert_eq!(sent, expected);
    }
}
