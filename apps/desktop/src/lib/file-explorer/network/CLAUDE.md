# Network browser

SMB network discovery UI: host list, per-host share list, login form, and a singleton reactive store.

## Module map

- **`network-store.svelte.ts`**: Module-level `$state` singleton for all network data
- **`lazy-trigger.ts`**: Single chokepoint for kicking off mDNS discovery on user intent
- **`upgrade-messages.ts`**: `UpgradeFailure` → the toast copy for a direct connection that didn't happen
- **`direct-connect.ts`**: the whole "Connect directly" flow, shared by every entry point
- **`smb-login-hosts.ts`**: which pane can render the credential form right now
- **`os-mount-notice-bridge.ts`** + **`SmbOsMountFallbackToastContent.svelte`**: the slow-connection notice + retry
- **`NetworkBrowser.svelte`**: Host list table, rendered when pane is on the `network` volume
- **`ShareBrowser.svelte`**: Share list for a host, handles auth flow
- **`NetworkLoginForm.svelte`**: Credential form rendered inside `ShareBrowser`
- **`ConnectToServerDialog.svelte`**: Modal for manually connecting by address/IP/`smb://` URL
- **`smb-reconnect-manager.svelte.ts`**: Per-volume backoff cycle on the `volume-connection-changed` event
  (backend-neutral; SMB is its first emitter)

Full architecture, data flows, auth-flow detail, and decision rationale: `DETAILS.md`.

## Must-knows

- **Never import raw `$state` from `network-store.svelte.ts`; use the exported getters.** Svelte 5 `$state` is reactive
  only inside `.svelte` / `.svelte.ts` files, so a raw import from a plain `.ts` silently loses reactivity.
- **`lazy-trigger.ts`'s `triggerNetworkDiscovery()` is the single chokepoint for starting mDNS.** Call it on any user
  networking intent; don't gate on `network.enabled` yourself, the helper does. Discovery is lazy because mDNS browsing
  fires the macOS Local Network prompt, which a fresh install shouldn't meet with no context. `DETAILS.md`.
- **A direct-connection failure arrives as a typed `UpgradeFailure`, never a sentence**: the words live in
  `upgrade-messages.ts` + the `directConnection*Toast` keys. ❌ Never toast `String(e)` or a backend message.
- **`direct-connect.ts::connectDirectly` is the ONE upgrade flow**: yellow dot, breadcrumb submenu, and fallback-notice
  button all press it. Route a new entry point through it instead of re-inlining the saved-password probe and the toast
  lifecycle. It always tells the user something before resolving, so a button can call it bare. `DETAILS.md`.
- **Don't pre-check `hasSmbCredentials` before `getSmbCredentials`.** Each macOS Keychain access can trigger a system
  prompt, so a pre-check doubles the prompts. Call `getSmbCredentials` directly and catch.
- **Share activation never pre-prompts** (`activateShare`, every path): try stored creds, then mount with whatever we
  have, and let the mount failure raise the form. A pre-prompt here was a real bug; pinned by `ShareBrowser.test.ts`.
- **Mount-phase auth failures route to the login form, not a dead-end error pane.** `NetworkMountView.svelte` (in
  `../pane/`) renders `NetworkLoginForm` on auth-class mount errors (`auth_failed` / `auth_required`, including NetAuth
  -6600); non-auth errors keep the error pane. Pinned by `../pane/NetworkMountView.test.ts`.
- **`NetworkMountView` must propagate its local `currentNetworkHost` via `onNetworkHostChange`.** It's mirrored in the
  parent `FilePane` (`initialNetworkHost` prop). Without propagation, switching volumes away from Network and back
  re-mounts with a stale host and opens `ShareBrowser` for the wrong host.
- **Credential status is keyed by lowercase `host.name`** (the stable Bonjour name); IP and hostname both drift.
- **`network` volume ID is virtual**: the `smb://` path is a sentinel, not a real mount. Mounted shares appear as
  separate `VolumeInfo` entries with real IDs.
- **Tab key in `NetworkLoginForm` calls `stopPropagation()`** so the parent pane handler doesn't read it as a
  pane-switch shortcut while tabbing between fields.
- **`NetworkLoginForm`'s `connectionMode` is a `$derived.by` the `RadioGroup` writes via `onValueChange`, never
  `bind:value`.** Binding would pin the derived and stop it resetting when `authMode` changes. `DETAILS.md`.
- **Host list MCP sync encodes metadata into the `name` field** as a flat string (MCP `PaneFileEntry` has only `name` /
  `path` / `isDirectory`), so agents read the IP, hostname, share count, and status the UI shows. The connect row syncs
  as `+ Connect to server...` with path `smb://connect`.
