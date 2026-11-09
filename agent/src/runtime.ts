/**
 * Polling and delivery, shared by the long-running worker and the local demo.
 *
 * The worker uses `pollWork`: one `get_open_work` call per poll returns every open job
 * across all agents, so the RPC load doesn't grow with the number of agents. The vault's
 * access key is only checked when an agent actually has work (and cached per epoch).
 */
import { vaultAccountFor } from "./config.js";
import { type AgentView, type JobView, type WorkItem, keyIsActive, operatorAccount, view } from "./chain.js";
import { answer, formatResult } from "./llm.js";

const inFlight = new Set<number>();
const lastStatus = new Map<string, string>();
const keyCache = new Map<string, { epoch: number; active: boolean; at: number }>();
const KEY_CACHE_MS = 60_000;

/** Failed jobs wait before the next try (30 s, 1 min, 2 min … up to 15 min), so a broken
 *  model or key can't burn AI credits by retrying every poll. */
const failures = new Map<number, { count: number; nextAt: number }>();
const RETRY_BASE_MS = 30_000;
const RETRY_MAX_MS = 15 * 60_000;

export function retryDelayMs(count: number): number {
  return Math.min(RETRY_BASE_MS * 2 ** Math.max(0, count - 1), RETRY_MAX_MS);
}

function coolingDown(jobId: number): boolean {
  const f = failures.get(jobId);
  return !!f && Date.now() < f.nextAt;
}

export function log(tokenId: string, msg: string) {
  console.log(`${new Date().toISOString().slice(11, 19)}  agent #${tokenId}  ${msg}`);
}

/** Only log a status line when it changes, so the console stays readable. */
function status(tokenId: string, msg: string) {
  if (lastStatus.get(tokenId) === msg) return;
  lastStatus.set(tokenId, msg);
  log(tokenId, msg);
}

async function operatorKeyActive(agent: AgentView, secretKey: string): Promise<boolean> {
  const cached = keyCache.get(agent.token_id);
  if (cached && cached.epoch === agent.epoch && Date.now() - cached.at < KEY_CACHE_MS) return cached.active;
  const active = await keyIsActive(vaultAccountFor(agent.token_id), secretKey);
  keyCache.set(agent.token_id, { epoch: agent.epoch, active, at: Date.now() });
  return active;
}

/** Answer one job and deliver it by signing complete_job as the vault. Returns 1 if paid. */
async function deliver(agent: AgentView, job: JobView, secretKey: string): Promise<number> {
  if (inFlight.has(job.id) || coolingDown(job.id)) return 0;
  inFlight.add(job.id);
  const vault = vaultAccountFor(agent.token_id);
  try {
    log(agent.token_id, `job ${job.id} from ${job.client}: "${job.task.slice(0, 80)}"`);
    const result = formatResult(await answer(agent, job.task));
    await operatorAccount(vault, secretKey).callFunction({
      contractId: vault,
      methodName: "complete_job",
      args: { job_id: job.id, result },
      gas: 100_000_000_000_000n,
    });
    log(agent.token_id, `job ${job.id} delivered and paid`);
    failures.delete(job.id);
    return 1;
  } catch (err) {
    keyCache.delete(agent.token_id); // the key may have been revoked by a sale
    const count = (failures.get(job.id)?.count ?? 0) + 1;
    const wait = retryDelayMs(count);
    failures.set(job.id, { count, nextAt: Date.now() + wait });
    log(agent.token_id, `job ${job.id} failed (attempt ${count}, retrying in ${Math.round(wait / 1000)}s): ${(err as Error).message.split("\n")[0]}`);
    return 0;
  } finally {
    inFlight.delete(job.id);
  }
}

/** One poll for every agent this runtime holds a key for. Returns the number of jobs delivered.
 *  `maxJobs` keeps a serverless call (Vercel) inside its time limit. */
export async function pollWork(secretFor: (tokenId: string) => string | undefined, maxJobs = 20): Promise<number> {
  const work = await view<WorkItem[]>("get_open_work", { limit: maxJobs });
  let delivered = 0;
  for (const { job, agent } of work) {
    const secret = secretFor(agent.token_id);
    if (!secret) continue;
    if (!(await operatorKeyActive(agent, secret))) {
      status(agent.token_id, `has open jobs, but this runtime's key isn't registered on ${vaultAccountFor(agent.token_id)}`);
      continue;
    }
    status(agent.token_id, `online · holder ${agent.owner}`);
    delivered += await deliver(agent, job, secret);
  }
  return delivered;
}

/** Check and serve a single agent (used by the local demo to narrate each step). */
export async function tick(tokenId: string, secretKey: string): Promise<number> {
  const agent = await view<AgentView | null>("get_agent", { token_id: tokenId });
  if (!agent) {
    status(tokenId, "no such agent on this contract");
    return 0;
  }
  keyCache.delete(tokenId);
  if (!(await operatorKeyActive(agent, secretKey))) {
    status(tokenId, `operator key not active on ${vaultAccountFor(tokenId)}: the holder (${agent.owner}) has not registered it, or the NFT changed hands`);
    return 0;
  }
  if (agent.paused) {
    status(tokenId, `paused by its holder ${agent.owner}`);
    return 0;
  }
  status(tokenId, `online · holder ${agent.owner} · ${agent.jobs_completed} jobs done`);
  let delivered = 0;
  for (const job of await view<JobView[]>("get_open_jobs", { token_id: tokenId })) delivered += await deliver(agent, job, secretKey);
  return delivered;
}
