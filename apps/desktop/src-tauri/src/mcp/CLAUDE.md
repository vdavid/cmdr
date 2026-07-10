# MCP server

Exposes Cmdr to AI agents via the Model Context Protocol. Security model is parity: agents can only do what users can
do, no filesystem access. Streamable HTTP transport, `127.0.0.1` only, ephemeral port by default.

For adding or changing tools, see `docs/guides/mcp-development.md`.

## Module map

- `server.rs`: HTTP server, bind/lifecycle, request dispatch (`process_request`), and response formatting only.
- `auth.rs`: token lifecycle and per-request validation (`tool_call_requires_token`, `validate_token`,
  `validate_origin`, header/protocol checks). One-directional: `server` uses `auth`, never the reverse.
- `tool_registry.rs`: single source for all 42 tools — one `mcp_tools!` table generates the list, dispatch, and auth
  gate (`tool_gate`/`TokenGate`); its schema/gate tests live in `tests/tool_registry_tests.rs`. `tools.rs` is a shim
  (`Tool` struct + re-export). Handlers + ack contract in `executor/` (see [`executor/CLAUDE.md`](executor/CLAUDE.md)).
- `resources/`: read-only YAML/text resources (`cmdr://state`, `logs`, `indexing` (per-volume), `importance`,
  `settings`). State
  stores: `PaneStateStore`, `SoftDialogTracker` (`dialog_state.rs`), `listing_errors`, `terminal_ops` (settled-op ring
  for `await operation_complete`). Config: `config.rs`, `port_file.rs`.

## Must-knows

- **Auth gates ONLY the calls that bypass the user's confirmation dialog, and the gate is a `TokenGate` on each tool's
  `tool_registry.rs` entry, not a hand-list.** Gated: `delete`/`move`/`copy` with `autoConfirm: true`, `dialog` with
  `action: "confirm"`, `set_setting`, `indexing` (per-drive config mutation), and `queue` with `rollback: true` (the
  `IfRollback` gate — a rollback cancel actively DELETES already-copied files; plain pause/resume/cancel stay `Open`).
  Everything else (reads, nav, search, dialog-prompting destructive ops) needs no token. A new destructive tool MUST
  declare its gate — a structural test fails if an `autoConfirm` (or `rollback`) tool is left `Open`. Don't widen the
  gate to reads/nav or narrow it past the auto-confirm bypass (threat model, and why `validate_origin` is no barrier:
  DETAILS.md § Authentication).
- **Token rejection is an in-band JSON-RPC error at HTTP 200, NOT 401** (401 makes clients launch an OAuth discovery
  flow; the Streamable-HTTP spec reserves it for that). Keep it 200 + JSON-RPC `error`. The gate fails closed (no token
  → reject). One uniform message for missing-vs-wrong token (no oracle); never echo the token.
- **Param naming is camelCase** (`tabId`, `autoConfirm`); tool names stay snake_case. Don't add snake_case params:
  agents pattern-match across tools and every inconsistency is a guessed-wrong call.
- **Action tools wait for a typed ack before returning `OK`** (1500 ms budget, 5 s for nav). `OK` means "the FE accepted
  the dispatched action," not "the operation completed"; poll `await` for completion. Don't return `OK` without waiting
  for the ack (a stalled FE silently drops the action). Details in [`executor/CLAUDE.md`](executor/CLAUDE.md).
- **`cmdr://state` and `cmdr://logs` redact through `crate::redact::redact_line`** before serialization (state's
  `recentErrors` `path`/`message`, every returned log line). These run on a loopback caller without filesystem read, so
  redaction is the only thing keeping home paths / SMB URIs / emails out. Don't remove it. `cmdr://logs` `filter` matches
  the RAW pre-redaction line.
- **Interactive rebinds use bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`): a busy port leaves the
  existing server up (`McpServerOutcome::PortInUse`) and drops no in-flight request; startup uses `ProbeOnCollision`. The
  two stop flavors and the token-clear-on-stop are in [DETAILS.md](DETAILS.md).
- **Live MCP control (`set_mcp_enabled`/`set_mcp_port`) only works from the settings window**; the main window's
  `settings-applier.ts` deliberately ignores these settings to avoid double-firing. An MCP tool toggling its own server
  while settings is closed saves the setting but doesn't apply until restart (acceptable; it's circular).
- **`select_volume` polls `volume_name`, not path change**: re-selecting the same volume is an instant no-op, and
  virtual volumes like `Network` work even when the path doesn't change.
- **JSON-RPC error codes are spec-defined** (`INVALID_PARAMS = -32602`, etc.). Don't change them.
- **MCP state stores are runtime-only, no `_schemaVersion`**; on a format change, just restart.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
