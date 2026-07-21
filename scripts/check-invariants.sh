#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-only
#
# This file is provided WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY
# or FITNESS FOR A PARTICULAR PURPOSE.

# Mechanically enforces grep-checkable invariants from agent/INVARIANTS.md.
# Companion to check-committee.sh (structural sync) and check-doc-sync.sh (doc drift).
#
# Checks:
#   1. do_send ratchet — `.do_send(` is allowed only for best-effort telemetry
#      (agent/ARCHITECTURE.md); the call-site count may only go DOWN. Baseline lives in
#      scripts/invariant-baselines.env.
#   2. skip-proof feature containment — `test-only-skip-proof-aggregation` must never be
#      a default feature and must only be *enabled* (as a dependency feature) by
#      crates/tests. Forwarding declarations in [features] sections are fine.
#   3. runtime proof-skip guard — validate_proof_aggregation_mode must exist and be
#      called in crates/entrypoint (INVARIANTS §No proof-disabled bypass, C-02).
#
# Exit 0 when all hold, 1 otherwise.

set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
baseline_file="scripts/invariant-baselines.env"
# shellcheck source=/dev/null
source "$baseline_file"

# --- 1. do_send ratchet -----------------------------------------------------------
count=$(grep -rE '\.do_send\(' crates --include='*.rs' | wc -l | tr -d ' ')
if ((count > DO_SEND_BASELINE)); then
  echo "check-invariants: FAILED — .do_send( call sites grew: $count > baseline $DO_SEND_BASELINE"
  echo "  do_send is fire-and-forget and allowed only for best-effort telemetry"
  echo "  (agent/ARCHITECTURE.md §Routing). Use an acknowledged, timeout-bounded send"
  echo "  for anything correctness-critical. If the new call site really is telemetry,"
  echo "  raise DO_SEND_BASELINE in $baseline_file in the same PR and say why."
  fail=1
elif ((count < DO_SEND_BASELINE)); then
  echo "check-invariants: do_send count dropped to $count (baseline $DO_SEND_BASELINE)."
  echo "  Please ratchet: set DO_SEND_BASELINE=$count in $baseline_file."
fi

# --- 2. skip-proof feature containment --------------------------------------------
# A default feature list must never include the test-only flag.
if grep -rn --include='Cargo.toml' -E '^default *= *\[[^]]*test-only-skip-proof-aggregation' crates; then
  echo "check-invariants: FAILED — test-only-skip-proof-aggregation is a default feature (above)."
  fail=1
fi
# Only crates/tests may enable the feature on a dependency (features = [...] on a dep line).
enablers=$(grep -rln --include='Cargo.toml' -E '^[a-zA-Z0-9_-]+ *= *\{[^}]*features *= *\[[^]]*"[a-z0-9/-]*test-only-skip-proof-aggregation"' crates | grep -v '^crates/tests/' || true)
if [[ -n "$enablers" ]]; then
  echo "check-invariants: FAILED — test-only-skip-proof-aggregation enabled outside crates/tests:"
  sed 's/^/  - /' <<<"$enablers"
  echo "  Production binaries must reject skip_proof_aggregation (INVARIANTS §C-02)."
  fail=1
fi

# --- 3. runtime proof-skip guard --------------------------------------------------
if ! grep -rq 'fn validate_proof_aggregation_mode' crates/entrypoint/src ||
  ! grep -rEq 'validate_proof_aggregation_mode\(config' crates/entrypoint/src; then
  echo "check-invariants: FAILED — validate_proof_aggregation_mode missing or no longer called"
  echo "  in crates/entrypoint/src (INVARIANTS §No proof-disabled bypass, C-02)."
  fail=1
fi

if ((fail == 0)); then
  echo "✓ check:invariants: do_send=$count (≤ $DO_SEND_BASELINE), skip-proof feature contained, runtime guard present"
fi
exit "$fail"
