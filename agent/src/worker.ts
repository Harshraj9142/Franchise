/**
 * The agent runtime. For every agent we hold an operator key for, it polls the
 * franchise contract for open jobs, answers them with a private model on NEAR AI
 * Cloud, and delivers the result by signing `complete_job` on the agent's vault
 * with the operator function-call key. The vault forwards it to the franchise
 * contract, which pays the vault and the creator's royalty.
 *
 *   npm start
 */
import { createServer } from "node:http";
import { config, requireContract } from "./config.js";
import { view } from "./chain.js";
import { ensureOperatorKey, hostedMode, operatedTokens, secretFor } from "./keys.js";
import { useNearAi } from "./llm.js";
import { log, tick } from "./runtime.js";

/**
 * Optional HTTP endpoint so holders can "hire" this hosted runtime from the web app:
 * GET /operator/<token_id> returns the public key the holder should register with
 * set_operator. The key does nothing until the holder registers it, and it can only
 * call complete_job and hire_agent on that agent's vault.
 */
function serve(port: number) {
  createServer(async (req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    const m = req.url?.match(/^\/operator\/(\d{1,9})$/);
    if (req.method === "GET" && m) {
      const agent = await view<object | null>("get_agent", { token_id: m[1] }).catch(() => null);
      if (!agent) {
        res.writeHead(404).end(JSON.stringify({ error: "no such agent" }));
        return;
      }
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ token_id: m[1], public_key: ensureOperatorKey(m[1]) }));
      return;
    }
    if (req.url === "/health") {
      res.writeHead(200).end("ok");
      return;
    }
    res.writeHead(404).end();
  }).listen(port, () => console.log(`Operator key endpoint: http://localhost:${port}/operator/<token_id>`));
}

async function main() {
  requireContract();
  console.log(`Franchise runtime · ${config.network} · ${config.contract}`);
  console.log(`LLM: ${useNearAi() ? `${config.nearaiModel} on NEAR AI Cloud` : "mock (set NEARAI_API_KEY for real answers)"}`);
  console.log(`Keys: ${hostedMode() ? "derived from OPERATOR_MASTER_SEED (operating every agent that registers them)" : `file ${config.keysFile}`}`);
  if (process.env.PORT) serve(Number(process.env.PORT));
  else if ((await operatedTokens()).length === 0) {
    console.error(`No operator keys in ${config.keysFile}. Run: npm run keygen -- <token_id>, or set PORT to serve keys to the web app.`);
    process.exit(1);
  }

  for (;;) {
    try {
      const tokens = await operatedTokens();
      await Promise.all(
        tokens.map((t) => tick(t, secretFor(t)!).catch((e) => log(t, `error: ${(e as Error).message.split("\n")[0]}`))),
      );
    } catch (e) {
      console.error(`poll failed: ${(e as Error).message.split("\n")[0]}`);
    }
    await new Promise((r) => setTimeout(r, config.pollMs));
  }
}

main();
