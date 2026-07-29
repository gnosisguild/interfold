#!/usr/bin/env bash

set -e


cd "$(git rev-parse --show-toplevel)"

# Fixture regeneration requires Nargo.
if ! command -v nargo &> /dev/null; then
    exit 0
fi

# In a clean checkout, build the circuit artifacts used by integration tests.
if ! find ./circuits/bin -name '*.json' -print -quit | grep -q .; then
    if ! command -v bb &> /dev/null; then
        exit 0
    fi
    echo "Building circuits..."
    pnpm install && pnpm build:circuits
fi

# Keep the integration-test fixture in sync with the current Noir serialization format.
dummy_package="./crates/zk-prover/tests/fixtures/dummy"
dummy_artifact="$dummy_package/target/dummy.json"
fixture="./crates/zk-prover/tests/fixtures/dummy.json"
normalize_compiled_circuit_paths() {
  # Noir emits machine-local absolute paths in file_map; keep fixtures stable.
  jq '
    if .file_map then
      .file_map |= with_entries(
        .value |= if (.path | type) == "string" then
          .path |= (
            if test("^/") and test("circuits/") then
              sub("^.*?circuits/"; "circuits/")
            elif test("^/") and test("crates/zk-prover/tests/fixtures/dummy/") then
              sub("^.*?crates/zk-prover/tests/fixtures/dummy/"; "crates/zk-prover/tests/fixtures/dummy/")
            else .
            end
          )
        else .
        end
      )
    else .
    end
  '
}

(cd "$dummy_package" && nargo compile)
normalize_compiled_circuit_paths <"$dummy_artifact" >"$fixture"
