const YOCTO = 10n ** 24n;

export function near(yocto: string | bigint | null | undefined, digits = 3): string {
  if (yocto == null) return "0";
  const v = BigInt(yocto);
  const whole = v / YOCTO;
  const frac = (v % YOCTO).toString().padStart(24, "0").slice(0, digits).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole.toString();
}

export function toYocto(nearAmount: string): string {
  const [w, f = ""] = nearAmount.trim().split(".");
  if (!/^\d+$/.test(w || "0") || !/^\d*$/.test(f)) throw new Error("not a number");
  return (BigInt(w || "0") * YOCTO + BigInt((f + "0".repeat(24)).slice(0, 24))).toString();
}

export function ago(ns: string | null): string {
  if (!ns) return "";
  const s = Math.max(0, Math.floor((Date.now() - Number(BigInt(ns) / 1_000_000n)) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

export function short(account: string, max = 22): string {
  return account.length <= max ? account : `${account.slice(0, max - 9)}…${account.slice(-8)}`;
}

export const bytes = (s: string) => new TextEncoder().encode(s).length;
