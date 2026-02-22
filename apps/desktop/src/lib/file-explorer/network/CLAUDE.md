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

## Gotchas

- `network` volume ID is virtual — `smb://` path is a sentinel, not a real mount.
- Mounted SMB shares appear as separate `VolumeInfo` entries with real volume IDs.
- Credential status is keyed by lowercase `host.name` (not IP or hostname).
- Share prefetch is fire-and-forget — errors are silently discarded.
- `hasSmbCredentials` does not exist in the codebase. `tryStoredCredentials` calls `getSmbCredentials` directly — a
  separate pre-check would cause an extra macOS Keychain dialog (each Keychain access can trigger a system permission
  prompt).

## Dependencies

- `$lib/tauri-commands` — `listNetworkHosts`, `resolveNetworkHost`, `listSharesOnHost`, `listSharesWithCredentials`,
  `prefetchShares`, `getSmbCredentials`, `saveSmbCredentials`, `getUsernameHints`, `getKnownShareByName`,
  `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`
- `$lib/settings/network-settings` — `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `../navigation/keyboard-shortcuts` — `handleNavigationShortcut`
- `../types` — `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
