# MCP server

Exposes Cmdr to AI agents over the Model Context Protocol. Security is parity: agents do only what users can do, no
filesystem access. Streamable HTTP, `127.0.0.1` only, ephemeral port by default. Adding or changing tools:
`docs/guides/mcp-development.md`.

`server.rs` binds and dispatches, `auth.rs` owns tokens (one-directional: `server` uses `auth`, never the reverse),
`tool_registry/` is the single source for every AI-callable tool (one `mcp_tools!` table generating the per-consumer
views and the gate; handlers in `executor/CLAUDE.md`), `resources/` serves the read-only `cmdr://` views over the state
stores (`resources/CLAUDE.md`).

## Must-knows

- **One registry, two consumer views.** Each entry declares `consumers` + `access`, and each transport dispatches only
  its own view. **The agent can propose; only the user can approve**: `[agent]` entries are `Read`, `Propose`, or
  `Memory`, never `Write` (pinned structurally). `access` beats `TokenGate::Open`, so tag any mutating tool `Write`.
  Agent handlers: `../agent/tools/CLAUDE.md`.
- **`Access::Memory` is the agent's only write, and it is agent-only.** ❌ Never add one to `Consumer::AiClient`: this
  transport's story is "no filesystem access" (a test pins that).
- **Auth gates ONLY the calls that bypass the user's confirmation dialog**, via a `TokenGate` per entry, never a
  hand-list: ❌ don't widen it to reads or nav, ❌ don't narrow it past the bypass. Rejection is an in-band JSON-RPC
  error at HTTP 200, ❌ never a 401 (that sends clients into OAuth discovery), and ❌ never echoes the token.
- **A call is checked against its tool's declared schema before a handler sees it** (`tool_registry/params.rs`), so
  author the schema as the truth: a `required` field is enforced, and an AGENT tool must close itself with
  `additionalProperties: false` (a test pins it). Params are camelCase, tool names snake_case; agents pattern-match
  across tools, so an inconsistency is a guessed-wrong call.
- **Whatever a user can reach, an agent must reach, answer, and OBSERVE.** A state only a hand can drive is where bugs
  accumulate invisibly (a conflict wedge lived in one for months). ❌ Don't add a modal state with no way to see it or
  answer it.
- **Action tools wait for a typed ack before returning `OK`**: it means the FE accepted the action, not that it
  finished; poll `await` for that.
- **Interactive rebinds bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`), so a busy port drops no
  in-flight request; startup uses `ProbeOnCollision`. Live MCP control only works from the settings window.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
