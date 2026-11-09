import { NearConnector } from "@hot-labs/near-connect";
import { useEffect, useState } from "react";
import { CONTRACT, NETWORK, TGAS } from "./config";

export const connector = new NearConnector({
  network: NETWORK,
  features: { signAndSendTransaction: true, ...(NETWORK === "testnet" ? { testnet: true } : {}) },
});

let current: string | null = null;
const listeners = new Set<(a: string | null) => void>();
const set = (a: string | null) => {
  current = a;
  listeners.forEach((l) => l(a));
};

connector.on("wallet:signIn", (e) => set(e.accounts[0]?.accountId ?? null));
connector.on("wallet:signOut", () => set(null));
connector
  .getConnectedWallet()
  .then(({ accounts }) => set(accounts[0]?.accountId ?? null))
  .catch(() => set(null));

export function useAccount(): string | null {
  const [a, setA] = useState(current);
  useEffect(() => {
    listeners.add(setA);
    return () => void listeners.delete(setA);
  }, []);
  return a;
}

export const signIn = () => connector.connect();
export const signOut = () => connector.disconnect();

export interface CallResult<T> {
  value: T | undefined;
  txHash: string;
}

/** Sign and send one function call to the franchise contract; returns the decoded return value. */
export async function call<T = unknown>(method: string, args: object, deposit = "0", tgas = 100): Promise<CallResult<T>> {
  const wallet = await connector.wallet();
  const outcome = await wallet.signAndSendTransaction({
    receiverId: CONTRACT,
    actions: [{ type: "FunctionCall", params: { methodName: method, args, gas: `${tgas}${TGAS}`, deposit } }],
  });
  const status = outcome.status as { SuccessValue?: string; Failure?: unknown };
  if (status && typeof status === "object" && "Failure" in status) {
    throw new Error(JSON.stringify(status.Failure).slice(0, 300));
  }
  const raw = status?.SuccessValue;
  const value = raw ? (JSON.parse(atob(raw)) as T) : undefined;
  return { value, txHash: outcome.transaction?.hash ?? outcome.transaction_outcome?.id ?? "" };
}
