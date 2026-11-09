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
  contract: process.env.FRANCHISE_CONTRACT ?? "",
  keysFile: process.env.OPERATOR_KEYS_FILE ?? ".operators.json",
  llmMode: (process.env.LLM_MODE ?? "auto") as "auto" | "nearai" | "mock",
  nearaiKey: process.env.NEARAI_API_KEY ?? "",
  nearaiModel: process.env.NEARAI_MODEL ?? "Qwen/Qwen3.6-35B-A3B-FP8",
  nearaiBaseUrl: process.env.NEARAI_BASE_URL ?? "https://cloud-api.near.ai/v1",
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
