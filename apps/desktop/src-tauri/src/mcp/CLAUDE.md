# MCP server

Exposes Cmdr to AI agents via the Model Context Protocol. Security model is parity: agents can only do what users can
do, no filesystem access. Streamable HTTP transport, `127.0.0.1` only, ephemeral port by default.

For adding or changing tools, see `docs/guides/mcp-development.md`.

## Module map

- `server.rs`: HTTP server, bind/lifecycle, request dispatch (`process_request`), and response formatting only.
- `auth.rs`: token lifecycle and per-request validation (`tool_call_requires_token`, `validate_token`,
  `validate_origin`, header/protocol checks). One-directional: `server` uses `auth`, never the reverse.
- `tool_registry/`: single source for every AI-callable tool — one `mcp_tools!` table (`mod.rs`) generates the
  per-consumer list/dispatch views + the auth gate (`tool_gate`/`TokenGate` in `gate.rs`; schemas hoisted to
  `schemas/*.rs`). Tests in `tests/tool_registry_tests.rs`; `tools.rs` is a shim. Handlers + ack contract in
  `executor/` (see [`executor/CLAUDE.md`](executor/CLAUDE.md)).
- `resources/`: read-only YAML/text resources (`cmdr://state`, `logs`, `indexing` (per-volume), `importance`,
  `settings`). State stores: `PaneStateStore`, `SoftDialogTracker` (`dialog_state.rs`), `listing_errors`,
  `terminal_ops` (settled-op ring for `await operation_complete`). Config: `config.rs`, `port_file.rs`.

## Must-knows

- **Auth gates ONLY the calls that bypass the user's confirmation dialog, via a `TokenGate` on each entry, not a
  hand-list.** Gated: the auto-confirm/rollback bypass (`copy`/`move`/`delete`/`operations_rollback` `autoConfirm`,
  `queue` `rollback`), `dialog` confirm, and silent-config mutation (`set_setting`, `indexing`, `tag`, `favorites`);
  everything else needs none. A structural test fails if an `autoConfirm`/`rollback` tool is left `Open`. Don't widen it
  to reads/nav or narrow it past the auto-confirm bypass. Full model: DETAILS.md § Authentication.
- **One registry, two consumer views (agent-spec D49/D59).** Each entry declares `consumers` (`ai_client`/`agent`) +
  `access` (`Read`/`Write`); each transport dispatches only its own view. The agent view is **read-only by
  construction** — `[agent]` entries, all `access: Read`, pinned structurally. `access` is stronger than
  `TokenGate::Open`, so tag any mutating tool `Write`. Agent handlers live under
  [`agent/tools`](../agent/tools/CLAUDE.md). DETAILS.md § Consumer and access views.
- **Token rejection is an in-band JSON-RPC error at HTTP 200, NOT 401** (401 makes clients launch an OAuth discovery
  flow; the Streamable-HTTP spec reserves it for that). Keep it 200 + JSON-RPC `error`. The gate fails closed (no token
  → reject). One uniform message for missing-vs-wrong token (no oracle); never echo the token.
- **Param naming is camelCase** (`tabId`, `autoConfirm`); tool names stay snake_case. Don't add snake_case params:
  agents pattern-match across tools and every inconsistency is a guessed-wrong call.
- **Action tools wait for a typed ack before returning `OK`** (1500 ms budget, 5 s for nav). `OK` means "the FE accepted
  the dispatched action," not "the operation completed"; poll `await` for completion. Don't return `OK` without waiting
  for the ack (a stalled FE silently drops it). Details in [`executor/CLAUDE.md`](executor/CLAUDE.md).
- **`cmdr://state` and `cmdr://logs` redact through `crate::redact::redact_line`** before serialization (state's
  `recentErrors` `path`/`message`, every returned log line). A loopback caller has no filesystem read, so redaction is
  the only thing keeping home paths / SMB URIs / emails out. Don't remove it. `cmdr://logs` `filter` matches the RAW
  pre-redaction line.
- **Interactive rebinds use bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`): a busy port leaves the
  existing server up (`McpServerOutcome::PortInUse`) and drops no in-flight request; startup uses `ProbeOnCollision`.
  Two stop flavors + token-clear-on-stop: [DETAILS.md](DETAILS.md).
- **Live MCP control (`set_mcp_enabled`/`set_mcp_port`) only works from the settings window**; the main window's
  `settings-applier.ts` ignores these to avoid double-firing. A tool toggling its own server while settings is closed
  saves the setting but applies only on restart (circular; acceptable).
- **`select_volume` polls `volume_name`, not path change**: re-selecting the same volume is an instant no-op; virtual
  volumes like `Network` work even without a path change.
- **JSON-RPC error codes are spec-defined** (`INVALID_PARAMS = -32602`, etc.). Don't change them.
- **MCP state stores are runtime-only, no `_schemaVersion`**; on a format change, just restart.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
