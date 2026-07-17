#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# The template integration deploys mock proof verifiers. Keep recursive proof
# aggregation enabled by default for users, but skip it in this bounded CI test.
export E3_NODES__CN1__SKIP_PROOF_AGGREGATION=true
export E3_NODES__CN2__SKIP_PROOF_AGGREGATION=true
export E3_NODES__CN3__SKIP_PROOF_AGGREGATION=true
export E3_NODES__CN4__SKIP_PROOF_AGGREGATION=true
export E3_NODES__CN5__SKIP_PROOF_AGGREGATION=true

passed_message() {
  echo ""
  echo "------------------------"
  echo "  ✅ Test has passed!   "
  echo "------------------------"
  echo ""
}

failed_message() {
  echo ""
  echo "------------------------"
  echo "  ❌ Test failed  "
  echo "------------------------"
  echo ""
  exit 1
}

(pnpm concurrently \
  --names "TEST,EVM,MINE,CIPHER,SERVER,PROGRAM" \
  --prefix-colors "blue,cyan,gray,magenta,yellow,green" \
  --kill-others \
  --success first \
  "wait-on file:/tmp/interfold_ciphernodes_ready tcp:localhost:8545 http://localhost:13151/health && export \$(interfold print-env --chain localhost) && pnpm vitest run ./tests/integration.spec.ts" \
  "anvil --host 0.0.0.0 --chain-id 31337 --block-time 1  --mnemonic 'test test test test test test test test test test test junk' --silent" \
  "wait-on tcp:localhost:8545 && node ./scripts/anvil-automine.mjs" \
  "pnpm dev:ciphernodes" \
  "TEST_MODE=1 pnpm dev:server" \
  "pnpm dev:program" && passed_message) || failed_message
