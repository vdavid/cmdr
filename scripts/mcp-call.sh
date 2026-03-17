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

PORT="${CMDR_MCP_PORT:-9224}"
HOST="127.0.0.1"
BASE_URL="http://${HOST}:${PORT}/mcp"
TIMEOUT=30

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
        echo "Environment: CMDR_MCP_PORT (default: 9224)"
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
