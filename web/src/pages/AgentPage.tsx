import { useState } from "react";
import { Avatar } from "../Avatar";
import { RUNTIME_URL } from "../config";
import { ago, bytes, near, toYocto } from "../format";
import { usePoll } from "../hooks";
import { accessKeys, view, type AccessKey, type Agent, type AgentPageView, type Config, type HistoryEntry, type Job, type VaultState } from "../rpc";
import { AccountLink, Action } from "../ui";
import { call, signIn, useAccount } from "../wallet";
import { StatusPill } from "./Market";

export function AgentPage({ tokenId }: { tokenId: string }) {
  const account = useAccount();
  const { data, error, refresh } = usePoll(
    async () => {
      // Three RPC calls per refresh: the agent page from the franchise contract, plus the
      // vault's own state and keys, which live on the agent's account.
      const page = await view<AgentPageView | null>("get_agent_page", { token_id: tokenId, jobs_limit: 20, history_limit: 40 });
      if (!page) return null;
      const [vault, keys] = await Promise.all([view<VaultState>("get_state", {}, page.agent.vault), accessKeys(page.agent.vault)]);
      return { ...page, vault, keys };
    },
    [tokenId],
    6000,
  );

  if (error && !data) return <p className="error">Could not load agent #{tokenId}: {error}</p>;
  if (data === undefined) return <p className="muted">Loading agent #{tokenId}…</p>;
  if (data === null) return <p className="muted">There is no agent #{tokenId}.</p>;
  const { agent, vault, keys, jobs, history, config } = data;
  const isHolder = account === agent.owner;

  return (
    <>
      <a className="back" href="#/">
        ← All agents
      </a>
      <header className="agent-head">
        <Avatar seed={`${agent.token_id}:${agent.name}`} size={88} />
        <div>
          <h1>
            {agent.name} <span className="muted mono">#{agent.token_id}</span>
          </h1>
          <div className="row">
            <StatusPill a={agent} />
            <span className="small muted">
              held by <AccountLink id={agent.owner} /> · created by <AccountLink id={agent.creator} />
            </span>
          </div>
          <p className="skill-full">{agent.skill}</p>
        </div>
      </header>

      <div className="two-col">
        <div className="col-main">
          <Hire agent={agent} config={config} onDone={refresh} />
          <Jobs jobs={jobs} />
        </div>
        <aside className="col-side">
          <VaultPanel agent={agent} vault={vault} keys={keys} />
          {isHolder && <OwnerPanel agent={agent} vault={vault} onDone={refresh} />}
          <History history={history} />
        </aside>
      </div>
    </>
  );
}

function VaultPanel({ agent, vault, keys }: { agent: Agent; vault: VaultState; keys: AccessKey[] }) {
  const full = keys.filter((k) => k.access_key.permission === "FullAccess").length;
  return (
    <section className="card vault">
      <h3>The account this NFT owns</h3>
      <p className="mono small">
        <AccountLink id={agent.vault} max={40} />
      </p>
      <div className="vault-balance">
        <span className="big">{near(vault.available)} Ⓝ</span>
        <span className="muted small">withdrawable by the holder</span>
      </div>
      <dl className="kv">
        <dt>earned (all time)</dt>
        <dd>{near(agent.total_earned)} Ⓝ</dd>
        <dt>creator royalties</dt>
        <dd>{near(agent.royalties_paid)} Ⓝ</dd>
        <dt>withdrawn</dt>
        <dd>{near(agent.total_withdrawn)} Ⓝ</dd>
        <dt>holder epoch</dt>
        <dd>
          {vault.epoch} <span className="muted small">(+1 on every sale)</span>
        </dd>
        <dt>agent hiring budget</dt>
        <dd>
          {near(vault.spent_today)} / {near(vault.max_per_day)} Ⓝ today
        </dd>
      </dl>
      <h4>
        Keys on this account <span className={`pill ${full ? "bad" : "live"}`}>{full} full-access</span>
      </h4>
      {keys.length === 0 && <p className="small muted">None. The agent is idle until the holder registers an operator.</p>}
      {keys.map((k) => {
        const p = k.access_key.permission;
        return (
          <div key={k.public_key} className="key">
            <span className="mono small">{k.public_key.slice(0, 20)}…</span>
            {p === "FullAccess" ? (
              <span className="error small">full access</span>
            ) : (
              <span className="small">
                operator key → can only call <span className="mono">{p.FunctionCall.method_names.join(", ")}</span>
              </span>
            )}
          </div>
        );
      })}
    </section>
  );
}

function Hire({ agent, config, onDone }: { agent: Agent; config: Config; onDone: () => void }) {
  const account = useAccount();
  const [task, setTask] = useState("");
  const cost = (BigInt(agent.price) + BigInt(config.job_storage_fee)).toString();
  const available = agent.operator_set && !agent.paused;
  return (
    <section className="card hire">
      <h3>Hire {agent.name}</h3>
      <textarea
        rows={3}
        value={task}
        placeholder="Describe the job…"
        onChange={(e) => setTask(e.target.value)}
        disabled={!available}
      />
      <div className="row between">
        <span className="small muted">
          {near(agent.price)} Ⓝ to the agent (held in escrow until delivery) + {near(config.job_storage_fee)} Ⓝ storage · {bytes(task)}/500
        </span>
        {account ? (
          <Action
            label={`Hire for ${near(cost)} Ⓝ`}
            disabled={!available || !task.trim() || bytes(task) > 500}
            run={async () => {
              const r = await call("hire", { token_id: agent.token_id, task: task.trim() }, cost, 50);
              setTask("");
              onDone();
              return r.txHash;
            }}
          />
        ) : (
          <button className="btn primary" onClick={() => signIn()}>
            Connect wallet to hire
          </button>
        )}
      </div>
      {!available && <p className="small muted">This agent is not taking jobs: its holder hasn't registered an operator, or it was just sold.</p>}
      <p className="small muted">If the agent doesn't deliver within {Math.round(config.job_timeout_sec / 60)} minutes you can cancel and get the price back.</p>
    </section>
  );
}

function Jobs({ jobs }: { jobs: Job[] }) {
  const account = useAccount();
  return (
    <section>
      <h3>Jobs</h3>
      {jobs.length === 0 && <p className="muted small">No jobs yet.</p>}
      {jobs.map((j) => (
        <article key={j.id} className={`job ${j.status.toLowerCase()}`}>
          <div className="row between">
            <span className="small">
              <span className={`dot ${j.status.toLowerCase()}`} /> job {j.id} · from <AccountLink id={j.client} /> · {ago(j.created_at)}
            </span>
            <span className="small mono">{near(j.price)} Ⓝ</span>
          </div>
          <p className="task">{j.task}</p>
          {j.result && <pre className="result">{j.result}</pre>}
          {j.status === "Open" && <p className="small muted working">The agent is working on it…</p>}
          {j.status === "Open" && account === j.client && (
            <Action kind="ghost" label="Cancel and refund (after timeout)" run={async () => (await call("cancel_job", { job_id: j.id }, "0", 30)).txHash} />
          )}
          {j.holder_at_completion && (
            <p className="small muted">
              paid to the account of NFT holder <AccountLink id={j.holder_at_completion} />
            </p>
          )}
        </article>
      ))}
    </section>
  );
}

const KIND_LABEL: Record<string, string> = {
  Minted: "minted",
  OperatorSet: "operator registered",
  Paused: "paused",
  Resumed: "resumed",
  JobHired: "hired",
  JobPaid: "job paid",
  RoyaltyPaid: "royalty paid",
  JobRefunded: "job refunded",
  Withdrawn: "withdrawn",
  Transferred: "changed hands",
  LimitsSet: "limits changed",
  PriceSet: "price changed",
};

function History({ history }: { history: HistoryEntry[] }) {
  return (
    <section className="card">
      <h3>History</h3>
      <ol className="history">
        {history.map((h) => (
          <li key={h.id} className={h.kind === "Transferred" ? "transfer" : ""}>
            <span className="h-kind">{KIND_LABEL[h.kind] ?? h.kind}</span>
            <span className="small">
              <AccountLink id={h.actor} max={18} />
              {h.amount && h.kind !== "LimitsSet" && h.kind !== "PriceSet" ? ` · ${near(h.amount)} Ⓝ` : ""}
            </span>
            <span className="small muted">{ago(h.timestamp)}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

function OwnerPanel({ agent, vault, onDone }: { agent: Agent; vault: VaultState; onDone: () => void }) {
  const [pk, setPk] = useState("");
  const [to, setTo] = useState("");
  const [price, setPrice] = useState(near(agent.price));
  const [perTx, setPerTx] = useState(near(vault.max_per_tx));
  const [perDay, setPerDay] = useState(near(vault.max_per_day));
  const id = agent.token_id;
  const done = (hash: string) => {
    onDone();
    return hash;
  };

  return (
    <section className="card owner">
      <h3>You hold this NFT</h3>

      <h4>Operator</h4>
      {RUNTIME_URL && (
        <Action
          label={agent.operator_set ? "Re-activate hosted runtime" : "Activate with hosted runtime"}
          run={async () => {
            const res = await fetch(`${RUNTIME_URL}/operator/${id}`);
            if (!res.ok) throw new Error(`runtime said ${res.status}`);
            const { public_key } = await res.json();
            return done((await call("set_operator", { token_id: id, public_key }, "1")).txHash);
          }}
        />
      )}
      <label className="small">
        {RUNTIME_URL ? "…or register your own runtime's key" : "Operator public key (from `npm run keygen` in agent/)"}
        <input className="mono" value={pk} placeholder="ed25519:…" onChange={(e) => setPk(e.target.value.trim())} />
      </label>
      <Action kind="ghost" label="Register key" disabled={!pk.startsWith("ed25519:")} run={async () => done((await call("set_operator", { token_id: id, public_key: pk }, "1")).txHash)} />
      {agent.operator_set && (
        <Action
          kind="ghost"
          label={agent.paused ? "Resume agent" : "Pause agent"}
          run={async () => done((await call("set_paused", { token_id: id, paused: !agent.paused }, "1")).txHash)}
        />
      )}

      <h4>Earnings</h4>
      <Action
        label={`Withdraw ${near(vault.available)} Ⓝ`}
        disabled={BigInt(vault.available) <= 0n}
        run={async () => done((await call("withdraw", { token_id: id }, "1")).txHash)}
      />

      <h4>Price and hiring budget</h4>
      <div className="inline-fields">
        <label className="small">
          price Ⓝ
          <input value={price} onChange={(e) => setPrice(e.target.value)} />
        </label>
        <Action kind="ghost" label="Set price" run={async () => done((await call("set_price", { token_id: id, price: toYocto(price) }, "1", 30)).txHash)} />
      </div>
      <div className="inline-fields">
        <label className="small">
          max/tx Ⓝ
          <input value={perTx} onChange={(e) => setPerTx(e.target.value)} />
        </label>
        <label className="small">
          max/day Ⓝ
          <input value={perDay} onChange={(e) => setPerDay(e.target.value)} />
        </label>
        <Action
          kind="ghost"
          label="Set limits"
          run={async () => done((await call("set_limits", { token_id: id, max_per_tx: toYocto(perTx), max_per_day: toYocto(perDay) }, "1")).txHash)}
        />
      </div>

      <h4>Sell or transfer</h4>
      <p className="small muted">The receiver gets the account, its {near(vault.available)} Ⓝ and future earnings. Your operator key is revoked in the same transaction.</p>
      <label className="small">
        Receiver account
        <input className="mono" value={to} placeholder="buyer.testnet" onChange={(e) => setTo(e.target.value.trim())} />
      </label>
      <Action
        kind="danger"
        label="Transfer NFT"
        disabled={!to}
        run={async () => done((await call("nft_transfer", { receiver_id: to, token_id: id }, "1")).txHash)}
      />
    </section>
  );
}

