#!/usr/bin/env bash

set -euo pipefail

export CARGO_INCREMENTAL=1

cleanup() {
  echo "Cleaning up processes..."
  pkill -9 -f "interfold start"
  sleep 1

  pkill -9 -f "anvil" 2>/dev/null || true
  
  # Kill any remaining background jobs from this script
  jobs -p | xargs -r kill -9 2>/dev/null || true
  
  # Give processes a moment to terminate
  sleep 1
  
  # Double-check if anvil is still running and force kill it
  if pgrep -f "anvil" > /dev/null; then
    echo "Anvil still running, force killing..."
    pkill -9 -f "anvil" || true
  fi
  
  echo "Cleanup complete"
  exit 0
}

trap cleanup INT TERM

echo "DEV SCRIPT STARTING..."

pnpm concurrently \
  -ks first \
  --names "ANVIL,RANDOMNESS,DEPLOY" \
  --prefix-colors "blue,gray,green" \
  "anvil --host 0.0.0.0 --chain-id 31337 --block-time 1 --mnemonic 'test test test test test test test test test test test junk' --silent" \
  "wait-on tcp:127.0.0.1:8545 && node ./scripts/anvil-randomness.mjs" \
  "./scripts/crisp_deploy.sh && ./scripts/dev_services.sh"
