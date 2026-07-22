# Network browser

SMB network discovery UI: host list, per-host share list, login form, and a singleton reactive store.

## Module map

- **`network-store.svelte.ts`**: Module-level `$state` singleton for all network data
- **`lazy-trigger.ts`**: Single chokepoint for kicking off mDNS discovery on user intent
- **`NetworkBrowser.svelte`**: Host list table, rendered when pane is on the `network` volume
- **`ShareBrowser.svelte`**: Share list for a host, handles auth flow
- **`NetworkLoginForm.svelte`**: Credential form rendered inside `ShareBrowser`
- **`ConnectToServerDialog.svelte`**: Modal for manually connecting by address/IP/`smb://` URL
- **`smb-reconnect-manager.svelte.ts`**: Per-volume backoff cycle that re-establishes a Disconnected `SmbVolume`

Full architecture, data flows, auth-flow detail, and decision rationale: [DETAILS.md](DETAILS.md).

## Must-knows

- **Never import raw `$state` from `network-store.svelte.ts`; use the exported getters.** Svelte 5 `$state` is reactive
  only inside `.svelte` / `.svelte.ts` files, so a raw import from a plain `.ts` silently loses reactivity.
- **`lazy-trigger.ts`'s `triggerNetworkDiscovery()` is the single chokepoint for starting mDNS.** Call it on any user
  networking intent (Network view, Connect to server, smb2 upgrade). Don't gate on `network.enabled` at call sites: the
  helper does that. Discovery runs lazily (not at startup) because macOS fires the Local Network permission prompt the
  moment mDNS browsing starts, and forcing that on fresh installs before any context is wrong. The `smb-e2e` build still
  starts at launch so tests don't wait.
- **Don't pre-check `hasSmbCredentials` before `getSmbCredentials`.** Each macOS Keychain access can trigger a system
  prompt, so a pre-check doubles the prompts. Call `getSmbCredentials` directly and catch.
- **Share activation never pre-prompts** (`activateShare`, every path): when `authMode === 'creds_required'` it tries
  stored creds then attempts the mount with whatever it has. An already-mounted share short-circuits in the backend (no
  re-auth), and a genuinely-locked share surfaces the login form via `NetworkMountView`'s mount-failure handler. A
  pre-prompt here was a real bug. Pinned by `ShareBrowser.test.ts`.
- **Mount-phase auth failures route to the login form, not a dead-end error pane.** `NetworkMountView.svelte` (in
  `../pane/`) renders `NetworkLoginForm` on auth-class mount errors (`auth_failed` / `auth_required`, including NetAuth
  -6600); non-auth errors keep the error pane. Pinned by `../pane/NetworkMountView.test.ts`.
- **`NetworkMountView` must propagate its local `currentNetworkHost` via `onNetworkHostChange`.** It's mirrored in the
  parent `FilePane` (`initialNetworkHost` prop). Without propagation, switching volumes away from Network and back
  re-mounts with a stale host and opens `ShareBrowser` for the wrong host.
- **Credential status is keyed by lowercase `host.name`** (the stable Bonjour service name), not IP/hostname, which both
  drift (DHCP, mDNS vs DNS).
- **`network` volume ID is virtual**: the `smb://` path is a sentinel, not a real mount. Mounted shares appear as
  separate `VolumeInfo` entries with real IDs.
- **Tab key in `NetworkLoginForm` calls `stopPropagation()`** so the parent pane handler doesn't read it as a
  pane-switch shortcut while tabbing between fields.
- **`connectionMode` in `NetworkLoginForm` is `$derived.by` from `authMode`; the `RadioGroup` writes it via
  `onValueChange` (not `bind:value`).** Assigning the derived sets a runtime override; it re-evaluates to reset the
  default when `authMode` changes (for example on retry). One-way `value=` + `onValueChange` also sidesteps binding
  `RadioGroup`'s `string` value to the narrower `ConnectionMode`.
- **Host list MCP sync encodes metadata into the `name` field** as a flat string (MCP `PaneFileEntry` has only `name` /
  `path` / `isDirectory`), so agents read the IP, hostname, share count, and status the UI shows. The connect row syncs
  as `+ Connect to server...` with path `smb://connect`.
