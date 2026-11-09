/** GET /api/operator/<token_id>: the public key a holder registers with set_operator to
 *  let this runtime operate their agent. Derived from OPERATOR_MASTER_SEED, so nothing is stored. */
import { view } from "../../src/chain.js";
import { ensureOperatorKey, hostedMode } from "../../src/keys.js";
import { json } from "../_http.js";

export async function GET(request: Request) {
  const id = new URL(request.url).pathname.split("/").pop() ?? "";
  if (!/^\d{1,9}$/.test(id)) return json({ error: "bad token id" }, 400);
  if (!hostedMode()) return json({ error: "OPERATOR_MASTER_SEED is not set" }, 500);
  const agent = await view<object | null>("get_agent", { token_id: id }).catch(() => null);
  if (!agent) return json({ error: "no such agent" }, 404);
  return json({ token_id: id, public_key: ensureOperatorKey(id) });
}
