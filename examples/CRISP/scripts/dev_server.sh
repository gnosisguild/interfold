#!/usr/bin/env bash

set -euo pipefail

export CARGO_INCREMENTAL=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/dev_config.sh"

load_crisp_dev_config

(cd "${CRISP_ROOT}/server" && rm -rf database && cargo run --bin server)
