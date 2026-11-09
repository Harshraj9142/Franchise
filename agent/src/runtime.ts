/**
 * One polling step for one agent: check the operator key is still live, then answer
 * and deliver every open job. Shared by the long-running worker and the local demo.
 */
import { vaultAccountFor } from "./config.js";
import { type AgentView, type JobView, keyIsActive, operatorAccount, view } from "./chain.js";
import { answer, formatResult } from "./llm.js";

const inFlight = new Set<number>();
const lastStatus = new Map<string, string>();

export function log(tokenId: string, msg: string) {
  console.log(`${new Date().toISOString().slice(11, 19)}  agent #${tokenId}  ${msg}`);
}

/** Only log a status line when it changes, so the console stays readable. */
function status(tokenId: string, msg: string) {
  if (lastStatus.get(tokenId) === msg) return;
  lastStatus.set(tokenId, msg);
  log(tokenId, msg);
}

/** Returns the number of jobs delivered in this step. */
export async function tick(tokenId: string, secretKey: string): Promise<number> {
  const agent = await view<AgentView | null>("get_agent", { token_id: tokenId });
  if (!agent) {
    status(tokenId, "no such agent on this contract");
    return 0;
  }
  const vault = vaultAccountFor(tokenId);
  if (!(await keyIsActive(vault, secretKey))) {
    status(tokenId, `operator key not active on ${vault}: the holder (${agent.owner}) has not registered it, or the NFT changed hands`);
    return 0;
  }
  if (agent.paused) {
    status(tokenId, `paused by its holder ${agent.owner}`);
    return 0;
  }
  status(tokenId, `online · holder ${agent.owner} · ${agent.jobs_completed} jobs done`);

  let delivered = 0;
  const jobs = await view<JobView[]>("get_open_jobs", { token_id: tokenId });
  for (const job of jobs) {
    if (inFlight.has(job.id)) continue;
    inFlight.add(job.id);
    try {
      log(tokenId, `job ${job.id} from ${job.client}: "${job.task.slice(0, 80)}"`);
      const result = formatResult(await answer(agent, job.task));
      await operatorAccount(vault, secretKey).callFunction({
        contractId: vault,
        methodName: "complete_job",
        args: { job_id: job.id, result },
        gas: 100_000_000_000_000n,
      });
      delivered++;
      log(tokenId, `job ${job.id} delivered and paid`);
    } catch (err) {
      log(tokenId, `job ${job.id} failed: ${(err as Error).message.split("\n")[0]}`);
    } finally {
      inFlight.delete(job.id);
    }
  }
  return delivered;
}
