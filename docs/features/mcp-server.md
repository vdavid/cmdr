# MCP server

The cmdr MCP (Model Context Protocol) server enables AI agents to interact with the file manager through a standardized protocol. This allows AI assistants like Claude to navigate directories, manage views, and query file information using the same capabilities available to users.

## Overview

The MCP server exposes 43 tools that mirror user capabilities exactly. AI agents can:

- Navigate directories (up, down, open, back, forward)
- Change sort order and view modes
- Switch between volumes
- Query pane contents and selected files

**Security principle**: Agents can only do what users can do through the UI. There are no elevated privileges like direct file system access or shell execution.

## Configuration

The MCP server is controlled by environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `CMDR_MCP_ENABLED` | `true` in dev, `false` in prod | Enable the MCP server |
| `CMDR_MCP_PORT` | `9223` | Port for the HTTP server |

### Enabling in development

The MCP server is enabled by default in development mode. Start the app normally:

```bash
pnpm tauri dev
```

The server will log:
```
MCP server listening on http://127.0.0.1:9223
```

### Enabling in production

Set the environment variable before launching:

```bash
CMDR_MCP_ENABLED=true open /Applications/cmdr.app
```

## Available tools

### App commands (3)

| Tool | Description |
|------|-------------|
| `app.quit` | Quit the application |
| `app.hide` | Hide the application window |
| `app.about` | Show the about window |

### View commands (3)

| Tool | Description |
|------|-------------|
| `view.showHidden` | Toggle hidden files visibility |
| `view.briefMode` | Switch to Brief view mode |
| `view.fullMode` | Switch to Full view mode |

### Pane commands (3)

| Tool | Description |
|------|-------------|
| `pane.switch` | Switch focus to the other pane |
| `pane.leftVolumeChooser` | Open volume chooser for left pane |
| `pane.rightVolumeChooser` | Open volume chooser for right pane |

### Navigation commands (12)

| Tool | Description |
|------|-------------|
| `nav.open` | Open/enter selected item |
| `nav.parent` | Navigate to parent folder |
| `nav.back` | Navigate back in history |
| `nav.forward` | Navigate forward in history |
| `nav.up` | Select previous file |
| `nav.down` | Select next file |
| `nav.home` | Go to first file |
| `nav.end` | Go to last file |
| `nav.pageUp` | Page up |
| `nav.pageDown` | Page down |
| `nav.left` | Previous column (Brief mode) |
| `nav.right` | Next column (Brief mode) |

### Sort commands (8)

| Tool | Description |
|------|-------------|
| `sort.byName` | Sort by filename |
| `sort.byExtension` | Sort by file extension |
| `sort.bySize` | Sort by file size |
| `sort.byModified` | Sort by modification date |
| `sort.byCreated` | Sort by creation date |
| `sort.ascending` | Set ascending order |
| `sort.descending` | Set descending order |
| `sort.toggleOrder` | Toggle sort order |

### File commands (5)

| Tool | Description |
|------|-------------|
| `file.showInFinder` | Show selected file in Finder |
| `file.copyPath` | Copy file path to clipboard |
| `file.copyFilename` | Copy filename to clipboard |
| `file.quickLook` | Preview with Quick Look |
| `file.getInfo` | Open Get Info window |

### Volume commands (3)

| Tool | Description | Parameters |
|------|-------------|------------|
| `volume.list` | List available volumes | None |
| `volume.selectLeft` | Select volume for left pane | `index: integer` |
| `volume.selectRight` | Select volume for right pane | `index: integer` |

### Context commands (6)

| Tool | Description |
|------|-------------|
| `context.getFocusedPane` | Get focused pane (left/right) |
| `context.getLeftPanePath` | Get left pane path and volume |
| `context.getRightPanePath` | Get right pane path and volume |
| `context.getLeftPaneContent` | Get left pane file listing |
| `context.getRightPaneContent` | Get right pane file listing |
| `context.getSelectedFileInfo` | Get selected file details |

## Protocol

The MCP server uses JSON-RPC 2.0 over HTTP:

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/mcp` | JSON-RPC endpoint |
| GET | `/mcp/sse` | Server-sent events stream |
| GET | `/mcp/health` | Health check |

### Example request

```bash
curl -X POST http://localhost:9223/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "nav.down",
      "arguments": {}
    }
  }'
```

### Example response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"success\":true,\"key\":\"ArrowDown\"}"
    }]
  }
}
```

## Security

The MCP server is designed with security in mind:

1. **Localhost only**: Binds to `127.0.0.1`, not accessible from network
2. **No elevated privileges**: Agents can only do what users can do
3. **No file system access**: No direct read/write/delete operations
4. **No shell access**: Cannot execute arbitrary commands
5. **Read-only context**: Context tools only return current state

### What agents CAN do

- Navigate directories using the same UI actions as users
- Change view settings (sort, hidden files, view mode)
- Query current state (paths, file listings, selection)
- Trigger macOS integrations (Finder, Quick Look, Get Info)

### What agents CANNOT do

- Read or write file contents directly
- Execute shell commands
- Access files outside the current view
- Modify files without going through Finder

## Troubleshooting

### Server not starting

Check if MCP is enabled:
```bash
# Development (should be enabled by default)
pnpm tauri dev

# Production
CMDR_MCP_ENABLED=true open /Applications/cmdr.app
```

### Port already in use

Change the port:
```bash
CMDR_MCP_PORT=9225 pnpm tauri dev
```

### Connection refused

1. Ensure the app is running
2. Check the port: `curl http://localhost:9223/mcp/health`
3. Verify MCP is enabled in logs

### Tools not responding

The app window must be focused for some commands to work. Ensure cmdr is the frontmost application.
