# MCP server

Carries `crate::tools` to outside AI clients over the Model Context Protocol: Streamable HTTP, `127.0.0.1` only,
ephemeral port by default. The tools themselves, their table, and the `cmdr://` resources: `../tools/CLAUDE.md`. Adding
or changing tools: `docs/guides/mcp-development.md`.

The official SDK (`rmcp`) speaks the protocol. `server.rs` binds and holds the route table, `handler.rs` is what `rmcp`
calls, `auth.rs` owns tokens and the guard in front of `rmcp` (one-directional: `server` uses `auth`, never the reverse),
`port_file.rs` writes the discovery files, and `safe_headers.rs` keeps the token out of logs.

## Must-knows

- **Transport only.** ❌ `mcp` imports no `agent`, enforced by `tools-module-boundary`.
- **Auth covers every `/mcp` request**: `auth::guard` runs before `rmcp` sees it, so nothing reaches the SDK on a bad
  token, not even to be told it's malformed. Only the minimal health endpoint and data-free CORS preflight stay open.
  Ordinary request rejection is an in-band JSON-RPC error at HTTP 200, ❌ never a 401 (that sends clients into OAuth
  discovery); anything with no id to echo gets an empty 403. Never echo the token.
- **❌ `rmcp` writes nothing to any log**: it logs request arguments at Debug, `unlock` passwords included, so
  `logging::dispatch::NEVER_LOGGED` drops it above both chains. Our own lines live in `handler.rs` and `auth.rs`.
- **Every reply passes `handler.rs`'s `show_strings`, BEFORE a result renders to text**, or a twin's NUL pick reaches a
  client as `\u0000`.
- **`transport_characterization.rs` must survive any `rmcp` upgrade unedited**; `transport_sdk_answers.rs` may move, so
  read its diff.
- **Interactive rebinds bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`), so a busy port drops no
  in-flight request; startup uses `ProbeOnCollision`. Live MCP control only works from the settings window.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
