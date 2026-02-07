# MCP server developer guide

This guide explains how to extend the cmdr MCP server with new tools and test your changes.

## Architecture

The MCP server consists of these components:

```
apps/desktop/src-tauri/src/
├── mcp/
│   ├── mod.rs           # Module exports
│   ├── config.rs        # Configuration (env vars)
│   ├── server.rs        # HTTP server (axum)
│   ├── protocol.rs      # JSON-RPC message handling
│   ├── tools.rs         # Tool definitions and schemas
│   ├── executor.rs      # Tool execution logic
│   ├── pane_state.rs    # Frontend state synchronization
│   └── tests.rs         # Comprehensive test suite
└── bin/
    └── cmdr-mcp-stdio.rs  # STDIO bridge binary
```

### Data flow

1. **HTTP request** → `server.rs` receives JSON-RPC request
2. **Parsing** → `protocol.rs` parses and validates the request
3. **Routing** → `tools/call` routes to `executor.rs`
4. **Execution** → `executor.rs` emits Tauri events or queries state
5. **Frontend** → `+page.svelte` handles events, updates UI
6. **State sync** → Frontend syncs state back to `pane_state.rs`

## Adding a new tool

### Step 1: Define the tool in `tools.rs`

Add your tool definition to the appropriate category function:

```rust
fn get_my_tools() -> Vec<Tool> {
    vec![
        // Tool with no parameters
        Tool::no_params("my.action", "Description of what it does"),
        
        // Tool with parameters
        Tool {
            name: "my.paramAction".to_string(),
            description: "Action with parameters".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "myParam": {
                        "type": "string",
                        "description": "Parameter description"
                    }
                },
                "required": ["myParam"]
            }),
        },
    ]
}
```

Add to `get_all_tools()`:

```rust
pub fn get_all_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    // ... existing tools ...
    tools.extend(get_my_tools());  // Add your category
    tools
}
```

### Step 2: Implement execution in `executor.rs`

Add a handler in `execute_tool()`:

```rust
pub fn execute_tool<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        // ... existing handlers ...
        
        // Add your category
        n if n.starts_with("my.") => execute_my_command(app, n, params),
        
        _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
    }
}

fn execute_my_command<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        "my.action" => {
            // Emit event to frontend
            app.emit("my-event", json!({"action": "doThing"}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({"success": true}))
        }
        
        "my.paramAction" => {
            // Extract parameter
            let my_param = params
                .get("myParam")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'myParam'"))?;
            
            // Use the parameter
            app.emit("my-event", json!({"action": "paramAction", "value": my_param}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({"success": true, "param": my_param}))
        }
        
        _ => Err(ToolError::invalid_params(format!("Unknown my command: {name}"))),
    }
}
```

### Step 3: Handle in frontend (if needed)

Add event listener in `+page.svelte`:

```typescript
// In setupTauriEventListeners()
try {
    await listen<{ action: string; value?: string }>('my-event', (event) => {
        const { action, value } = event.payload
        if (action === 'doThing') {
            explorerRef?.doThing()
        } else if (action === 'paramAction') {
            explorerRef?.paramAction(value!)
        }
    })
} catch {
    // Not in Tauri environment
}
```

### Step 4: Add tests

Add tests in `tests.rs`:

```rust
#[test]
fn test_my_tools_exist() {
    let tools = get_all_tools();
    let my_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("my.")).collect();
    
    assert!(my_tools.iter().any(|t| t.name == "my.action"));
    assert!(my_tools.iter().any(|t| t.name == "my.paramAction"));
}

#[test]
fn test_my_param_action_requires_param() {
    let tools = get_all_tools();
    let tool = tools.iter().find(|t| t.name == "my.paramAction").unwrap();
    
    let required = tool.input_schema.get("required").and_then(|r| r.as_array());
    assert!(required.is_some_and(|r| r.iter().any(|v| v == "myParam")));
}
```

### Step 5: Update tool count

Update the test in `tests.rs`:

```rust
#[test]
fn test_total_tool_count() {
    let tools = get_all_tools();
    // Update count: was 43, now 45 (added 2 my.* tools)
    assert_eq!(tools.len(), 45);
}
```

## Testing locally

### Run unit tests

```bash
cd apps/desktop/src-tauri
cargo test mcp::tests
```

### Run all checks

```bash
./scripts/check.sh --app desktop
```

### Manual testing with curl

```bash
# Start the app
pnpm tauri dev

# In another terminal:
# List all tools
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Call your tool
curl -X POST http://localhost:9224/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"my.action","arguments":{}}}'
```

## Security guidelines

When adding new tools, follow these principles:

1. **Match user capabilities**: Tools should only do what users can do through the UI
2. **No direct file access**: Don't add `fs.read`, `fs.write`, or similar
3. **No shell execution**: Don't add `shell.run` or similar
4. **Validate all inputs**: Check types, bounds, and sanitize strings
5. **Use events, not direct calls**: Emit events that frontend handles
6. **Bound string lengths**: Prevent DoS with very long inputs

### Security test checklist

Add these tests for new tools:

```rust
#[test]
fn test_my_tool_names_valid() {
    let tools = get_all_tools();
    for tool in tools.iter().filter(|t| t.name.starts_with("my.")) {
        // No shell injection chars
        assert!(!tool.name.contains(';'));
        assert!(!tool.name.contains('|'));
        // Reasonable length
        assert!(tool.name.len() <= 64);
    }
}
```

## Common patterns

### Reading state (context tools)

```rust
fn execute_context_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("State not initialized"))?;
    
    match name {
        "context.myQuery" => {
            let data = store.get_my_data();
            Ok(json!({"result": data}))
        }
        _ => Err(ToolError::invalid_params("Unknown")),
    }
}
```

### Triggering UI actions (nav/sort tools)

```rust
app.emit("mcp-key", json!({"key": "ArrowDown"}))
    .map_err(|e| ToolError::internal(e.to_string()))?;
Ok(json!({"success": true}))
```

### Calling existing Tauri commands

```rust
use crate::commands::ui::toggle_hidden_files;

let result = toggle_hidden_files(app.clone())
    .map_err(ToolError::internal)?;
Ok(json!({"success": true, "value": result}))
```

## STDIO bridge

The `cmdr-mcp-stdio` binary provides STDIO transport for MCP clients.

### Building

```bash
cd apps/desktop/src-tauri
cargo build --bin cmdr-mcp-stdio
# Binary at: target/debug/cmdr-mcp-stdio
```

### Testing manually

```bash
# Start cmdr app first, then:
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | ./target/debug/cmdr-mcp-stdio
```

### How it works

1. Reads newline-delimited JSON-RPC from stdin
2. POSTs each message to `http://127.0.0.1:9224/mcp`
3. Writes response to stdout (newline-delimited)
4. Logs errors to stderr

The bridge is stateless—all state lives in the main cmdr app's HTTP server.

## Troubleshooting development

### Compilation errors

Run `cargo check` in the `src-tauri` directory:
```bash
cd apps/desktop/src-tauri
cargo check
```

### Tests failing

Run specific test:
```bash
cargo test mcp::tests::test_my_tools_exist -- --nocapture
```

### Events not reaching frontend

1. Check event name matches exactly
2. Verify listener is registered in `setupTauriEventListeners()`
3. Add `console.log` in the listener to debug
4. Check browser devtools console in Tauri dev mode

### State not syncing

1. Verify `pane_state.rs` has the field
2. Check frontend calls the sync command
3. Add logging in `update_*_pane_state` commands
