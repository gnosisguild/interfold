#!/usr/bin/env bash

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
START_SCRIPT="$REPOSITORY_ROOT/crates/support-scripts/ctl/start"
TEST_REVISION="abc123456"
TEST_PARENT="${TMPDIR:-/tmp}"
TEST_PARENT="${TEST_PARENT%/}"
TEST_DIRECTORIES=()

cleanup() {
  local directory
  for directory in "${TEST_DIRECTORIES[@]}"; do
    case "$directory" in
      "$TEST_PARENT"/e3-support-test.*) rm -rf -- "$directory" ;;
      *)
        echo "Refusing to remove unexpected test directory: $directory" >&2
        return 1
        ;;
    esac
  done
}
trap cleanup EXIT

interfold() {
  if [[ "${1:-}" != "rev" ]]; then
    return 1
  fi
  printf '%s\n' "$TEST_REVISION"
}

docker() {
  if [[ "${1:-}" == "image" && "${2:-}" == "inspect" ]]; then
    return "${LOCAL_IMAGE_STATUS:-0}"
  fi
  if [[ "${1:-}" == "pull" ]]; then
    printf 'DOCKER_PULL %s\n' "$2"
    return "${PULL_STATUS:-0}"
  fi
  if [[ "${1:-}" == "ps" ]]; then
    return 0
  fi
  if [[ "${1:-}" == "run" ]]; then
    printf 'DOCKER_RUN %s\n' "$*"
    return 0
  fi
  if [[ "${1:-}" == "exec" || "${1:-}" == "stop" ]]; then
    return 0
  fi
  echo "Unexpected docker command: $*" >&2
  return 1
}

sleep() {
  return 0
}

export TEST_REVISION
export -f interfold docker sleep

CASE_OUTPUT=""
CASE_STATUS=0

run_case() {
  local local_image_status="$1"
  local pull_status="$2"
  local image_repository="${3:-}"
  local start_args=("${@:4}")
  local directory

  directory=$(mktemp -d "$TEST_PARENT/e3-support-test.XXXXXX")
  TEST_DIRECTORIES+=("$directory")

  set +e
  CASE_OUTPUT=$(
    cd "$directory" || exit 1
    export LOCAL_IMAGE_STATUS="$local_image_status"
    export PULL_STATUS="$pull_status"
    if [[ -n "$image_repository" ]]; then
      export E3_SUPPORT_IMAGE_REPOSITORY="$image_repository"
    else
      unset E3_SUPPORT_IMAGE_REPOSITORY
    fi
    bash "$START_SCRIPT" --risc0-dev-mode false "${start_args[@]}" 2>&1
  )
  CASE_STATUS=$?
  set -e
}

assert_contains() {
  local expected="$1"
  if [[ "$CASE_OUTPUT" != *"$expected"* ]]; then
    echo "Expected output to contain: $expected" >&2
    echo "$CASE_OUTPUT" >&2
    exit 1
  fi
}

assert_not_contains() {
  local unexpected="$1"
  if [[ "$CASE_OUTPUT" == *"$unexpected"* ]]; then
    echo "Expected output not to contain: $unexpected" >&2
    echo "$CASE_OUTPUT" >&2
    exit 1
  fi
}

run_case 0 0
[[ "$CASE_STATUS" -eq 0 ]]
assert_not_contains "DOCKER_PULL"
assert_contains "DOCKER_RUN"
assert_contains "ghcr.io/theinterfold/e3-support:$TEST_REVISION"

run_case 1 0
[[ "$CASE_STATUS" -eq 0 ]]
assert_contains "DOCKER_PULL ghcr.io/theinterfold/e3-support:$TEST_REVISION"
assert_contains "DOCKER_RUN"

run_case 1 1
[[ "$CASE_STATUS" -ne 0 ]]
assert_contains "Support image ghcr.io/theinterfold/e3-support:$TEST_REVISION is unavailable"
assert_not_contains "DOCKER_RUN"

run_case 0 0 "registry.example/e3-support"
[[ "$CASE_STATUS" -eq 0 ]]
assert_contains "registry.example/e3-support:$TEST_REVISION"

run_case 0 0 "" \
  --boundless-min-price-eth 0.0001 \
  --boundless-max-price-eth 0.004 \
  --boundless-timeout-secs 2700 \
  --boundless-lock-timeout-secs 1200 \
  --boundless-ramp-up-secs 300 \
  --boundless-lock-collateral-zkc 3.5
[[ "$CASE_STATUS" -eq 0 ]]
assert_contains "--boundless-min-price-eth 0.0001"
assert_contains "--boundless-max-price-eth 0.004"
assert_contains "--boundless-timeout-secs 2700"
assert_contains "--boundless-lock-timeout-secs 1200"
assert_contains "--boundless-ramp-up-secs 300"
assert_contains "--boundless-lock-collateral-zkc 3.5"

echo "Support image container tests passed."
