/** GET /api/work: one pass over open jobs. Answers each job for an agent whose holder
 *  registered this runtime's key, and delivers it by signing complete_job as the vault.
 *  Called by the web app after a hire and while a job is open, and every 5 minutes by a
 *  GitHub Actions schedule. Anyone may call it: it only does work clients already paid for. */
import { hostedMode, secretFor } from "../src/keys.js";
import { pollWork } from "../src/runtime.js";
import { json } from "./_http.js";

export const config = { maxDuration: 60 };

export async function GET() {
  if (!hostedMode()) return json({ error: "OPERATOR_MASTER_SEED is not set" }, 500);
  const started = Date.now();
  try {
    const delivered = await pollWork(secretFor, 3);
    return json({ delivered, ms: Date.now() - started });
  } catch (e) {
    return json({ error: (e as Error).message.split("\n")[0] }, 502);
  }
}
