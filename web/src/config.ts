export const NETWORK = (import.meta.env.VITE_NETWORK ?? "testnet") as "testnet" | "mainnet";
export const CONTRACT: string = import.meta.env.VITE_CONTRACT ?? "";
/** Tried in order; the first one that answers is remembered. */
export const RPC_URLS: string[] = [
  ...(import.meta.env.VITE_RPC_URL ? [import.meta.env.VITE_RPC_URL as string] : []),
  ...(NETWORK === "mainnet"
    ? ["https://free.rpc.fastnear.com", "https://rpc.mainnet.near.org"]
    : ["https://test.rpc.fastnear.com", "https://rpc.testnet.near.org"]),
];
export const RUNTIME_URL: string = import.meta.env.VITE_RUNTIME_URL ?? "";
export const EXPLORER = NETWORK === "mainnet" ? "https://nearblocks.io" : "https://testnet.nearblocks.io";
export const TGAS = "000000000000";
export const REPO_URL: string = import.meta.env.VITE_REPO_URL ?? "";
