#!/usr/bin/env bash
# Deploy Franchise to NEAR testnet as franchise.<your-account>.testnet
#
#   ./scripts/deploy-testnet.sh your-account.testnet
#
# Needs near-cli-rs (https://github.com/near/near-cli-rs) with <your-account> in ~/.near-credentials,
# and about 7 testnet NEAR on that account (the contract is ~475 KB of storage).
set -euo pipefail
PARENT="${1:?usage: $0 <your-account.testnet>}"
CONTRACT="franchise.${PARENT}"
cd "$(dirname "$0")/.."

./scripts/build.sh

near account create-account fund-myself "$CONTRACT" '6 NEAR' \
  autogenerate-new-keypair --signature-scheme ed25519 save-to-legacy-keychain \
  sign-as "$PARENT" network-config testnet sign-with-legacy-keychain send

near contract deploy "$CONTRACT" use-file res/franchise.wasm \
  with-init-call new json-args '{"job_timeout_sec":600}' \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-legacy-keychain send

echo
echo "Deployed: $CONTRACT"
echo "Next:  agent/.env  -> FRANCHISE_CONTRACT=$CONTRACT"
echo "       web/.env    -> VITE_CONTRACT=$CONTRACT"
