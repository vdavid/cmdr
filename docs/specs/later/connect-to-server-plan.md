# Connect to server

## Why

SMB shares that don't advertise via mDNS are invisible to Cmdr. This is common: Windows file servers, Samba without
Avahi, NAS on different subnets, VPN-connected shares, and Docker test containers. Every file manager has manual connect
(Finder's ⌘K, Windows `\\server\share`). This is table stakes for shipping.

Secondary win: unblocks Docker-based E2E testing of network features on macOS, where mDNS from Docker containers can't
reach the host.

## What we're building

1. A **"Connect to server..." pseudo-row** at the bottom of the network host list (+ icon, activatable like any row).
2. A **connect dialog** with a text input, help text, and Connect/Cancel buttons.
3. **Backend** that parses the address, does a TCP reachability check, and injects a synthetic host into the discovery
   state — from there, the existing share listing / auth / mount pipeline takes over.
4. **Persistence** of manually-added servers across restarts (new `manual-servers.json` file).
5. **Removal** via F8 or right-click context menu (only for manually-added hosts, not discovered ones).
6. **MCP tool** `connect_to_server` for automation.

## What we're NOT building

- **Favorites / recent servers list.** No history UI, no starred servers. Just add and remove.
- **Non-SMB protocols.** Only SMB. The parser should recognize unsupported protocols and show a clear message, but not
  connect.
- **Server browsing / WS-Discovery.** Only manual address entry.

## Design decisions

### Pseudo-row in host list (not a toolbar button or address bar)

The "Connect to server..." action lives inside the host list as the last row, with a "+" icon. Activated by
Enter/double-click like any host row. **Why:** Consistent with Cmdr's paradigm — everything is a list item you navigate
to and act on. No separate toolbar to discover. The host list already has keyboard navigation; adding a pseudo-row means
the feature is automatically keyboard-accessible.

### TCP check in the dialog, not after adding to list

The dialog does a quick TCP connect to port 445 (or custom port) before adding the host. If unreachable, error is shown
inline in the dialog. **Why:** Prevents dead hosts from cluttering the list. The user can fix their typo and retry
without navigating away. Discovered hosts get to be in "resolving..." states because mDNS guarantees they exist;
manually-typed addresses have no such guarantee.

**Note:** TCP reachability proves the port is open, not that SMB is working. A reachable host can still fail at the SMB
level (wrong protocol, auth issues, etc.). That's fine — those errors are handled by the existing share listing pipeline
once the host is in the list. The TCP check's job is to catch typos and unreachable hosts early.

### Separate `manual-servers.json` (not extending `known-shares.json`)

`known-shares.json` stores share-level connection history (server+share pairs with auth mode and timestamps).
Manually-added servers are server-level data with different semantics. **Why:** Clean separation of concerns. The files
have different lifecycles — a manual server entry exists because the user explicitly added it, while known-share entries
are auto-created on first connection.

### Synthetic NetworkHost injected via existing event system

When a server is added, the backend creates a `NetworkHost` with `source: Manual` and a `manual-` prefixed ID, then
calls `on_host_found()` (which inserts into `DISCOVERY_STATE` and emits `network-host-found`). **Why:** Reuses the
entire existing pipeline. The frontend's `network-store` picks it up automatically. Share listing, auth, mounting — all
work without changes.

**NetworkHost field mapping for manual hosts:**

| Input type                   | `name`                 | `hostname`              | `ip_address`            | `port` |
| ---------------------------- | ---------------------- | ----------------------- | ----------------------- | ------ |
| IP `192.168.1.100`           | `"192.168.1.100"`      | `Some("192.168.1.100")` | `Some("192.168.1.100")` | 445    |
| IP:port `192.168.1.100:9445` | `"192.168.1.100:9445"` | `Some("192.168.1.100")` | `Some("192.168.1.100")` | 9445   |
| Hostname `mynas`             | `"mynas"`              | `Some("mynas")`         | None                    | 445    |
| Hostname `mynas.local`       | `"mynas.local"`        | `Some("mynas.local")`   | None                    | 445    |

**Why `hostname` is always set:** The share listing pipeline (`fetchShares` / `fetchSharesSilent` in network-store)
guards on `host.hostname` being truthy — it silently returns without fetching if hostname is missing. Setting hostname
ensures manual hosts flow through the pipeline. For IP inputs, hostname = IP is fine because `smb-rs` and
`smbutil`/`smbclient` both accept IPs as the connect target.

**Display name rules:** IP or hostname without non-default port shows the bare address (`192.168.1.100`, `mynas`). With
a non-default port, the port is appended (`192.168.1.100:9445`, `mynas:9445`). The `share_path` is never part of the
display name.

### `source` field on NetworkHost to distinguish manual from discovered

Add `source: HostSource` enum (`Discovered` | `Manual`) to `NetworkHost`. Defaults to `Discovered` for backward
compatibility. Serialized to frontend. **Why:** The frontend needs to know which hosts can be deleted (manual only), and
the status display differs (manual hosts skip "Resolving..." since there's no mDNS to resolve).

### F8 for removal (matching file delete)

F8 on a manual host opens a confirmation dialog, then removes the host and deletes it from `manual-servers.json`. Auto-
discovered hosts ignore F8. Right-click context menu also shows "Remove" for manual hosts. **Why:** F8 = delete is
deeply ingrained in Cmdr's interaction model. Reusing it for "remove manual server" is intuitive. The confirmation
dialog prevents accidents.

### Skip mDNS resolution for manual hosts

The frontend's `network-host-found` event handler calls `startResolution(host)` for newly discovered hosts. For manual
hosts, resolution must be skipped — there's no mDNS service to resolve, and `resolveNetworkHost` would call
`service_name_to_hostname()` which appends `.local` (nonsensical for `192.168.1.100`). **Why:** The existing
`hostname`-is-truthy guard in `startResolution` would handle this IF hostname is already set (which it is, per the field
mapping above). But as a safety measure, the event handler should also check `host.source !== 'manual'` before starting
resolution. This makes the intent explicit.

### No shortcut for now

The pseudo-row is keyboard-accessible via normal arrow navigation + Enter. A dedicated shortcut like ⌘K is not necessary
for v1 and risks confusion (Cmd+K is overloaded across apps). Can be added later if users ask for it.

### No IPv6 for now

IPv6 addresses (`[::1]:9445`, `fe80::1`) have special parsing requirements and would complicate ID generation. For v1,
if the parser detects an IPv6 address (contains `:` without a `://` prefix or starts with `[`), show: "IPv6 addresses
aren't supported yet. Use an IPv4 address or hostname."

### Duplicate detection: manual + discovered hosts for the same server

A user may manually add a server that later appears via mDNS (or was already discovered but mDNS was slow). The manual
host gets `manual-{address}` ID while the discovered one gets a service-name-based ID — they're different entries. For
v1, this is a known limitation: the user sees two entries for the same machine. They can remove the manual one once
discovery finds it. Deduplication by IP matching is a future improvement.

## Input parsing

The text input accepts flexible formats. All are normalized to `(host, port, share_path)`:

| Input                   | host            | port | share_path                          |
| ----------------------- | --------------- | ---- | ----------------------------------- |
| `192.168.1.100`         | `192.168.1.100` | 445  | None                                |
| `192.168.1.100:9445`    | `192.168.1.100` | 9445 | None                                |
| `mynas`                 | `mynas`         | 445  | None                                |
| `mynas.local`           | `mynas.local`   | 445  | None                                |
| `smb://mynas`           | `mynas`         | 445  | None                                |
| `smb://mynas:9445`      | `mynas`         | 9445 | None                                |
| `smb://mynas/docs`      | `mynas`         | 445  | `docs`                              |
| `smb://user@mynas/docs` | `mynas`         | 445  | `docs` (user ignored at parse time) |

Validation rules:

- Empty input → "Enter a server address"
- `afp://`, `nfs://`, `ftp://`, `vnc://` → "Only SMB shares are supported right now"
- IPv6 address detected → "IPv6 addresses aren't supported yet. Use an IPv4 address or hostname."
- Invalid characters or malformed URL → "Couldn't parse this address. Try a hostname, IP, or smb:// URL."
- Port out of range → "Port must be between 1 and 65535"

---

## Milestone 1: Backend — manual server storage and injection

**Goal:** `connect_to_server` Tauri command that parses an address, checks TCP reachability, persists the server, and
injects it into the discovery state.

### 1a. Add `HostSource` to `NetworkHost`

**File:** `apps/desktop/src-tauri/src/network/mod.rs`

Add a `HostSource` enum and a `source` field to `NetworkHost`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSource {
    Discovered,
    Manual,
}

impl Default for HostSource {
    fn default() -> Self { Self::Discovered }
}
```

Add to `NetworkHost`:

```rust
#[serde(default)]
pub source: HostSource,
```

`#[serde(default)]` means existing serialized hosts without the field deserialize as `Discovered`.

Update the frontend `NetworkHost` type in `apps/desktop/src/lib/file-explorer/types.ts` to include
`source?: 'discovered' | 'manual'`.

### 1b. Create `manual_servers.rs`

**File:** `apps/desktop/src-tauri/src/network/manual_servers.rs`

Add `pub mod manual_servers;` to `apps/desktop/src-tauri/src/network/mod.rs` (alongside the existing module
declarations).

Responsibilities:

- Parse addresses (the flexible format table above)
- TCP reachability check (connect to `host:port` with a 5-second timeout)
- Read/write `manual-servers.json` (same `app_data_dir` as `known-shares.json`)
- On app startup, load persisted servers and inject them into `DISCOVERY_STATE`
- Remove a manual server by ID

**Persistence format:**

```json
{
  "servers": [
    {
      "id": "manual-192-168-1-100-9445",
      "displayName": "192.168.1.100:9445",
      "address": "192.168.1.100",
      "port": 9445,
      "addedAt": "2026-04-02T10:00:00Z"
    }
  ]
}
```

The `id` is deterministic: `manual-{address}-{port}` with dots/colons replaced by dashes. This prevents duplicate
entries for the same server.

**Key functions:**

```rust
/// Parse user input into (host, port, share_path).
pub fn parse_server_address(input: &str) -> Result<ParsedAddress, ParseError>

/// TCP connect check with 5-second timeout.
pub async fn check_reachability(host: &str, port: u16) -> Result<(), String>

/// Add a manual server: persist + inject into DISCOVERY_STATE + emit event.
pub async fn add_manual_server(input: &str, app_handle: &AppHandle) -> Result<ManualConnectResult, String>

/// Remove a manual server: delete from storage + DISCOVERY_STATE + emit host-lost.
pub fn remove_manual_server(server_id: &str, app_handle: &AppHandle) -> Result<(), String>

/// Load persisted servers and inject into DISCOVERY_STATE. Called at startup.
pub fn load_manual_servers(app_handle: &AppHandle) -> Result<(), String>
```

`ManualConnectResult` returns the `NetworkHost` and optional `share_path` (if the user typed `smb://host/share`).

### 1c. Register Tauri commands

**File:** `apps/desktop/src-tauri/src/commands/network.rs` (or wherever network commands are registered)

```rust
#[tauri::command]
pub async fn connect_to_server(address: String, app_handle: AppHandle) -> Result<ManualConnectResult, String>

#[tauri::command]
pub fn remove_manual_server(server_id: String, app_handle: AppHandle) -> Result<(), String>
```

### 1d. Load manual servers at startup

Call `load_manual_servers(app_handle)` from the Tauri `setup()` closure, alongside or just after `start_discovery`. This
runs before the frontend window loads, so when the frontend's `initNetworkDiscovery()` calls `listNetworkHosts()`, the
manual servers are already in `DISCOVERY_STATE`. No emit needed at this stage — the frontend subscribes to events after
`initNetworkDiscovery()`, and `listNetworkHosts()` returns the full `DISCOVERY_STATE` snapshot including manual hosts.

### 1e. Unit tests

- `parse_server_address`: all formats from the table, plus error cases (empty, unsupported protocols, IPv6 rejection,
  out-of-range ports, malformed URLs)
- ID generation: deterministic, no duplicates for same host+port
- Serialization round-trip for `ManualServerEntry`
- NetworkHost field mapping: verify hostname/ip_address/name are set correctly for IP vs hostname inputs

**Verify:** `cd apps/desktop/src-tauri && cargo test manual_servers && cd ..`

---

## Milestone 2: Frontend — "Connect to server..." row and dialog

**Goal:** The pseudo-row appears in the host list. Clicking it opens a dialog. Successful connection adds a host and
auto-navigates to it.

### 2a. "Connect to server..." pseudo-row in `NetworkBrowser.svelte`

After the last host row (and after the "Searching..." indicator if visible), render a pseudo-row:

```svelte
<div class="host-row connect-row" ...>
    <div class="col-name">
        <span class="connect-icon">+</span>
        <span>Connect to server...</span>
    </div>
</div>
```

Behavior:

- Keyboard navigable (cursor can land on it, arrow keys include it)
- Enter or double-click → opens the dialog
- Visually distinct: the "+" icon replaces the 🖥️ emoji, and the name text uses tertiary color or italic to
  differentiate it from real hosts
- Not counted in the "N hosts" status bar
- Always at the bottom, even when sorting

Adjust `cursorIndex` bounds to include this extra row. The hosts array length + 1 is the new max.

### 2b. `ConnectToServerDialog.svelte`

**File:** `apps/desktop/src/lib/file-explorer/network/ConnectToServerDialog.svelte`

A modal dialog (matching Cmdr's existing dialog style from `TransferDialog`). Structure:

1. **Title:** "Connect to server"
2. **Input field:** placeholder "hostname, IP address, or smb:// URL", auto-focused, with accent border (matching the
   transfer dialog's path input style)
3. **Help text** below input: "Examples: mynas.local, 192.168.1.100, smb://server/share"
4. **Error area** (shown conditionally): inline error message for validation or connection failures
5. **Button row:** Cancel (secondary) + Connect (primary). Connect button shows "Connecting..." with spinner during the
   TCP check.

Keyboard:

- Enter → submit (if input is non-empty)
- Escape → cancel/close
- Tab stays within the dialog (focus trap — only two focusable elements: input and Connect button)

State machine:

- `idle` → user is typing
- `connecting` → TCP check in progress (Connect button disabled + spinner, Cancel still active)
- `error` → connection failed (error message shown, input re-enabled for retry)

**Cancellation:** Clicking Cancel while in `connecting` state closes the dialog immediately. The backend TCP check may
still be in flight but it's lightweight (single socket attempt) and will time out on its own. The host is NOT added.

On success: call `onConnect(host, sharePath)` prop. Parent (`NetworkMountView`) handles the navigation — set
`currentNetworkHost` to the new host (enters `ShareBrowser`). If `sharePath` was provided (for example,
`smb://server/docs`), pass it to `ShareBrowser` as an `autoMountShare` prop. `ShareBrowser` will, after shares finish
loading, find the matching share by name and auto-trigger mount — same as if the user pressed Enter on it. This matches
Finder's behavior where typing `smb://server/share` in ⌘K mounts that share directly.

### 2c. Wire up the dialog in `NetworkMountView.svelte`

The dialog is managed by `NetworkMountView` (which already routes between NetworkBrowser/ShareBrowser/mount states). Add
a new state: `showConnectDialog`. When the dialog succeeds:

1. The host is already in the store (backend emitted `network-host-found`)
2. Set `currentNetworkHost` to the new host (enters ShareBrowser)
3. If `sharePath` was provided, pass it as `autoMountShare` prop to ShareBrowser

### 2d. `autoMountShare` prop on `ShareBrowser.svelte`

When `autoMountShare` is set, after `loadShares()` completes successfully, find the share whose name matches (case-
insensitive) and call `onShareSelect` with it — triggering the normal mount flow. If no match, show a toast: "Share
'{name}' not found on {host}." and display the share list normally so the user can pick manually. Clear the prop after
the attempt to avoid re-triggering on re-render.

**Verify:** Manual testing — navigate to Network, see the "+" row, Enter on it, type `localhost:9445`, click Connect
with Docker containers running.

---

## Milestone 3: Removal (F8 and context menu)

**Goal:** Users can remove manually-added servers. Discovered hosts are not removable.

### 3a. F8 handler in `NetworkBrowser.svelte`

In `handleKeyDown`, add F8/Delete handling:

```
if key === 'F8' and cursor is on a host (not the connect-row):
    if host.source === 'manual':
        confirmDialog("Remove {host.name} from the server list?", "Remove")
        if confirmed: call removeManualServer(host.id) Tauri command
    else:
        // Discovered hosts: ignore F8 or show toast "Can't remove discovered hosts"
```

### 3b. Context menu for manual hosts

In the right-click handler (which already shows "Forget saved password" for hosts with stored credentials), add "Remove"
as a menu item for manual hosts. Use the same `confirmDialog` + `removeManualServer` flow.

### 3c. Frontend cleanup on removal

When `network-host-lost` fires for a manual host, the existing `network-store` handler already removes the host from the
`hosts` array and deletes its `shareStates`. No additional frontend code needed for cleanup.

### 3d. Adjust "Forget saved password" flow

Manual hosts should also support "Forget saved password" if they have stored credentials. Both context menu items can
coexist: "Forget saved password" and "Remove server".

**Verify:** Manual testing — add a server, right-click it (see "Remove"), press F8 (see confirmation), confirm, verify
it's gone. Restart app, verify it stays gone.

---

## Milestone 4: MCP tool

**Goal:** Agents can add servers programmatically, enabling Docker-based E2E tests.

### 4a. `connect_to_server` MCP tool

**File:** `apps/desktop/src-tauri/src/mcp/executor.rs`

New tool:

```
connect_to_server(address: "192.168.1.100:9445")
→ OK: Connected to 192.168.1.100:9445 (host ID: manual-192-168-1-100-9445)
→ ERROR: Couldn't reach 192.168.1.100:9445 — connection refused
```

Implementation: call the `connect_to_server` Tauri command directly. Return the host ID on success (so the agent can use
it with `move_cursor` or `open_under_cursor`).

### 4b. `remove_manual_server` MCP tool

For cleanup after tests:

```
remove_manual_server(hostId: "manual-192-168-1-100-9445")
→ OK: Removed server manual-192-168-1-100-9445
```

### 4c. Update `cmdr://state` for network view

The network MCP state sync already encodes host metadata into the name field. Add the `source` field to the encoded
string so agents can tell manual from discovered hosts:

```
"i:0 d Naspolya  ip=192.168.1.111  source=discovered  ..."
"i:2 d 192.168.1.100  ip=192.168.1.100  source=manual  ..."
```

**Verify:** Start Docker SMB containers, use MCP to `connect_to_server("localhost:9445")`, verify host appears in state,
open it, list shares, mount, browse, copy a file. Then `remove_manual_server` and verify cleanup.

---

## Milestone 5: Testing and docs

### 5a. Rust unit tests

- Address parsing (all formats, edge cases, error cases)
- Manual server persistence (save, load, remove, duplicate detection)
- ID generation determinism

### 5b. Integration test with Docker containers

Feature-gated behind `integration-tests`. Start minimal SMB containers, call `connect_to_server("localhost:9445")`,
verify share listing works through the fallback path.

### 5c. Manual test checklist

Add to `docs/guides/testing/manual-checklist.md`:

- [ ] Network view shows "Connect to server..." row
- [ ] Enter on the row opens dialog
- [ ] Connect with hostname, IP, IP:port, smb:// URL
- [ ] Error shown for unreachable server
- [ ] Error shown for unsupported protocol
- [ ] Successful connect adds host, navigates to share list
- [ ] F8 on manual host shows confirmation, removes on confirm
- [ ] F8 on discovered host does nothing
- [ ] Right-click manual host shows "Remove"
- [ ] Manual hosts persist across app restart
- [ ] Removing a manual host persists across app restart

### 5d. Update CLAUDE.md files

- `apps/desktop/src-tauri/src/network/CLAUDE.md` — add `manual_servers.rs` to architecture, add design decision about
  `HostSource`, add gotcha about `manual-` ID prefix convention
- `apps/desktop/src/lib/file-explorer/network/CLAUDE.md` — add `ConnectToServerDialog.svelte` to key files table, update
  `NetworkBrowser` docs for the pseudo-row, add connect-to-server to the data flow diagram
- `apps/desktop/src-tauri/src/mcp/CLAUDE.md` — add `connect_to_server` and `remove_manual_server` tools
- `docs/guides/testing/smb-servers.md` — add section on using `connect_to_server` MCP tool with Docker containers

### 5e. Run checks

```bash
./scripts/check/check.sh
```
