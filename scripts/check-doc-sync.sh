#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-only
#
# This file is provided WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY
# or FITNESS FOR A PARTICULAR PURPOSE.

# Guards the agent/ harness docs against drift: if a branch changes protocol-bearing
# code (contracts, circuits, or core crates) without touching agent/, the push is
# rejected. agent/RULES.md requires flow-trace and invariant docs to be updated in the
# same PR as the change they describe.
#
# Escape hatches:
#   - include "[skip-doc-sync]" in any commit message in the range, or
#   - set SKIP_DOC_SYNC=1
# for changes that genuinely do not alter documented behavior (pure refactors,
# test-only changes, dependency bumps).
#
# Run from .husky/pre-push. Exit 0 when consistent or not applicable, 1 on drift.

set -euo pipefail

if [[ "${SKIP_DOC_SYNC:-0}" == "1" ]]; then
  echo "check-doc-sync: skipped via SKIP_DOC_SYNC=1"
  exit 0
fi

# Paths whose changes are expected to be reflected in agent/ docs. Mirrors the
# "When to update" table in agent/RULES.md and the flow-trace area mapping.
WATCHED_REGEX='^(packages/interfold-contracts/contracts/|circuits/(lib|bin)/|crates/(events|keyshare|aggregator|slashing|sortition|evm|evm-helpers|trbfv|zk-helpers|zk-prover|request|sync|data|cli|entrypoint)/src/)'
DOCS_REGEX='^agent/'

base="$(git merge-base origin/main HEAD 2>/dev/null || true)"
if [[ -z "$base" ]]; then
  # No origin/main (fresh clone/fork mirror) — nothing to compare against.
  exit 0
fi

head="$(git rev-parse HEAD)"
if [[ "$base" == "$head" ]]; then
  # Nothing ahead of origin/main.
  exit 0
fi

changed="$(git diff --name-only "$base" "$head")"

watched_hits="$(grep -E "$WATCHED_REGEX" <<<"$changed" || true)"
doc_hits="$(grep -E "$DOCS_REGEX" <<<"$changed" || true)"

if [[ -z "$watched_hits" || -n "$doc_hits" ]]; then
  exit 0
fi

if git log --format=%B "$base..$head" | grep -qF '[skip-doc-sync]'; then
  echo "check-doc-sync: skipped via [skip-doc-sync] commit tag"
  exit 0
fi

echo "check-doc-sync: FAILED"
echo
echo "This branch changes protocol-bearing code but no file under agent/ was updated:"
echo
sed 's/^/  - /' <<<"$watched_hits"
echo
echo "agent/RULES.md requires harness docs (flow-trace, INVARIANTS.md, architecture docs)"
echo "to be updated in the same PR as the change they describe. Either:"
echo
echo "  1. update the relevant agent/ doc (start from agent/flow-trace/00_INDEX.md), or"
echo "  2. if no documented behavior changed, add \"[skip-doc-sync]\" to a commit message"
echo "     or re-run with SKIP_DOC_SYNC=1."
exit 1
