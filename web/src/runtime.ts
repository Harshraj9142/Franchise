import { RUNTIME_URL } from "./config";

let last = 0;

/** Ask the hosted runtime to process open jobs now. A serverless runtime (Vercel) only
 *  works when called, so the app calls it after a hire and while a job is waiting.
 *  Harmless on an always-on runtime, which simply has no /work route. */
export function nudgeRuntime(force = false) {
  if (!RUNTIME_URL) return;
  const now = Date.now();
  if (!force && now - last < 15_000) return;
  last = now;
  fetch(`${RUNTIME_URL}/work`).catch(() => {});
}
