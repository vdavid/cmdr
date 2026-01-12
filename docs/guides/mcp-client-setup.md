# MCP client configuration

This guide shows how to configure popular AI assistants to use cmdr's MCP server.

## Prerequisites

1. cmdr must be running with MCP enabled
2. Default port is `9224` (configurable via `CMDR_MCP_PORT`)
3. Verify the server is running:
   ```bash
   curl http://localhost:9224/mcp/health
   # Should return: {"status":"ok"}
   ```

## Claude Desktop

Add to your Claude Desktop configuration file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "cmdr": {
      "url": "http://localhost:9224/mcp"
    }
  }
}
```

After saving, restart Claude Desktop. You should see "cmdr" in the MCP servers list.

### Verifying connection

Ask Claude:
> "What tools are available from the cmdr MCP server?"

Claude should list the 34 available tools.

## Amp (ampcode.com)

Add to your VS Code settings (`settings.json`):

```json
{
  "amp.mcpServers": {
    "cmdr": {
      "url": "http://localhost:9224/mcp"
    }
  }
}
```

If you already have other MCP servers configured, just add the `cmdr` entry to the existing `amp.mcpServers` object.

## Cursor

Add to your Cursor settings (`.cursor/mcp.json` in your project or global config):

```json
{
  "mcpServers": {
    "cmdr": {
      "transport": "http",
      "url": "http://localhost:9224/mcp"
    }
  }
}
```

## Continue.dev

Add to your Continue configuration (`~/.continue/config.json`):

```json
{
  "mcpServers": [
    {
      "name": "cmdr",
      "transport": {
        "type": "http",
        "url": "http://localhost:9224/mcp"
      }
    }
  ]
}
```

## Generic MCP client

For any MCP-compatible client, use these connection details:

| Setting      | Value                              |
|--------------|------------------------------------|
| Transport    | Streamable HTTP                    |
| URL          | `http://localhost:9224/mcp`        |
| Health check | `http://localhost:9224/mcp/health` |

### Manual testing with curl

```bash
# Initialize connection
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'

# List available tools
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

# Navigate down
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nav.down","arguments":{}}}'

# Get focused pane
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"context.getFocusedPane","arguments":{}}}'
```

## Troubleshooting

### "Connection refused"

1. Ensure cmdr is running
2. Check MCP is enabled: look for "MCP server listening" in logs
3. Verify port: `lsof -i :9224`

### "Server not found" in Claude Desktop

1. Restart Claude Desktop after editing config
2. Check JSON syntax is valid
3. Ensure cmdr is running before starting Claude

### Tools not appearing

1. Try reinitializing: disconnect and reconnect in your client
2. Check the health endpoint works
3. Verify with `tools/list` request manually
