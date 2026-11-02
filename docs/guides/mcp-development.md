# MCP server developer guide

This guide explains how to extend the Cmdr MCP server with new tools and test your changes. For the architecture, flows,
and decision rationale, read `apps/desktop/src-tauri/src/tools/DETAILS.md` (the tools) and
`apps/desktop/src-tauri/src/mcp/DETAILS.md` (the transport); for the must-know invariants, their `CLAUDE.md` files and
`apps/desktop/src-tauri/src/tools/actions/CLAUDE.md`.

## Architecture

The transport lives under `apps/desktop/src-tauri/src/mcp/`, and the tools under `apps/desktop/src-tauri/src/tools/`:

```
mcp/
├── mod.rs            # Module wiring and exports
├── config.rs         # Port / bind configuration
├── server.rs         # HTTP server (axum), bind/rebind lifecycle, the route table
├── auth.rs           # Bearer-token lifecycle + the guard in front of the SDK
├── handler.rs        # What the official SDK (rmcp) calls: tools, resources, server info
└── tests/            # Transport test suite, split by category

tools/
├── table.rs          # THE single source: one authored table → `tools/list` and the dispatch
├── registry.rs       #   the `mcp_tools!` macro, the `Tool` struct, `Access` and `Load`
├── policy.rs         #   `admit`, the one decision over who may run what
├── params.rs         #   the schema gate
├── schemas/          #   per-tool `fn <tool>_schema()` blocks, grouped by category
├── actions/          # Tool handlers, grouped by category (app.rs, nav.rs, file_ops.rs, …)
├── resources/        # Read-only resources (cmdr://state, logs, indexing, settings)
└── tests/            # Tool and resource test suite, split by category

ui_state/             # Frontend → backend state sync stores (pane_state.rs and friends)
```

Every tool is authored **exactly once** in the `mcp_tools!` table in `tools/table.rs`. That one table generates
everything else, so nothing can drift:

- `get_all_tools()`: the whole table but `Meta` entries, the `tools/list` payload.
- `execute_tool()`: the dispatch (generic over `Runtime`), which asks `policy::admit` first, for both callers.
- `tool_access()`: what `admit` reads about a tool, and all it reads.

An outside client and the in-process agent share this one table (agent decision D49: extend it, don't fork a parallel
agent table), and each entry declares only what the tool does. See `apps/desktop/src-tauri/src/tools/DETAILS.md` § The
rule.

### Data flow

1. **HTTP request** → `auth::guard` validates Origin.
2. **Auth** → every `/mcp` request needs the bearer token before the SDK sees it.
3. **Parsing** → `rmcp` parses and validates the authenticated request and calls `handler.rs`.
4. **Routing** → `tools/call` dispatches through the generated `execute_tool()` (the macro in `tools/registry.rs`
   expands it over `table.rs`).
5. **Execution** → the handler in the matching `tools/actions/` category file emits a Tauri event (or queries state) and
   waits for the frontend ack before returning `OK`.
6. **Frontend** → `mcp-listeners.ts` validate-parses the event payload and dispatches on the typed command bus, then
   syncs state back to `pane_state.rs`.

## Adding a new tool

The full list, with #871's sync as the worked example, is `apps/desktop/src-tauri/src/tools/DETAILS.md` § Adding a tool.
The how-to for the two parts you write:

### Step 1: add the handler to the right `tools/actions/` category file

Handlers take `&Value` and do their own extraction/validation. Pick the category file that fits (`nav.rs`,
`file_ops.rs`, `view.rs`, `app.rs`, `dialogs.rs`, `search.rs`, `async_tools.rs`, `downloads.rs`) — or add a new one and
declare it `pub(crate) mod …` in `tools/actions/mod.rs` so the generated dispatch can reach it.

```rust
// in tools/actions/view.rs
pub async fn execute_my_action<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let target = params
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'target'"))?;

    // Emit an event and wait for the frontend ack (see tools/actions/CLAUDE.md for the ack contract).
    mcp_round_trip(app, "mcp-my-action", json!({ "target": target }), format!("OK: did {target}")).await
}
```

Follow the `tools/actions/CLAUDE.md` must-knows: read path params through `user_path_param`, choose the right ack
(`wait_for_ack` vs `mcp_round_trip`), and never return `OK` without waiting for the ack.

### Step 2: add the schema to `schemas/` and ONE entry to the `mcp_tools!` table in `tools/table.rs`

Add a `fn <tool>_schema() -> Value` to the matching `schemas/<category>.rs` (a no-parameter tool reuses
`schemas::no_params_schema()`):

```rust
// in tools/schemas/view.rs
pub fn my_action_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": { "type": "string", "description": "What to act on" }
        },
        "required": ["target"],
        "additionalProperties": false
    })
}
```

Then the table entry:

```rust
"my_action" => {
    id: MyAction,
    brief: "Do the thing to a target.",
    desc: "Do the thing to a target",
    schema: schemas::my_action_schema(),
    subject: SubjectSpec::None,
    access: Access::AppState,
    load: Load::Deferred,
    run: app_params view::execute_my_action
},
```

Each entry bundles every facet: name, `ToolId` variant, brief, description, JSON schema, the rail line's subject, the
`access` class, the `load`, and the handler (as a shape tag plus a path). You can't add an entry without all of them,
and you can't add a handler the dispatch doesn't know about, so schema and dispatch can't fall out of sync. HTTP
authentication sits above the registry and automatically covers every entry.

- **`brief`** is one line, at most 100 characters, and PREFIX for a `Deferred` tool: the prompt lists it on every call.
  It describes what the tool does, and names no parameter.
- **`desc`** is PREFIX for a `Core` tool, whose declaration rides every call, so a sentence here is paid thousands of
  times. Say what the tool answers and, when a neighboring tool would be the wrong pick, where the line falls (`search`
  vs `list_dir`); leave everything else to the schema, the result, or the system prompt.
  `test_tools_have_bounded_descriptions` caps a description at 512 chars, and most sit far under;
  `agent/chat/context/cost_tests.rs` is what actually holds the prefix to `FIXED_PROMPT_OVERHEAD_TOKENS`.
- **`access`** says what the tool does, and `admit` decides from it alone who may run it, so pick the class honestly:
  `Read`, `ReadContents`, `Propose`, `Memory`, `Session`, `AppState`, `Initiate`, `LightWrite`, `Sensitive` (it needs an
  `ask:` plan the table entry names, and a person confirms before it runs), or `DevOnly("one-line reason")`. Every class
  but `Read` and `AppState` means adding the name to its hand-written allowlist in `tool_registry_tests/access.rs`,
  having read the handler. Split a tool whose power depends on a parameter.
- **`load`** is `Core` when an everyday flow or the system prompt names the tool, else `Deferred`.
- **`run`** is a shape tag then the handler path. The tag tells the generated dispatch how to call the handler:
  `app_params` (`handler(app, params).await`, most tools), `app_only` (`handler(app).await`, no params), `ctx_params`
  (`handler(app, ctx, params).await`, for a handler that needs the caller or a dialog opener's stamp), `params_only`
  (`handler(params).await`, no `app` — `search`, `ai_search`), `sync_app` / `sync_app_params` (sync handlers, no
  `.await`), and `nav` / `nav_params` (the nav family, which routes several tools through one handler by passing the
  tool name). See the macro doc comment in `tools/registry.rs` for the full list.
- **Schema keys** serialize alphabetically (serde_json `Map` is a `BTreeMap`), so authored key order doesn't affect the
  wire bytes. A tools/list snapshot test (`tools/tests/tool_snapshot_tests.rs`) pins the exact output; run it after any
  schema edit and update the fixture when the change is intentional.
- **The schema is ENFORCED, so author it as the truth.** `validate_params` refuses a call whose params miss a `required`
  property, or one carrying a property `properties` doesn't list, before your handler runs. Don't mark a field
  `required` the handler is happy without. Every object in every schema MUST declare `additionalProperties: false` (a
  structural test fails otherwise): a swallowed argument becomes a confident wrong answer. The gate reads one level, so
  a nested object checks itself. `apps/desktop/src-tauri/src/tools/DETAILS.md` § The schema gate.

No separate dispatch match, and nothing else names the tool in Rust unless its `subject` is `Custom`.

### Step 3: handle the event in the frontend (if the tool emits one)

If the handler emits a new Tauri event, add a parser plus dispatch for it in the frontend's MCP transport adapter,
`apps/desktop/src/routes/(main)/mcp-listeners.ts` (`setupMcpListeners`). That adapter validate-parses each `mcp-*`
payload into a typed command and dispatches on the command bus. No business logic lives there. Add the event's name to
`FRONTEND_EVENTS` in `agent/chat/runtime/capability_tests.rs`, and a dialog opener's payload goes through `ctx.stamp`.

### Step 4: add tests

Look tools up by name in `get_all_tools()`; there are no per-category list functions. Schema-shape tests live in
`tools/tests/tool_registry_tests/` (beside the other suites, so the authored `mcp_tools!` table stays a lean,
single-purpose source); cross-cutting checks (name charset, no `fs.`/`shell.` tools) live in `tests/security_tests.rs`.

```rust
#[test]
fn test_my_action_schema() {
    let tools = get_all_tools();
    let t = tools.iter().find(|t| t.name == "my_action").unwrap();
    assert!(t.input_schema["properties"].get("target").is_some());
}
```

## Testing locally

### Run unit tests

```bash
pnpm check rust     # or, scoped tighter, from apps/desktop/src-tauri: cargo test mcp::
```

Use `pnpm check` (cache-aware) rather than raw `cargo test`; see `scripts/check/CLAUDE.md`.

### Manual testing with curl

```bash
# Start the app
pnpm dev

# In another terminal. The MCP port is ephemeral per instance; read it from the data dir:
PORT=$(cat ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/mcp.port)
TOKEN=$(cat ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/mcp.token)

# List all tools
curl -X POST "http://127.0.0.1:${PORT}/mcp" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Call a tool
curl -X POST "http://127.0.0.1:${PORT}/mcp" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nav_to_parent","arguments":{}}}'
```

Every `/mcp` request needs the bearer token from `<data_dir>/mcp.token` (or `CMDR_MCP_TOKEN`) as an
`Authorization: Bearer <token>` header. See `apps/desktop/src-tauri/src/mcp/DETAILS.md` § Authentication.

## Security guidelines

When adding new tools, follow these principles:

1. **Match user capabilities**: tools do only what a user can do through the UI.
2. **No raw file access**: don't add `fs.read`, `fs.write`, or similar. `inspect_file` is the bounded way in, and
   anything yielding file contents passes `tools/sensitive.rs`.
3. **No shell execution**: don't add `shell.run` or similar.
4. **Validate all inputs**: check types, bounds, and sanitize strings.
5. **Use events, not direct calls**: emit events the frontend handles.
6. **No way past a person in a schema**: a tool opens a dialog (`Initiate`) or asks (`Sensitive`). `dialog_confirm` is
   the one way past a person, and it's `DevOnly`; transport auth does not replace action-level intent.

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
`apps/desktop/src-tauri/src/tools/actions/CLAUDE.md` for the ack contract.

## Troubleshooting development

### Compilation errors

```bash
cd apps/desktop/src-tauri && cargo check
```

### Tests failing

```bash
cargo test tools::tests::tool_registry_tests::schemas::test_my_action_schema -- --nocapture
```

### Events not reaching the frontend

1. Check the event name matches exactly between the handler and `mcp-listeners.ts`.
2. Verify the parser is registered in `setupMcpListeners()`.
3. Check the browser devtools console in dev mode.

### tools/list snapshot mismatch

`tools/tests/tool_snapshot_tests.rs` pins the exact `tools/list` bytes. A mismatch means a schema, name, description, or
the tool set changed. If the change is intentional, regenerate the fixture with `UPDATE_SNAPSHOT=1` and read the diff;
otherwise you drifted the wire output.
