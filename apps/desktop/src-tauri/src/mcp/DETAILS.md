# MCP server details

Pull-tier docs for `src-tauri/src/mcp/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in `CLAUDE.md`.

## Purpose

Expose Cmdr functionality to AI agents via the Model Context Protocol (MCP). Agents can control the app using the same capabilities available to users, no more, no less.

## Architecture

### Server (`server.rs`)

`server.rs` is server/dispatch only: HTTP/Axum setup, the bind/rebind lifecycle, request dispatch (`process_request`),
and response formatting (`format_sse_event` / `build_sse_response` / `build_json_response`). Per-request auth and
validation live in `auth.rs` (below); the server `use`s them. The split is one-directional — `server` depends on
`auth`, never the reverse.

- Runs in a background tokio task spawned at app startup
- Binds `127.0.0.1` only (localhost). Port defaults to ephemeral: when `developer.mcpPort` (the user setting) is 0 (the new default), the server binds `127.0.0.1:0` and asks the kernel for an unused port. A non-zero setting pins that port. `resolve_bind_strategy` turns the resolved port into `BindStrategy::Ephemeral` or `BindStrategy::Pinned(port)`; `bind_listener` then binds it under one of two `BindMode`s: **`ProbeOnCollision`** (startup auto-start — probe upward up to 100 ports so *a* server comes up when nobody's watching) or **`Exact`** (interactive settings changes — fail with `BindError::PortInUse` so the user is told their chosen port is busy instead of silently landing elsewhere). Both helpers are unit-tested in `server.rs`. The legacy fixed defaults (`19224` prod / `19225` dev) still live in `config.rs::DEFAULT_PORT` as the fallback for `CMDR_MCP_PORT` parse failures and are mirrored in the FE registry for users who want to pin a port.
- Writes the actual bound port to `<data_dir>/mcp.port` atomically (tempfile + fsync + rename, mode 0o600) after `bind`, plus a fresh per-instance bearer token to `<data_dir>/mcp.token` (same atomic write, 0o600). External readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent helpers) discover the port and token from those files and send `Authorization: Bearer <token>` on every request; the FE still uses the `get_mcp_port` / `get_mcp_token` IPC to read the same in-process state. On clean shutdown both files are removed and the in-process token is cleared; on crash they stay and readers discover staleness via `ECONNREFUSED`. See `port_file.rs` for the `write_port_file` / `write_secret_file` / read / remove API and typed `PortDiscoveryError`.
- **Auth**: every `/mcp` GET or POST runs `validate_origin` (browser-CSRF / DNS-rebinding layer) and requires the bearer token before session handling or dispatch. `validate_token` reads `Authorization: Bearer <token>`, compares against the in-process token in constant time, and returns a typed `TokenRejection` (`Missing` for an absent or empty bearer, `Wrong`, or `NotConfigured` when no token is set, failing closed). The response a client sees is the same for each, so it's no oracle; the reason goes to the security event log, coalesced per episode (`security_log::note_mcp_rejection`), as does a refused `Origin`. Only a refused `Origin` also
tells the person, in a toast (`../security_notices/CLAUDE.md`): a wrong or missing token is a client being set up or
holding the previous run's token, so it stays record-only. An ordinary JSON-RPC request gets an actionable in-band error via `authentication_required_response` (HTTP 200, **not** 401 — see Authentication for why); notifications and GET/SSE get an empty HTTP 403. Only `GET /mcp/health` and a data-free loopback CORS preflight stay open.
- Streamable HTTP transport (MCP spec 2025-11-25)
- Endpoints: `POST /mcp` (JSON-RPC), `GET /mcp` (optional SSE), `GET /mcp/health`

### Auth (`auth.rs`)

Owns the per-instance bearer-token lifecycle and all per-request validation the POST handler runs. The token statics
(`MCP_TOKEN` + `mcp_token_slot`) and accessors `current_mcp_token` (re-exported from `mod.rs` for the `get_mcp_token`
IPC) / `set_mcp_token` (server-lifecycle only) live here, alongside `generate_token` and the constant-time compare.
Validation: `validate_origin` (+ `is_localhost_origin`), `validate_accept_header`, `prefers_sse`, `get_protocol_version`,
`validate_token`, and `authentication_required_response`. The full model — what is open and why ordinary request
rejection is HTTP 200 rather than 401 — is in the Authentication section below.

### Protocol (`protocol.rs`)

- JSON-RPC 2.0 message parsing
- Routes to `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- Session management (though most clients don't use sessions)

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). The default port for users who pin (setting `developer.mcpPort` to non-zero) is build-mode-dependent: 19224 in prod, 19225 in dev. Different defaults so a dev session and an installed prod build don't collide when both pin. With the post-instance-isolation default of `developer.mcpPort = 0`, the server binds ephemeral and these constants only matter as the pinned-mode fallback. Mirrored in the FE registry; both are in 10000–29999 per AGENTS.md. See `docs/tooling/instance-isolation.md` for the cross-resource view.

### Redacting request headers (`safe_headers.rs`)

`SafeHeaders`: a newtype over `HeaderMap` whose `Debug` replaces the `authorization` value with `<redacted>`, plus the
axum extractor both `/mcp` handlers use so a raw `HeaderMap` never enters either one. Validators still reach the real
values through `Deref`. Why this is a type and not a rule: § Authentication.

### Tests (`tests/`)

Directory module split by test category. Tool, resource, and store tests live with their code (`../tools/tests/`,
`../ui_state/pane_state_tests.rs`).

- `protocol_tests.rs`: tool name validation, schema checks, tool count
- `security_tests.rs`: shell injection, forbidden tool patterns, input injection
- `request_response_tests.rs`: McpRequest parsing, McpResponse serialization
- `spec_compliance_tests.rs`: MCP spec 2025-11-25 compliance, origin validation, SSE events
- `server_http_auth.rs`: the real listener's auth, registered from `server.rs` with `#[path]` because `build_router` is
  private

## Key decisions

### Why localhost only?

Binding to `0.0.0.0` would expose the server to the network, so we bind `127.0.0.1` only. But localhost binding alone is **not** a security boundary against the real threat: a local non-Cmdr process. macOS doesn't isolate loopback between local processes, and a non-browser process can set any HTTP header (or none), so `validate_origin` (a browser-CSRF / DNS-rebinding defense) is no barrier to it. That's why the bearer token exists — see Authentication.

## Authentication

### What's authenticated

The bearer token is required for **every request that reaches `/mcp`**: initialize, discovery, resources, tool calls,
notifications, and GET/SSE. Authentication runs after Origin validation but before protocol negotiation, session
mutation, or dispatch. A future method or tool is therefore protected by construction; it cannot be accidentally left
open by missing per-method metadata.

Two data-free exceptions remain:

- `GET /mcp/health` returns only `{ "status": "ok" }`, for liveness checks.
- CORS may answer an OPTIONS preflight from an exact HTTP(S) loopback Origin. The actual GET or POST still requires the
  token.

Origin validation allows a missing Origin for native clients and exact `http://` / `https://` loopback origins
(`localhost`, `127.0.0.1`, or `::1`, with an optional port). It rejects `null`, `tauri://`, external hosts, and
deceptive names such as `localhost.evil.example`. A valid token never bypasses that check.

### Why the entire surface

The MCP surface exposes confidential data before any destructive action: paths and live app state, settings and logs,
and image-derived text including OCR through `image_facts` and `search_photos`. It also exposes disruptive controls
such as navigation and quit. Loopback limits network reachability but does not identify the caller, so discovery,
reads, and prompting actions need the same client proof as silent mutations.

The token is defense in depth against untrusted local callers that cannot read Cmdr's owner-only data directory. It is
not a sandbox against unrestricted code already running as the same macOS user: such code can read the token file and
often access the underlying files directly.

### How a client obtains and sends the token

The token is a fresh CSPRNG value (`Uuid::new_v4`, 122 random bits), generated on every server start and written to `<data_dir>/mcp.token` at mode 0o600 (owner-only). The port file (`mcp.port`) is 0o600 too. A client gets the token by either:

- reading `<data_dir>/mcp.token` (the same filesystem access an attacker would need to do damage directly), or
- reading the `CMDR_MCP_TOKEN` env var (see override below).

It then sends `Authorization: Bearer <token>` on every `/mcp` request. `scripts/mcp-call.sh` already does this (reads `<data_dir>/mcp.token`, or `CMDR_MCP_TOKEN`); the E2E harness fetches it via the `get_mcp_token` Tauri IPC. The app's own frontend does NOT talk to this HTTP server (it uses the separate Tauri MCP bridge), so it needs no token.

An ordinary JSON-RPC request rejection is an **in-band error at HTTP 200, not a 401**. The MCP Streamable-HTTP spec reserves HTTP 401 for an OAuth challenge, so a 401 makes clients (Claude Code, etc.) launch an OAuth discovery flow and surface a confusing "Invalid OAuth error response" instead of our message. A 200 + JSON-RPC `error` preserves the request id and renders client-side as `MCP error <code>: <message>` (the same path `nav_to_path`'s "path does not exist" takes). Our local bearer token isn't OAuth, so it must not look like one. Notifications have no id to correlate, and GET/SSE has no JSON-RPC envelope, so those reject with HTTP 403 and an empty body.

**The token must not reach a LOG either, and that is enforced by a type rather than a rule.** Both handlers take `SafeHeaders` (`safe_headers.rs`), a newtype over `HeaderMap` whose `Debug` renders the `authorization` value as `<redacted>` and everything else verbatim; the raw map never enters either handler, so a `{:?}` on the headers is safe by construction. It replaced a rule-shaped fix that patched only the POST route and left the SSE one leaking — which is the shape to avoid here, because a header dump is the natural thing to reach for while debugging a transport. The SSE route is precisely where a client sends `Authorization` to open the stream. Pinned by `both_routes_take_redacting_headers`, which fails to COMPILE if either handler goes back to a bare `HeaderMap`.

The message tells the caller exactly where the token lives (the `CMDR_MCP_TOKEN` env var and the resolved `<data_dir>/mcp.token` path). That's safe: the secret is the file's 0o600 contents and the env value, not the path, which is already discoverable. The message never echoes the token and is identical for a missing or wrong token, so the response is not a token-validity oracle.

### `CMDR_MCP_TOKEN` override

If `CMDR_MCP_TOKEN` is set and non-empty (after trim) when the server starts, that value is used as the token instead of a random one. The token file is still written 0o600 with the chosen value. Tradeoff: a fixed env token is **stable across restarts** (so a static `Authorization: Bearer ${CMDR_MCP_TOKEN}` client header keeps working) but **loses the per-launch replay protection** the random token gives. It's opt-in for the dev workflow, not the default.

### Connecting an external MCP client

To let an external MCP client (for example Claude Code) connect, export `CMDR_MCP_TOKEN` for the running Cmdr and add a matching header to the server entry in `.mcp.json`:

```json
{
  "mcpServers": {
    "cmdr": {
      "url": "http://127.0.0.1:<port>/mcp",
      "headers": { "Authorization": "Bearer ${CMDR_MCP_TOKEN}" }
    }
  }
}
```

Without that header the client still works for everything else (reads, nav, search, and destructive ops that prompt in the app); only auto-confirm and `dialog` confirm are rejected (as the in-band JSON-RPC error above).

## Gotchas

### Server lifecycle is managed at runtime

There are two start paths. **Startup** (`start_mcp_server` → `start_mcp_server_background`, fire-and-forget) binds with `BindMode::ProbeOnCollision` and serves. **Interactive** settings changes go through `rebind_interactive` instead, which is the race-free core and is **bind-new-before-stop**: it binds the new listener (`BindMode::Exact`) *before* retiring the old server, so (a) a busy port leaves the existing server untouched and returns `McpServerOutcome::PortInUse`, (b) a successful change never drops an in-flight request, and (c) we never collide with our own still-open socket. The pure `decide_rebind_action(strategy, actual_port)` short-circuits a re-apply of the live pinned port to `NoOp` (the guard against Exact-binding a port we already hold). Both paths funnel into `serve_on`, which stores `MCP_ACTUAL_PORT`, mints + writes the bearer token, writes `<data_dir>/mcp.port` (0o600, atomic tempfile + fsync + rename — see `port_file.rs`), caches the data dir in `MCP_PORT_FILE_DIR`, and spawns the serve task into `MCP_HANDLE`. The token is a fresh CSPRNG value by default, or the `CMDR_MCP_TOKEN` env value when set non-empty (see Authentication § override).

**Stopping has two flavors, and the difference is load-bearing.** `stop_mcp_server()` (sync) only `abort()`s the task — `abort()` *requests* cancellation, so the listener socket may linger briefly after it returns. That's fine for app shutdown (process exiting) and for the retire-the-old-server step inside `rebind_interactive` (the new listener is already up on a *different* port, no contention). `stop_mcp_server_and_wait()` (async) aborts **and awaits the handle**, guaranteeing the socket is released before returning; the interactive disable path (`set_mcp_enabled(false)`) uses it so an immediate re-enable on the **same** port binds cleanly instead of racing a not-yet-closed socket. Both clear `MCP_TOKEN` to None (so `validate_token` fails closed) and remove the port + token files; the crash path resets `MCP_ACTUAL_PORT` to 0 the same way. `is_mcp_running()` reads `MCP_ACTUAL_PORT != 0`.

The live-control commands (`set_mcp_enabled`, `set_mcp_port`, no restart needed) return a typed `McpServerOutcome` (`Running { port }` / `Stopped` / `PortInUse { requested }`) so the frontend branches on `kind`, never a message string; the wire shape is pinned by `mcp_server_outcome_json_shape`. The frontend shows "(ephemeral)" when the setting was 0, "(port N was in use)" when a *startup* probe landed off the pinned port, and on `PortInUse` keeps the server on its current port while offering the suggested free port (`findAvailablePort`). If the server crashes mid-serve, `MCP_ACTUAL_PORT` resets to 0 but the on-disk file may linger; external readers retry on `ECONNREFUSED`. Check logs for "MCP server crashed" errors.

### Live MCP control only works from the settings window

`McpServerSection.svelte` subscribes to `developer.mcpEnabled` and `developer.mcpPort` changes and calls the Tauri commands directly. The main window's `settings-applier.ts` intentionally does NOT handle these settings to avoid double-firing (both windows receive setting change events). This means if an MCP tool changes `developer.mcpEnabled` via the settings bridge while the settings window is closed, the setting is saved but the server state doesn't change until the next app restart. This is acceptable, since an MCP tool toggling its own server is circular.

### Error codes are JSON-RPC standard

`INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, etc. These are defined by the JSON-RPC spec, not MCP. Don't change them.
