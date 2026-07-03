# MCP server developer guide

This guide explains how to extend the Cmdr MCP server with new tools and test your changes. For the architecture, flows,
and decision rationale, read
[`apps/desktop/src-tauri/src/mcp/DETAILS.md`](../../apps/desktop/src-tauri/src/mcp/DETAILS.md); for the must-know
invariants, its [`CLAUDE.md`](../../apps/desktop/src-tauri/src/mcp/CLAUDE.md) and the executor's
[`CLAUDE.md`](../../apps/desktop/src-tauri/src/mcp/executor/CLAUDE.md).

## Architecture

The MCP server lives under `apps/desktop/src-tauri/src/mcp/`:

```
mcp/
├── mod.rs            # Module wiring and exports
├── config.rs         # Port / bind configuration
├── server.rs         # HTTP server (axum), bind/rebind lifecycle, request dispatch
├── auth.rs           # Bearer-token lifecycle + per-request validation (reads the gate)
├── protocol.rs       # JSON-RPC message handling
├── tool_registry.rs  # THE single source: one authored table → tools/list, dispatch, and auth gate
├── tools.rs          # Thin shim: the `Tool` struct + re-export of `get_all_tools`
├── executor/         # Tool handlers, grouped by category (app.rs, nav.rs, file_ops.rs, …)
├── resources/        # Read-only resources (cmdr://state, logs, indexing, settings)
├── pane_state.rs     # Frontend → backend state sync store
└── tests/            # Test suite, split by category
```

Every tool is authored **exactly once** in the `mcp_tools!` table in `tool_registry.rs`. That one table generates all
three consumers, so they can't drift:

- `get_all_tools()` — the `tools/list` payload.
- `execute_tool()` — the `tools/call` dispatch (generic over `Runtime`).
- `tool_gate()` + `TokenGate` — the bearer-token classification `auth.rs` reads.

### Data flow

1. **HTTP request** → `server.rs` receives the JSON-RPC request.
2. **Parsing** → `protocol.rs` parses and validates it.
3. **Auth** → for `tools/call`, `auth::tool_call_requires_token` looks up the tool's `TokenGate` and requires the bearer
   token only for calls that bypass the user's confirmation dialog.
4. **Routing** → `tools/call` dispatches through the generated `execute_tool()` in `tool_registry.rs`.
5. **Execution** → the handler in the matching `executor/` category file emits a Tauri event (or queries state) and
   waits for the frontend ack before returning `OK`.
6. **Frontend** → `mcp-listeners.ts` validate-parses the event payload and dispatches on the typed command bus, then
   syncs state back to `pane_state.rs`.

## Adding a new tool

Two edits: one registry entry, and one handler function.

### Step 1: add the handler to the right `executor/` category file

Handlers take `&Value` and do their own extraction/validation. Pick the category file that fits (`nav.rs`,
`file_ops.rs`, `view.rs`, `app.rs`, `dialogs.rs`, `search.rs`, `async_tools.rs`, `downloads.rs`) — or add a new one and
declare it `pub(crate) mod …` in `executor/mod.rs` so the generated dispatch can reach it.

```rust
// in executor/view.rs
pub async fn execute_my_action<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let target = params
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'target'"))?;

    // Emit an event and wait for the frontend ack (see executor/CLAUDE.md for the ack contract).
    mcp_round_trip(app, "mcp-my-action", json!({ "target": target }), format!("OK: did {target}")).await
}
```

Follow the executor must-knows: read path params through `user_path_param`, choose the right ack (`wait_for_ack` vs
`mcp_round_trip`), and never return `OK` without waiting for the ack.

### Step 2: add ONE entry to the `mcp_tools!` table in `tool_registry.rs`

```rust
"my_action" => {
    desc: "Do the thing to a target",
    schema: json!({
        "type": "object",
        "properties": {
            "target": { "type": "string", "description": "What to act on" }
        },
        "required": ["target"]
    }),
    gate: TokenGate::Open,
    run: app_params view::execute_my_action
},
```

Each entry bundles every facet: name, description, JSON schema, the bearer-token `gate`, and the handler (as a shape tag
plus a path). You can't add an entry without all five, and you can't add a handler the dispatch doesn't know about, so
schema/dispatch/auth can't fall out of sync.

- **`gate`** classifies the tool for the bearer token: `Open` (no token — reads, nav, search, and destructive ops that
  still prompt the user), `Always` (always gated — config mutation with no confirmation, like `set_setting`),
  `IfAutoConfirm` (gated when `arguments.autoConfirm == true`, like `copy`/`move`/`delete`), or `IfConfirmAction` (gated
  when `arguments.action == "confirm"`, like `dialog`). A structural test fails if a tool exposing `autoConfirm` is left
  `Open`, so a destructive tool can't ship ungated by accident.
- **`run`** is a shape tag then the handler path. The tag tells the generated dispatch how to call the handler:
  `app_params` (`handler(app, params).await`, most tools), `app_only` (`handler(app).await`, no params), `params_only`
  (`handler(params).await`, no `app` — `search`, `ai_search`), `sync_app` / `sync_app_params` (sync handlers, no
  `.await`), and `nav` / `nav_params` (the nav family, which routes several tools through one handler by passing the
  tool name). See the macro doc comment in `tool_registry.rs` for the full list.
- **Schema keys** serialize alphabetically (serde_json `Map` is a `BTreeMap`), so authored key order doesn't affect the
  wire bytes. A tools/list snapshot test (`tests/tool_snapshot_tests.rs`) pins the exact output; run it after any schema
  edit and update the fixture when the change is intentional.

That's the whole change. No separate dispatch match, no auth string-list, and no hand-bumped tool count — the count and
coverage tests are cheap guards over a property that's now true by construction.

### Step 3: handle the event in the frontend (if the tool emits one)

If the handler emits a new Tauri event, add a parser plus dispatch for it in the frontend's MCP transport adapter,
`apps/desktop/src/routes/(main)/mcp-listeners.ts` (`setupMcpListeners`). That adapter validate-parses each `mcp-*`
payload into a typed command and dispatches on the command bus. No business logic lives there.

### Step 4: add tests

Look tools up by name in `get_all_tools()`; there are no per-category list functions. Schema-shape tests live inline in
`tool_registry.rs`; cross-cutting checks (name charset, no `fs.`/`shell.` tools) live in `tests/`.

```rust
#[test]
fn test_my_action_schema() {
    let tools = get_all_tools();
    let t = tools.iter().find(|t| t.name == "my_action").unwrap();
    assert!(t.input_schema["properties"].get("target").is_some());
    assert_eq!(tool_gate("my_action"), Some(TokenGate::Open));
}
```

## Testing locally

### Run unit tests

```bash
pnpm check rust     # or, scoped tighter, from apps/desktop/src-tauri: cargo test mcp::
```

Use `pnpm check` (cache-aware) rather than raw `cargo test`; see
[`scripts/check/CLAUDE.md`](../../scripts/check/CLAUDE.md).

### Manual testing with curl

```bash
# Start the app
pnpm dev

# In another terminal. The MCP port is ephemeral per instance; read it from the data dir:
PORT=$(cat ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/mcp.port)

# List all tools (ungated)
curl -X POST "http://127.0.0.1:${PORT}/mcp" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Call a tool (ungated)
curl -X POST "http://127.0.0.1:${PORT}/mcp" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nav_to_parent","arguments":{}}}'
```

Gated calls (auto-confirm `delete`/`move`/`copy`, `dialog` confirm, `set_setting`) need the bearer token from
`<data_dir>/mcp.token` (or `CMDR_MCP_TOKEN`) as an `Authorization: Bearer <token>` header. See
[`mcp/DETAILS.md`](../../apps/desktop/src-tauri/src/mcp/DETAILS.md) § Authentication.

## Security guidelines

When adding new tools, follow these principles:

1. **Match user capabilities**: tools do only what a user can do through the UI.
2. **No direct file access**: don't add `fs.read`, `fs.write`, or similar.
3. **No shell execution**: don't add `shell.run` or similar.
4. **Validate all inputs**: check types, bounds, and sanitize strings.
5. **Use events, not direct calls**: emit events the frontend handles.
6. **Gate destructive shortcuts**: any tool that bypasses the user's confirmation dialog gets a `TokenGate` other than
   `Open`.

A `tests/security_tests.rs` check enforces the no-`fs.`/`shell.` rule and name charset for every tool in
`get_all_tools()`, so a new tool is covered automatically.

## Common handler patterns

### Round-trip tools (the backend can't fully validate the precondition)

```rust
mcp_round_trip(app, "mcp-nav-to-path", json!({ "pane": pane, "path": path }), format!("OK: navigated to {path}")).await
```

Emits an event with a `requestId` and waits for the frontend's `mcp-response`. Use when the frontend is the authority
(navigation, cursor moves, selection).

### Fire-and-forget action tools (wait on a state ack)

```rust
let gen = snapshot_generation(app);
app.emit("mcp-toggle-hidden", json!({}))?;
wait_for_ack(app, AckSignal::GenerationAdvanced(gen), DEFAULT_ACK_TIMEOUT).await?;
Ok(json!("OK"))
```

`OK` means "the frontend accepted the dispatched action," not "the operation completed." See
[`executor/CLAUDE.md`](../../apps/desktop/src-tauri/src/mcp/executor/CLAUDE.md) for the ack contract.

## Troubleshooting development

### Compilation errors

```bash
cd apps/desktop/src-tauri && cargo check
```

### Tests failing

```bash
cargo test mcp::tool_registry::tests::test_my_action_schema -- --nocapture
```

### Events not reaching the frontend

1. Check the event name matches exactly between the handler and `mcp-listeners.ts`.
2. Verify the parser is registered in `setupMcpListeners()`.
3. Check the browser devtools console in dev mode.

### tools/list snapshot mismatch

`tests/tool_snapshot_tests.rs` pins the exact `tools/list` bytes. A mismatch means a schema, name, description, or the
tool set changed. If the change is intentional, update the committed fixture; otherwise you drifted the wire output.
</content>
