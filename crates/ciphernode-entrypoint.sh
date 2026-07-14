#!/bin/bash
set -Eeuo pipefail

umask 077

# Paths to config and secrets
CONFIG_FILE="$CONFIG_DIR/config.yaml"
SECRETS_FILE="/run/secrets/secrets.json"

# Ensure required files exist
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Error: Config file $CONFIG_FILE not found!"
    exit 1
fi

if [ ! -f "$SECRETS_FILE" ]; then
    echo "Error: Secrets file $SECRETS_FILE not found!"
    exit 1
fi

jq -e '
    type == "object" and
    (.password | type == "string" and length > 0) and
    (.private_key | type == "string" and test("^0x[0-9a-fA-F]{64}$"))
' "$SECRETS_FILE" >/dev/null || {
    echo "Error: Invalid 'password' or 'private_key' in secrets file!"
    exit 1
}

# Set password
echo "Setting password"
jq -er '.password' "$SECRETS_FILE" | interfold password set --config "$CONFIG_FILE" --password-stdin

echo "Setting wallet key"
# The refactored wallet command atomically derives and stores the libp2p key
# from the operator key, so a separate network-key command is no longer used.
jq -er '.private_key' "$SECRETS_FILE" | interfold wallet set --config "$CONFIG_FILE" --private-key-stdin

rm -f "$SECRETS_FILE"

echo "Starting ciphernode"
exec interfold start -v --config "$CONFIG_FILE"

