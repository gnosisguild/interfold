# Get the current block timestamp from a local EVM node
# Usage: get_evm_timestamp [rpc_url]
get_evm_timestamp() {
  local rpc_url="${1:-http://localhost:8545}"
  curl -s -X POST "$rpc_url" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest",false],"id":1}' \
    | jq -r '.result.timestamp' | xargs printf "%d\n"
}

# Extract and validate the E3 ID printed by committee:new.
# Usage: extract_e3_id "$request_output"
extract_e3_id() {
  local request_output="$1"
  local e3_id
  e3_id=$(printf '%s\n' "$request_output" | sed -n 's/^E3_ID=//p' | tail -n 1)

  case "$e3_id" in
    ''|*[!0-9]*)
      echo "Committee request did not return a valid E3 ID" >&2
      return 1
      ;;
  esac

  printf '%s\n' "$e3_id"
}
