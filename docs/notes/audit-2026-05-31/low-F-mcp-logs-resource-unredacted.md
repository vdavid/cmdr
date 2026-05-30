# MCP `cmdr://logs` resource serves the raw, unredacted log tail without the bearer token

**Severity:** low **Lens:** F — Security **Confidence:** high

## Location

`apps/desktop/src-tauri/src/mcp/resources.rs:637-674` (`read_log_tail`); gate at
`apps/desktop/src-tauri/src/mcp/server.rs:400-417` (`tool_call_requires_token`)

## What

When the MCP server is enabled, the `cmdr://logs` resource returns the live `cmdr.log` tail verbatim. The reader does no
redaction (`grep redact` across `mcp/` returns nothing), and resource reads are never token-gated:
`tool_call_requires_token` returns `false` for any `method != "tools/call"`, so `resources/read` always passes. The same
paths / hostnames / IPs / emails / SMB URIs that the `redact` module exists to scrub from error-report and crash bundles
ship in the clear here.

## Why it matters

`mcp/CLAUDE.md` is explicit that the loopback binding is not a security boundary against a local non-Cmdr process. A
process that can reach the MCP port but is otherwise filesystem-sandboxed (or an SSRF-style vector that coaxes some
other component into making loopback requests) can `resources/read cmdr://logs` and exfiltrate the user's home path,
mounted volume labels, SMB hostnames + share names, `*.local` device names, and any emails that hit the log — bypassing
the redactor that every other log consumer honors.

The reason this is **low**, not medium: the log file lives in the user's own `~/Library/Logs/…`, so any same-user local
process can already read it directly without MCP. MCP only widens the surface for a caller that has loopback reach but
_not_ filesystem read — a narrow case. It's also gated by the MCP server defaulting to **off** (`developer.mcpEnabled`
default `false`).

## Evidence

```rust
// resources.rs:637 — no redaction anywhere in this function
fn read_log_tail(opts: &LogOptions) -> Result<String, String> {
    let log_path = log_dir.join("cmdr.log");
    let mut file = std::fs::File::open(&log_path) ...;
    file.read_to_end(&mut buf) ...;
    let text = String::from_utf8_lossy(&buf);
    ...
    Ok(lines[start..].join("\n"))   // raw lines straight out
}
```

```rust
// server.rs:400 — resources/read is never gated
pub fn tool_call_requires_token(method: &str, params: &Value) -> bool {
    if method != "tools/call" { return false; }   // <- resources/read short-circuits to false
    ...
    match name { "delete" | "move" | "copy" => ...autoConfirm..., "dialog" => ...confirm..., _ => false }
}
```

## Suggested fix

Run each line of the `cmdr://logs` payload through `crate::redact::redact_line` before returning it (the redactor is
already a per-line `Cow`-returning hot path built for exactly this). That preserves the resource's debugging value while
closing the cleartext-PII path and matches the contract the error/crash reporters already honor. Cheap and consistent.

## Notes

- `redact/CLAUDE.md` and `docs/security.md` describe redaction as covering the crash reporter and the error reporter;
  the MCP logs resource is a third consumer of the same log data that was left out.
- A sibling, even lower-severity gap exists on `cmdr://state` (`resources.rs`): it exposes both panes' full file
  listings + `recentErrors[].path/message` ungated. The "reads mirror the UI" rationale mostly covers it, but
  `recentErrors` can carry SMB URIs / home paths from failed listings the user never saw rendered — consider redacting
  those fields too.
