# MCP server allows destructive ops from any local process without Origin

**Severity:** high
**Lens:** F — Security
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/mcp/server.rs` (`validate_origin`, lines ~276–304)
- `apps/desktop/src-tauri/src/mcp/executor/file_ops.rs` (`execute_delete`, `execute_copy`, `execute_move` — `autoConfirm` branches)

## What

The MCP HTTP server binds `127.0.0.1` (good), but `validate_origin` deliberately allows requests with **no `Origin` header at all** ("non-browser clients typically don't send it", line 302). Any local process — including another app sandboxed at the user level, a curl one-liner from a malicious shell script, or a compromised dev-tool — can hit `POST /mcp` with no Origin and call any tool.

Combined with the `autoConfirm: true` parameter on `copy` / `move` / `delete` (and the unified `dialog` tool's `action: "confirm"`), a local attacker can:

1. Read the live `<CMDR_DATA_DIR>/mcp.port` file (file is world-readable on macOS by default).
2. POST `tools/call` with `delete { autoConfirm: true }` — Cmdr deletes the currently-selected files with no user dialog.
3. Or POST `move` with `autoConfirm: true` + `onConflict: "overwrite_all"` — moves into the inactive pane, overwriting destination files.

The CLAUDE.md doc claims security via "parity with user," but a real user faces a confirmation dialog; `autoConfirm` removes it. That breaks parity.

## Why it matters

Cmdr ships with FDA (Full Disk Access). The MCP server, by design, exposes the same destructive surface Cmdr itself has — to anything that can reach loopback. macOS does NOT isolate processes from each other on `127.0.0.1`: any unsandboxed app on the user's machine (including any Electron app, any Homebrew CLI, anything in `~/Downloads` the user just launched) can connect.

This is the classic "MCP localhost server" footgun. The MCP spec explicitly recommends Origin validation specifically to mitigate it; `validate_origin` reads correctly at first glance but the no-Origin bypass undoes the protection. A curl call without `--header "Origin: ..."` succeeds.

The port-discovery file `<data_dir>/mcp.port` is per-instance but the prod data dir is in a predictable path (`~/Library/Application Support/com.veszelovszki.cmdr/`). Any local process can read it.

## Evidence

`server.rs` lines 302–303:

```rust
// If no Origin header, allow (non-browser clients typically don't send it)
Ok(())
```

`file_ops.rs` lines 103–116:

```rust
pub async fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let pre_gen = snapshot_generation(app);
    app.emit("mcp-delete", json!({"autoConfirm": auto_confirm}))?;
    if auto_confirm {
        // ...waits only for pane generation to advance; no user dialog
        Ok(json!("OK: Delete started with auto-confirm."))
```

Frontend wiring (`DeleteDialog.svelte` line 162 and `TransferDialog.svelte` line 376): the dialog auto-confirms without user interaction when `autoConfirm` is true.

The MCP server settings (`developer.mcpEnabled`) defaults — worth verifying separately whether the server starts by default or requires opt-in. Even if opt-in, users who enable MCP for legitimate use don't expect "any local process" exposure.

Reproduction (with MCP running):

```bash
PORT=$(cat ~/Library/Application\ Support/com.veszelovszki.cmdr/mcp.port)
curl -sS -X POST "http://127.0.0.1:$PORT/mcp" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete","arguments":{"autoConfirm":true}}}'
```

## Suggested fix

Two independent hardening moves (apply both):

1. **Require Origin** for non-loopback-tooling paths. Drop the "no Origin → allow" branch. For genuine CLI tooling (the bundled `scripts/mcp-call.sh`), add a per-instance bearer token written next to `mcp.port` (mode 0600, owner-only) and require either a valid `Origin: tauri://localhost` / `http://localhost*` OR a matching `Authorization: Bearer <token>`. Tooling reads the token from `<data_dir>/mcp.token`.

2. **Refuse `autoConfirm: true` for destructive tools** (`delete`, `move`, `copy` with `overwrite_all`) unless the request carries the bearer token above. Or simpler: drop `autoConfirm` from the public tool surface entirely. The agent's job is "drive the UI like a user"; auto-confirming destructive dialogs isn't UI parity, it's privilege escalation.

Also tighten `<data_dir>/mcp.port` and `mcp.token` permissions to `0o600` and verify on read.

## Notes

- The Tauri MCP bridge (separate `tauri-mcp.port`) has the same Origin shape; check it too.
- `connect_to_server` / `nav_to_path` accept arbitrary input — lower severity but the same surface (a local attacker can navigate Cmdr into a network share and observe what's there). Token-gating the whole MCP server fixes this transitively.
- `set_setting` MCP tool can change `developer.mcpPort` and `developer.mcpEnabled`. A token gate also stops "attacker disables MCP after exfil to cover tracks."
- The existing CLAUDE.md note "Why localhost only?" reads the threat as "remote attacker over LAN." The real threat is "local non-Cmdr process." Update the doc when fixing.
