# MCP servers

Two MCP servers are available when the app is running via `pnpm dev`.

## Servers

- **cmdr** (Cmdr MCP HTTP server): High-level app control: navigation, file operations, search, dialogs, state
  inspection. This is the primary way to test and interact with the running app. Architecture docs:
  `src-tauri/src/mcp/CLAUDE.md`.
- **tauri** (`tauri-plugin-mcp-bridge` WebSocket bridge): Low-level Tauri access: screenshots, DOM inspection, JS
  execution, IPC calls. Use for visual verification and UI automation.

Both bind `127.0.0.1` only (the bridge had to be forced; the plugin default is `0.0.0.0`, which would expose the
WebSocket to the LAN).

## Port discovery

Both servers use ephemeral ports by default so concurrent dev / E2E instances can coexist without collisions. The actual
port is written to the per-instance data dir as a plain text file (ASCII port + `\n`, written via tempfile+rename so a
zero-byte read can't happen):

| Server           | Port file (under `<data_dir>`) | Writer                                     |
| ---------------- | ------------------------------ | ------------------------------------------ |
| Cmdr MCP HTTP    | `mcp.port`                     | Rust (after `bind`)                        |
| Tauri MCP bridge | `tauri-mcp.port`               | `tauri-wrapper.ts` (before Tauri launches) |

Resolve `<data_dir>` for the instance you care about:

- Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`
- `pnpm dev`: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`
- `pnpm dev --worktree foo`: `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`
- Anything else: whatever the wrapper exported as `CMDR_DATA_DIR` (E2E shards land under
  `/tmp/cmdr-e2e-data-<instance>/`).

**Read precedence for external clients** (CLI, agent helpers, E2E fixtures):

1. `CMDR_MCP_PORT` env var (manual pin).
2. `<data_dir>/mcp.port` (Cmdr MCP server) or `<data_dir>/tauri-mcp.port` (bridge).
3. Fail loud with a typed error. Don't fall back to a legacy hardcoded port; that hides bugs.

The FE keeps using the `get_mcp_port` IPC inside the running webview (it reads the same in-process atomic). The port
file is for **out-of-process** readers only.

Pinning a fixed port stays supported: set `CMDR_MCP_PORT` (or set the `developer.mcpPort` setting in the UI). The server
still writes the file in that case so external readers don't have to special-case pinned vs ephemeral.

## How to use

**Prefer the Cmdr MCP over the Tauri MCP.** The Cmdr MCP has purpose-built tools for app interaction (navigation,
search, file ops, state inspection). The Tauri MCP is generic low-level access. For example, read `cmdr://state` to
understand what the app is showing instead of taking a Tauri screenshot. Only fall back to the Tauri MCP for things the
Cmdr MCP can't do (window management, DOM inspection, arbitrary JS execution).

**Prefer the wired-up MCP tools** (e.g. `mcp__cmdr__search`, `mcp__cmdr__nav_to_path`). These are available when Claude
Code's MCP integration is connected. Always call `tools/list` first if you're unsure about parameter names.

**Spawned agents often start without the MCP connected.** A freshly spawned Claude Code session (a subagent, or a
session a lead spins up) frequently doesn't auto-connect the wired-up `mcp__cmdr__*` / `mcp__tauri__*` tools even though
they're configured, and there may be no way to trigger a refresh from inside that session. So when orchestrating agents,
assume they'll drive the app through the CLI fallback `./scripts/mcp-call.sh` rather than the wired-up tools. It
connects independently and works regardless of the session's MCP state.

**Fallback: `./scripts/mcp-call.sh`**: a curl wrapper for Cmdr's MCP server:

```bash
# Search for files
./scripts/mcp-call.sh search '{"pattern":"*.pdf","limit":5}'
./scripts/mcp-call.sh ai_search '{"query":"recent invoices"}'

# Navigate and inspect
./scripts/mcp-call.sh nav_to_path '{"pane":"left","path":"/Users"}'
./scripts/mcp-call.sh --read-resource 'cmdr://state'

# Discover available tools and their parameter schemas
./scripts/mcp-call.sh --list-tools

# A worktree session: name the instance, or it discovers the plain "dev" one
CMDR_INSTANCE_ID=dev-<slug> ./scripts/mcp-call.sh --list-tools
```

**When it says it couldn't reach the server, read the rest of the message.** It names the data dir it resolved, the
port, and curl's own verdict; a `curl exit 7` (nothing listening) on a worktree almost always means the port file is
stale, because the server takes a fresh ephemeral port every launch and `pnpm dev` relaunches the app on every Rust
edit. `pgrep -fl Cmdr` plus `cat "<data_dir>/mcp.port"` settles it in one step. An HTTP status instead means the request
landed and was refused, and the message carries the body.

`search` and `ai_search` can hold a reply for their whole `maxWaitSeconds` (up to 120), so the script waits 150 s by
default; `CMDR_MCP_TIMEOUT` moves it.

## Authentication (token-gated tools)

Most tools (resource reads, nav, search, dialog-prompting ops) need no auth. A bearer token is required ONLY for the
calls that bypass the user's confirmation dialog: `set_setting`, `delete` / `move` / `copy` with `autoConfirm: true`,
and `dialog` with `action: "confirm"` (including `quit-confirmation`; the matching `close`, which keeps working, is
open). Calling one of these without the token logs `MCP: rejected request with missing/invalid bearer token` and returns
a JSON-RPC error pointing at the token file. To get it right on the first try:

- **`./scripts/mcp-call.sh` handles the token for you.** It reads `<data_dir>/mcp.token` (or `CMDR_MCP_TOKEN`) and sends
  `Authorization: Bearer <token>` on every request. Prefer it for any gated call: `./scripts/mcp-call.sh set_setting …`
  just works.
- **The wired-up `mcp__cmdr-*__*` tools can't add headers**, so gated ops through them fail unless you start Cmdr with
  `CMDR_MCP_TOKEN` exported and add `"headers": { "Authorization": "Bearer ${CMDR_MCP_TOKEN}" }` to the server entry in
  `.mcp.json`. Without that setup, route gated ops through `mcp-call.sh` instead. Read-only / nav / search tools work
  through the wired-up tools regardless.

Full token model (why a per-launch CSPRNG token, the `CMDR_MCP_TOKEN` override, why rejection is HTTP 200 not 401):
`apps/desktop/src-tauri/src/mcp/DETAILS.md` § Authentication.

## Action-tool ack contract

Action tools (`copy`, `move`, `delete`, `mkdir`, `mkfile`, `select`, `toggle_hidden`, `set_view_mode`, `sort`, `tab`,
`nav_to_parent`, `nav_back`, `nav_forward`, and `dialog` open/close/focus/confirm) now wait for the frontend to
acknowledge the dispatched action before returning `OK`. Default budget is 1500 ms; the navigation family
(`nav_to_parent`, `nav_back`, `nav_forward`) gets 5 s because remote backends (SMB, MTP) routinely need a few seconds
for their directory listing even on success. `open_under_cursor` uses a true round-trip (`mcp-response` from the FE
after the action resolves, 5 s timeout) because Enter on a non-directory file delegates to the OS default app, which
produces neither a state push nor a viewer window — the FE is the only source of truth for "this finished." If the FE is
stalled, the tool returns a typed error naming the missing signal — no more false-positive `OK`s.

`refresh` and `select` are full round-trips: the FE replies to the specific request (`mcp-response` carrying the
request's ID) only after the work settles, so the ack is independent of pane-state pushes and their byte-identical
dedupe. `refresh`'s `OK` means the backend actually re-read the directory (5 s budget; local volumes re-read from disk,
watcher-backed MTP/SMB listings short-circuit) — it acks reliably even when the re-listing matches the cached state and
no state push fires.

What this means for automation:

- `OK` is now a meaningful contract: the FE accepted the action. The downstream operation may still be running (a copy
  of 10 GB returns `OK` when the FE accepts the dialog, not when bytes finish).
- For long-running operations, poll completion via the `await` tool just like before.
- Timeouts surface as JSON-RPC errors with a clear message ("Action not acknowledged by backend within 1500 ms (waiting
  for: …)"). Treat them as real failures — don't retry blindly; check `cmdr://state` to triage.
- `dialog close file-viewer` on a path that isn't open returns an `invalid_params` error fast ("No file viewer windows
  are open"); closing one of multiple viewers acks as soon as the count drops by one, not when all viewers vanish.
- Very slow remote shares can still exceed even the 5 s nav budget. If a nav tool times out but the navigation actually
  succeeds in the background, follow up with `await` (`path` / `path_contains`) to confirm the destination landed.

Architecture details: `apps/desktop/src-tauri/src/mcp/executor/DETAILS.md` § "Ack contract".

## Why isn't this operation moving?

A `running` row with counters that never move has four very different causes, and `waitingOn` on the row tells them
apart without subscribing to anything:

- `moving`: bytes are flowing. It's a slow copy, not a stuck one.
- `paused`: somebody paused it. `queue resume` starts it again.
- `destination` / `source`: every in-flight file is parked waiting on that side. `stillForSeconds` says for how long,
  `inFlight` how many files are open. A busy share recovers; a dead mount doesn't.
- `conflict`: the operation is parked on a name clash. The `pendingConflict` block on the same row carries the
  `conflictId` to answer with (next section).
- `unknown`: nothing is moving and nothing explains it. This is the wedge shape, and the one worth escalating on.

A row with no `waitingOn` at all means the backend can't classify itself (a local copy keeps no in-flight table). Read
that as "can't tell", never as "it's fine".

## Answering a name clash one file at a time

A transfer confirmed with `onConflict: "stop"` decides nothing upfront: it parks on each clashing file and waits for an
answer, which is what a person gets by leaving the dialog on "Ask for each". Drive it entirely over MCP:

```bash
./scripts/mcp-call.sh select '{"pane":"left","names":["photos"]}'
./scripts/mcp-call.sh copy '{}'
./scripts/mcp-call.sh dialog '{"action":"confirm","type":"transfer-confirmation","onConflict":"stop"}'

# The parked clash shows up under the operation, with the id an answer must carry.
./scripts/mcp-call.sh --read-resource 'cmdr://state?include=operations'
#   - operationId: op-7
#     status: running
#     waitingOn: conflict
#     pendingConflict:
#       conflictId: 1
#       destination: "/Users/…/photos/dsc-1.raw"
#       answerWith: resolve_conflict (…)

./scripts/mcp-call.sh resolve_conflict '{"operationId":"op-7","conflictId":1,"resolution":"skip"}'
```

The reply carries a typed `outcome`: `resolved` (yours is the answer the operation used) or `already_resolved` (somebody
answered the same clash first). A refusal carries `data.outcome` instead: `stale_answer` (the operation has moved on,
re-read `cmdr://state`), `no_pending_conflict`, or `unknown_operation`. Branch on those, never on the sentence.
`applyToAll: true` answers the rest of the operation's clashes the same way. Both `dialog confirm` and
`resolve_conflict` are token-gated, so route them through `./scripts/mcp-call.sh`.

## Quitting the app

`quit` goes through the same gate ⌘Q does, so it can't kill a running transfer by surprise. With nothing that moves data
running it answers `{"outcome": "quitting"}` and the app goes. While a copy, move, delete, trash, or archive edit is
still going it does **not** quit: it raises the quit confirmation, starts the 15-second countdown, and answers

```json
{
  "outcome": "held",
  "countdownMs": 15000,
  "operations": [{ "operationId": "op-7", "operationType": "copy", "status": "running", "source": "…" }],
  "total": 1,
  "message": "…"
}
```

Answer it, either way:

```bash
# Quit now, stopping what's running (token-gated, like every dialog confirm).
./scripts/mcp-call.sh dialog '{"action":"confirm","type":"quit-confirmation"}'

# Or leave the operations alone. The countdown is deleted, not deferred.
./scripts/mcp-call.sh dialog '{"action":"close","type":"quit-confirmation"}'
```

`confirm` answers `{"outcome": "quitting"}`, `close` answers `{"outcome": "kept_working"}` plus `promptClosed` (whether
the dialog left the screen; a `false` there doesn't undo the answer), and either is a refusal carrying
`data.outcome: "no_quit_pending"` when the gate had nothing pending: the countdown ran out, or somebody answered first.
Branch on those, never on the sentence. `dialog open quit-confirmation` is refused, since only the gate raises it.

**Silence is an answer, and it's the destructive one.** A held quit nobody answers within the countdown quits the app
anyway and stops those operations (they keep every file already copied; only the file in flight loses its partial). So
treat an `"outcome": "held"` reply as a question you must answer in the same turn, and use
`./scripts/mcp-call.sh --read-resource 'cmdr://state?include=operations'` if you need more than the reply's list to
decide.

## Connection resilience

The MCP server goes down during hot reloads (up to 15s for Rust changes, up to 3s for frontend changes). Multiple agents
working simultaneously can trigger frequent reloads. Follow this escalation:

1. **Try the wired-up MCP tools** (`mcp__cmdr__*`). If they work, use them.
2. **If disconnected, try `./scripts/mcp-call.sh`**: it connects independently and may work when the MCP integration is
   temporarily down.
3. **If both fail, wait ~15 seconds and retry**: the app is probably mid-reload from a Rust change.
4. **If still failing, ask the user** to stop other agents that may be triggering hot reloads, and report back when it's
   clear.

Do NOT retry in a tight loop. One retry after 15 seconds is enough before escalating.

## Tauri MCP pitfalls

### Connect first: start a `driver_session` on the per-instance port

Every `mcp__tauri__*` tool (`manage_window`, `webview_execute_js`, `webview_screenshot`, the `ipc_*` tools) needs an
active driver session, or it fails with "No active session. Call driver_session with action 'start' first". The bridge's
default port (9223) is **not** the running app's port, so a bare `driver_session action: "start"` connects to nothing.

Read the actual port from `<CMDR_DATA_DIR>/tauri-mcp.port` and pass it explicitly:

```
# dev instance: cat "~/Library/Application Support/com.veszelovszki.cmdr-dev/tauri-mcp.port"
driver_session action: "start", port: <that number>
```

(Per-worktree dev sessions live at `…/com.veszelovszki.cmdr-dev-<slug>/tauri-mcp.port`; prod at the non-`-dev` data
dir.) Use `driver_session action: "status"` to check, `"stop"` to disconnect.

### After an app restart, `stop` the session before starting a new one

Across an app restart every `webview_*` call fails with an EMPTY error, and re-pointing the session at the new port on
its own doesn't fix it: the session has to be torn down and rebuilt (`driver_session action: "stop"`, then `"start"`
with the port re-read from `tauri-mcp.port`). The bridge is a third-party plugin (`tauri-plugin-mcp-bridge`) talking to
an external MCP server, so neither end of that handshake is Cmdr's code to fix; the empty error is the whole symptom.

Note the port really does move: the wrapper allocates a fresh ephemeral bridge port per `pnpm dev` launch
(`tauri-wrapper.ts`, `pickEphemeralPort`), so a session started before a restart is pointing at a dead socket. A watcher
rebuild (a Rust edit) keeps the port, since the child process inherits `CMDR_MCP_BRIDGE_PORT`; a full `pnpm dev` restart
does not.

### Window management: use `manage_window`, not JS APIs

The Tauri webview's `window.__TAURI__` JS APIs (e.g. `getCurrentWindow().setSize()`) are gated by per-window capability
permissions in `src-tauri/capabilities/`. Calling them via `webview_execute_js` will fail with "not allowed" errors
unless the specific permission is granted.

Instead, use the Tauri MCP's dedicated `manage_window` tool:

```
manage_window action: "resize", width: 1142, height: 705, logical: true
manage_window action: "info"
manage_window action: "list"
```

These operate through the MCP bridge and bypass capability restrictions.

**General rule**: before reaching for `webview_execute_js` with `window.__TAURI__` APIs, check if there's a dedicated
Tauri MCP tool for the job. `webview_execute_js` is best for DOM manipulation and reading app state, not for Tauri
platform APIs.
