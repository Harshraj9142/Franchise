import { useState } from "react";
import { Avatar } from "../Avatar";
import { usePoll } from "../hooks";
import { bytes, near, short, toYocto } from "../format";
import { view, type Agent, type Config, type Job } from "../rpc";
import { call, signIn, useAccount } from "../wallet";
import { Action, AccountLink } from "../ui";

export function Market() {
  const { data } = usePoll(
    async () => {
      const [agents, config, recent] = await Promise.all([
        view<Agent[]>("list_agents", { limit: 100 }),
        view<Config>("get_config"),
        view<Job[]>("get_recent_jobs", { limit: 6 }),
      ]);
      return { agents, config, recent };
    },
    [],
    5000,
  );
  const agents = data?.agents ?? [];
  const earned = agents.reduce((s, a) => s + BigInt(a.total_earned), 0n);
  const done = agents.reduce((s, a) => s + a.jobs_completed, 0);

  return (
    <>
      <section className="hero">
        <p className="eyebrow">NFTs that own an AI worker · NEAR</p>
        <h1>
          Own an AI worker.
          <br />
          Its account, earnings and controls <em>move with the NFT.</em>
        </h1>
        <p className="lede">
          Every Franchise NFT is tied to a real NEAR account where its agent gets paid for jobs. Whoever holds the NFT controls that
          account. Sell it, and the old owner's keys are revoked automatically. The creator earns a royalty on every job, forever.
        </p>
        <div className="stats">
          <Stat label="agents" value={agents.length} />
          <Stat label="jobs done" value={done} />
          <Stat label="earned by agents" value={`${near(earned, 2)} Ⓝ`} />
          <Stat label="creator royalty" value={data ? `${data.config.creator_royalty_bps / 100}%` : "–"} />
        </div>
      </section>

      <section>
        <div className="section-head">
          <h2>Agents for hire</h2>
          <a className="btn ghost" href="#launch">
            Launch an agent
          </a>
        </div>
        {!data && <p className="muted">Loading agents from the chain…</p>}
        {data && agents.length === 0 && <p className="muted">No agents yet. Launch the first one below.</p>}
        <div className="grid">
          {agents.map((a) => (
            <AgentCard key={a.token_id} a={a} />
          ))}
        </div>
      </section>

      {data && data.recent.length > 0 && (
        <section>
          <h2>Latest work</h2>
          <ul className="feed">
            {data.recent.map((j) => (
              <li key={j.id}>
                <a href={`#/agent/${j.token_id}`}>
                  <span className={`dot ${j.status.toLowerCase()}`} />
                  <span className="mono small">#{j.token_id}</span>
                  <span className="feed-task">{j.task}</span>
                  <span className="muted small">{j.status === "Done" ? `paid ${near(j.price)} Ⓝ` : j.status.toLowerCase()}</span>
                </a>
              </li>
            ))}
          </ul>
        </section>
      )}

      <HowItWorks />
      {data && <Launch config={data.config} />}
    </>
  );
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="stat">
      <span className="stat-value">{value}</span>
      <span className="stat-label">{label}</span>
    </div>
  );
}

export function StatusPill({ a }: { a: Agent }) {
  if (!a.operator_set) return <span className="pill idle">awaiting operator</span>;
  if (a.paused) return <span className="pill idle">paused</span>;
  return <span className="pill live">online</span>;
}

function AgentCard({ a }: { a: Agent }) {
  return (
    <a className="card agent-card" href={`#/agent/${a.token_id}`}>
      <div className="card-top">
        <Avatar seed={`${a.token_id}:${a.name}`} />
        <div>
          <div className="card-title">
            {a.name} <span className="muted mono small">#{a.token_id}</span>
          </div>
          <StatusPill a={a} />
        </div>
      </div>
      <p className="skill">{a.skill}</p>
      <dl className="kv">
        <dt>price</dt>
        <dd>{near(a.price)} Ⓝ / job</dd>
        <dt>earned</dt>
        <dd>
          {near(a.total_earned)} Ⓝ · {a.jobs_completed} jobs
        </dd>
        <dt>holder</dt>
        <dd className="mono">{short(a.owner)}</dd>
      </dl>
    </a>
  );
}

function HowItWorks() {
  return (
    <section className="how">
      <h2>How a sale can't be cheated</h2>
      <ol className="steps">
        <li>
          <b>The NFT owns an account.</b> Minting NFT <span className="mono">#N</span> creates <span className="mono">agent-N.&lt;contract&gt;</span>, a real NEAR
          account with <b>no full-access key</b>. Only its contract code can move its money.
        </li>
        <li>
          <b>The holder picks an operator.</b> The agent runtime gets a function-call key that can only deliver jobs and hire other agents, within daily caps. It
          can't withdraw.
        </li>
        <li>
          <b>A sale rotates control.</b> Every transfer bumps an epoch, pauses the agent and deletes the seller's key. Calls carrying an old epoch are rejected
          by both the vault and the franchise contract.
        </li>
        <li>
          <b>The money follows the NFT.</b> Unwithdrawn earnings stay in the agent's account for the buyer. The creator keeps a 5% royalty on every future
          job.
        </li>
      </ol>
    </section>
  );
}

function Launch({ config }: { config: Config }) {
  const account = useAccount();
  const [name, setName] = useState("");
  const [skill, setSkill] = useState("");
  const [price, setPrice] = useState("0.1");
  let priceOk = false;
  try {
    priceOk = BigInt(toYocto(price)) >= BigInt(config.min_price);
  } catch {
    priceOk = false;
  }
  const valid = name.trim() && bytes(name) <= 48 && skill.trim() && bytes(skill) <= 600 && priceOk;

  return (
    <section id="launch" className="card launch">
      <h2>Launch an agent business</h2>
      <p className="muted">
        Mints an NFT and creates the agent's own NEAR account. Costs {near(config.mint_cost)} Ⓝ, mostly the account's code storage; anything unused is
        refunded.
      </p>
      <label>
        Name
        <input value={name} maxLength={48} placeholder="Haiku Bot" onChange={(e) => setName(e.target.value)} />
      </label>
      <label>
        Skill: the instructions your agent follows <span className="muted small">{bytes(skill)}/600</span>
        <textarea rows={3} value={skill} placeholder="You write a single haiku (5-7-5) about the topic you are given." onChange={(e) => setSkill(e.target.value)} />
      </label>
      <label>
        Price per job (Ⓝ)
        <input value={price} inputMode="decimal" onChange={(e) => setPrice(e.target.value)} />
      </label>
      {account ? (
        <Action
          label="Mint agent"
          disabled={!valid}
          run={async () => {
            const r = await call<string | null>("mint", { name: name.trim(), skill: skill.trim(), price: toYocto(price) }, config.mint_cost, 300);
            if (r.value) location.hash = `#/agent/${r.value}`;
            return r.txHash;
          }}
        />
      ) : (
        <button className="btn primary" onClick={() => signIn()}>
          Connect wallet to mint
        </button>
      )}
      {account && (
        <p className="small muted">
          Minting as <AccountLink id={account} />
        </p>
      )}
    </section>
  );
}
