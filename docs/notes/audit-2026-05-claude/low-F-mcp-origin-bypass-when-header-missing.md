# MCP origin validation bypassed when Origin header absent

**Severity:** low **Lens:** F — Security **Confidence:** high

## Location

`apps/desktop/src-tauri/src/mcp/server.rs:278-304` (`validate_origin`), comment at line 302-303

## What

The Origin-header check is the documented DNS-rebinding mitigation for the MCP server (`mcp/CLAUDE.md` § "Why localhost
only?"). But the code allows any request that arrives **without** an Origin header to pass through unchecked. The
comment is explicit: "If no Origin header, allow (non-browser clients typically don't send it)." Combined with
`CorsLayer::new().allow_origin(Any)` at line 168, the server treats no-Origin as trusted.

## Why it matters

The DNS-rebinding attack works by getting a malicious page to make requests to `127.0.0.1:<mcp_port>` after a DNS
rebind. Modern browsers send Origin on `fetch` and `XMLHttpRequest`, so the check fires there. But:

- A `<form method="POST" action="http://127.0.0.1:PORT/mcp">` POST in older browsers sends no Origin header. The MCP
  server accepts the body as JSON-RPC, parses it, dispatches a tool. Tools include `quit`, `delete`, `nav_to_path`,
  `move`. Damage potential is high.
- A `<form enctype="text/plain">` POST in any browser can craft a valid-looking JSON-RPC body without an Origin header
  in some configurations.
- Local malware that wants to control the file manager doesn't need any of this — it can hit the loopback port directly.
  But that's not the threat model the Origin check addresses; the Origin check exists for cross-origin browser attacks.

The likelihood of a successful exploit is low because the attacker also needs to know the (ephemeral, per-instance) port
— which they can't from inside a sandboxed browser tab. But the port is sometimes pinned (`developer.mcpPort = 19224`),
and the `mcp.port` file is world-readable for any local process. Worth tightening on principle.

## Evidence

```rust
pub fn validate_origin(headers: &HeaderMap) -> Result<(), Box<Response>> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        // ... allow null, tauri://, localhost; reject everything else
    }
    // If no Origin header, allow (non-browser clients typically don't send it)
    Ok(())
}
```

## Suggested fix

Require Origin to be present on every state-changing request (`POST /mcp` with non-notification methods). Two practical
shapes:

1. Strict: reject every POST without an Origin header except `tools/list`, `resources/list`, `ping`, `initialize` (the
   discovery surface). The CLI helper `scripts/mcp-call.sh` is the only non-browser client today; it can be made to send
   `Origin: tauri://localhost`.
2. Defense-in-depth: require either `Origin` or a custom `X-MCP-Origin: cmdr` header. The custom header isn't settable
   from `<form>` posts in any browser, so it acts as a CSRF token equivalent without sessionful state.

Either approach closes the form-POST hole. Option 1 is cleaner.

`CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)` is also broader than needed — narrow
`allow_origin` to the same allowlist the Origin validator uses, so preflight responses don't paper over the validation.

## Notes

The spec_compliance_tests in `mcp/tests/spec_compliance_tests.rs` should pin the new behavior. Existing
`test_validate_origin_no_header` at `server.rs:730` would need to flip its assertion.
