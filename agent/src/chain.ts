import { Account, JsonRpcProvider, KeyPair, type KeyPairString } from "near-api-js";
import { config } from "./config.js";

export const provider = new JsonRpcProvider({ url: config.rpcUrl });

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

/** False once the holder sold the NFT: the vault deleted the key. */
export async function keyIsActive(vault: string, secretKey: string): Promise<boolean> {
  try {
    await provider.viewAccessKey({ accountId: vault, publicKey: publicKeyOf(secretKey), finalityQuery: { finality: "optimistic" } });
    return true;
  } catch {
    return false;
  }
}
