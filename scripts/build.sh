#!/usr/bin/env bash
# Build both contracts. The vault is built first because the franchise contract
# embeds its wasm (res/vault.wasm) for accounts that don't use a global contract.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p res
(cd contracts/vault && cargo near build non-reproducible-wasm --no-abi)
cp target/near/vault/vault.wasm res/vault.wasm
(cd contracts/franchise && cargo near build non-reproducible-wasm --no-abi)
cp target/near/franchise/franchise.wasm res/franchise.wasm
ls -l res/*.wasm
