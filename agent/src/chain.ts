import { Account, FailoverRpcProvider, JsonRpcProvider, KeyPair, type KeyPairString } from "near-api-js";
import { config } from "./config.js";

/** The configured RPC first, then NEAR's public RPC if it is down or rate-limiting. */
export const provider = new FailoverRpcProvider(
  [config.rpcUrl, ...(config.rpcUrl.includes("127.0.0.1") || config.rpcUrl.includes("localhost") ? [] : [config.fallbackRpcUrl])].map(
    (url) => new JsonRpcProvider({ url }),
  ),
);

export async function view<T>(method: string, args: Record<string, unknown> = {}, contractId = config.contract): Promise<T> {
  return (await provider.callFunction<T & object>({ contractId, method, args, blockQuery: { finality: "optimistic" } })) as T;
}

export interface AgentView {
  token_id: string;
  name: string;
  skill: string;
  price: string;
  owner: string;
  creator: string;
  vault: string;
  epoch: number;
  paused: boolean;
  operator_set: boolean;
  jobs_completed: number;
  open_jobs: number;
  total_earned: string;
}

export interface WorkItem {
  job: JobView;
  agent: AgentView;
}

export interface JobView {
  id: number;
  token_id: string;
  client: string;
  task: string;
  price: string;
  status: "Open" | "Done" | "Refunded";
  result: string | null;
}

/** An Account that signs as the vault using the operator function-call key. */
export function operatorAccount(vault: string, secretKey: string): Account {
  return new Account(vault, provider, secretKey as KeyPairString);
}

export function publicKeyOf(secretKey: string): string {
  return KeyPair.fromString(secretKey as KeyPairString).getPublicKey().toString();
}

/** False once the holder sold the NFT: the vault deleted the key. Lists keys rather than
 *  querying one, so a missing key is a normal answer instead of an RPC error. */
export async function keyIsActive(vault: string, secretKey: string): Promise<boolean> {
  const wanted = publicKeyOf(secretKey);
  const list = await provider.viewAccessKeyList({ accountId: vault, finalityQuery: { finality: "optimistic" } });
  return list.keys.some((k) => k.public_key.toString() === wanted);
}
