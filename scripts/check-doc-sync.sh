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
#   - include "[skip-doc-sync]" in the commit that contains a non-behavioral change, or
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
WATCHED_REGEX='^(packages/interfold-contracts/(contracts|scripts|tasks)/|circuits/(lib|bin)/|crates/(aggregator|bfv-client|ciphernode-builder|cli|committee-hash|compute-provider|config|crypto|daemon-server|data|entrypoint|events|evm|evm-helpers|fhe|fhe-params|fs|indexer|keyshare|multithread|net|parity-matrix|polynomial|program-server|request|safe|slashing|sortition|sync|trbfv|zk-helpers|zk-prover)/src/)'
DOCS_REGEX='^agent/(RULES|CONTEXT|INVARIANTS|ARCHITECTURE|CRATES_ARCHITECTURE)\.md$|^agent/flow-trace/'

base_ref="${DOC_SYNC_BASE_REF:-origin/main}"
base="$(git merge-base "$base_ref" HEAD 2>/dev/null || true)"
if [[ -z "$base" ]]; then
  if [[ "${CI:-false}" == "true" ]]; then
    echo "check-doc-sync: FAILED — cannot resolve merge-base for $base_ref" >&2
    exit 1
  fi
  echo "check-doc-sync: skipped because $base_ref is unavailable"
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

if [[ -z "$watched_hits" ]]; then
  exit 0
fi

# A skip tag applies only to the commit that contains it. An old skip tag must not
# exempt later protocol-bearing commits.
unskipped_watched=""
while IFS= read -r commit; do
  if git log -1 --format=%B "$commit" | grep -qF '[skip-doc-sync]'; then
    continue
  fi
  parent_count="$(git rev-list --parents -n 1 "$commit" | awk '{ print NF - 1 }')"
  if ((parent_count > 1)); then
    # A normal diff-tree invocation emits no paths for merge commits. Combined diff
    # catches conflict-resolution changes that exist in neither parent.
    commit_hits="$(git diff-tree --cc --no-commit-id --name-only -r "$commit" | grep -E "$WATCHED_REGEX" || true)"
  else
    commit_hits="$(git diff-tree --no-commit-id --name-only -r "$commit" | grep -E "$WATCHED_REGEX" || true)"
  fi
  if [[ -n "$commit_hits" ]]; then
    unskipped_watched+="${unskipped_watched:+$'\n'}$commit_hits"
  fi
done < <(git rev-list --reverse "$base..$head")

if [[ -z "$unskipped_watched" ]]; then
  echo "check-doc-sync: skipped watched changes via commit-local [skip-doc-sync] tags"
  exit 0
fi

if [[ -n "$doc_hits" ]]; then
  exit 0
fi

echo "check-doc-sync: FAILED"
echo
echo "This branch changes protocol-bearing code but no file under agent/ was updated:"
echo
while IFS= read -r path; do
  echo "  - $path"
done <<<"$unskipped_watched"
echo
echo "agent/RULES.md requires harness docs (flow-trace, INVARIANTS.md, architecture docs)"
echo "to be updated in the same PR as the change they describe. Either:"
echo
echo "  1. update the relevant agent/ doc (start from agent/flow-trace/00_INDEX.md), or"
echo "  2. if no documented behavior changed, add \"[skip-doc-sync]\" to a commit message"
echo "     or re-run with SKIP_DOC_SYNC=1."
exit 1
