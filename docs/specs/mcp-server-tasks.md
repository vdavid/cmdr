# MCP server tasks

Implementation tasks for the cmdr MCP server. See `mcp-server-spec.md` for the full specification.

## Prerequisites

Before starting, read:
- `mcp-server-spec.md` - Full specification
- `apps/desktop/src/lib/commands/command-registry.ts` - Existing command definitions
- `apps/desktop/src-tauri/src/commands/ui.rs` - Existing Tauri commands

## Testing notes

**E2E testing isn't possible** because macOS has no WebDriver support. Instead:
- Write **Rust unit tests** for protocol parsing, tool registry, and executor logic
- Write **Rust integration tests** for HTTP endpoints (spawn test server, make HTTP requests)
- Test app command execution manually (they depend on running Tauri app state)

## Phase 1: Core infrastructure

### 1.1 Create MCP module structure

Create the Rust module structure for MCP:

```
apps/desktop/src-tauri/src/
├── mcp/
│   ├── mod.rs           # Module exports
│   ├── server.rs        # HTTP server setup
│   ├── protocol.rs      # MCP message handling
│   ├── tools.rs         # Tool registry and definitions
│   └── executor.rs      # Tool execution logic
```

**Files to create:**
- [x] `src/mcp/mod.rs` - Export submodules, feature flag check
- [x] `src/mcp/server.rs` - HTTP server (Streamable HTTP transport)
- [x] `src/mcp/protocol.rs` - MCP JSON-RPC message parsing
- [x] `src/mcp/tools.rs` - Tool definitions and schemas
- [x] `src/mcp/executor.rs` - Tool execution dispatcher

**Acceptance criteria:**
- Module compiles
- Feature flag `CMDR_MCP_ENABLED` controls server startup

### 1.2 Set up HTTP server

Implement the HTTP server using `axum` or `warp`.

**Add dependencies to `Cargo.toml`:**
```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["cors"] }
```

**Implement in `server.rs`:**
- [x] Create `start_mcp_server()` function
- [x] Bind to `127.0.0.1:9224`
- [x] Add routes:
  - `POST /mcp` - JSON-RPC endpoint
  - `GET /mcp` - Optional SSE stream (Streamable HTTP spec)
  - `GET /mcp/health` - Returns `{"status": "ok"}`
- [x] Call from `lib.rs` setup if enabled

**Acceptance criteria:**
- `curl http://localhost:9224/mcp/health` returns `{"status": "ok"}`
- Server only starts when `CMDR_MCP_ENABLED=true`

### 1.3 Implement MCP protocol handler

Parse and route MCP messages in `protocol.rs`.

**Implement:**
- [x] `McpRequest` struct (JSON-RPC format)
- [x] `McpResponse` struct
- [x] Message routing:
  - `initialize` → Return capabilities
  - `tools/list` → Return tool definitions
  - `tools/call` → Execute tool and return result

**Message format:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

**Acceptance criteria:**
- Can parse valid MCP requests
- Returns proper error for invalid requests
- `initialize` returns server capabilities

## Phase 3: Tool execution

### 3.1 Implement app command executor

Execute app commands in `executor.rs`.

**Approach:**
- Emit Tauri events that frontend listens to (matches current menu behavior)
- Or call existing UI commands directly

**Implement:**
- [x] `execute_tool(app: AppHandle, name: &str, params: Value) -> Result<Value>`
- [x] Route each app command to appropriate handler
- [x] Return success/error response

**Example:**
```
"view.showHidden" => {
    toggle_hidden_files(app)?;
    Ok(json!({"success": true}))
}
```

**Acceptance criteria:**
- `tools/call` with `view.showHidden` toggles hidden files
- Menu checkbox updates correctly

### 3.2 Implement file system executor

**Status: N/A** — Removed in Phase 6 for security. Agents should not have direct file system access.

~~Execute file system operations.~~

~~**Implement handlers for:**~~
- [N/A] `fs.list` - Removed in Phase 6
- [N/A] `fs.read` - Removed in Phase 6
- [N/A] `fs.write` - Removed in Phase 6
- [N/A] `fs.create` - Removed in Phase 6
- [N/A] `fs.delete` - Removed in Phase 6
- [N/A] `fs.move` - Removed in Phase 6
- [N/A] `fs.copy` - Removed in Phase 6
- [N/A] `fs.exists` - Removed in Phase 6
- [N/A] `fs.stat` - Removed in Phase 6

### 3.3 Implement shell and context executor

**Shell: N/A** — Removed in Phase 6 for security. Agents should not have shell access.

- [N/A] `shell.run` - Removed in Phase 6

**Context:** Implemented in Phase 6.5

- [x] Get cursor position from app state via PaneStateStore
- [x] Return current pane paths from stored state

## Phase 4: Integration and polish

### 4.1 Wire up to app startup

**In `lib.rs`:**
- [x] Import mcp module
- [x] Start MCP server in setup if enabled
- [x] Log startup message with port

**Acceptance criteria:**
- MCP server starts automatically with app in dev mode
- Logs: `MCP server listening on http://127.0.0.1:9224`

### 4.2 Add configuration

**Implement:**
- [x] Read `CMDR_MCP_ENABLED` (default: debug only)
- [x] Read `CMDR_MCP_PORT` (default: 9224)
- [N/A] Read `CMDR_MCP_ALLOW_SHELL` - Removed in Phase 6
- [N/A] Read `CMDR_MCP_ALLOW_WRITE` - Removed in Phase 6

**Acceptance criteria:**
- Port is configurable
- Dangerous operations are off by default

### 4.3 Add error handling and logging

**Implement:**
- [x] Proper error types for MCP errors
- [x] Log all tool calls with parameters
- [x] Log errors with stack traces

**Acceptance criteria:**
- Invalid tool calls return proper MCP error response
- Tool calls are visible in logs

### 4.4 Write tests

**Unit tests:**
- [x] Protocol parsing
- [x] Tool registry completeness
- [x] Permission flag checks (N/A - no permission flags after Phase 6)

**Integration tests:**
- [x] Health endpoint
- [x] tools/list response
- [x] tools/call for each tool category

**Acceptance criteria:**
- `cargo test` passes
- All tool categories have at least one integration test

## Phase 5: Documentation and client setup

### 5.1 Update feature documentation

- [x] Add `docs/features/mcp-server.md`
- [x] Document all tools with examples
- [x] Document configuration options

### 5.2 Create client configuration examples

- [x] Claude Desktop config example
- [x] Cursor config example
- [x] Generic MCP client example

See: `docs/guides/mcp-client-setup.md`

### 5.3 Add developer guide

- [x] How to add new tools
- [x] How to test MCP locally
- [x] Troubleshooting common issues

See: `docs/guides/mcp-development.md`

## Completion checklist

- [x] All tools implemented and tested
- [x] Server starts/stops cleanly with app
- [x] Configuration works as documented
- [ ] Can connect from Claude Desktop (needs testing)
- [x] All checks pass (`./scripts/check.sh`)
- [x] Documentation complete

---

## Phase 6: Parity refactoring (Jan 2026)

Goal: Agent can do exactly what user can do, nothing more.

### 6.1 Remove elevated privileges

- [x] Remove `fs.*` tools (list, read, write, create, delete, move, copy, exists, stat)
- [x] Remove `shell.run` tool
- [x] Remove `allow_shell` and `allow_write` config options
- [x] Update executor.rs to remove fs/shell execution
- [x] Update config.rs to simplify
- [x] Update server.rs to not pass config to get_all_tools

### 6.2 Add navigation commands

- [x] Define `nav.up`, `nav.down` tools (cursor movement)
- [x] Define `nav.home`, `nav.end` tools (go to first/last)
- [x] Define `nav.pageUp`, `nav.pageDown` tools
- [x] Define `nav.left`, `nav.right` tools (Brief mode column nav)
- [x] Implement executor to emit keyboard events to frontend
- [x] Wire frontend to handle MCP navigation events (same as keyboard)

### 6.3 Add sort commands

- [x] Define `sort.byName`, `sort.byExtension`, `sort.bySize`, `sort.byModified`, `sort.byCreated`
- [x] Define `sort.ascending`, `sort.descending`, `sort.toggleOrder`
- [x] Add sort commands to command-registry.ts
- [x] Add "Sort by" submenu to View menu
- [x] Implement executor to emit sort events to frontend
- [x] Wire frontend to handle MCP sort events

### 6.4 Add volume tools

- [x] Define `volume.list` tool
- [x] Define `volume.selectLeft(index)`, `volume.selectRight(index)` tools
- [x] Implement executor to emit volume events to frontend
- [x] Wire frontend to handle volume events

### 6.5 Improve context tools

- [x] Define `context.getFocusedPane` tool
- [x] Define `context.getLeftPanePath`, `context.getRightPanePath` tools
- [x] Define `context.getLeftPaneContent`, `context.getRightPaneContent` tools
- [x] Define `context.getInfoForFileUnderCursor` tool
- [x] Implement pane state sync from frontend to Rust
- [x] Implement executor to return pane state from store

### 6.6 Wire up frontend state sync

- [x] In FilePane.svelte, sync pane state when files loaded
- [x] In DualPaneExplorer.svelte, sync focused pane
- [x] In NetworkBrowser.svelte, sync network hosts as file entries
- Note: Sort and cursor position changes work correctly because they go through Rust's listing backend

### 6.7 Test and verify parity

- [x] All MCP tools match user capabilities (43 tools)
- [x] No MCP tool can do something user cannot (removed fs.*, shell.run)
- [x] Navigation via MCP uses same code paths as keyboard
- [x] All checks pass

### 6.8 Comprehensive security tests

- [x] 41 automated tests covering:
  - Protocol-level security (malformed requests, injection attempts)
  - Tool definitions (names, schemas, descriptions)
  - Input validation (missing params, wrong types)
  - Volume tools (schema validation, required params)
  - Pane state store (thread safety, edge cases)
  - Security checks (no fs.*, shell.*, exec.* tools)
  - Unicode and special characters in file paths
  - Null byte injection attempts
  - Very long method names (DoS prevention)
- [x] All security tests pass
