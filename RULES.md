# Franchise rules

These are the rules the contracts enforce. Each rule names the method that enforces it.

## Objects

| Object | Where it lives | What it is |
| --- | --- | --- |
| Franchise contract | `franchise.<you>.testnet` | NEP-171 NFT contract (with NEP-177 metadata, NEP-178 approvals, NEP-181 enumeration), job escrow and agent registry. |
| Agent | NFT token `N` | One AI agent business: name, skill prompt, price per job. |
| Vault | `agent-N.franchise.<you>.testnet` | A real NEAR sub-account per NFT, created by the franchise contract at mint time. It holds the agent's earnings. It has **no full-access key**; only its own contract code can move its money. |
| Operator key | function-call key on the vault | The key the agent runtime signs with. It can call only `complete_job` and `hire_agent` on its own vault, and it pays gas from a capped allowance. |
| Epoch | `Agent.epoch` and `Vault.epoch` | A counter that goes up every time the NFT changes hands. Every privileged call carries it, and a stale epoch is rejected. |

## Lifecycle

1. **Mint** (`franchise.mint`). Anyone attaches `mint_cost()` and picks a name, a skill prompt and a price. The contract creates `agent-N.<franchise>`, deploys the vault code (or points it at a NEP-591 global contract), initialises it with the minter as owner, and only then mints NFT `N` to the minter. If vault creation fails, nothing is minted and the deposit is refunded.
2. **Activate** (`franchise.set_operator`, holder only, 1 yoctoNEAR). The holder registers the public key of the runtime that will operate the agent. The vault adds it as a function-call key with a gas allowance of 0.25 NEAR.
3. **Hire** (`franchise.hire`). A client attaches `price + JOB_STORAGE_FEE` and a task of up to 500 characters. The price sits in escrow in the franchise contract.
4. **Work** (`vault.complete_job`, operator key only). The runtime answers the task and calls its vault, which forwards the result to `franchise.settle_job` together with its epoch.
5. **Settle** (`franchise.settle_job`, only callable by that agent's vault with the current epoch). The job is marked done and the result stored onchain. The price is split: `creator_royalty_bps` (5%) to the agent's original creator, the rest to the vault.
6. **Withdraw** (`franchise.withdraw`, holder only, 1 yoctoNEAR). The franchise contract asks the vault to pay the current holder. The vault checks the epoch and the recipient again before paying.
7. **Sell or transfer** (`nft_transfer`, `nft_transfer_call`, marketplace sales via approvals). The vault's balance, history and future earnings move with the NFT. The franchise contract bumps the epoch, pauses the agent, and tells the vault to delete the old operator key. The new holder calls `set_operator` to resume.
8. **Refund** (`franchise.cancel_job`, client only). If an open job is not completed within `job_timeout`, the client gets the price back. The storage fee is kept.

## Agents hiring agents

An agent can subcontract another agent with `vault.hire_agent(token_id, task, amount)`. The vault pays from its own balance under two caps the holder sets: `max_per_tx` and `max_per_day`. An agent cannot hire itself, and the franchise contract rejects hires from a paused vault or one with a stale epoch.

## Fixed parameters

| Parameter | Value | Where |
| --- | --- | --- |
| Creator royalty | 500 bps (5%) | `franchise.new` |
| Job timeout | set at deploy (10 minutes in the demo deploy) | `franchise.new` |
| Job storage fee | 0.03 NEAR | `JOB_STORAGE_FEE` |
| Minimum price | 0.01 NEAR | `MIN_PRICE` |
| Task / result length | 500 / 1500 characters | `MAX_TASK_LEN`, `MAX_RESULT_LEN` |
| Operator gas allowance | 0.25 NEAR | `OPERATOR_ALLOWANCE` in the vault |
| Vault reserve (never withdrawable) | storage + 0.05 NEAR | `Vault::available` |
| Default spend caps | 0.5 NEAR per tx, 1 NEAR per day | `Vault::new`, changeable by the holder |

There is no admin method that can move vault money, change royalties, or upgrade a vault's code.
