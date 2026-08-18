#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-only
#
# This file is provided WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY
# or FITNESS FOR A PARTICULAR PURPOSE.

# Guards the RISC Zero guest artifact against silent drift.
#
# The on-chain `Risc0BfvCiphertextVerifier.imageId` is immutable, and it names exactly one guest
# image. Nothing else ties that value to the source in this repository, so a guest change that
# leaves `crates/support/contracts/ImageID.sol` untouched produces a deployed verifier that no
# longer matches the tree.
#
# Three checks run here. Two are cheap and always run. The third needs Docker.
#
#   1. Pin consistency  — every Interfold git pin the guest workspace reads names one revision.
#   2. Toolchain sync   — the Dockerfile guest toolchain matches `rust-toolchain.toml`.
#   3. Source drift     — the recorded digest of the guest-affecting inputs matches those inputs.
#
# Check 3 compares against `crates/support/contracts/ImageID.stamp.json`. That stamp is a drift
# ratchet, not a proof. It shows that the inputs did not change since the image ID was last
# recorded. It cannot show that the recorded image ID is the one those inputs produce. Only a
# rebuild does that:
#
#   ./scripts/check-image-id.sh --rebuild
#
# Exit 0 when the checks pass, 1 on drift.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SUPPORT="crates/support"
IMAGE_ID_SOL="$SUPPORT/contracts/ImageID.sol"
STAMP="$SUPPORT/contracts/ImageID.stamp.json"
DOCKERFILE="$SUPPORT/Dockerfile"
TOOLCHAIN_TOML="rust-toolchain.toml"
REBUILD=false

for arg in "$@"; do
  case "$arg" in
    --rebuild) REBUILD=true ;;
    *) echo "check:image-id: unknown argument '$arg'" >&2; exit 1 ;;
  esac
done

fail() {
  echo "❌ check:image-id: $*" >&2
  exit 1
}

warn() {
  echo "⚠️  check:image-id: $*" >&2
}

# --- 1. Pin consistency -------------------------------------------------------------------------
# The guest workspace and the host workspace must read the same Interfold revision. A split pin
# means the host proves against a guest built from different sources.

pins="$(grep -rhoE 'rev = "[0-9a-f]{40}"' \
  "$SUPPORT/Cargo.toml" "$SUPPORT/methods/guest/Cargo.toml" \
  | sed -E 's/rev = "(.*)"/\1/' | sort -u)"

pin_count="$(printf '%s\n' "$pins" | grep -c . || true)"
if [ "$pin_count" -ne 1 ]; then
  fail "the guest and host workspaces pin different Interfold revisions:
$(printf '%s\n' "$pins" | sed 's/^/    /')
  Every Interfold dependency in crates/support must name one revision."
fi
PINNED_REV="$pins"

# The paths the pin decides the contents of. The guest compiles these through the git pin, so a
# change to them here does not reach the guest until the pin moves past it.
GUEST_SOURCE_PATHS="crates/compute-provider crates/support/types"

# The pin must be a commit this repository knows, so a reviewer can diff it against the tree.
if git cat-file -e "${PINNED_REV}^{commit}" 2>/dev/null; then
  if ! git merge-base --is-ancestor "$PINNED_REV" HEAD 2>/dev/null; then
    warn "pinned revision ${PINNED_REV:0:8} is not an ancestor of HEAD.
  The guest is built from sources that are not in this branch's history."
  fi

  # Ancestry says the pin is a commit we know. It does not say the pin is current, and those are
  # different questions: a pin can be a perfectly good ancestor and still predate every fix to the
  # code it compiles. Ask the second question directly.
  # shellcheck disable=SC2086
  last_source_change="$(git log -1 --format=%H -- $GUEST_SOURCE_PATHS 2>/dev/null || true)"
  if [ -n "$last_source_change" ] &&
    ! git merge-base --is-ancestor "$last_source_change" "$PINNED_REV" 2>/dev/null; then
    warn "the guest pin predates the last change to the code it compiles.
    pinned:      ${PINNED_REV:0:8}
    last change: ${last_source_change:0:8} ($GUEST_SOURCE_PATHS)
  The guest still runs the older code. Move the pin to a pushed commit that contains the change,
  update both lockfiles, rebuild the guest, and commit the regenerated $IMAGE_ID_SOL."
  fi
else
  warn "pinned revision ${PINNED_REV:0:8} is not present locally; run 'git fetch' to check it."
fi

# A bare hash says nothing about why it was chosen. Tie it to the baseline recorded in the audit
# index, so moving the pin is a deliberate act that updates the documented reason with it.
AUDIT_BASELINE="c2097da61b4d07c4ce83840393ff4e9f171eefb4"
AUDIT_README="packages/interfold-contracts/audits/README.md"

if ! grep -q "$AUDIT_BASELINE" "$AUDIT_README" 2>/dev/null; then
  fail "the audit baseline $AUDIT_BASELINE recorded in this script is not documented in
  $AUDIT_README. Keep the two in step."
fi

if [ "$PINNED_REV" != "$AUDIT_BASELINE" ]; then
  warn "the guest pin has moved off the documented audit baseline.
    pinned:   $PINNED_REV
    baseline: $AUDIT_BASELINE
  That is expected once the pin is bumped past the audit. Update AUDIT_BASELINE in this script and
  the rationale in crates/support/Cargo.toml, so the reason for the pin stays true."
fi

# --- 2. Toolchain sync --------------------------------------------------------------------------
# The guest lockfile pins fhe.rs, whose workspace declares an MSRV. A guest toolchain below that
# MSRV cannot build the guest at all, and the failure surfaces only at container start.

docker_toolchain="$(grep -oE '^ARG RISC0_TOOLCHAIN=.*' "$DOCKERFILE" | cut -d= -f2 || true)"
repo_toolchain="$(grep -oE '^channel = "[^"]+"' "$TOOLCHAIN_TOML" | cut -d'"' -f2 || true)"

[ -n "$docker_toolchain" ] || fail "no ARG RISC0_TOOLCHAIN in $DOCKERFILE"
[ -n "$repo_toolchain" ] || fail "no channel in $TOOLCHAIN_TOML"

if [ "$docker_toolchain" != "$repo_toolchain" ]; then
  fail "guest toolchain drift.
    $DOCKERFILE  ARG RISC0_TOOLCHAIN=$docker_toolchain
    $TOOLCHAIN_TOML     channel = \"$repo_toolchain\"
  The guest builds the same dependency tree as the host, so these must match."
fi

# --- 3. Source drift ----------------------------------------------------------------------------
# Digest every input that changes the guest image: the guest crate, the journal types, the user
# program, the build script, the resolved dependency graph, and the pinned revision.

#
# Manifest files are normalized first: a TOML comment cannot reach the compiled guest, and a gate
# that fails on one teaches people to re-stamp without thinking. Rust sources are digested byte for
# byte — over-strictness there costs a rebuild, while under-strictness ships a wrong image ID.
digest_manifest() {
  # Drop comment-only lines and blank lines, so documentation edits do not read as source drift.
  sed -E 's/^[[:space:]]*#.*$//' "$1" | grep -v '^[[:space:]]*$'
}

guest_inputs_digest() {
  {
    find "$SUPPORT/methods/guest" "$SUPPORT/types" "$SUPPORT/program" \
      -type f -name '*.rs' | LC_ALL=C sort | xargs shasum -a 256
    shasum -a 256 "$SUPPORT/methods/build.rs"

    for manifest in $(find "$SUPPORT/methods/guest" "$SUPPORT/types" "$SUPPORT/program" \
      -type f \( -name 'Cargo.toml' -o -name 'Cargo.lock' \) | LC_ALL=C sort); do
      echo "manifest $manifest"
      digest_manifest "$manifest"
    done
    echo "manifest $SUPPORT/Cargo.toml"
    digest_manifest "$SUPPORT/Cargo.toml"

    echo "pinned-rev $PINNED_REV"
    echo "risc0-toolchain $docker_toolchain"
    grep -oE '^ARG RISC0_VERSION=.*' "$DOCKERFILE"
  } | shasum -a 256 | cut -d' ' -f1
}

current_digest="$(guest_inputs_digest)"
current_image_id="$(grep -oE '0x[0-9a-fA-F]{64}' "$IMAGE_ID_SOL" | head -1)"

[ -n "$current_image_id" ] || fail "no PROGRAM_ID found in $IMAGE_ID_SOL"

if [ "$REBUILD" = true ]; then
  command -v docker >/dev/null 2>&1 || fail "--rebuild needs Docker, which is not on PATH"
  echo "check:image-id: rebuilding the guest with the RISC Zero Docker builder..."
  ( cd "$SUPPORT" && RISC0_USE_DOCKER=1 cargo build --release -p methods )
  rebuilt_image_id="$(grep -oE '0x[0-9a-fA-F]{64}' "$IMAGE_ID_SOL" | head -1)"
  if [ "$rebuilt_image_id" != "$current_image_id" ]; then
    fail "the rebuilt guest does not match the committed image ID.
    committed: $current_image_id
    rebuilt:   $rebuilt_image_id
  Commit the regenerated $IMAGE_ID_SOL and refresh $STAMP."
  fi
  echo "✅ check:image-id: rebuild reproduces the committed image ID."

  # Record it. Leaving the stamp at imageIdVerified:false after a successful rebuild would keep
  # every downstream provenance manifest incomplete and keep this script warning about an image ID
  # that has in fact been reproduced.
  rebuilt_digest="$(guest_inputs_digest)"

  # Written to a temporary file in the same directory and renamed, never truncated in place. A
  # partial stamp is worse than no stamp: `generate-provenance-manifest.ts` reads the absence of a
  # literal `"imageIdVerified": false` as verified, so a write interrupted mid-JSON would let an
  # unverified guest produce a complete provenance manifest. `mv` within one directory is atomic.
  stamp_tmp="$(mktemp "$(dirname "$STAMP")/.ImageID.stamp.json.XXXXXX")"
  trap 'rm -f "$stamp_tmp"' EXIT
  cat > "$stamp_tmp" <<STAMP_JSON
{
  "imageId": "$current_image_id",
  "guestInputsDigest": "$rebuilt_digest",
  "imageIdVerified": true
}
STAMP_JSON
  mv "$stamp_tmp" "$STAMP"
  trap - EXIT
  echo "✅ check:image-id: recorded the verified image ID in $STAMP."
fi

if [ ! -f "$STAMP" ]; then
  fail "missing $STAMP.
  Create it with the current inputs:
    { \"imageId\": \"$current_image_id\", \"guestInputsDigest\": \"$current_digest\" }"
fi

stamp_digest="$(grep -oE '"guestInputsDigest"[[:space:]]*:[[:space:]]*"[0-9a-f]+"' "$STAMP" \
  | grep -oE '[0-9a-f]{64}' || true)"
stamp_image_id="$(grep -oE '"imageId"[[:space:]]*:[[:space:]]*"0x[0-9a-fA-F]{64}"' "$STAMP" \
  | grep -oE '0x[0-9a-fA-F]{64}' || true)"

if [ "$stamp_image_id" != "$current_image_id" ]; then
  fail "$STAMP records image ID $stamp_image_id, but $IMAGE_ID_SOL holds $current_image_id."
fi

if [ "$stamp_digest" != "$current_digest" ]; then
  fail "the guest sources changed, but $IMAGE_ID_SOL did not.
    recorded inputs: $stamp_digest
    current inputs:  $current_digest
  Rebuild the guest and commit the regenerated image ID:
    ./scripts/check-image-id.sh --rebuild
  Then record the new inputs in $STAMP."
fi

# The stamp says the inputs are unchanged. Say plainly when the recorded image ID was never
# reproduced from those inputs, so a green check is not read as a verified artifact.
if grep -qE '"imageIdVerified"[[:space:]]*:[[:space:]]*false' "$STAMP"; then
  warn "the guest inputs are unchanged, but the recorded image ID has never been reproduced
  from them. See the 'reason' field in $STAMP. Run with --rebuild to verify it."
fi

echo "✅ check:image-id: pins, toolchain, and guest inputs are consistent."
