#!/bin/bash
# DAppNode Interfold Ciphernode Entrypoint
set -Eeuo pipefail

umask 077

CONFIG_DIR="${CONFIG_DIR:-/data}"
CONFIG_FILE="${CONFIG_FILE:-$CONFIG_DIR/config.yaml}"
TEMPLATE_FILE="${TEMPLATE_FILE:-/opt/config.template.yaml}"
SECRETS_FILE="${SECRETS_FILE:-/run/secrets/secrets.json}"
CREDENTIAL_PROVISIONER="${CREDENTIAL_PROVISIONER:-/opt/provision-credentials.exp}"
# Interfold v0.1.8 resolves a relative `key_file: key` beside a discovered
# /data/config.yaml to this path for the default node profile.
PASSWORD_FILE="${PASSWORD_FILE:-$CONFIG_DIR/.enclave/config/_default/key}"

log() { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$1"; }
fail() {
    log "ERROR: $1"
    exit 1
}

echo "=========================================="
echo "  Interfold Ciphernode - ${NETWORK:-sepolia}"
echo "=========================================="

# Environment variables are visible in Docker/DAppNode metadata. Refuse the
# legacy secret injection contract instead of silently preferring one source.
if [ -n "${ENCRYPTION_PASSWORD:-}" ] || [ -n "${NETWORK_PRIVATE_KEY:-}" ] || [ -n "${PRIVATE_KEY:-}" ]; then
    fail "credential environment variables are unsupported; upload the DAppNode credentials JSON file"
fi

# Validate RPC URL (required).
[ -n "${RPC_URL:-}" ] || fail "RPC_URL is required; set it in the DAppNode package configuration"
[[ "$RPC_URL" =~ ^wss?:// ]] || fail "RPC_URL must be a WebSocket URL (ws:// or wss://)"

[ -r "$TEMPLATE_FILE" ] || fail "configuration template is not readable: $TEMPLATE_FILE"
mkdir -p "$CONFIG_DIR"

# Set non-secret defaults.
export NETWORK="${NETWORK:-sepolia}"
export QUIC_PORT="${QUIC_PORT:-37173}"
export NODE_ADDRESS="${NODE_ADDRESS:-}"
export LOG_LEVEL="${LOG_LEVEL:-info}"

case "$LOG_LEVEL" in
    info|debug|trace) ;;
    *) fail "LOG_LEVEL must be one of: info, debug, trace" ;;
esac

# Generate config from the fixed template. The 0077 umask keeps RPC
# credentials in the rendered URL out of group/world-readable files.
log "Generating configuration..."
envsubst < "$TEMPLATE_FILE" > "$CONFIG_FILE"
chmod 600 "$CONFIG_FILE"

validate_secret_file() {
    [ -f "$SECRETS_FILE" ] || fail "credentials path is not a regular file: $SECRETS_FILE"
    [ ! -L "$SECRETS_FILE" ] || fail "credentials path must not be a symbolic link: $SECRETS_FILE"
    [ -r "$SECRETS_FILE" ] || fail "credentials file is not readable: $SECRETS_FILE"

    local size
    size=$(wc -c < "$SECRETS_FILE")
    [ "$size" -le 16384 ] || fail "credentials file exceeds the 16 KiB limit"

    jq -e '
        type == "object" and
        (keys | sort == ["network_private_key", "password", "private_key"]) and
        (.password | type == "string" and length > 0 and length <= 1024 and
            test("^[^\\r\\n\\u0000]+$") and . == gsub("^\\s+|\\s+$"; "")) and
        (.private_key | type == "string" and test("^0x[0-9a-fA-F]{64}$")) and
        (.network_private_key | type == "string" and test("^0x[0-9a-fA-F]{64}$"))
    ' "$SECRETS_FILE" >/dev/null || fail "credentials file must contain valid password, private_key, and network_private_key strings"
}

validate_persisted_password_file() {
    [ -f "$PASSWORD_FILE" ] || fail "persisted password path is not a regular file: $PASSWORD_FILE"
    [ ! -L "$PASSWORD_FILE" ] || fail "persisted password path must not be a symbolic link: $PASSWORD_FILE"
    chmod 400 "$PASSWORD_FILE" || fail "could not restrict persisted password permissions"
    [ -r "$PASSWORD_FILE" ] || fail "persisted password file is not readable: $PASSWORD_FILE"
}

configure_credentials() {
    validate_secret_file

    [ -r "$CREDENTIAL_PROVISIONER" ] || fail "credential provisioner is not readable: $CREDENTIAL_PROVISIONER"

    local provisioning_mode=new

    if [ -e "$PASSWORD_FILE" ]; then
        validate_persisted_password_file
        jq -er '.password' "$SECRETS_FILE" | tr -d '\n' | cmp -s - "$PASSWORD_FILE" \
            || fail "uploaded password does not match the persisted credential key"
        provisioning_mode=existing
        log "Using the matching persisted encryption password."
    fi

    log "Provisioning encrypted credentials through hidden stdin prompts..."
    jq -jr '[.password, .network_private_key, .private_key][] | @base64 + "\n"' "$SECRETS_FILE" \
        | expect "$CREDENTIAL_PROVISIONER" "$CONFIG_FILE" "$provisioning_mode" \
        || fail "one or more credential commands failed"

    # DAppNode copies fileUpload content into this container before startup.
    # Wallet/network keys are encrypted in /data and v0.1.8 stores the password
    # key there with mode 0400. Remove the combined plaintext upload.
    rm -f "$SECRETS_FILE"
    log "Credential setup completed."
}

if [ -e "$SECRETS_FILE" ]; then
    configure_credentials
elif [ -s "$PASSWORD_FILE" ]; then
    # Backward-compatible restart/upgrade path: DAppNode file uploads are copied
    # when configuring a container, while encrypted credentials persist in
    # /data. Interfold itself will fail startup if wallet/network state is absent.
    validate_persisted_password_file
    log "No credential upload present; using persisted credential state."
else
    fail "credentials file is required for first startup: $SECRETS_FILE"
fi

# Build CLI args without shell evaluation.
CLI_ARGS=(--config "$CONFIG_FILE")

case "$LOG_LEVEL" in
    trace) CLI_ARGS=(-vvv "${CLI_ARGS[@]}") ;;
    debug) CLI_ARGS=(-vv "${CLI_ARGS[@]}") ;;
    info)  CLI_ARGS=(-v "${CLI_ARGS[@]}") ;;
esac

# Add peers if provided.
if [ -n "${PEERS:-}" ]; then
    IFS=',' read -ra PEER_ARRAY <<< "$PEERS"
    for peer in "${PEER_ARRAY[@]}"; do
        peer="$(echo "$peer" | xargs)"
        [ -n "$peer" ] && CLI_ARGS+=(--peer "$peer")
    done
fi

# EXTRA_OPTS remains an advanced, non-secret escape hatch. Split it as plain
# arguments; never evaluate it as shell source.
if [ -n "${EXTRA_OPTS:-}" ]; then
    read -r -a EXTRA_ARGS <<< "$EXTRA_OPTS"
    CLI_ARGS+=("${EXTRA_ARGS[@]}")
fi

log "Starting Interfold ciphernode."
exec interfold start "${CLI_ARGS[@]}"
