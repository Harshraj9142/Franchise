import { createHmac, createPrivateKey, createPublicKey } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { baseEncode, KeyPair, type KeyPairString } from "near-api-js";
import { config } from "./config.js";
import { view } from "./chain.js";

/**
 * Operator keys come from one of two places:
 *  - OPERATOR_MASTER_SEED set (hosted mode): every agent's key is derived from the seed,
 *    so the runtime keeps no state and survives restarts and redeploys.
 *  - otherwise: random keys stored in OPERATOR_KEYS_FILE (local mode, `npm run keygen`).
 */

const PKCS8_ED25519_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");

/** Deterministic ed25519 key for one agent of one contract, in NEAR's "ed25519:<base58 seed||pub>" format. */
export function deriveOperatorKey(masterSeed: string, contract: string, tokenId: string): string {
  const seed = createHmac("sha256", masterSeed).update(`franchise-operator:${contract}:${tokenId}`).digest();
  const priv = createPrivateKey({ key: Buffer.concat([PKCS8_ED25519_PREFIX, seed]), format: "der", type: "pkcs8" });
  const pub = createPublicKey(priv).export({ format: "der", type: "spki" }).subarray(-32);
  return `ed25519:${baseEncode(Buffer.concat([seed, pub]))}`;
}

export function hostedMode(): boolean {
  if (!config.masterSeed) return false;
  if (config.masterSeed.length < 32) throw new Error("OPERATOR_MASTER_SEED must be at least 32 characters (use: openssl rand -hex 32)");
  return true;
}

/** token_id -> operator secret key, from the keys file. Re-read on every call so new keys are picked up. */
export function loadOperatorKeys(): Record<string, string> {
  if (!existsSync(config.keysFile)) return {};
  return JSON.parse(readFileSync(config.keysFile, "utf8"));
}

export function secretFor(tokenId: string): string | undefined {
  if (hostedMode()) return deriveOperatorKey(config.masterSeed, config.contract, tokenId);
  return loadOperatorKeys()[tokenId];
}

/** Returns the operator public key for a token, creating and storing a key if needed (file mode). */
export function ensureOperatorKey(tokenId: string): string {
  let secret = secretFor(tokenId);
  if (!secret) {
    const keys = loadOperatorKeys();
    secret = keys[tokenId] = KeyPair.fromRandom("ed25519").toString();
    writeFileSync(config.keysFile, JSON.stringify(keys, null, 2) + "\n", { mode: 0o600 });
  }
  return KeyPair.fromString(secret as KeyPairString).getPublicKey().toString();
}

/** The agents this runtime should look after: all of them in hosted mode, else those in the keys file. */
export async function operatedTokens(): Promise<string[]> {
  if (!hostedMode()) return Object.keys(loadOperatorKeys());
  const ids: string[] = [];
  for (let from = 0; ; from += 100) {
    const page = await view<{ token_id: string }[]>("list_agents", { from_index: from, limit: 100 });
    ids.push(...page.map((a) => a.token_id));
    if (page.length < 100) return ids;
  }
}
