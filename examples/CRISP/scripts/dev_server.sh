#!/usr/bin/env bash

set -euo pipefail

export CARGO_INCREMENTAL=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
(cd "${SCRIPT_DIR}/../server" && rm -rf database && cargo run --bin server)
