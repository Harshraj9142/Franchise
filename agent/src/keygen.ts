/**
 * Creates (or reuses) an operator key for an agent; the secret stays in OPERATOR_KEYS_FILE.
 * The NFT holder then registers the printed public key with `set_operator`
 * (from the web app's Owner panel, or with near-cli as shown).
 *
 *   npm run keygen -- 3
 */
import { config, requireContract } from "./config.js";
import { ensureOperatorKey } from "./keys.js";

const tokenId = process.argv[2];
if (!tokenId || !/^\d+$/.test(tokenId)) {
  console.error("usage: npm run keygen -- <token_id>");
  process.exit(1);
}
const contract = requireContract();
const publicKey = ensureOperatorKey(tokenId);

console.log(`\nOperator public key for agent #${tokenId}:\n\n  ${publicKey}\n`);
console.log("Register it as the NFT holder, either in the web app (Owner panel → Activate) or with near-cli:\n");
console.log(
  `  near contract call-function as-transaction ${contract} set_operator json-args '{"token_id":"${tokenId}","public_key":"${publicKey}"}' prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as <HOLDER_ACCOUNT> network-config ${config.network} sign-with-keychain send\n`,
);
