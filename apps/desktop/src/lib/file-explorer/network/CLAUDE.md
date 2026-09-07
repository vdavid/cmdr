# Network browser

SMB discovery and browsing: the servers hub, a host's places, the direct-connect upgrade, and a singleton reactive
store. Signing in is `$lib/servers`' one sheet; this module only says what to ask and what to do with the answer.

## Module map

- **`network-store.svelte.ts`**: module-level `$state` singleton for all network data. Use the exported getters.
- **`lazy-trigger.ts`**: the one chokepoint for kicking off mDNS discovery on user intent.
- **`smb-sign-in.ts`**: SMB's side of the sign-in sheet — the endpoint header, the remembered username, and every SMB
  error in the sheet's refusal vocabulary.
- **`direct-connect.ts`**: the whole "Connect directly" flow. **`upgrade-messages.ts`**: its typed failure → toast copy.
- **`os-mount-notice-bridge.ts`** + **`SmbOsMountFallbackToastContent.svelte`**: the slow-connection notice + retry.
- **`ServersHub.svelte`** (+ `servers-hub-{rows,actions,mcp}.ts`, `host-status.ts`): the hub table on the `network`
  volume. **`PlacesBrowser.svelte`**: one account's places, and the listing's sign-in.
- **`smb-reconnect-manager.svelte.ts`**: per-volume backoff cycle on `volume-connection-changed` (backend-neutral; SMB
  is its first emitter).

Architecture, data flows, the auth hand-offs, and decision rationale: `DETAILS.md`.

## Must-knows

- **Never import raw `$state` from `network-store.svelte.ts`; use the exported getters.** Svelte 5 `$state` is reactive
  only inside `.svelte` / `.svelte.ts` files, so a raw import from a plain `.ts` silently loses reactivity.
- **`lazy-trigger.ts`'s `triggerNetworkDiscovery()` is the single chokepoint for starting mDNS.** Call it on any user
  networking intent; don't gate on `network.enabled` yourself, the helper does. Discovery is lazy because mDNS browsing
  fires the macOS Local Network prompt, which a fresh install shouldn't meet with no context.
- **A direct-connection failure arrives as a typed `UpgradeFailure`, never a sentence**: the words live in
  `upgrade-messages.ts` + the `directConnection*Toast` keys. ❌ Never toast `String(e)` or a backend message.
- **`direct-connect.ts::connectDirectly` is the ONE upgrade flow**: yellow dot, breadcrumb submenu, and fallback-notice
  button all press it. Route a new entry point through it rather than re-inlining the saved-password probe and the toast
  lifecycle. It always tells the user something before resolving, so a button can call it bare.
- **Don't pre-check `hasSmbCredentials` before `getSmbCredentials`.** Each macOS Keychain access can trigger a system
  prompt, so a pre-check doubles them. Call `getSmbCredentials` directly and catch. Same reason `smb-sign-in.ts` never
  probes whether a password is stored.
- **Share activation never pre-prompts** (`activateShare`, every path): try stored creds, then mount with whatever we
  have, and let the mount's own refusal ask. A pre-prompt here was a real bug; pinned by `PlacesBrowser.test.ts`.
- **A credential is asked for on the sheet and ❌ never in the pane**, through `smb-sign-in.ts`. Three sites do it: the
  listing (`PlacesBrowser`), the mount (`../pane/NetworkMountView.svelte`), and the upgrade (`direct-connect.ts`).
  Cancelling returns to where the user was. `DETAILS.md`.
- **`NetworkMountView` must propagate its local `currentNetworkHost` via `onNetworkHostChange`.** It's mirrored in the
  parent `FilePane` (`initialNetworkHost` prop). Without it, leaving Network and coming back re-mounts with a stale host
  and opens `PlacesBrowser` for the wrong one.
- **Credential status is keyed by lowercase `host.name`** (the stable Bonjour name); IP and hostname both drift.
- **`network` volume ID is virtual**: the `smb://` path is a sentinel, not a real mount. Mounted shares appear as
  separate `VolumeInfo` entries with real IDs.
- **Hub MCP sync encodes metadata into the `name` field** as a flat string (MCP `PaneFileEntry` has only `name` / `path`
  / `isDirectory`), so agents read what the UI shows.
