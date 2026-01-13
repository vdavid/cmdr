# MCP server specification

This document specifies the MCP (Model Context Protocol) server for cmdr, enabling AI agents to control the file manager.

## Goals

1. Expose all cmdr commands as MCP tools
2. Add file system operations (read, write, list, create, delete)
3. Add shell command execution
4. Support both internal agent (future) and external AI clients (Claude, etc.)
5. Run as part of the Tauri app process (no separate server)

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Tauri App                                                       │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ MCP Module (Rust)                                        │  │
│  │                                                          │  │
│  │  ┌─────────────────┐   ┌─────────────────────────────┐   │  │
│  │  │ Tool Registry   │   │ MCP Protocol Handler        │   │  │
│  │  │ - App commands  │   │ - tools/list                │   │  │
│  │  │ - File system   │   │ - tools/call                │   │  │
│  │  │ - Shell         │   │ - resources/list (optional) │   │  │
│  │  └────────┬────────┘   └──────────────┬──────────────┘   │  │
│  │           │                           │                   │  │
│  │           ▼                           ▼                   │  │
│  │  ┌─────────────────────────────────────────────────────┐ │  │
│  │  │ Tool Executor                                       │ │  │
│  │  │ - Invokes Tauri commands                            │ │  │
│  │  │ - Performs file operations                          │ │  │
│  │  │ - Runs shell commands                               │ │  │
│  │  └─────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────┘  │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐        ┌─────────────────────────────────┐│
│  │ HTTP Server     │◀───────│ External clients (Claude, etc.) ││
│  │ localhost:9224  │        └─────────────────────────────────┘│
│  └─────────────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ Internal Agent  │  (future - direct Rust calls)             │
│  │ Thread          │                                           │
│  └─────────────────┘                                           │
└─────────────────────────────────────────────────────────────────┘
```

## Tool categories

### 1. App commands

These map directly to the command palette commands.

| Tool ID | Description | Parameters |
|---------|-------------|------------|
| `app.quit` | Quit the application | none |
| `app.hide` | Hide the application | none |
| `app.about` | Show about window | none |
| `view.showHidden` | Toggle hidden files visibility | none |
| `view.briefMode` | Switch to brief view mode | none |
| `view.fullMode` | Switch to full view mode | none |
| `pane.switch` | Switch focus to other pane | none |
| `pane.leftVolumeChooser` | Open left pane volume chooser | none |
| `pane.rightVolumeChooser` | Open right pane volume chooser | none |
| `nav.open` | Open selected item | none |
| `nav.parent` | Navigate to parent folder | none |
| `nav.back` | Navigate back in history | none |
| `nav.forward` | Navigate forward in history | none |
| `file.showInFinder` | Show selected file in Finder | none |
| `file.copyPath` | Copy selected file path to clipboard | none |
| `file.copyFilename` | Copy selected filename to clipboard | none |
| `file.quickLook` | Preview selected file with Quick Look | none |
| `file.getInfo` | Open Get Info window for selected file | none |

### 2. File system operations

Direct file system access for AI agents.

| Tool ID | Description | Parameters |
|---------|-------------|------------|
| `fs.list` | List directory contents | `path: string` |
| `fs.read` | Read file contents | `path: string` |
| `fs.write` | Write file contents | `path: string, content: string` |
| `fs.create` | Create file or directory | `path: string, type: "file" \| "directory"` |
| `fs.delete` | Delete file or directory | `path: string` |
| `fs.move` | Move/rename file or directory | `from: string, to: string` |
| `fs.copy` | Copy file or directory | `from: string, to: string` |
| `fs.exists` | Check if path exists | `path: string` |
| `fs.stat` | Get file/directory metadata | `path: string` |

### 3. Shell commands

Execute shell commands with appropriate sandboxing.

| Tool ID | Description | Parameters |
|---------|-------------|------------|
| `shell.run` | Run a shell command | `command: string, cwd?: string, timeout?: number` |

### 4. Context operations

Get current app state for context-aware operations.

| Tool ID | Description | Parameters |
|---------|-------------|------------|
| `context.getSelection` | Get currently selected file(s) | none |
| `context.getCurrentPaths` | Get paths of both panes | none |
| `context.getFocusedPane` | Get which pane is focused | none |

## MCP protocol implementation

### Transports

cmdr supports two MCP transports:

#### Streamable HTTP (primary)

- **Protocol**: Streamable HTTP (MCP spec 2025-11-25)
- **Port**: 9224 (configurable via `CMDR_MCP_PORT`)
- **Host**: localhost only (security)

**Endpoints:**

```
POST /mcp                 # JSON-RPC endpoint for all MCP messages
GET  /mcp                 # Optional SSE stream (Streamable HTTP spec)
GET  /mcp/health          # Health check
```

#### STDIO (bridge)

For clients that spawn subprocesses, the `cmdr-mcp-stdio` binary bridges STDIO to the HTTP server:

- **Binary**: `cmdr-mcp-stdio`
- **Protocol**: Newline-delimited JSON-RPC over stdin/stdout
- **Requires**: cmdr app running with HTTP server enabled

The STDIO binary forwards all requests to `http://127.0.0.1:9224/mcp`.

### MCP messages supported

| Method | Description |
|--------|-------------|
| `initialize` | Initialize MCP session |
| `tools/list` | List available tools with schemas |
| `tools/call` | Execute a tool |
| `resources/list` | List available resources (optional) |
| `resources/read` | Read a resource (optional) |

### Tool schema format

Each tool is defined with a JSON schema for parameters:

```json
{
  "name": "fs.read",
  "description": "Read the contents of a file",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Absolute path to the file"
      }
    },
    "required": ["path"]
  }
}
```

## Configuration

### Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CMDR_MCP_ENABLED` | Enable MCP server | `true` in debug, `false` in release |
| `CMDR_MCP_PORT` | HTTP server port | `9224` |
| `CMDR_MCP_ALLOW_SHELL` | Allow shell command execution | `false` |
| `CMDR_MCP_ALLOW_WRITE` | Allow file write operations | `false` |

### Security

- **Localhost only**: Server binds to `127.0.0.1`, not `0.0.0.0`
- **Opt-in dangerous operations**: Shell and write operations require explicit config
- **Path sandboxing**: Future enhancement to restrict accessible paths

## Integration with existing code

### Tool registry generation

Generate tool definitions from `command-registry.ts`:

```typescript
// Build script or dev tool
import { commands } from './command-registry'

const mcpTools = commands.map(cmd => ({
  name: cmd.id,
  description: cmd.name,
  inputSchema: { type: 'object', properties: {}, required: [] }
}))
```

### Tool execution

Route tool calls to existing Tauri commands:

```rust
async fn execute_tool(name: &str, params: Value) -> Result<Value, Error> {
    match name {
        "view.showHidden" => toggle_hidden_files(app),
        "fs.read" => read_file(params["path"].as_str()),
        "shell.run" => run_shell(params),
        _ => Err(Error::UnknownTool(name))
    }
}
```

## Client configuration

### Claude Desktop

Add to `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "cmdr": {
      "url": "http://localhost:9224/mcp"
    }
  }
}
```

### Cursor / other MCP clients

Similar configuration pointing to `http://localhost:9224/mcp`.

## Future enhancements

1. **Internal agent** - Rust thread with LLM integration calling tools directly
2. **Path sandboxing** - Restrict file operations to allowed directories
3. **Audit logging** - Log all tool executions for debugging
4. **Rate limiting** - Prevent runaway agents
5. **Tool permissions** - Per-client tool access control
