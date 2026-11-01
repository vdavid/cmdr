# MCP server

Carries `crate::tools` to outside AI clients over the Model Context Protocol: Streamable HTTP, `127.0.0.1` only,
ephemeral port by default. The tools themselves, their table, and the `cmdr://` resources: `../tools/CLAUDE.md`. Adding
or changing tools: `docs/guides/mcp-development.md`.

`server.rs` binds and dispatches, `auth.rs` owns tokens (one-directional: `server` uses `auth`, never the reverse),
`protocol.rs` is the JSON-RPC envelope, `port_file.rs` writes the discovery files, and `safe_headers.rs` keeps the token
out of logs.

## Must-knows

- **Transport only.** ❌ `mcp` imports no `agent`, enforced by `tools-module-boundary`.
- **Auth covers every `/mcp` request** before session handling or dispatch; only the minimal health endpoint and
  data-free CORS preflight stay open. Ordinary request rejection is an in-band JSON-RPC error at HTTP 200, ❌ never a
  401 (that sends clients into OAuth discovery); notification and SSE rejection is HTTP 403. Never echo the token.
- **Interactive rebinds bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`), so a busy port drops no
  in-flight request; startup uses `ProbeOnCollision`. Live MCP control only works from the settings window.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
