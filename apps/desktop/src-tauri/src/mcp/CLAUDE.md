# MCP server

Exposes Cmdr to AI agents over the Model Context Protocol. Security is parity: agents do only what users can do, no
filesystem access. Streamable HTTP, `127.0.0.1` only, ephemeral port by default. Adding or changing tools:
`docs/guides/mcp-development.md`.

`server.rs` binds and dispatches, `auth.rs` owns tokens (one-directional: `server` uses `auth`, never the reverse),
`tool_registry/` is the single source for every AI-callable tool (one `mcp_tools!` table generating the per-consumer
views and the gate; handlers in `executor/CLAUDE.md`), `resources/` serves the read-only YAML/text views over the state
stores.

## Must-knows

- **Auth gates ONLY the calls that bypass the user's confirmation dialog**, via a `TokenGate` per registry entry, never
  a hand-list: the auto-confirm/rollback bypass, `dialog` confirm, and silent-config mutation. ❌ Don't widen it to
  reads or nav, ❌ don't narrow it past the bypass. DETAILS § Authentication.
- **One registry, two consumer views.** Each entry declares `consumers` + `access`, and each transport dispatches only
  its own view. **The agent can propose; only the user can approve**: `[agent]` entries are `Read`, `Propose`, or
  `Memory`, never `Write` (pinned structurally). `access` beats `TokenGate::Open`, so tag any mutating tool `Write`.
  Agent handlers: `../agent/tools/CLAUDE.md`.
- **`Access::Memory` is the agent's only write, and it is agent-only.** It covers the two tools writing
  `<data-dir>/ai/memory/`, held by that module's jail plus the hand-authored `EXPECTED_MEMORY_TOOL_NAMES`. ❌ Never add
  one to `Consumer::AiClient`: this transport's security story is "no filesystem access" (a test pins that).
- **Token rejection is an in-band JSON-RPC error at HTTP 200, NOT 401** (401 sends clients into OAuth discovery). Fails
  closed, one message for missing-vs-wrong, ❌ never echo the token.
- **Params are camelCase, tool names snake_case.** Agents pattern-match across tools, so an inconsistency is a
  guessed-wrong call.
- **Action tools wait for a typed ack before returning `OK`.** `OK` means the FE accepted the action, not that it
  finished; poll `await` for that.
- **Whatever a user can reach, an agent must be able to reach, answer, and OBSERVE.** A state only a hand can drive is
  where bugs accumulate invisibly (a conflict wedge lived in one for months). So `cmdr://state` names the parked clash
  and the archive-password prompt, which `resolve_conflict` and `unlock_archive` answer. ❌ Don't add a modal state with
  no way to see it or answer it. DETAILS §§ Answering one clash, the archive password.
- **A directory size in `cmdr://state` is never a bare number.** `≥` means lower bound, `[size-pending]` /
  `[size-stale]` qualify it, and `(N on disk)` counts hard links and APFS clones in FULL, so it isn't "what deleting
  frees". ❌ Don't strip a qualifier to save tokens: an agent acts on the number. DETAILS § Directory sizes.
- **Volume capacity/free comes from the space poller's CACHE, ❌ never a `statfs` here**: that syscall blocks 30–120 s
  on a hung mount and `cmdr://state` is read constantly. Unwatched volume ⇒ absent, ❌ never a zero.
- **`cmdr://state` and `cmdr://logs` redact through `crate::redact::redact_line`** — the only thing keeping home paths,
  SMB URIs, and emails out, since a loopback caller has no filesystem read. `logs` `filter` matches the RAW line.
- **Interactive rebinds bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`): a busy port leaves the running
  server up and drops no in-flight request. Startup uses `ProbeOnCollision`.
- **Live MCP control only works from the settings window**; the main window's `settings-applier.ts` ignores it to avoid
  double-firing.
- **`select_volume` polls `volume_name`, not path change**, so re-selecting is a no-op and virtual volumes work.
- **JSON-RPC error codes are spec-defined**; ❌ don't change them. **State stores are runtime-only** (no
  `_schemaVersion`): on a format change, restart.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
