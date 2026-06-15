# Network browser details

Depth and rationale. `CLAUDE.md` holds the must-knows; the architecture, flows, and full decision detail are here.

## `network-store.svelte.ts`

Module-level `$state`, consumed via exported getter functions (never import raw state).

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
2. `startPrefetchShares(host)`: calls `prefetchSharesCmd` (backend caches result), then `fetchSharesSilent` to populate
   `shareStates`.

Key exported functions: `getNetworkHosts()` (sorted copy), `fetchShares(host)` (explicit, throws on error),
`refreshSharesIfStale(host)`, `refreshAllStaleShares()` (call on entering network view),
`checkCredentialsForHost(serverName)` (one-time Keychain probe, idempotent), `forgetCredentials(serverName)`,
`setCredentialStatus` / `getCredentialStatus` (in-memory only), `setShareState` / `clearShareState`,
`getDiscoveryState()`, `isHostResolving(hostId)`, `getShareState(hostId)`, `getShareCount(hostId)`,
`isListingShares(hostId)`, `isShareDataStale(hostId)`.

## `NetworkBrowser.svelte`

Host table (Name, IP, Hostname, Shares, Status), reads from `network-store` getters. A "Connect to server..." pseudo-row
sits at the bottom (keyboard navigable, "+" icon, italic), firing `onConnectToServer`; not counted in the status-bar
host count, so total navigable items = `hosts.length + 1`. Keyboard nav via `handleNavigationShortcut`
(`../navigation/keyboard-shortcuts`); Left/Right jump to first/last.

F8 on a host row removes manual hosts (with confirmation) or toasts "Can't remove discovered hosts"; ignored on the
connect row. Right-click shows a native OS context menu (`show_network_host_context_menu`): Disconnect (always), Forget
server (manual), Forget saved password (hosts with stored creds); credential status is Keychain-checked before showing
if unknown; actions arrive via the `network-host-context-action` event. Cursor auto-clamps when a host is removed.

Exports for parent: `setCursorIndex(index)`, `findItemIndex(name)`, `handleKeyDown(e)`.

## `ShareBrowser.svelte`

Auth flow on mount:

1. Check `shareStates` cache; use if loaded.
2. If cache shows `auth_required` / `signing_required`: call `tryStoredCredentials()`, which calls `getSmbCredentials`
   directly (no `hasSmbCredentials` pre-check, to avoid a redundant Keychain dialog). If stored creds work,
   `authenticatedCredentials` is set and auth is transparent; otherwise show `NetworkLoginForm`.
3. If cache shows another error (`host_unreachable`, `timeout`, ...): fall through to a fresh fetch (user-initiated host
   open is an implicit retry; the background prefetch may have run before the host was ready).
4. Otherwise (no cache or loading): `fetchShares(host)`, same auth fallback.

`authenticatedCredentials` is passed to `onShareSelect` so the caller mounts without re-prompting.

The stored-creds attempt matters because the share list often loads via the SYSTEM Keychain (`smbutil view -N`) without
exercising Cmdr's own creds, so `authenticatedCredentials` is null even when a working password is saved. ShareBrowser's
own `NetworkLoginForm` appears ONLY when the share **listing** needs auth (`loadShares`); cancelling returns to the host
list.

When `authenticatedCredentials` is set, a "Forget saved password" button appears in the header; clicking it calls
`forgetCredentials` and clears `authenticatedCredentials`. Shares sort case-insensitively. Escape/Backspace go back.

The `autoMountShare` prop fires once per distinct value (tracked via `lastAutoMountAttempt`), not once per instance, so
"Copy path between panes" can auto-mount a different share without forcing a remount when the source cursor moves to
another share on the same host.

## `NetworkLoginForm.svelte`

Props: `host`, `shareName?`, `authMode`, `defaultConnectionMode?`, `initialUsername?`, `errorMessage?`, `isConnecting?`,
`onConnect`, `onCancel`.

- Guest/credentials radio when `authMode === 'guest_allowed'`.
- Pre-fills username from `getUsernameHints()` (server-keyed) or `getKnownShareByName()`; an explicit `initialUsername`
  (for example from a failed mount) wins over both.
- Tab stops propagation (prevents the parent pane-switch while tabbing between fields).
- `connectionMode` is `$derived.by` from `authMode` (guest default when guest allowed). `bind:group` writes a `let`, not
  the read-only derived; the derived re-evaluates when `authMode` changes.

## Data flow

```
App startup
  └─ initNetworkDiscovery() → listNetworkHosts() + event listeners
       └─ startResolution() → resolveNetworkHost()
            └─ startPrefetchShares() → prefetchSharesCmd() → fetchSharesSilent()

User opens Network volume → NetworkBrowser mounts → refreshAllStaleShares()

User double-clicks host → ShareBrowser mounts → loadShares()
       ├─ cache hit → render
       └─ auth required → tryStoredCredentials() → login form if needed

User activates "Connect to server..." row → ConnectToServerDialog opens
       └─ connectToServer(address) → TCP check → inject host
            └─ onConnect(host, sharePath)
                 ├─ ShareBrowser mounts (host set)
                 └─ if sharePath → autoMountShare triggers mount
```

## Mount-phase auth failures

`NetworkMountView.svelte` (in `../pane/`) renders `NetworkLoginForm` instead of its error pane whenever
`mountNetworkShare` rejects with an auth-class error (`auth_failed` / `auth_required`, including the NetAuth -6600 code
the backend maps). The form shows the error inline, pre-fills the previously tried username via `initialUsername`,
retries the mount on submit, and saves via `saveSmbCredentials` on success when "Remember in Keychain" is checked.
Escape/Cancel returns to the share list. Non-auth errors (unreachable, timeout, share gone) keep the error pane with
"Try again" / "Back". Pinned by `../pane/NetworkMountView.test.ts`.

## SMB live-reconnect flow (cross-component)

When a direct-SMB session drops mid-use, four pieces coordinate to recover:

1. **Backend** (`SmbVolume::handle_smb_result` in `volume/smb.rs`) detects `ConnectionLost` / `SessionExpired`, flips to
   `Disconnected`, and emits `smb-connection-changed { volumeId, state: "disconnected" }`. (See
   `volume/backends/DETAILS.md` § SMB live-reconnect lifecycle.)
2. **`stores/volume-store.svelte.ts`** patches the matching volume's `smbConnectionState`, keeping the picker dot, the
   breadcrumb, and `currentVolumeInfo` reactive without waiting for the next `volumes-changed`.
3. **`smb-reconnect-manager.svelte.ts`** (if any subscribers are present) starts a per-volume backoff cycle calling
   `reconnectSmbVolume(volumeId)` per tick. Resolves when the BE emits a follow-up `state: "direct"` event.
4. **`FilePane.svelte`** subscribes via `$effect` whenever the pane is on an SMB volume. Subscription is refcounted
   (both panes on one share share a cycle). During an active cycle FilePane swaps the list for `SmbReconnectingView`; on
   `gave-up` it swaps to `VolumeUnreachableBanner` (`smbGaveUp` variant); on success the `onSuccess` callback re-runs
   `loadDirectory`.

Auth-failure give-up → "Sign in", not "unreachable" (`needs-auth` status): when reconnect fails on an auth error the
saved password can't fix, the backend emits `state: "needs_auth"`. The manager's `handleNeedsAuth` stops the backoff
(retrying a stale password is futile) and flips to `needs-auth`; FilePane shows `pane/SmbReauthView.svelte` (a thin
wrapper over `NetworkLoginForm`). Submitting calls `reconnectSmbVolumeWithCredentials(volumeId, …)`, which persists the
new password and reconnects; success arrives as a `direct` event that clears the state and reloads. Pinned by
`smb-reconnect-manager.svelte.test.ts`.

Lazy-nav path: opening a share that's already `Disconnected` (no fresh event in flight), the FilePane `$effect` notices
`currentVolumeInfo?.smbConnectionState === 'disconnected'` and calls `manager.startCycle(volumeId)` directly.

Disconnect button: `disconnectSmbVolume(volumeId)` shells out to `diskutil unmount` (macOS) → FSEvents fires →
`SmbVolume::on_unmount` → volume removed from `VolumeManager` → `volumes-changed` removes it from the picker.

## Lazy mDNS trigger

`triggerNetworkDiscovery()`:

1. No-ops if `network.enabled === false`.
2. Calls `ensureNetworkDiscoveryStarted()` (idempotent backend command; the first call kicks off the mDNS daemon, firing
   the macOS "Cmdr wants to find devices on local networks" prompt the first time).
3. Sets `network.firstTriggerDone = true` so subsequent launches start mDNS eagerly (returning users get full speed
   without re-prompts).

Call sites: `NetworkBrowser.onMount`, `ConnectToServerDialog.onMount` (manual entry opens a TCP socket to a private IP,
which triggers the prompt anyway), and `VolumeBreadcrumb.handleSubmenuAction` (the OS-mount → direct-smb2 upgrade also
opens a private-IP socket). Backend side: `src-tauri/src/network/DETAILS.md` § "Lazy mDNS startup".

## Key decisions

- **Lazy discovery on first user intent, not at startup**: avoids the macOS Local Network prompt on fresh installs
  before the user has context; `network.firstTriggerDone` persists so returning users keep the warm-cache benefit.
- **Resolution and share prefetch are fire-and-forget**: hosts come and go, so a timeout / unreachable during prefetch
  is normal, not worth surfacing. The UI shows "Not checked" / "Waiting..." until data arrives; only user-initiated
  actions surface errors.
- **State via getters, not raw `$state` exports**: raw exports lose reactivity when imported from a plain `.ts`; getters
  work everywhere and make the API boundary explicit.
- **`tryStoredCredentials` skips the `hasSmbCredentials` pre-check**: two Keychain calls = two system prompts; one
  direct call plus catch = one.

## Gotchas

- **`currentNetworkHost` lives in both `NetworkMountView` (local) and `FilePane` (via `initialNetworkHost` +
  `onNetworkHostChange`).** When `NetworkMountView` mutates its copy (mount success, back), it must propagate via
  `onNetworkHostChange`, or switching away from Network and back re-mounts with a stale host (bit E2E test 436 "unicode
  shares render" where a prior guest-share mount left FilePane stuck on guest).
- **`network` volume ID is virtual**: the discovery UI has no real mount point until a share is mounted via
  `mount_smbfs`. Mounted shares then appear as separate `VolumeInfo` entries with real IDs.
- **Credential status keyed by lowercase `host.name`**: the same physical host can change IP (DHCP) and hostname (mDNS
  vs DNS); the Bonjour service name is the stable identifier. Lowercasing avoids case mismatches.
- **Tab in `NetworkLoginForm` calls `stopPropagation()`**: the parent reads Tab as pane-switch otherwise.
- **Host list MCP sync encodes metadata into the `name` field** as a flat string because MCP `PaneFileEntry` has only
  `name` / `path` / `isDirectory`; the encoding lets agents read what the UI shows without a schema change.

## Dependencies

- `$lib/tauri-commands`: `listNetworkHosts`, `resolveNetworkHost`, `listSharesOnHost`, `listSharesWithCredentials`,
  `prefetchShares`, `getSmbCredentials`, `saveSmbCredentials`, `deleteSmbCredentials`, `getUsernameHints`,
  `getKnownShareByName`, `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`, `connectToServer`,
  `removeManualServer`
- `$lib/settings/network-settings`: `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `$lib/utils/confirm-dialog`: `confirmDialog`
- `$lib/ui/toast`: `addToast`
- `../navigation/keyboard-shortcuts`: `handleNavigationShortcut`
- `../types`: `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
