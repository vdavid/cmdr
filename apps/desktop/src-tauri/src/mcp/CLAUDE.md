# MCP server

Exposes Cmdr to AI agents via the Model Context Protocol. Security model is parity: agents can only do what users can
do, no filesystem access. Streamable HTTP transport, `127.0.0.1` only, ephemeral port by default.

## Module map

- `server.rs`: HTTP server, bind/lifecycle, auth gate (`tool_call_requires_token`, `validate_token`, `validate_origin`).
- `tools.rs` + `executor/`: 32 semantic tools and the ack contract (see [`executor/CLAUDE.md`](executor/CLAUDE.md)).
- `resources/`: read-only YAML/text resources (`cmdr://state`, `logs`, `indexing`, `settings`). State stores:
  `PaneStateStore`, `SoftDialogTracker` (`dialog_state.rs`), `listing_errors`. Config: `config.rs`, `port_file.rs`.

## Must-knows

- **Auth gates ONLY the calls that bypass the user's confirmation dialog.** The bearer token is required iff
  `tool_call_requires_token(method, params)` is true: `delete`/`move`/`copy` with `autoConfirm: true`, `dialog` with
  `action: "confirm"`, and `set_setting` (all of it). Everything else (resource reads, nav, search, dialog-prompting
  destructive ops) needs no token. The threat is a local non-Cmdr process silently auto-confirming a destructive op;
  `validate_origin` is browser-CSRF defense only and is no barrier to it. Don't widen the gate to reads/nav (no security
  gained, server gets annoying) or narrow it past the auto-confirm bypass.
- **Token rejection is an in-band JSON-RPC error at HTTP 200, NOT 401.** The Streamable-HTTP spec reserves 401 for an
  OAuth challenge, so a 401 makes clients launch an OAuth discovery flow. Keep it 200 + JSON-RPC `error`. The gate fails
  closed (no token set → reject). One uniform message for missing-vs-wrong token (no oracle); never echo the token.
- **Param naming is camelCase** (`tabId`, `autoConfirm`); tool names stay snake_case. Don't add snake_case params:
  agents pattern-match across tools and every inconsistency is a guessed-wrong call.
- **Action tools wait for a typed ack before returning `OK`** (1500 ms budget, 5 s for nav). `OK` means "the FE accepted
  the dispatched action," not "the operation completed"; for long ops the agent polls via the `await` tool. Don't make
  an action tool return `OK` without waiting for its ack signal: pre-ack, tools returned `OK` while the FE was stalled
  and the action was silently dropped. Details in [`executor/CLAUDE.md`](executor/CLAUDE.md).
- **`cmdr://state` and `cmdr://logs` redact through `crate::redact::redact_line`** before serialization (state's
  `recentErrors` `path`/`message`, every returned log line). These run on a loopback caller without filesystem read, so
  the redaction is the only thing keeping home paths / SMB URIs / emails out. Don't remove it. `cmdr://logs` `filter`
  matches the RAW pre-redaction line (redaction runs last).
- **Interactive rebinds use bind-new-before-stop** (`rebind_interactive`, `BindMode::Exact`): bind the new listener
  before retiring the old server, so a busy port leaves the existing server up (`McpServerOutcome::PortInUse`) and no
  in-flight request drops. Startup uses `BindMode::ProbeOnCollision` (probe upward so a server comes up). The two
  stop flavors differ load-bearingly: `stop_mcp_server()` only `abort()`s (socket may linger, fine for shutdown / the
  retire step); `stop_mcp_server_and_wait()` awaits the handle (needed before an immediate re-enable on the same port).
  Both clear `MCP_TOKEN` to None so `validate_token` fails closed.
- **Live MCP control (`set_mcp_enabled`/`set_mcp_port`) only works from the settings window**; the main window's
  `settings-applier.ts` deliberately ignores these settings to avoid double-firing. An MCP tool toggling its own server
  while settings is closed saves the setting but doesn't change server state until restart (acceptable, it's circular).
- **`select_volume` polls `volume_name`, not path change**: re-selecting the same volume is an instant no-op, and
  virtual volumes like `Network` work even when the path doesn't change.
- **JSON-RPC error codes are spec-defined** (`INVALID_PARAMS = -32602`, etc.). Don't change them.
- **MCP state stores are runtime-only, no `_schemaVersion`**; on a format change, just restart.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
