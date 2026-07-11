#!/bin/bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

assert_contains() {
    grep -Fq "$2" "$1" || fail "expected '$2' in $1"
}

assert_not_contains() {
    if grep -Fq "$2" "$1"; then
        fail "did not expect '$2' in $1"
    fi
}

make_mock_interfold() {
    local bin_dir=$1
    mkdir -p "$bin_dir"
    printf '%s\n' \
        '#!/bin/bash' \
        'set -Eeuo pipefail' \
        'case "${1:-} ${2:-} ${3:-}" in' \
        '  "password set "*) operation=password ;;' \
        '  "net keypair set"*) operation=network ;;' \
        '  "wallet set "*) operation=wallet ;;' \
        '  "start "*) operation=start ;;' \
        '  *) operation=unexpected ;;' \
        'esac' \
        'printf "%s\n" "$operation" >> "$CALL_LOG"' \
        '[ "${FAIL_ON:-}" != "$operation" ] || exit 42' \
        'if [ "$operation" = password ]; then' \
        '  password=' \
        '  while [ "$#" -gt 0 ]; do' \
        '    if [ "$1" = --password ]; then password=$2; break; fi' \
        '    shift' \
        '  done' \
        '  mkdir -p "$(dirname "$PASSWORD_FILE")"' \
        '  printf "%s" "$password" > "$PASSWORD_FILE"' \
        '  chmod 400 "$PASSWORD_FILE"' \
        'fi' \
        '[ "$operation" != unexpected ]' \
        > "$bin_dir/interfold"
    chmod +x "$bin_dir/interfold"
}

write_secrets() {
    local path=$1
    printf '%s\n' '{' \
        '  "password": "correct horse battery staple",' \
        '  "private_key": "0x1111111111111111111111111111111111111111111111111111111111111111",' \
        '  "network_private_key": "0x2222222222222222222222222222222222222222222222222222222222222222"' \
        '}' > "$path"
}

run_entrypoint() {
    local case_dir=$1
    shift
    mkdir -p "$case_dir/data" "$case_dir/secrets" "$case_dir/bin"
    : > "$case_dir/calls"
    make_mock_interfold "$case_dir/bin"

    env -u ENCRYPTION_PASSWORD -u NETWORK_PRIVATE_KEY -u PRIVATE_KEY \
        PATH="$case_dir/bin:$PATH" \
        CONFIG_DIR="$case_dir/data" \
        CONFIG_FILE="$case_dir/data/config.yaml" \
        TEMPLATE_FILE="$ROOT_DIR/config.template.yaml" \
        SECRETS_FILE="$case_dir/secrets/secrets.json" \
        PASSWORD_FILE="$case_dir/data/password" \
        CALL_LOG="$case_dir/calls" \
        RPC_URL="ws://127.0.0.1:8545" \
        NODE_ADDRESS="0x3333333333333333333333333333333333333333" \
        INTERFOLD_CONTRACT="0x4444444444444444444444444444444444444444" \
        CIPHERNODE_REGISTRY_CONTRACT="0x5555555555555555555555555555555555555555" \
        BONDING_REGISTRY_CONTRACT="0x6666666666666666666666666666666666666666" \
        INTERFOLD_DEPLOY_BLOCK=1 \
        CIPHERNODE_REGISTRY_DEPLOY_BLOCK=2 \
        BONDING_REGISTRY_DEPLOY_BLOCK=3 \
        PRIVATE_KEY="${TEST_PRIVATE_KEY:-}" \
        "$@" bash "$ROOT_DIR/entrypoint.sh" > "$case_dir/output" 2>&1
}

# Successful provisioning uses the exact v0.1.8 commands, removes the
# plaintext upload, and starts only after every credential command succeeds.
success_dir="$TEST_ROOT/success"
mkdir -p "$success_dir/secrets"
write_secrets "$success_dir/secrets/secrets.json"
run_entrypoint "$success_dir"
[ ! -e "$success_dir/secrets/secrets.json" ] || fail "successful setup retained plaintext credentials"
[ "$(tr '\n' ' ' < "$success_dir/calls")" = "password network wallet start " ] || fail "unexpected successful command order"
assert_contains "$success_dir/data/config.yaml" 'autopassword: false'
assert_contains "$success_dir/data/config.yaml" 'autonetkey: false'
assert_contains "$success_dir/data/config.yaml" 'autowallet: false'

# A credential command failure must propagate and must never start the node.
failure_dir="$TEST_ROOT/failure"
mkdir -p "$failure_dir/secrets"
write_secrets "$failure_dir/secrets/secrets.json"
if run_entrypoint "$failure_dir" FAIL_ON=network; then
    fail "network credential failure was ignored"
fi
assert_contains "$failure_dir/calls" 'network'
assert_not_contains "$failure_dir/calls" 'start'
[ -e "$failure_dir/secrets/secrets.json" ] || fail "failed setup removed recovery input"

# Existing state may only be reused with the password that encrypted it.
mismatch_dir="$TEST_ROOT/password-mismatch"
mkdir -p "$mismatch_dir/data" "$mismatch_dir/secrets"
printf '%s' 'different-password' > "$mismatch_dir/data/password"
write_secrets "$mismatch_dir/secrets/secrets.json"
if run_entrypoint "$mismatch_dir"; then
    fail "mismatched persisted password was accepted"
fi
[ ! -s "$mismatch_dir/calls" ] || fail "password mismatch mutated credentials"

# Malformed or absent first-start credentials fail before invoking Interfold.
malformed_dir="$TEST_ROOT/malformed"
mkdir -p "$malformed_dir/secrets"
printf '%s\n' '{"password":"only-one-field"}' > "$malformed_dir/secrets/secrets.json"
if run_entrypoint "$malformed_dir"; then
    fail "malformed credentials were accepted"
fi
[ ! -s "$malformed_dir/calls" ] || fail "malformed credentials invoked Interfold"

missing_dir="$TEST_ROOT/missing"
if run_entrypoint "$missing_dir"; then
    fail "first startup without credentials was accepted"
fi
[ ! -s "$missing_dir/calls" ] || fail "missing credentials invoked Interfold"

# A normal restart may reuse credentials already encrypted in the persistent
# /data volume without requiring the one-time plaintext upload again.
restart_dir="$TEST_ROOT/restart"
mkdir -p "$restart_dir/data"
printf '%s' 'persisted-password' > "$restart_dir/data/password"
chmod 400 "$restart_dir/data/password"
run_entrypoint "$restart_dir"
[ "$(tr '\n' ' ' < "$restart_dir/calls")" = "start " ] || fail "persisted restart unexpectedly re-provisioned credentials"

# Legacy secret environment variables are explicitly rejected.
legacy_dir="$TEST_ROOT/legacy-env"
mkdir -p "$legacy_dir/data"
printf '%s' 'persisted-password' > "$legacy_dir/data/password"
if TEST_PRIVATE_KEY=legacy run_entrypoint "$legacy_dir"; then
    fail "legacy secret environment variable was accepted"
fi

# Health probe regression: require the exact process/config, protected files,
# and bound QUIC listener rather than accepting an arbitrary matching PID.
health_dir="$TEST_ROOT/health"
mkdir -p "$health_dir/proc/1" "$health_dir/bin" "$health_dir/data" "$health_dir/data/db" "$health_dir/data/log.0"
ln -s "$health_dir/bin/interfold" "$health_dir/proc/1/exe"
: > "$health_dir/bin/interfold"
printf '/usr/local/bin/interfold\0start\0-v\0--config\0%s\0' "$health_dir/data/config.yaml" > "$health_dir/proc/1/cmdline"
printf 'config' > "$health_dir/data/config.yaml"
printf 'password' > "$health_dir/data/password"

printf '%s\n' \
    '#!/bin/sh' \
    '[ "${SS_READY:-1}" = 1 ] && printf "udp listener\n"' \
    > "$health_dir/bin/ss"
printf '%s\n' \
    '#!/bin/sh' \
    'printf "%s\n" "${STAT_MODE:-600}"' \
    > "$health_dir/bin/stat"
chmod +x "$health_dir/bin/ss" "$health_dir/bin/stat"

PROC_ROOT="$health_dir/proc" \
CONFIG_FILE="$health_dir/data/config.yaml" \
PASSWORD_FILE="$health_dir/data/password" \
DB_PATH="$health_dir/data/db" \
EVENT_LOG_PATH="$health_dir/data/log.0" \
SS_BIN="$health_dir/bin/ss" \
STAT_BIN="$health_dir/bin/stat" \
sh "$ROOT_DIR/healthcheck.sh" || fail "healthy local state was rejected"

if PROC_ROOT="$health_dir/proc" \
    CONFIG_FILE="$health_dir/data/config.yaml" \
    PASSWORD_FILE="$health_dir/data/password" \
    DB_PATH="$health_dir/data/db" \
    EVENT_LOG_PATH="$health_dir/data/log.0" \
    SS_BIN="$health_dir/bin/ss" \
    STAT_BIN="$health_dir/bin/stat" \
    SS_READY=0 sh "$ROOT_DIR/healthcheck.sh"; then
    fail "missing QUIC listener was considered healthy"
fi

if PROC_ROOT="$health_dir/proc" \
    CONFIG_FILE="$health_dir/data/config.yaml" \
    PASSWORD_FILE="$health_dir/data/password" \
    DB_PATH="$health_dir/data/db" \
    EVENT_LOG_PATH="$health_dir/data/log.0" \
    SS_BIN="$health_dir/bin/ss" \
    STAT_BIN="$health_dir/bin/stat" \
    STAT_MODE=644 sh "$ROOT_DIR/healthcheck.sh"; then
    fail "insecure credential permissions were considered healthy"
fi

ln -sfn "$health_dir/bin/not-interfold" "$health_dir/proc/1/exe"
if PROC_ROOT="$health_dir/proc" \
    CONFIG_FILE="$health_dir/data/config.yaml" \
    PASSWORD_FILE="$health_dir/data/password" \
    DB_PATH="$health_dir/data/db" \
    EVENT_LOG_PATH="$health_dir/data/log.0" \
    SS_BIN="$health_dir/bin/ss" \
    STAT_BIN="$health_dir/bin/stat" \
    sh "$ROOT_DIR/healthcheck.sh"; then
    fail "unrelated PID 1 was considered healthy"
fi

ln -sfn "$health_dir/bin/interfold" "$health_dir/proc/1/exe"
rmdir "$health_dir/data/log.0"
if PROC_ROOT="$health_dir/proc" \
    CONFIG_FILE="$health_dir/data/config.yaml" \
    PASSWORD_FILE="$health_dir/data/password" \
    DB_PATH="$health_dir/data/db" \
    EVENT_LOG_PATH="$health_dir/data/log.0" \
    SS_BIN="$health_dir/bin/ss" \
    STAT_BIN="$health_dir/bin/stat" \
    sh "$ROOT_DIR/healthcheck.sh"; then
    fail "uninitialized event persistence was considered healthy"
fi

printf 'PASS: DAppNode credential and health hardening regressions\n'
