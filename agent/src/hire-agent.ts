/**
 * Make one agent subcontract another, paying from its own vault under the caps
 * its holder set (max_per_tx, max_per_day). Signs with the hiring agent's operator key.
 *
 *   npm run hire-agent -- <from_token> <to_token> "<task>"
 */
import { config, requireContract, vaultAccountFor } from "./config.js";
import { secretFor } from "./keys.js";
import { type AgentView, operatorAccount, view } from "./chain.js";

const [from, to, ...taskParts] = process.argv.slice(2);
const task = taskParts.join(" ");
if (!from || !to || !task) {
  console.error('usage: npm run hire-agent -- <from_token> <to_token> "<task>"');
  process.exit(1);
}
requireContract();
const secret = secretFor(from);
if (!secret) {
  console.error(`No operator key for agent #${from} (keys file ${config.keysFile} or OPERATOR_MASTER_SEED)`);
  process.exit(1);
}
const target = await view<AgentView | null>("get_agent", { token_id: to });
if (!target) throw new Error(`agent #${to} not found`);
const cfg = await view<{ job_storage_fee: string }>("get_config");
const amount = (BigInt(target.price) + BigInt(cfg.job_storage_fee)).toString();

const vault = vaultAccountFor(from);
const jobId = await operatorAccount(vault, secret).callFunction({
  contractId: vault,
  methodName: "hire_agent",
  args: { token_id: to, task, amount },
  gas: 100_000_000_000_000n,
});
console.log(jobId ? `Agent #${from} hired agent #${to}: job ${jobId}` : "Hire was rejected by the franchise contract (see explorer).");
