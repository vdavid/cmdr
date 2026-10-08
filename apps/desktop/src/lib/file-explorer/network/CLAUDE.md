# Network browser

SMB discovery and browsing: the servers hub, a host's places, the direct-connect upgrade, and the reconnect manager.
Signing in is `$lib/servers`' one sheet; this module only says what to ask and what to do with the answer.

## Module map

- `network-store.svelte.ts`: the `$state` singleton behind all network data. `lazy-trigger.ts`: the network-action hook.
- `ServersHub.svelte` (+ `servers-hub-{rows,items,keys,actions,mcp,columns}.ts`, `servers-hub-list.svelte.ts`,
  `host-status.ts`, `ServersHubRowMenu.svelte`, `ServersHubStatusBar.svelte`): the hub table on the `network` volume.
  `PlacesBrowser.svelte` (+ `PlacesHeader.svelte`): one account's places, the listing's sign-in, and "Sign in as…".
  `smb-sign-in.ts`: SMB's side of the sheet. `list-cursor.ts`: both lists' arrow keys. `mount-error-messages.ts` (+
  `mount-error.ts`): a refused mount's words.
- `direct-connect.ts` (+ `upgrade-messages.ts`, `LocalNetworkBlockedToastContent.svelte`): the "Connect directly"
  upgrade. `os-mount-notice-bridge.ts` + `SmbOsMountFallbackToastContent.svelte`: the slow-connection notice.
  `smb-reconnect-manager.svelte.ts`: the per-volume backoff cycle on `volume-connection-changed` (backend-neutral; SMB
  is its first emitter).

## Must-knows

- **❌ Never import raw `$state` from `network-store.svelte.ts`; use the exported getters.** Svelte 5 `$state` is
  reactive only inside `.svelte` / `.svelte.ts`, so a raw import from a plain `.ts` silently loses reactivity.
- **`triggerNetworkDiscovery()` is the single chokepoint for a networking intent**; ❌ don't gate on `network.enabled`
  yourself, the helper does. The browse runs while a Servers view holds it (`holdDiscoveryForServersView`).
- **❌ Never list the shares of a host Cmdr only found, unless the person opens it** (#324): listing signs in to it.
  Prefetch, the stale refresh, and ⌘R all gate on `savedSmbHostIds`; a new `listSharesOnHost` caller must too. Even a
  saved server's prefetch waits for a Servers view on screen: nothing is listed at launch.
- **The hub's cursor counts what's on screen; MCP indexes the full list** (a collapsed nearby group hides rows). Cross
  with `servers-hub-items.ts`, ❌ never index `rows` by the cursor. DETAILS § "The nearby group".
- **❌ Never ask the Keychain twice**: each access can raise a system prompt. No `hasSmbCredentials` pre-check before
  `getSmbCredentials`, and share activation never pre-prompts (`activateShare`): try stored creds, mount, and let the
  mount's own refusal ask (`PlacesBrowser.test.ts` pins it). "Is one stored?" for a button reads the backend's in-memory
  cache only (`noteCachedCredentials`).
- **`direct-connect.ts::connectDirectly` is the ONE upgrade flow** (the chip's yellow dot, the switcher's
  direct-connection switch, the fallback notice): route a new entry point through it. Every answer is a typed
  `UpgradeResult` status worded by `upgrade-messages.ts`; ❌ never toast `String(e)`.
- **A credential is asked for on the sheet and ❌ never in the pane**, through `smb-sign-in.ts`. The three sites, what
  each `attempt` runs, and where cancelling lands: `../../servers/DETAILS.md`.
- **`NetworkMountView` must propagate its local `currentNetworkHost` via `onNetworkHostChange`**, mirrored in the parent
  `FilePane` (`initialNetworkHost`). Without it, leaving Network and coming back re-mounts a stale host and opens
  `PlacesBrowser` for the wrong one.
- **"as testuser" / "as guest" comes only from what's known** (`signed-in-as.ts`), ❌ never a Keychain read. A SHARE
  row: its live mount's account. A SERVER row and its share list's header: the same server-level account (the listing's,
  `getListedAccount`, else the one it's set to be used with), ❌ never a share's mount, which made the two disagree.
- **Credential status is keyed by lowercase `host.name`** (the stable Bonjour name); IP and hostname both drift.
- **The `network` volume id is virtual**: its `smb://` path is a sentinel, not a mount. Mounted shares arrive as
  separate `VolumeInfo` entries with real ids.

Architecture, the auth hand-offs, the reconnect flow, and decision rationale: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
