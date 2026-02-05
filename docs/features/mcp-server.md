# MCP server

The cmdr MCP (Model Context Protocol) server enables AI agents to interact with the file manager through a standardized
protocol. This allows AI assistants like Claude to navigate directories, manage views, and query file information using
the same capabilities available to users.

## Overview

The MCP server exposes 18 tools and 2 resources that mirror user capabilities exactly. AI agents can:

- Navigate directories (up, down, open, back, forward)
- Change sort order and view modes
- Switch between volumes
- Copy files and create folders
- Query complete app state in YAML format

**Security principle**: Agents can only do what users can do through the UI. There are no elevated privileges like
direct file system access or shell execution.

## Configuration

The MCP server is controlled by environment variables:

| Variable           | Default                        | Description              |
|--------------------|--------------------------------|--------------------------|
| `CMDR_MCP_ENABLED` | `true` in dev, `false` in prod | Enable the MCP server    |
| `CMDR_MCP_PORT`    | `9224`                         | Port for the HTTP server |

### Enabling in development

The MCP server is enabled by default in development mode. Start the app normally:

```bash
pnpm tauri dev
```

The server will log:

```
MCP server listening on http://127.0.0.1:9224
```

### Enabling in production

Set the environment variable before launching:

```bash
CMDR_MCP_ENABLED=true open /Applications/cmdr.app
```

## Resources

Resources provide read-only state that agents can query.

| URI                        | Description                                          | MIME type   |
|----------------------------|------------------------------------------------------|-------------|
| `cmdr://state`             | Complete app state including panes, volumes, dialogs | `text/yaml` |
| `cmdr://dialogs/available` | Available dialog types and their parameters          | `text/yaml` |

## Tools

All tools return plain text responses: `"OK: ..."` on success, `"ERROR: ..."` on failure.

### Navigation (6)

| Tool            | Description                           | Parameters                                    |
|-----------------|---------------------------------------|-----------------------------------------------|
| `select_volume` | Switch pane to specified volume       | `pane`: left/right, `name`: volume name       |
| `nav_to_path`   | Navigate pane to specified path       | `pane`: left/right, `path`: absolute path     |
| `nav_to_parent` | Navigate to parent folder             | None                                          |
| `nav_back`      | Navigate back in history              | None                                          |
| `nav_forward`   | Navigate forward in history           | None                                          |
| `scroll_to`     | Load region around index (large dirs) | `pane`: left/right, `index`: zero-based index |

### Cursor and selection (3)

| Tool                | Description                      | Parameters                                                                                 |
|---------------------|----------------------------------|--------------------------------------------------------------------------------------------|
| `move_cursor`       | Move cursor to index or filename | `pane`: left/right, `to`: index (number) or filename (string)                              |
| `open_under_cursor` | Open/enter item under cursor     | None                                                                                       |
| `select`            | Select files in pane             | `pane`: left/right, `start`: index, `count`: number or "all", `mode`: replace/add/subtract |

### File operations (3)

| Tool      | Description                                                      | Parameters |
|-----------|------------------------------------------------------------------|------------|
| `copy`    | Copy selected files to other pane (triggers confirmation dialog) | None       |
| `mkdir`   | Create folder in focused pane (triggers naming dialog)           | None       |
| `refresh` | Refresh focused pane                                             | None       |

### View (3)

| Tool            | Description                    | Parameters                                                                  |
|-----------------|--------------------------------|-----------------------------------------------------------------------------|
| `sort`          | Sort files in pane             | `pane`: left/right, `by`: name/ext/size/modified/created, `order`: asc/desc |
| `toggle_hidden` | Toggle hidden files visibility | None                                                                        |
| `set_view_mode` | Set view mode for pane         | `pane`: left/right, `mode`: brief/full                                      |

### Dialogs (1)

| Tool     | Description                   | Parameters                                                                                                                                    |
|----------|-------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------|
| `dialog` | Open, focus, or close dialogs | `action`: open/focus/close, `type`: settings/volume-picker/file-viewer/about/confirmation, `section`?: for settings, `path`?: for file-viewer |

### App (2)

| Tool          | Description                    | Parameters |
|---------------|--------------------------------|------------|
| `switch_pane` | Switch focus to the other pane | None       |
| `quit`        | Quit the application           | None       |

## Protocol

The MCP server uses JSON-RPC 2.0 over HTTP:

### Endpoints

| Method | Path          | Description               |
|--------|---------------|---------------------------|
| POST   | `/mcp`        | JSON-RPC endpoint         |
| GET    | `/mcp/sse`    | Server-sent events stream |
| GET    | `/mcp/health` | Health check              |

### Example request

```bash
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "nav_to_path",
      "arguments": {"pane": "left", "path": "/Users"}
    }
  }'
```

### Example response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "OK: Navigated left pane to /Users"
      }
    ]
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
- Query current state (paths, file listings, file under cursor)
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
2. Check the port: `curl http://localhost:9224/mcp/health`
3. Verify MCP is enabled in logs

### Tools not responding

The app window must be focused for some commands to work. Ensure cmdr is the frontmost application.
