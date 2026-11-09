import { json } from "./_http.js";

export function GET() {
  return json({ ok: true });
}
