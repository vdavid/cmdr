# MCP `set_setting` mutates any setting without the bearer token, including diagnostic opt-ins

**Severity:** low **Lens:** F — Security **Confidence:** high

## Location

`apps/desktop/src-tauri/src/mcp/executor/async_tools.rs:209-227` (`execute_set_setting`); gate at
`apps/desktop/src-tauri/src/mcp/server.rs:400-417`

## What

The `set_setting` MCP tool round-trips an arbitrary `settingId` + `value` to the frontend, which applies it. It is not
in the token-gated set (only `delete` / `move` / `copy` with `autoConfirm`, and `dialog confirm`, require the token). So
an unauthenticated local process can change any registry setting, including `updates.errorReports` (enable auto-send of
diagnostic bundles to the maintainer), `network.directSmbConnection`, or `developer.mcpPort`.

## Why it matters

The stated MCP threat model is "a local non-Cmdr process." Such a process can't directly delete files without the token,
but it can silently flip `updates.errorReports` on (so the app starts shipping log bundles on the next error), or change
network/SMB behavior — none of which prompts the user. It's a config-tampering primitive, not a data-loss one, hence low
severity, but it's inconsistent with the "only the auto-confirm bypass is gated" framing: changing a setting also
bypasses any user confirmation.

## Evidence

```rust
// async_tools.rs:210 — no token check, no allowlist of settings
pub async fn execute_set_setting<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let id = params.get("id").and_then(|v| v.as_str()) ...;
    let value = params.get("value") ...;
    mcp_round_trip(app, "mcp-set-setting",
        json!({"settingId": id, "value": value}),
        format!("OK: Set '{id}' to {value}")).await
}
```

```rust
// server.rs:400 — set_setting falls into the `_ => false` arm (no token)
match name {
    "delete" | "move" | "copy" => ...autoConfirm...,
    "dialog" => ...action == "confirm",
    _ => false,   // set_setting, resource reads, nav, search -> no token
}
```

## Suggested fix

Either add `set_setting` to `tool_call_requires_token` (treat config mutation as a confirmation-bypass, the same class
as auto-confirm), or maintain a small denylist of security-relevant setting IDs (`updates.errorReports`,
`updates.crashReports`, `network.*`, `developer.mcp*`) that `set_setting` refuses to change over MCP. Gating the whole
tool behind the token is the cleaner, more future-proof choice and matches the predicate's existing shape.

## Notes

- `mcp/CLAUDE.md` § Authentication frames the gate as protecting "calls that bypass the user's in-app confirmation
  dialog." `set_setting` also bypasses confirmation, so by that same logic it belongs in the gated set; it was simply
  scoped to destructive file ops. Bounded by the MCP-off-by-default control (`developer.mcpEnabled` default `false`).
