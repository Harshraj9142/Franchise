export const NETWORK = (import.meta.env.VITE_NETWORK ?? "testnet") as "testnet" | "mainnet";
export const CONTRACT: string = import.meta.env.VITE_CONTRACT ?? "";
export const RPC_URL: string =
  import.meta.env.VITE_RPC_URL ?? (NETWORK === "mainnet" ? "https://free.rpc.fastnear.com" : "https://test.rpc.fastnear.com");
export const RUNTIME_URL: string = import.meta.env.VITE_RUNTIME_URL ?? "";
export const EXPLORER = NETWORK === "mainnet" ? "https://nearblocks.io" : "https://testnet.nearblocks.io";
export const TGAS = "000000000000";
export const REPO_URL: string = import.meta.env.VITE_REPO_URL ?? "";
