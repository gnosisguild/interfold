#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/dev_config.sh"

load_template_dev_config
cd "${TEMPLATE_ROOT}"

echo "Waiting for evm node..."
pnpm wait-on tcp:localhost:8545
echo "Waiting for contracts to be deployed..."
pnpm wait-on file:/tmp/interfold_ciphernodes_ready && \
  (export PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" CHAIN_ID=31337 && \
  export $(interfold print-env --chain localhost) && \
  export RPC_URL="http://localhost:8545" && \
  pnpm tsx ./server/index.ts)
