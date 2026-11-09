# Threat model: can a seller keep the money?

The core promise of Franchise is: **whoever holds the NFT controls the agent's account and its money.** This file lists every way a previous holder (the seller) could try to break that promise, what stops them, and the test that proves it. Sandbox tests live in `integration-tests/tests/franchise.rs`; unit tests (marked *unit*) in `contracts/*/src`.

## Assets

- The vault's NEAR balance (past earnings not yet withdrawn).
- Future earnings from jobs.
- Control of the agent: who may operate it, pause it, price it.

## Attacks by the seller

| # | Attack | What stops it | Test |
| --- | --- | --- | --- |
| 1 | Keep using the old operator key after the sale | On every transfer the franchise contract bumps the epoch and calls `vault.on_transfer`, which deletes the operator key and pauses the agent. | `transfer_revokes_operator_key`, `buyer_resumes_and_withdraws_everything` |
| 2 | The key deletion fails or arrives late, and the seller races it | Second, independent lock: `settle_job` and vault-initiated `hire` both require the vault's epoch to equal the franchise's epoch and the agent to be unpaused. A stale operator is rejected by the franchise contract even if the key still exists. | *unit* `settle_rejects_vault_with_stale_epoch`, `hire_from_vault_with_stale_epoch_rejected` |
| 3 | Withdraw in the middle of a sale | `franchise.withdraw` checks the holder at call time and passes the epoch. The vault rejects the payout if the epoch changed or the recipient is no longer the holder. If the withdrawal lands first, the seller received money they asked for while still the holder; if the transfer lands first, the money stays for the buyer. Both outcomes are safe. | `withdraw_racing_a_sale`, *unit* `withdraw_with_old_epoch_rejected`, `withdraw_to_previous_holder_rejected` |
| 4 | Keep a full-access key on the vault | The vault account is created by the franchise contract with no full-access key at all. Its code has no method that adds a full-access key; the only key it ever adds is a function-call key limited to its own `complete_job` and `hire_agent`. | `mint_creates_vault_and_nft`, `vault_has_only_function_call_keys` |
| 5 | Redeploy the vault with code that pays the seller | Deploying code needs a full-access key or a contract action, and the vault has neither an upgrade method nor a full-access key. | covered by #4 |
| 6 | Drain the vault through the operator key right before selling | The operator key cannot transfer NEAR. It can only call `hire_agent`, capped by `max_per_tx` and `max_per_day`. A seller could hire their own second agent up to the daily cap; buyers should check the history view, which lists every hire. | `hire_agent_respects_caps` |
| 7 | Get a marketplace approval to survive the sale | Standard NEP-178 behaviour: approvals are cleared on every transfer. | `approvals_cleared_on_transfer_and_marketplace_sale_syncs_vault` |
| 8 | Call `withdraw` through a dApp's function-call key | Holder methods require exactly 1 yoctoNEAR, which only a full-access key can attach. | `holder_methods_need_one_yocto`, `vault_rejects_direct_calls` |
| 9 | A fake vault pays out a job | `settle_job` requires the caller to be exactly `agent-N.<franchise>`. Only the franchise contract can create sub-accounts of itself. | `only_own_vault_can_settle`, *unit* `lookalike_account_is_not_a_vault`, `other_vault_cannot_settle` |

## Other risks

| Risk | Status |
| --- | --- |
| **The franchise deployer keeps a full-access key** on the franchise account, redeploys it and calls vaults as the controller. | This is a real trust assumption. For a production launch, delete the franchise account's full-access keys after deploying (see "Locking the contract" in the README), which makes the code immutable. The testnet demo keeps the key so it can be redeployed during the hackathon; this README says so. |
| A client and an agent both act at the job timeout (cancel vs complete). | The job status moves from `Open` exactly once; the second call fails. Test: `cancel_after_timeout_and_no_double_payout`. |
| Prompt injection in a task makes the agent spend money. | The runtime gives the model no tools. The task text only influences the answer string. Spending (`hire_agent`) is never triggered by task text in this version. |
| The vault runs out of gas allowance. | The operator key stops working until the holder sets a new key. Earnings are never at risk. |
| Two transfers in quick succession deliver `on_transfer` out of order. | The vault only accepts a strictly higher epoch, so a late, older message is rejected (*unit* `older_transfer_message_rejected`). Anyone can call `franchise.resync(token_id)` to push the latest owner again. |
