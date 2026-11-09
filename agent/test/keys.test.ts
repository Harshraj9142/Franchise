import { test } from "node:test";
import assert from "node:assert/strict";
import { KeyPair, type KeyPairString } from "near-api-js";
import { deriveOperatorKey } from "../src/keys.js";

const SEED = "0123456789abcdef0123456789abcdef";

test("derived operator keys are valid NEAR ed25519 keys", () => {
  const sk = deriveOperatorKey(SEED, "franchise.example.testnet", "1");
  const kp = KeyPair.fromString(sk as KeyPairString);
  // Signing and verifying proves the public half matches the secret half.
  const msg = new TextEncoder().encode("franchise");
  const { signature } = kp.sign(msg);
  assert.ok(kp.verify(msg, signature));
  assert.match(kp.getPublicKey().toString(), /^ed25519:/);
});

test("derivation is deterministic and unique per agent and per contract", () => {
  const a = deriveOperatorKey(SEED, "franchise.example.testnet", "1");
  assert.equal(a, deriveOperatorKey(SEED, "franchise.example.testnet", "1"));
  assert.notEqual(a, deriveOperatorKey(SEED, "franchise.example.testnet", "2"));
  assert.notEqual(a, deriveOperatorKey(SEED, "other.example.testnet", "1"));
  assert.notEqual(a, deriveOperatorKey(SEED + "x", "franchise.example.testnet", "1"));
});
