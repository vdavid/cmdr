# Network browser

SMB network discovery UI: host list, per-host share list, login form, and a singleton reactive store.

## Key files

| File                      | Purpose                                                         |
| ------------------------- | --------------------------------------------------------------- |
| `network-store.svelte.ts` | Module-level `$state` singleton for all network data            |
| `NetworkBrowser.svelte`   | Host list table — rendered when pane is on the `network` volume |
| `ShareBrowser.svelte`     | Share list for a specific host, handles auth flow               |
| `NetworkLoginForm.svelte` | Credential form rendered inside `ShareBrowser`                  |

## `network-store.svelte.ts`

Module-level `$state` (reactive only in `.svelte`/`.svelte.ts` files). Consumed via exported getter functions — never
import the raw state variables.

Key state:

- `hosts: NetworkHost[]` — discovered hosts, sorted alphabetically by getters
- `discoveryState: DiscoveryState` — `'idle' | 'searching'`
- `resolvingHosts: SvelteSet<string>` — host IDs currently being resolved
- `shareStates: SvelteMap<string, ShareState>` — per-host share listing status + result
- `prefetchingHosts: SvelteSet<string>` — hosts being background-prefetched
- `credentialStatuses: SvelteMap<string, CredentialStatus>` — `'unknown' | 'has_creds' | 'no_creds' | 'failed'`

Lifecycle:

- `initNetworkDiscovery()` — call once at app startup. Idempotent. Subscribes to Tauri events (`network-host-found`,
  `network-host-lost`, `network-host-resolved`, `network-discovery-state-changed`).
- `cleanupNetworkDiscovery()` — unlisten all events, reset `initialized`.

Resolution → prefetch pipeline (fire-and-forget):

1. `startResolution(host)` — calls `resolveNetworkHost`, updates host, then calls `startPrefetchShares`.
2. `startPrefetchShares(host)` — calls `prefetchSharesCmd` (backend caches result), then triggers `fetchSharesSilent` to
   populate `shareStates`.

Key exported functions:

| Function                                    | Notes                                               |
| ------------------------------------------- | --------------------------------------------------- |
| `getNetworkHosts()`                         | Returns sorted copy                                 |
| `fetchShares(host)`                         | Explicit fetch, sets `shareStates`, throws on error |
| `refreshSharesIfStale(host)`                | Background refresh if TTL expired                   |
| `refreshAllStaleShares()`                   | Call on entering network view                       |
| `checkCredentialsForHost(serverName)`       | One-time async Keychain probe; idempotent           |
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

Keyboard navigation via `handleNavigationShortcut` from `../navigation/keyboard-shortcuts`. Arrow keys also handled
directly (Left/Right jump to first/last).

Syncs to MCP pane API (`updateLeftPaneState` / `updateRightPaneState`) on every cursor/hosts change. Host metadata is
encoded into the synthetic `name` field so MCP agents can read IP, hostname, share count, and status.

Exported for parent: `setCursorIndex(index)`, `findItemIndex(name)`, `handleKeyDown(e)`.

## `ShareBrowser.svelte`

Rendered after user selects a host. Auth flow on mount:

1. Check `shareStates` cache — use if loaded.
2. If cache shows `auth_required` / `signing_required`: call `tryStoredCredentials()`.
    - `tryStoredCredentials` calls `getSmbCredentials` directly — **no** `hasSmbCredentials` pre-check to avoid a
      redundant macOS Keychain dialog.
    - If stored creds work, `authenticatedCredentials` is set and auth is transparent to user.
    - If no stored creds, show `NetworkLoginForm`.
3. Otherwise fetch via `fetchShares(host)`, same auth fallback.

`authenticatedCredentials` is passed to `onShareSelect` so the caller can mount the share without re-prompting.

Shares displayed sorted case-insensitively. Escape/Backspace go back to host list.

## `NetworkLoginForm.svelte`

Props: `host`, `shareName?`, `authMode`, `errorMessage?`, `isConnecting?`, `onConnect`, `onCancel`.

- Shows guest/credentials radio when `authMode === 'guest_allowed'`.
- Pre-fills username from `getUsernameHints()` (server-keyed map) or `getKnownShareByName()`.
- Tab key stops propagation — prevents the parent pane-switch shortcut from firing while tabbing between fields.
- `connectionMode` is `$derived.by` from `authMode` prop (guest default when guest allowed). In Svelte 5, `$derived`
  values are read-only — the reactive behavior works because `$derived.by` re-evaluates when `authMode` changes.
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
```

## Key decisions

**Decision**: Network discovery runs at app startup, not when the user opens the Network volume **Why**: mDNS host
discovery and resolution are slow (seconds). Starting early means hosts and their share counts are already populated by
the time the user navigates to the Network view. The cost is a few background IPC calls on startup.

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

- `$lib/tauri-commands` — `listNetworkHosts`, `resolveNetworkHost`, `listSharesOnHost`, `listSharesWithCredentials`,
  `prefetchShares`, `getSmbCredentials`, `saveSmbCredentials`, `getUsernameHints`, `getKnownShareByName`,
  `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`
- `$lib/settings/network-settings` — `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `../navigation/keyboard-shortcuts` — `handleNavigationShortcut`
- `../types` — `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
