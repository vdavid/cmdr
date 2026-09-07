# Network browser

SMB discovery and browsing: the servers hub, a host's places, the direct-connect upgrade, and the reconnect manager.
Signing in is `$lib/servers`' one sheet; this module only says what to ask and what to do with the answer.

## Module map

- `network-store.svelte.ts`: the `$state` singleton behind all network data. `lazy-trigger.ts`: starting mDNS.
- `ServersHub.svelte` (+ `servers-hub-{rows,actions,mcp}.ts`, `host-status.ts`): the hub table on the `network` volume.
  `PlacesBrowser.svelte`: one account's places, and the listing's sign-in. `smb-sign-in.ts`: SMB's side of the sheet.
- `direct-connect.ts` (+ `upgrade-messages.ts`): the "Connect directly" upgrade. `os-mount-notice-bridge.ts` +
  `SmbOsMountFallbackToastContent.svelte`: the slow-connection notice. `smb-reconnect-manager.svelte.ts`: the per-volume
  backoff cycle on `volume-connection-changed` (backend-neutral; SMB is its first emitter).

## Must-knows

- **❌ Never import raw `$state` from `network-store.svelte.ts`; use the exported getters.** Svelte 5 `$state` is
  reactive only inside `.svelte` / `.svelte.ts`, so a raw import from a plain `.ts` silently loses reactivity.
- **`triggerNetworkDiscovery()` is the single chokepoint for starting mDNS.** Call it on any networking intent; ❌ don't
  gate on `network.enabled` yourself, the helper does.
- **❌ Never ask the Keychain twice.** Each macOS access can raise a system prompt, so don't pre-check
  `hasSmbCredentials` before `getSmbCredentials`, and don't probe whether a password is stored. Share activation never
  pre-prompts either (`activateShare`, every path): try stored creds, mount with what we have, and let the mount's own
  refusal ask. A pre-prompt there was a real bug; `PlacesBrowser.test.ts` pins it.
- **`direct-connect.ts::connectDirectly` is the ONE upgrade flow**: the yellow dot, the breadcrumb submenu, and the
  fallback notice all press it, so route a new entry point through it rather than re-inlining the saved-password probe
  and the toast lifecycle. Its failures are typed `UpgradeFailure`s worded by `upgrade-messages.ts`; ❌ never toast
  `String(e)`.
- **A credential is asked for on the sheet and ❌ never in the pane**, through `smb-sign-in.ts`. The three sites, what
  each `attempt` runs, and where cancelling lands: `../../servers/DETAILS.md`.
- **`NetworkMountView` must propagate its local `currentNetworkHost` via `onNetworkHostChange`**, mirrored in the parent
  `FilePane` (`initialNetworkHost`). Without it, leaving Network and coming back re-mounts a stale host and opens
  `PlacesBrowser` for the wrong one.
- **Credential status is keyed by lowercase `host.name`** (the stable Bonjour name); IP and hostname both drift.
- **The `network` volume id is virtual**: its `smb://` path is a sentinel, not a mount. Mounted shares arrive as
  separate `VolumeInfo` entries with real ids.

Architecture, the auth hand-offs, the reconnect flow, and decision rationale: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
