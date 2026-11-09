try {
  process.loadEnvFile?.(".env");
} catch {
  /* no .env file: rely on the environment */
}

const network = (process.env.NEAR_NETWORK ?? "testnet") as "testnet" | "mainnet";

export const config = {
  network,
  rpcUrl:
    process.env.NEAR_RPC_URL ??
    (network === "mainnet" ? "https://free.rpc.fastnear.com" : "https://test.rpc.fastnear.com"),
  fallbackRpcUrl: network === "mainnet" ? "https://rpc.mainnet.near.org" : "https://rpc.testnet.near.org",
  contract: process.env.FRANCHISE_CONTRACT ?? "",
  keysFile: process.env.OPERATOR_KEYS_FILE ?? ".operators.json",
  masterSeed: process.env.OPERATOR_MASTER_SEED ?? "",
  llmMode: (process.env.LLM_MODE ?? "auto") as "auto" | "nearai" | "mock" | "scripted",
  /** LLM_MODE=scripted: the exact answer to deliver, labelled as scripted (for recordings without AI credits). */
  scriptedAnswer: process.env.SCRIPTED_ANSWER ?? "",
  nearaiKey: process.env.NEARAI_API_KEY ?? "",
  nearaiModel: process.env.NEARAI_MODEL ?? "Qwen/Qwen3.6-35B-A3B-FP8",
  nearaiBaseUrl: process.env.NEARAI_BASE_URL ?? "https://cloud-api.near.ai/v1",
  /** Reasoning models can spend the whole token budget thinking and return no answer.
   *  "none" turns thinking off; set NEARAI_REASONING="" to send nothing. */
  nearaiReasoning: process.env.NEARAI_REASONING ?? "none",
  pollMs: Number(process.env.POLL_MS ?? 4000),
};

export function requireContract(): string {
  if (!config.contract) {
    console.error("FRANCHISE_CONTRACT is not set. Copy .env.example to .env and fill it in.");
    process.exit(1);
  }
  return config.contract;
}

export function vaultAccountFor(tokenId: string, contract = config.contract): string {
  return `agent-${tokenId}.${contract}`;
}
