import { CONTRACT, RPC_URL } from "./config";

async function rpc<T>(method: string, params: unknown): Promise<T> {
  const res = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "franchise", method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(body.error.data ?? body.error.message ?? "RPC error");
  if (body.result?.error) throw new Error(body.result.error);
  return body.result as T;
}

const enc = (o: unknown) => btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(o))));

export async function view<T>(method: string, args: object = {}, contract = CONTRACT): Promise<T> {
  const r = await rpc<{ result: number[] }>("query", {
    request_type: "call_function",
    finality: "optimistic",
    account_id: contract,
    method_name: method,
    args_base64: enc(args),
  });
  return JSON.parse(new TextDecoder().decode(new Uint8Array(r.result)));
}

export interface AccessKey {
  public_key: string;
  access_key: { permission: "FullAccess" | { FunctionCall: { receiver_id: string; method_names: string[]; allowance: string | null } } };
}

export async function accessKeys(account: string): Promise<AccessKey[]> {
  const r = await rpc<{ keys: AccessKey[] }>("query", { request_type: "view_access_key_list", finality: "optimistic", account_id: account });
  return r.keys;
}

// ----- contract types -----

export interface Agent {
  token_id: string;
  name: string;
  skill: string;
  price: string;
  creator: string;
  owner: string;
  vault: string;
  epoch: number;
  operator_set: boolean;
  paused: boolean;
  jobs_completed: number;
  open_jobs: number;
  total_earned: string;
  royalties_paid: string;
  total_withdrawn: string;
  created_at: string;
}

export interface Job {
  id: number;
  token_id: string;
  client: string;
  task: string;
  price: string;
  status: "Open" | "Done" | "Refunded";
  result: string | null;
  created_at: string;
  completed_at: string | null;
  holder_at_completion: string | null;
}

export interface HistoryEntry {
  id: number;
  kind: string;
  token_id: string;
  actor: string;
  amount: string | null;
  job_id: number | null;
  epoch: number;
  timestamp: string;
}

export interface Config {
  mint_cost: string;
  job_storage_fee: string;
  min_price: string;
  creator_royalty_bps: number;
  job_timeout_sec: number;
  total_agents: number;
  total_jobs: number;
}

export interface VaultState {
  owner: string;
  epoch: number;
  paused: boolean;
  balance: string;
  available: string;
  max_per_tx: string;
  max_per_day: string;
  spent_today: string;
  total_spent: string;
  total_withdrawn: string;
}
