# Port-discovery files use default (world-readable) permissions

**Severity:** low
**Lens:** F — Security
**Confidence:** medium

## Location

- `apps/desktop/src-tauri/src/mcp/port_file.rs` (Cmdr MCP `mcp.port`)
- `apps/desktop/scripts/instance-id.js` and `tauri-wrapper.js` (Tauri MCP bridge `tauri-mcp.port`)
- Path: `<CMDR_DATA_DIR>/mcp.port`, `<CMDR_DATA_DIR>/tauri-mcp.port`. Prod is `~/Library/Application Support/com.veszelovszki.cmdr/`.

## What

The port files are written with the default `umask`-derived permissions (typically `0o644` on macOS). Any local process running as the user can read them and discover the ephemeral MCP port.

By itself this is fine — the data dir is per-user. The real concern is the broader threat model: macOS allows any non-sandboxed app the user launches to read files under `~/Library/Application Support/com.veszelovszki.cmdr/`. There's no per-app ACL on user-owned files outside `~/Documents`, `~/Downloads`, etc. (the TCC-protected paths).

This isn't a bug on its own; combined with the MCP findings (`high-F-mcp-destructive-ops-no-origin.md`), discovering the port is step 1 in the attack chain. Tightening permissions to `0o600` raises the bar slightly (a process would need to also be running as the user, which on macOS is essentially every non-sandboxed user-launched app, so the bar moves by inches).

## Why it matters

Marginal. Filed only because:

- It pairs naturally with adding a per-instance bearer token next to `mcp.port` (see the high-F MCP note). The token MUST be `0o600`. May as well do the port file too.
- "Defense in depth" — costs nothing.

## Suggested fix

If implementing the MCP token suggestion:

1. After `write_port_file` succeeds, `fs::set_permissions(path, Permissions::from_mode(0o600))`.
2. Same for the tauri-MCP port file in `tauri-wrapper.js`: pass `{ mode: 0o600 }` to `writeFileSync`.
3. The new `mcp.token` file: `0o600` mandatory.

If not implementing the token: skip this. The port number alone isn't load-bearing.

## Notes

- Linux file mode applies cleanly; macOS too. Windows we're not on yet, so no cross-platform mess.
- The dev/per-worktree data dirs share the same shape; same fix applies.
