# Franchise: NFTs that own an AI worker

Franchise turns an AI agent into an asset you can own and sell. Each NFT is tied to a real NEAR account where its agent gets paid for jobs. Whoever holds the NFT controls that account: they choose who operates the agent, set its price and spending caps, and withdraw its earnings. Sell the NFT and the account, its unwithdrawn money and all future earnings go to the buyer, and the seller's keys are revoked in the same transaction. The original creator keeps a 5% royalty on every job, forever.

Built at NEAR HACKATHON (November 2026). Tracks: NFTs That Do Something, Agent Economy, Autonomous Business.

- Live demo: https://harshraj9142.github.io/Franchise/ (NEAR testnet)
- Agent runtime: https://franchise-sooty.vercel.app (Vercel)
- Contract: [`franchise.harshraj9142.testnet`](https://testnet.nearblocks.io/address/franchise.harshraj9142.testnet)
- Demo video: https://x.com/harsh_sinh7610/status/2121549012688130057

## The problem

AI agents can now earn money, but there is no clean way to own one. Today an agent's earnings sit in a wallet controlled by whoever has the key. You can't sell a working agent, split its revenue with the person who built it, or prove to a buyer that the seller can't keep using it after the sale. Franchise makes the agent's account, its cash flow and its controls a single transferable NFT.

## Try it in 3 commands (no NEAR account needed)

Requirements: Rust (the repo pins 1.95 via `rust-toolchain.toml`), [cargo-near](https://github.com/near/cargo-near) (`cargo install --locked cargo-near`), Node 22+.

```bash
./scripts/build.sh                     # builds res/vault.wasm and res/franchise.wasm
cargo test                             # 14 unit tests + 13 tests on a local NEAR sandbox
cd agent && npm install && npm run demo:local
```

What you should see from `npm run demo:local`. It starts a local NEAR node and runs the whole story with the real contracts and the real agent runtime:

1. **Alice mints "Haiku Bot".** Output shows `agent-1.franchise.sandbox` was created with `access keys: 0`.
2. **Alice registers an operator key.** Output shows one key: `FunctionCall → agent-1.franchise.sandbox [complete_job, hire_agent]`.
3. **Carol hires the agent** for 0.1 NEAR (held in escrow).
4. **The runtime answers and delivers.** Status `Done`, `vault earned 0.095 NEAR · creator royalty 0.005 NEAR`.
5. **Alice sells NFT #1 to Bob.** `alice's operator key still active? false`, the agent is paused, the vault epoch is 1, and the runtime refuses to work for the old holder.
6. **Bob registers his own key.** Carol hires again and the job is paid to Bob's agent account.
7. **Alice tries to withdraw** and is rejected with `only the current holder`. Bob withdraws everything.
8. **The NFT's history** prints every mint, hire, payment, royalty, sale and withdrawal.

Answers come from a clearly labelled mock model unless you set `NEARAI_API_KEY` (see below). With a key, a private model on NEAR AI Cloud writes them.

To click through the web app against that local node:

```bash
cd agent && npm run demo:local -- --keep        # prints VITE_RPC_URL and VITE_CONTRACT
cd web && npm install && VITE_RPC_URL=<printed url> VITE_CONTRACT=franchise.sandbox npm run dev
```

Open http://localhost:5173. You'll see three agents, their jobs and results, each NFT's account, its live access keys and its history. Wallet actions need testnet (below), because wallets can't connect to a local node.

## How it works

```
                 holds NFT #N                          hires (price in escrow)
   Holder ───────────────────────┐            Client ──────────────────────┐
     │ set_operator / withdraw /  │                                         ▼
     │ set_limits / nft_transfer  ▼                               ┌────────────────────┐
     │                  ┌────────────────────┐   settle_job(epoch) │  franchise         │
     └─────────────────►│ franchise contract │◄────────────────────│  (NEP-171 NFT,     │
                        │  NFT + job escrow  │                     │   job escrow,      │
                        └─────────┬──────────┘                     │   royalties)       │
          on_transfer(epoch) /    │ creates at mint                └────────────────────┘
          set_operator / withdraw ▼                                          ▲
                        ┌────────────────────┐  complete_job / hire_agent    │
                        │ agent-N.franchise  │───────────────────────────────┘
                        │ (vault contract,   │
                        │  NO full-access    │◄── operator function-call key
                        │  key, holds money) │    held by the agent runtime
                        └────────────────────┘    (answers with NEAR AI Cloud)
```

- **The NFT owns an account.** `mint` creates `agent-N.<contract>` with the vault code and no full-access key, then mints NFT `N`. If account creation fails, nothing is minted and the deposit is refunded.
- **The operator can work but can't take money.** The holder registers a function-call key that can only call `complete_job` and `hire_agent` on the agent's own account. Its gas is paid from a capped allowance (0.25 NEAR). It cannot transfer NEAR.
- **A sale rotates control atomically.** Every transfer path (`nft_transfer`, `nft_transfer_call` including reverted ones, marketplace sales via approvals) bumps an epoch, pauses the agent and tells the vault to delete the operator key. As a second, independent lock, the franchise contract rejects any vault call that carries an old epoch, so a seller can't act even if the key deletion failed.
- **Agents can hire agents.** `hire_agent` lets one agent subcontract another through the same escrow. The spend is capped per transaction and per day by limits only the holder can change. Try it with `npm run hire-agent -- 1 2 "Translate this"`.
- **Every movement is on the record.** `get_history` returns each NFT's mints, hires, payments, royalties, withdrawals and changes of hands. The contract also emits NEP-297 events (`EVENT_JSON:{"standard":"franchise",...}`) for indexers.

The exact rules are in [RULES.md](RULES.md). Every attack a seller could try, with the test that covers it, is in [THREAT_MODEL.md](THREAT_MODEL.md).

### What NEAR enables here

| NEAR feature | What it does in Franchise |
| --- | --- |
| Named sub-accounts | Each NFT gets a real, human-readable account (`agent-7.franchise.near`) created by the contract itself. Only the franchise contract can create its sub-accounts, so a vault's identity can't be faked. |
| Function-call access keys with allowances | The agent runtime signs as the agent's account but can only call two methods and can only spend a capped gas allowance. This is native NEAR, not a smart-contract wallet trick. |
| Accounts with no full-access key | The vault account has no key that could redeploy it or drain it. Its money can only move through its code. |
| NEP-171 / 177 / 178 / 181 | Standard NFTs, so the agents work with existing NEAR wallets and marketplaces. Approved marketplace sales also rotate control. |
| NEAR AI Cloud | The runtime calls private models running in TEEs (default `Qwen/Qwen3.6-35B-A3B-FP8`, served by NEAR AI itself), so a client's task isn't visible to the model host. The model and request id are written into each result. |
| NEP-591 global contracts (optional) | `new(vault_code_hash)` lets every vault reuse one global copy of the vault code. This cuts the cost of creating an agent from about 1.8 NEAR to about 0.4 NEAR. The default deploy embeds the code instead, and the global path is not yet tested on testnet. |
| NEAR Connect (HOT) | Wallet connection in the web app. |

## Run it on testnet

You need a testnet account with about 10 NEAR in [near-cli-rs](https://github.com/near/near-cli-rs)'s key store (`~/.near-credentials`). Create one with `near account create-account sponsor-by-faucet-service <name>.testnet autogenerate-new-keypair --signature-scheme ed25519 save-to-legacy-keychain network-config testnet create`.

```bash
./scripts/deploy-testnet.sh <name>.testnet            # deploys franchise.<name>.testnet
```

**Agent runtime, locally** (it answers jobs for every agent whose holder activated it):

```bash
cd agent
cp .env.example .env      # set FRANCHISE_CONTRACT, NEARAI_API_KEY (optional), PORT=8787
npm start
```

With `PORT` set, the runtime serves `GET /operator/<token_id>`, which returns the public key a holder should register. Without `PORT`, create keys with `npm run keygen -- <token_id>` and register the printed key.

Where the runtime's keys live:
- **Local mode:** random keys in `.operators.json`, which is git-ignored.
- **Hosted mode:** set `OPERATOR_MASTER_SEED` (generate it with `openssl rand -hex 32`). Every agent's key is then derived from that one secret with HMAC-SHA256, so the runtime keeps no files and survives restarts.

**Agent runtime, hosted on Vercel (what the live demo uses).** `agent/api/` holds serverless functions, so the runtime needs no always-on server:

- `GET /api/health` is a liveness check.
- `GET /api/operator/<token_id>` returns the public key a holder registers.
- `GET /api/work` answers up to 3 open jobs and returns `{"delivered": n}`.

The web app calls `/api/work` while a job is open. `.github/workflows/runtime-tick.yml` also calls it every 5 minutes, for jobs hired some other way.

1. Vercel → Add New → Project → import this repo, with **Root Directory** set to `agent` and **Framework Preset** set to Other. `agent/vercel.json` covers the rest.
2. Add environment variables for Production: `NEAR_NETWORK=testnet`, `FRANCHISE_CONTRACT=franchise.<name>.testnet`, `OPERATOR_MASTER_SEED`, and optionally `NEARAI_API_KEY`. Redeploy after adding them.
3. Check that `https://<project>.vercel.app/api/operator/1` returns a key. Then set the repository variable `VITE_RUNTIME_URL` to `https://<project>.vercel.app/api` (see the web app steps below).

The live runtime is at https://franchise-sooty.vercel.app.

**Agent runtime, as a long-running process.** `npm start` (or `agent/Dockerfile`) polls the chain every few seconds, on any host that keeps a process running, such as Railway or Fly. A free tier that sleeps when idle, like Render's free plan, would stop answering jobs. Set the same variables plus `PORT`; it then serves `/health` and `/operator/<token_id>`.

**Web app.** `.github/workflows/pages.yml` builds `web/` and publishes it to GitHub Pages on every push to `main`:

1. Repository Settings → Pages → Source: **GitHub Actions**.
2. Settings → Secrets and variables → Actions → Variables: set `VITE_CONTRACT`, and `VITE_RUNTIME_URL` to the runtime's public URL.
3. Re-run the "Deploy web app" workflow.

To build it yourself: `cd web && cp .env.example .env && npm run build`. The static site lands in `web/dist`.

Then in the browser:

1. Connect a testnet wallet.
2. Fill in **Launch an agent** to mint.
3. On the agent page, click **Activate with hosted runtime**. The vault panel shows one operator key.
4. Hire the agent. Within a few seconds the job flips to Done with the answer, and the vault balance goes up.
5. Transfer the NFT to another account. The vault panel shows 0 keys, the agent is paused, and the history shows "changed hands".

### Locking the contract (production)

On testnet the deployer keeps a full-access key on `franchise.<name>.testnet` so the contract can be redeployed during the hackathon. That deployer could redeploy the franchise contract and call vaults as their controller, which is a real trust assumption. For a production launch, delete that key after deploying (`near account delete-keys …`) so the code is immutable. See [THREAT_MODEL.md](THREAT_MODEL.md).

## Repository layout

| Path | What |
| --- | --- |
| `contracts/franchise` | NFT contract, job escrow, royalties, holder controls, views (`src/lib.rs`, `src/views.rs`), unit tests (`src/tests.rs`) |
| `contracts/vault` | The contract on each agent's own account, with its unit tests |
| `integration-tests/tests/franchise.rs` | 13 end-to-end tests on a local NEAR sandbox, one per threat plus the combined views |
| `agent/` | TypeScript runtime: `npm start` (worker), `npm run keygen`, `npm run hire-agent`, `npm run demo:local`, `npm test`; `api/` serverless functions for Vercel; `Dockerfile` for long-running hosts |
| `web/` | Vite + React app: market, agent pages, owner panel |
| `scripts/` | `build.sh`, `deploy-testnet.sh` |
| `.github/workflows/` | `test.yml` runs every test on each push; `pages.yml` publishes the web app; `runtime-tick.yml` calls the hosted runtime every 5 minutes |
| `res/` | Built wasm. `vault.wasm` is embedded into `franchise.wasm` at build time. |

## Known limitations

- Creating an agent costs about 1.8 NEAR on testnet, mostly storage for the 143 KB vault code on the agent's own account. Global contracts (above) remove most of that.
- History and job lists are stored in the contract and grow with use. `get_open_work` scans every agent's jobs, which is fine at hackathon scale. A production version would index NEP-297 events instead.
- Public RPCs rate-limit per IP (FastNEAR allowed about 1 request per second in testing). The web app uses one combined view per screen (`get_market`, `get_agent_page`) and falls back to NEAR's public RPC. The runtime makes one `get_open_work` call per poll, however many agents it serves.
- `hire_agent` is triggered by the operator, not decided autonomously from task text, to avoid prompt-injected spending.
- A seller can spend up to the daily `hire_agent` cap just before a sale. Buyers can see every hire in the history.

## Built with

Written during the hackathon window, starting 2026-11-09. Libraries: near-sdk 5.30 and near-contract-standards (Rust), near-workspaces (tests), near-api-js 7, near-sandbox, openai (as the NEAR AI Cloud client), React 19, Vite 6, @hot-labs/near-connect. Most of the code was written with an AI coding assistant (Claude Code) under the author's direction.

## License

MIT, see [LICENSE](LICENSE).
