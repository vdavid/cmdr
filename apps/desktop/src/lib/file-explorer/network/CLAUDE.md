# Network browser

SMB network discovery UI: host list, per-host share list, login form, and a singleton reactive store.

## Key files

| File                              | Purpose                                                                                                                                                                                                                                          |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `lazy-trigger.ts`                 | Single chokepoint for kicking off mDNS discovery on user intent. See "Lazy mDNS trigger" below                                                                                                                                                   |
| `network-store.svelte.ts`         | Module-level `$state` singleton for all network data                                                                                                                                                                                             |
| `NetworkBrowser.svelte`           | Host list table, rendered when pane is on the `network` volume                                                                                                                                                                                   |
| `ShareBrowser.svelte`             | Share list for a specific host, handles auth flow                                                                                                                                                                                                |
| `NetworkLoginForm.svelte`         | Credential form rendered inside `ShareBrowser`                                                                                                                                                                                                   |
| `ConnectToServerDialog.svelte`    | Modal dialog for manually connecting to a server by address/IP/smb:// URL                                                                                                                                                                        |
| `smb-reconnect-manager.svelte.ts` | Per-volume backoff cycle that re-establishes a Disconnected `SmbVolume`. Listens to `smb-connection-changed` from the backend, drives the FE state machine for `SmbReconnectingView`, exposes `subscribe` / `startCycle` / `retryNow` / `cancel` |

## `network-store.svelte.ts`

Module-level `$state` (reactive only in `.svelte`/`.svelte.ts` files). Consumed via exported getter functions; never
import the raw state variables.

Key state:

- `hosts: NetworkHost[]`: discovered hosts, sorted alphabetically by getters
- `discoveryState: DiscoveryState`: `'idle' | 'searching'`
- `resolvingHosts: SvelteSet<string>`: host IDs currently being resolved
- `shareStates: SvelteMap<string, ShareState>`: per-host share listing status + result
- `prefetchingHosts: SvelteSet<string>`: hosts being background-prefetched
- `credentialStatuses: SvelteMap<string, CredentialStatus>`: `'unknown' | 'has_creds' | 'no_creds' | 'failed'`

Lifecycle:

- `initNetworkDiscovery()`: call once at app startup. Idempotent. Subscribes to Tauri events (`network-host-found`,
  `network-host-lost`, `network-host-resolved`, `network-discovery-state-changed`).
- `cleanupNetworkDiscovery()`: unlisten all events, reset `initialized`.

Resolution → prefetch pipeline (fire-and-forget):

1. `startResolution(host)`: calls `resolveNetworkHost`, updates host, then calls `startPrefetchShares`.
2. `startPrefetchShares(host)`: calls `prefetchSharesCmd` (backend caches result), then triggers `fetchSharesSilent` to
   populate `shareStates`.

Key exported functions:

| Function                                    | Notes                                               |
| ------------------------------------------- | --------------------------------------------------- |
| `getNetworkHosts()`                         | Returns sorted copy                                 |
| `fetchShares(host)`                         | Explicit fetch, sets `shareStates`, throws on error |
| `refreshSharesIfStale(host)`                | Background refresh if TTL expired                   |
| `refreshAllStaleShares()`                   | Call on entering network view                       |
| `checkCredentialsForHost(serverName)`       | One-time async Keychain probe; idempotent           |
| `forgetCredentials(serverName)`             | Deletes stored creds, sets status to `no_creds`     |
| `setCredentialStatus / getCredentialStatus` | In-memory only, not persisted                       |
| `setShareState / clearShareState`           | Used by `ShareBrowser` after successful auth        |
| `getDiscoveryState()`                       | Returns current `DiscoveryState`                    |
| `isHostResolving(hostId)`                   | Whether a host is currently being resolved          |
| `getShareState(hostId)`                     | Returns `ShareState` for a host                     |
| `getShareCount(hostId)`                     | Returns number of shares for a host                 |
| `isListingShares(hostId)`                   | Whether shares are currently being fetched          |
| `isShareDataStale(hostId)`                  | Whether cached share data has expired               |

## `NetworkBrowser.svelte`

Displays the host table (Name, IP, Hostname, Shares, Status). Reads from `network-store` getters.

A **"Connect to server..." pseudo-row** is always at the bottom with a "+" icon and italic text. It's keyboard navigable
(cursor can land on it). Enter or double-click fires `onConnectToServer` prop. Not counted in the status bar host count.
Total navigable items = `hosts.length + 1`.

Keyboard navigation via `handleNavigationShortcut` from `../navigation/keyboard-shortcuts`. Arrow keys also handled
directly (Left/Right jump to first/last).

Syncs to MCP pane API (`updateLeftPaneState` / `updateRightPaneState`) on every cursor/hosts change. Host metadata is
encoded into the synthetic `name` field so MCP agents can read IP, hostname, share count, and status. The connect row is
included in MCP sync as `+ Connect to server...` with path `smb://connect`.

Exported for parent: `setCursorIndex(index)`, `findItemIndex(name)`, `handleKeyDown(e)`.

**F8** on a host row triggers removal for manual hosts (with confirmation dialog) or shows "Can't remove discovered
hosts" toast for discovered hosts. F8 is ignored on the connect row.

**Right-click** shows a native OS context menu (via `show_network_host_context_menu` Tauri command). Items: "Disconnect"
(always), "Forget server" (manual hosts), "Forget saved password" (hosts with stored credentials). The credential status
is checked via Keychain lookup before showing the menu if it's unknown. Actions arrive asynchronously via
`network-host-context-action` Tauri event. Cursor auto-clamps when a host is removed via a `$effect` on
`totalNavigableItems`.

## `ShareBrowser.svelte`

Rendered after user selects a host. Auth flow on mount:

1. Check `shareStates` cache: use if loaded.
2. If cache shows `auth_required` / `signing_required`: call `tryStoredCredentials()`.
   - `tryStoredCredentials` calls `getSmbCredentials` directly (**no** `hasSmbCredentials` pre-check) to avoid a
     redundant macOS Keychain dialog.
   - If stored creds work, `authenticatedCredentials` is set and auth is transparent to user.
   - If no stored creds, show `NetworkLoginForm`.
3. If cache shows any other error (`host_unreachable`, `timeout`, ...): fall through to a fresh fetch. User-initiated
   host open is an implicit retry (the initial background prefetch may have run before the host was ready).
4. Otherwise (no cache or 'loading'): fetch via `fetchShares(host)`, same auth fallback.

`authenticatedCredentials` is passed to `onShareSelect` so the caller can mount the share without re-prompting.

When `authenticatedCredentials` is set (stored creds were used), a "Forget saved password" button appears in the header
row. Clicking it calls `forgetCredentials` and clears `authenticatedCredentials`.

Shares displayed sorted case-insensitively. Escape/Backspace go back to host list.

## `NetworkLoginForm.svelte`

Props: `host`, `shareName?`, `authMode`, `errorMessage?`, `isConnecting?`, `onConnect`, `onCancel`.

- Shows guest/credentials radio when `authMode === 'guest_allowed'`.
- Pre-fills username from `getUsernameHints()` (server-keyed map) or `getKnownShareByName()`.
- Tab key stops propagation, which prevents the parent pane-switch shortcut from firing while tabbing between fields.
- `connectionMode` is `$derived.by` from `authMode` prop (guest default when guest allowed). In Svelte 5, `$derived`
  values are read-only; the reactive behavior works because `$derived.by` re-evaluates when `authMode` changes.
  `bind:group` on the radio buttons writes to the `let` binding, not to a derived value.

## Data flow

```
App startup
  └─ initNetworkDiscovery()
       └─ listNetworkHosts() + event listeners
            └─ startResolution() → resolveNetworkHost()
                 └─ startPrefetchShares() → prefetchSharesCmd() → fetchSharesSilent()

User opens Network volume
  └─ NetworkBrowser mounts → refreshAllStaleShares()

User double-clicks host
  └─ ShareBrowser mounts → loadShares()
       ├─ cache hit → render
       └─ auth required → tryStoredCredentials() → login form if needed

User activates "Connect to server..." row
  └─ ConnectToServerDialog opens
       └─ connectToServer(address) → TCP check → inject host
            └─ onConnect(host, sharePath)
                 ├─ ShareBrowser mounts (host set)
                 └─ if sharePath → autoMountShare triggers mount
```

## SMB live-reconnect flow (cross-component)

When a direct-SMB session drops mid-use, four pieces coordinate to recover:

1. **Backend** (`SmbVolume::handle_smb_result` in `volume/smb.rs`) detects `ConnectionLost` / `SessionExpired`, flips
   state to `Disconnected`, and emits `smb-connection-changed { volumeId, state: "disconnected" }`. (See
   `volume/CLAUDE.md` § SMB live-reconnect lifecycle for the BE detail.)
2. **`stores/volume-store.svelte.ts`** listens for that event and patches the matching volume's `smbConnectionState`
   field, which keeps the picker dot, the breadcrumb, and `currentVolumeInfo` reactive without waiting for the next
   `volumes-changed`.
3. **`smb-reconnect-manager.svelte.ts`** also listens, and (if any subscribers are present) starts a per-volume backoff
   cycle by calling `reconnectSmbVolume(volumeId)` on each tick. Cycle resolves when the BE emits a follow-up
   `state: "direct"` event.
4. **`FilePane.svelte`** subscribes to the manager via `$effect` whenever the pane is on an SMB volume. Subscription is
   refcounted (both panes on the same share share one cycle). When the manager has an active cycle, FilePane swaps the
   file list for `SmbReconnectingView`. On `gave-up`, it swaps to `VolumeUnreachableBanner` (`smbGaveUp` variant). On
   success, the registered `onSuccess` callback re-runs `loadDirectory`.

The lazy-nav path: if the user opens a share that's already `Disconnected` (no fresh event in flight), the FilePane
`$effect` notices `currentVolumeInfo?.smbConnectionState === 'disconnected'` and calls `manager.startCycle(volumeId)`
directly.

The Disconnect button: `disconnectSmbVolume(volumeId)` Tauri command shells out to `diskutil unmount` (macOS) → FSEvents
fires → `SmbVolume::on_unmount` runs → volume removed from `VolumeManager` → `volumes-changed` event removes it from the
picker.

## Lazy mDNS trigger

`lazy-trigger.ts` exports a single `triggerNetworkDiscovery()` function. Call it whenever the user signals intent to do
networking. It:

1. No-ops if `network.enabled === false`.
2. Calls `ensureNetworkDiscoveryStarted()` (idempotent backend command; first call kicks off the mDNS daemon, which in
   turn fires the macOS "Cmdr wants to find devices on local networks" prompt the very first time it runs).
3. Sets `network.firstTriggerDone = true` so subsequent app launches start mDNS eagerly (returning users get full speed
   without re-prompts).

Call sites: `NetworkBrowser.onMount` (entering the Network view), `ConnectToServerDialog.onMount` (manual server entry
opens a TCP socket to a private IP, which would trigger the prompt anyway), and `VolumeBreadcrumb.handleSubmenuAction`
(the OS-mount → direct-smb2 upgrade also opens a private-IP socket).

Don't gate on `network.enabled` at the call site: the helper is the single chokepoint.

## Key decisions

**Decision**: Network discovery runs lazily on first user intent, not at app startup **Why**: macOS's Local Network
permission prompt fires the moment we start mDNS browsing. Doing that at app launch forces the prompt on fresh installs
before the user has any context. We defer to `triggerNetworkDiscovery()` calls from user actions (Network click, Connect
to server…, smb2 upgrade) and persist `network.firstTriggerDone` so returning users still get the warm-cache benefit.
See `src-tauri/src/network/CLAUDE.md` § "Lazy mDNS startup" for the backend side. The old behavior (start at launch) is
preserved for `smb-e2e` feature builds so tests don't have to wait for discovery.

**Decision**: Resolution and share prefetch are fire-and-forget (non-blocking, errors silently discarded) **Why**:
Network hosts come and go. A timeout or unreachable host during prefetch is normal, not an error worth showing. The UI
shows "Not checked" or "Waiting..." until data arrives. Only user-initiated actions (double-click, explicit fetch)
surface errors.

**Decision**: State exposed via getter functions, not raw `$state` exports **Why**: Svelte 5 `$state` is only reactive
inside `.svelte` and `.svelte.ts` files. Exporting raw state variables would silently lose reactivity if imported from a
plain `.ts` file. Getter functions work everywhere and make the API boundary explicit.

**Decision**: `tryStoredCredentials` calls `getSmbCredentials` directly without a `hasSmbCredentials` pre-check **Why**:
Each macOS Keychain access can trigger a system permission prompt. Calling `hasSmbCredentials` then `getSmbCredentials`
would be two prompts. Calling `getSmbCredentials` directly and catching the error reduces it to one.

**Decision**: `connectionMode` in `NetworkLoginForm` is `$derived.by` from `authMode`, with `bind:group` on radio
buttons **Why**: In Svelte 5, `$derived` values are read-only. The radio `bind:group` writes to a `let` binding, not the
derived value. When `authMode` prop changes (e.g. on retry), the derived re-evaluates and resets the radio selection to
the correct default. This avoids stale connection mode after auth mode changes.

## Gotchas

**Gotcha**: `currentNetworkHost` lives in both `NetworkMountView` (local) and its parent `FilePane` (via the
`initialNetworkHost` prop + `onNetworkHostChange` callback). When `NetworkMountView` mutates its local copy (mount
success, back button), it must propagate the new value via `onNetworkHostChange`. Otherwise, when the user switches
volumes away from Network and back, `FilePane` re-mounts `NetworkMountView` with the stale `initialNetworkHost` and
`ShareBrowser` opens for the old host instead of the host list. Bit E2E test 436 ("unicode shares render") where a prior
test that mounted a guest share left FilePane stuck on guest.

**Gotcha**: `network` volume ID is virtual -- `smb://` path is a sentinel, not a real mount **Why**: The network browser
is a discovery UI, not a filesystem view. There is no real mount point until the user selects a share and it gets
mounted via `mount_smbfs`. Mounted shares then appear as separate `VolumeInfo` entries with real volume IDs.

**Gotcha**: Credential status is keyed by lowercase `host.name`, not by IP or hostname **Why**: The same physical host
can have different IPs (DHCP) and different hostnames (mDNS vs DNS). The Bonjour service name (`host.name`) is the most
stable identifier across network changes. Lowercasing avoids case-sensitive mismatches.

**Gotcha**: Tab key in `NetworkLoginForm` calls `stopPropagation()` **Why**: The parent pane handler interprets Tab as a
pane-switch shortcut. Without `stopPropagation`, pressing Tab to move from the username field to the password field
would switch panes instead.

**Gotcha**: Host list MCP sync encodes metadata into the `name` field as a flat string **Why**: The MCP `PaneFileEntry`
type only has `name`, `path`, and `isDirectory`. There is no metadata field. Encoding IP, hostname, share count, and
status into the name string is a workaround so MCP agents can read the same info the UI shows without a schema change.

## Dependencies

- `$lib/tauri-commands`: `listNetworkHosts`, `resolveNetworkHost`, `listSharesOnHost`, `listSharesWithCredentials`,
  `prefetchShares`, `getSmbCredentials`, `saveSmbCredentials`, `deleteSmbCredentials`, `getUsernameHints`,
  `getKnownShareByName`, `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`, `connectToServer`,
  `removeManualServer`
- `$lib/settings/network-settings`: `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `$lib/utils/confirm-dialog`: `confirmDialog` (used by `NetworkBrowser` for forget-password confirmation)
- `$lib/ui/toast`: `addToast` (feedback after credential operations)
- `../navigation/keyboard-shortcuts`: `handleNavigationShortcut`
- `../types`: `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
