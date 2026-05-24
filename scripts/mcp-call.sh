#!/bin/bash
set -euo pipefail

# Call a tool on Cmdr's MCP server (JSON-RPC over HTTP).
#
# Usage:
#   ./scripts/mcp-call.sh <tool_name> [json_args]
#   ./scripts/mcp-call.sh --list-tools
#   ./scripts/mcp-call.sh --read-resource <uri>
#   ./scripts/mcp-call.sh --raw <json-rpc-body>
#
# Examples:
#   ./scripts/mcp-call.sh search '{"pattern":"*.pdf","limit":5}'
#   ./scripts/mcp-call.sh ai_search '{"query":"recent invoices"}'
#   ./scripts/mcp-call.sh nav_to_path '{"pane":"left","path":"/Users"}'
#   ./scripts/mcp-call.sh --list-tools
#   ./scripts/mcp-call.sh --read-resource 'cmdr://state'

# Port discovery precedence (P2/P3 ephemeral-port design):
#   1. CMDR_MCP_PORT env: explicit pin wins.
#   2. CMDR_INSTANCE_ID set: read <data_dir>/mcp.port (the server writes it atomically
#      after bind). Data dir is derived per OS to mirror tauri-wrapper.js + instance-id.js.
#   3. Fall back to 19225 (dev default port) so a bare `./scripts/mcp-call.sh` against a
#      stock dev session still works.
#
# Don't silently fall back to the legacy default when an instance is configured: a missing
# port file means the server hasn't bound yet (or crashed), and we want a clear error
# rather than connecting to whatever's listening on 19225.
HOST="127.0.0.1"
TIMEOUT=30

resolve_data_dir() {
    # Mirrors computeAppDataDir() in apps/desktop/scripts/instance-id.js.
    local instance="$1"
    local identifier="com.veszelovszki.cmdr-${instance}"
    case "$(uname -s)" in
        Darwin)
            echo "${HOME}/Library/Application Support/${identifier}"
            ;;
        *)
            echo "${XDG_DATA_HOME:-${HOME}/.local/share}/${identifier}"
            ;;
    esac
}

read_port_file() {
    local data_dir="$1"
    local port_file="${data_dir}/mcp.port"
    if [[ ! -f "$port_file" ]]; then
        echo "Error: port file not found at ${port_file}. Is Cmdr running with CMDR_INSTANCE_ID=${CMDR_INSTANCE_ID}?" >&2
        exit 1
    fi
    # File format: ASCII decimal port + newline (see port_file.rs).
    local raw
    raw="$(<"$port_file")"
    raw="${raw//[[:space:]]/}"
    if [[ ! "$raw" =~ ^[0-9]+$ ]]; then
        echo "Error: port file ${port_file} content not a valid u16: ${raw}" >&2
        exit 1
    fi
    echo "$raw"
}

if [[ -n "${CMDR_MCP_PORT:-}" ]]; then
    PORT="$CMDR_MCP_PORT"
elif [[ -n "${CMDR_INSTANCE_ID:-}" ]]; then
    DATA_DIR="${CMDR_DATA_DIR:-$(resolve_data_dir "$CMDR_INSTANCE_ID")}"
    PORT="$(read_port_file "$DATA_DIR")"
else
    PORT=19225
fi

BASE_URL="http://${HOST}:${PORT}/mcp"

# Track JSON-RPC request ID
ID=1

rpc() {
    local body="$1"
    curl -sf --max-time "$TIMEOUT" -X POST "$BASE_URL" \
        -H 'Content-Type: application/json' \
        -d "$body" 2>/dev/null
}

rpc_pretty() {
    local result
    result=$(rpc "$1") || { echo "Error: MCP server not reachable at ${BASE_URL}" >&2; exit 1; }

    if command -v jq &>/dev/null; then
        echo "$result" | jq .
    else
        echo "$result"
    fi
}

# Initialize session (required before tools/call)
init() {
    local body
    body=$(cat <<'JSON'
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"mcp-call","version":"1.0"}}}
JSON
    )
    rpc "$body" >/dev/null 2>&1 || { echo "Error: MCP server not reachable at ${BASE_URL}" >&2; exit 1; }
}

case "${1:-}" in
    -h|--help|"")
        echo "Usage:"
        echo "  ./scripts/mcp-call.sh <tool_name> [json_args]"
        echo "  ./scripts/mcp-call.sh --list-tools"
        echo "  ./scripts/mcp-call.sh --read-resource <uri>"
        echo "  ./scripts/mcp-call.sh --raw <json-rpc-body>"
        echo ""
        echo "Environment:"
        echo "  CMDR_MCP_PORT     Pin a specific port; wins over the file."
        echo "  CMDR_INSTANCE_ID  Reads <data_dir>/mcp.port for ephemeral port discovery."
        echo "  CMDR_DATA_DIR     Overrides the data-dir derivation when set with the instance."
        echo "  (Falls back to port 19225 (dev default) when neither is set.)"
        echo ""
        echo "Examples:"
        echo "  ./scripts/mcp-call.sh search '{\"pattern\":\"*.pdf\",\"limit\":5}'"
        echo "  ./scripts/mcp-call.sh ai_search '{\"query\":\"recent invoices\"}'"
        echo "  ./scripts/mcp-call.sh --list-tools"
        echo "  ./scripts/mcp-call.sh --read-resource 'cmdr://state'"
        exit 0
        ;;
    --list-tools)
        init
        rpc_pretty '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
        ;;
    --read-resource)
        uri="${2:?Missing resource URI}"
        init
        rpc_pretty "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"resources/read\",\"params\":{\"uri\":\"${uri}\"}}"
        ;;
    --raw)
        body="${2:?Missing JSON-RPC body}"
        rpc_pretty "$body"
        ;;
    *)
        tool_name="$1"
        args="${2:-\{\}}"
        init
        # Build JSON-RPC request
        body="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"${tool_name}\",\"arguments\":${args}}}"
        rpc_pretty "$body"
        ;;
esac
