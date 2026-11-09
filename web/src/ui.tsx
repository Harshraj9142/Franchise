import { useState, type ReactNode } from "react";
import { EXPLORER } from "./config";
import { short } from "./format";

export function AccountLink({ id, max }: { id: string; max?: number }) {
  return (
    <a className="mono" href={`${EXPLORER}/address/${id}`} target="_blank" rel="noreferrer" title={id}>
      {short(id, max)}
    </a>
  );
}

export function TxLink({ hash }: { hash: string }) {
  if (!hash) return null;
  return (
    <a className="mono small" href={`${EXPLORER}/txns/${hash}`} target="_blank" rel="noreferrer">
      view transaction ↗
    </a>
  );
}

/** Button that runs an async action, shows progress, the resulting tx link, or the error. */
export function Action({
  label,
  run,
  disabled,
  kind = "primary",
}: {
  label: ReactNode;
  run: () => Promise<string | void>;
  disabled?: boolean;
  kind?: "primary" | "ghost" | "danger";
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [tx, setTx] = useState("");
  return (
    <span className="action">
      <button
        className={`btn ${kind}`}
        disabled={disabled || busy}
        onClick={async () => {
          setBusy(true);
          setErr(null);
          setTx("");
          try {
            setTx((await run()) || "");
          } catch (e) {
            setErr(friendly((e as Error).message));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? "Waiting for wallet…" : label}
      </button>
      {tx && <TxLink hash={tx} />}
      {err && <span className="error small">{err}</span>}
    </span>
  );
}

function friendly(msg: string): string {
  const m = msg.match(/Smart contract panicked: ([^"\\]+)/);
  return m ? m[1] : msg.slice(0, 200);
}
