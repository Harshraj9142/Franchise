import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { KeyPair, type KeyPairString } from "near-api-js";
import { config } from "./config.js";

/** token_id -> operator secret key ("ed25519:..."). Re-read on every call so new keys are picked up. */
export function loadOperatorKeys(): Record<string, string> {
  if (!existsSync(config.keysFile)) return {};
  return JSON.parse(readFileSync(config.keysFile, "utf8"));
}

/** Returns the operator public key for a token, creating and storing a key if needed. */
export function ensureOperatorKey(tokenId: string): string {
  const keys = loadOperatorKeys();
  if (!keys[tokenId]) {
    keys[tokenId] = KeyPair.fromRandom("ed25519").toString();
    writeFileSync(config.keysFile, JSON.stringify(keys, null, 2) + "\n", { mode: 0o600 });
  }
  return KeyPair.fromString(keys[tokenId] as KeyPairString).getPublicKey().toString();
}
