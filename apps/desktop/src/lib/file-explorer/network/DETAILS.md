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

## `ServersHub.svelte`

The pane state behind the switcher's "Servers" row: a table of Name, Type, Address, Status, and Last used over every
server the user saved plus every host mDNS is seeing, with an "Add server…" pseudo-row at the bottom (keyboard
navigable, "+" icon, italic), firing `onConnectToServer`. Total navigable items = `rows.length + 1`. Keyboard nav via
`handleNavigationShortcut` (`../navigation/keyboard-shortcuts`); Left/Right jump to first/last.

Three inputs, and one of them is a command rather than a store: `listSavedServers()` (re-read whenever the volume list
is reassigned, which every pin, forget, connect, and disconnect causes through `volumes-changed`), the discovery store's
hosts, and the volume list, which is where a place's standing lives.

### What Enter does, per row kind

- **An SMB host** opens its places list (`onHostSelect`). A saved host mDNS isn't seeing right now is handed over as a
  synthesized `NetworkHost` whose `hostname` is the saved address, the only spelling anything has for it.
- **A one-place server** (SFTP, WebDAV) takes the pane to its place (`onServerSelect` → `NetworkMountView` →
  `onVolumeChange` with the place's `appRoot`). ❗ The PANE does the dialing, not the hub: landing on a `saved` volume
  is what `../pane/place-connect.svelte.ts` watches for, so the connecting view and its Cancel render where every other
  wait does.
- **The add row** opens the one sign-in sheet in add mode (`../../servers/open-sign-in.ts`). An SMB address
  comes back as a hand-off, and `NetworkMountView` opens the injected host's places.

### The three pure modules beside it

`ServersHub.svelte` is the table, the cursor, and the keys. Everything that can go quietly wrong lives next door and is
unit-tested:

- **`servers-hub-rows.ts`**: the merge, the status derivation, and the order. ❗ The merge is the part that goes wrong:
  a manually-typed SMB host is BOTH a saved server and a discovered host (adding one injects it into the discovery
  state), so concatenating the two sources shows a person's NAS twice. The dedup matches on the id first, then on the
  name or resolved hostname, because `known_shares` files a host under the server name `statfs` reported while mDNS
  files the same machine under its Bonjour name. Status comes off the VOLUME LIST, ❌ never off `SavedPlace.connected`,
  which is a snapshot from when the listing was built; the switcher's dot reads the same field, and two surfaces
  disagreeing about whether a server is up is worse than either being briefly stale. Order: live sessions, then the ones
  asking something of the user (`signed_out`, `waiting_for_key`), then the rest of what they saved by recency, then what
  is merely nearby.
- **`servers-hub-mcp.ts`**: the `name` encoding. MCP's `PaneFileEntry` has only `name` / `path` / `isDirectory`, so the
  columns are encoded as `protocol=` / `status=` / `address=` tokens (plus `shares=` on an SMB host, which is what
  `smb.spec.ts` polls on). ❗ The status token is locale-independent even though the column beside it is translated: an
  agent parses these strings and a translation landing in the wire would break both silently. A one-place row's path is
  the place's `appRoot` (from `SavedPlace`, which Rust mints in one function); an SMB host keeps the `smb://<address>`
  spelling the host list has always published; the add row is `+ Add server…` at `smb://add`.
- **`servers-hub-actions.ts`**: F8, the two row menus, and the SMB host menu's answers, behind live getters (❌ never
  snapshots: the rows change under a menu that is still open). ❗ A one-place row and an SMB host take different paths
  at every branch, which is why they live in one unit: a one-place row is a PLACE the servers family speaks for, an SMB
  host is a manual-server entry whose "disconnect" unmounts shares rather than dropping a session.
- **`../navigation/servers-hub-rows` consumers**: `../pane/types.ts`'s `NetworkCursorEntry` gains a `server` arm, which
  is how the palette's server commands reach the row under the cursor (`$lib/servers/server-command-target.ts` owns the
  rule: the hub IS a pane, so "the focused pane's volume" would answer the synthetic hub row).

### Discovery off

`network.enabled` gates mDNS and SMB, which is what the macOS Local Network permission is about; SFTP and WebDAV need
none of it. So the hub opens either way, keeps listing saved servers, and shows one line plus a link to the switch in
place of the nearby hosts. ❗ There is no "(disabled)" label and no redirect to Settings any more;
`network-toggle.spec.ts` is the regression guard.

### Context menu and F8

F8 forgets the SAVED server under the cursor: a one-place row through `forgetSavedServer` (so the hub asks exactly what
the switcher's menu asks), an SMB host through `removeManualServer`, and a host only mDNS knows about gets the "Can't
remove discovered hosts" toast. Right-click on a one-place row raises the SERVERS menu (`openServerRowMenu`), the same
one the switcher row raises; an SMB host keeps its own native host menu (`show_network_host_context_menu`: Disconnect,
Forget server for a manual one, Forget saved password when creds are stored), whose actions arrive on the
`network-host-context-action` event. Cursor auto-clamps when a row disappears.

Exports for parent: `setCursorIndex(index)`, `findItemIndex(name)`, `handleKeyDown(e)`, `refresh()`,
`getHostUnderCursor()`, `getRowUnderCursor()`, `getItemCount()`, `openCursorItem()`. `refresh()` is `pane.refresh`'s
entry point from the command layer and is the same body ⌘R runs locally, which is why the local branch stops propagation
(see § Gotchas).

## `PlacesBrowser.svelte`

The places under ONE account: an SMB host's shares today, a storage account's buckets later. ❗ Its `account` prop is a
tagged union with one arm (`{ protocol: 'smb', host }`), on purpose: the second lister is then a compile-checked
addition rather than a second component. Inside, `const host = $derived(account.host)` keeps the SMB body untouched.

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
exercising Cmdr's own creds, so `authenticatedCredentials` is null even when a working password is saved.
PlacesBrowser's own `NetworkLoginForm` appears ONLY when the share **listing** needs auth (`loadShares`); cancelling
returns to the host list.

When `authenticatedCredentials` is set, a "Forget saved password" button appears in the header; clicking it calls
`forgetCredentials` and clears `authenticatedCredentials`. Shares sort case-insensitively. Escape/Backspace go back.

The `autoMountShare` prop fires once per distinct value (tracked via `lastAutoMountAttempt`), not once per instance, so
"Copy path between panes" can auto-mount a different share without forcing a remount when the source cursor moves to
another share on the same host.

## `NetworkLoginForm.svelte`

Props: `host`, `shareName?`, `authMode`, `defaultConnectionMode?`, `initialUsername?`, `errorMessage?`, `isConnecting?`,
`onConnect`, `onCancel`.

- Guest/credentials radio when `authMode === 'guest_allowed'`.
- Pre-fills username from `getUsernameHint(host.name)` or `getKnownShareByName()`; an explicit `initialUsername` (for
  example from a failed mount) wins over both. Both take the server BY NAME and match on its stable identity in Rust, so
  a hint saved under one name form (`Naspolya`) is found when the form opens under another (`Naspolya._smb._tcp.local`).
  ❌ Don't reintroduce a keyed map here: rebuilding the key in TypeScript is what made the two sides disagree.
- Tab stops propagation (prevents the parent pane-switch while tabbing between fields).
- `connectionMode` is `$derived.by` from `authMode` (guest default when guest allowed). `bind:group` writes a `let`, not
  the read-only derived; the derived re-evaluates when `authMode` changes.

## Data flow

```
App startup
  └─ initNetworkDiscovery() → listNetworkHosts() + event listeners
       └─ startResolution() → resolveNetworkHost()
            └─ startPrefetchShares() → prefetchSharesCmd() → fetchSharesSilent()

User opens the Servers volume → ServersHub mounts → listSavedServers() + refreshAllStaleShares()

User double-clicks an SMB host → PlacesBrowser mounts → loadShares()
       ├─ cache hit → render
       └─ auth required → tryStoredCredentials() → login form if needed

User activates a one-place server → onVolumeChange → the pane lands on a `saved`
       volume → ../pane/place-connect dials, RemoteConnectView renders the wait

User activates the "Add server…" row → the sign-in sheet opens in add mode
       ├─ SFTP / WebDAV → connectServer(target) → the pane lands on the volume
       └─ SMB → connectToServer(address) → TCP check → inject host
            └─ the sheet answers `handed_off`
                 ├─ PlacesBrowser mounts (host set)
                 └─ if sharePath → autoMountShare triggers mount
```

## Connect directly

`direct-connect.ts::connectDirectly(volumeId, raiseCredentialsForm)` is the single implementation behind every "turn
this OS-mounted share into a direct smb2 session" affordance: the yellow-dot popup and the dropdown submenu in
`../navigation/VolumeBreadcrumb.svelte`, and the retry button on the OS-mount fallback notice.

The sequence, and who speaks at each step:

1. `triggerNetworkDiscovery()` — the direct connect opens a TCP socket to a private IP, which fires the macOS Local
   Network prompt anyway, so this is the honest moment to also start mDNS.
2. A persistent "Connecting directly…" toast goes up and comes down on every exit path.
3. `upgradeToSmbVolume(volumeId)`. `success` → success toast + `requestVolumeRefresh()`. A typed `networkError` → the
   `upgrade-messages.ts` sentence for that `UpgradeFailure`.
4. `credentialsNeeded` → `systemHasSavedSmbPassword` (a prompt-free probe). If macOS/Finder saved one, a native primer
   dialog ("Use the saved password?") cushions the system Keychain consent dialog, whose own text we can't customize. On
   "Use saved password", `upgradeToSmbVolumeUsingSavedPassword` reads consent → direct smb2 → copies the password into
   Cmdr's store.
5. Anything still short of a session goes to `raiseCredentialsForm`, which the caller supplies.

Two properties callers rely on:

- **It never resolves without having said something.** Every branch, including a thrown IPC error and a
  `raiseCredentialsForm` that returns `false`, raises a toast first. That's what lets a button call it bare and be sure
  a press can't look inert.
- **The returned `DirectConnectOutcome` describes the volume, not the call.** `connected` / `askingForCredentials` /
  `stillOnOsMount` is exactly the distinction a notice needs to decide whether it still has anything to say.

The credential form is the one piece the flow can't own: it renders inside a pane (`FilePane` off
`smbView.smbUpgradeLogin`). `VolumeBreadcrumb` passes its own pane's opener, so the form lands where the click was.
Anything outside a pane passes `smb-login-hosts.ts::promptForSmbCredentials`, which each `createSmbViewState` registers
into for its pane's lifetime; it prefers a pane already showing that volume and falls back to any pane, mirroring how
the breadcrumb dropdown already opens a form for a volume the pane isn't on. It returns `false` when nothing is mounted
to host the form, which is what turns an impossible prompt into a sentence instead of silence.

## The OS-mount fallback notice

**The gap it fills.** A direct connect that fails on the AUTO paths (the startup pass over existing mounts, the FSEvents
mount watcher) leaves the share working on the macOS kernel mount, at a fraction of the speed and outside Cmdr's
control, announced by nothing louder than a yellow dot the user has no reason to be looking at. That silence once cost
an evening of debugging a "slow" transfer. The manual "Connect directly" path is deliberately NOT a source here: it
already reports its own failure to the person who clicked it.

**The flow.** The backend emits `smb-fell-back-to-os-mount { volumeId, share }` at most once per SERVER per app run (the
ledger and its rationale: `src-tauri/src/network/DETAILS.md` § "Telling the user about a kernel-mount fallback").
`os-mount-notice-bridge.ts`, mounted from `routes/(main)/+page.svelte` beside the other event bridges, turns it into a
persistent INFO toast rendering `SmbOsMountFallbackToastContent.svelte`, dedup id `smb-os-mount:<volumeId>`.

**Dismissal watches the volume list, not the button.** A share can reach a direct session four ways: this notice's
button, the yellow dot, the breadcrumb submenu, and the pane's credential form after a working password. All four end in
`register_replacing_predecessor`, which broadcasts the volume list, so the bridge dismisses on any `volumes-changed`
carrying that volume as `direct`. One rule covers every route, and a fifth route can't forget it. The check is stateless
(dismissing a toast that isn't up is a no-op), so there's no frontend ledger to fall out of step with the backend's.

**What the button does on a second failure.** It runs `connectDirectly`, which raises its own error toast naming the
typed reason, and the notice STAYS UP with the button live again: the situation it describes hasn't changed, and the
next press is worth making once the server wakes or the password is fixed. It retires itself on `connected` (the share
is fast now) and on `askingForCredentials` (the form is a better surface for the same job, and two stacked prompts for
one share is noise). A press while an attempt is in flight is ignored.

## Mount-phase auth failures

`NetworkMountView.svelte` (in `../pane/`) renders `NetworkLoginForm` instead of its error pane whenever
`mountNetworkShare` rejects with an auth-class error (`auth_failed` / `auth_required`, including the NetAuth -6600 code
the backend maps). The form shows the error inline, pre-fills the previously tried username via `initialUsername`,
retries the mount on submit, and saves via `saveSmbCredentials` on success when "Remember in Keychain" is checked.
Escape/Cancel returns to the share list. Non-auth errors (unreachable, timeout, share gone) keep the error pane with
"Try again" / "Back". Pinned by `../pane/NetworkMountView.test.ts`.

## SMB live-reconnect flow (cross-component)

When a direct-SMB session drops mid-use, four pieces coordinate to recover:

1. **Backend** (`SmbVolume::handle_smb_result` in `crates/cmdr-smb/src/volume/session.rs`) detects `ConnectionLost` /
   `SessionExpired`, flips to `Disconnected`, and emits `volume-connection-changed { volumeId, state: "disconnected" }`.
   The event is backend-neutral (any connecting backend emits it); everything below is what the frontend does with it,
   and a new backend inherits all of it. (See `crates/cmdr-smb/DETAILS.md` § SMB live-reconnect lifecycle.)
2. **`stores/volume-store.svelte.ts`** patches the matching volume's `connectionState`, keeping the picker dot, the
   breadcrumb, and `currentVolumeInfo` reactive without waiting for the next `volumes-changed`.
3. **`smb-reconnect-manager.svelte.ts`** (if any subscribers are present) starts a per-volume backoff cycle calling
   `reconnectVolume(volumeId)` per tick. Resolves when the BE emits a follow-up `state: "connected"` event. Only its
   per-tick command is SMB-specific; the cycle itself is backend-agnostic, so it keeps the SMB name until a second
   backend needs it and the command generalizes. A tick that doesn't take arrives as a typed `ReconnectError`
   (`volumeNotFound`, or `volume` wrapping the whole `VolumeError`), thrown as a `ReconnectFailure` by
   `reconnect-error.ts` and read back with `asReconnectError`. The manager LOGS it through `describeReconnectRefusal`,
   which names the reason (`volume/needsPassword`) rather than a sentence a copy edit could change under it. None of
   this is user-facing: the reconnecting view, the gave-up banner, and the sign-in form each carry their own translated
   copy. The error-path map is `docs/guides/error-handling.md`.
4. **`FilePane.svelte`** subscribes via `$effect` whenever the pane is on an SMB volume. Subscription is refcounted
   (both panes on one share share a cycle). During an active cycle FilePane swaps the list for `SmbReconnectingView`; on
   `gave-up` it swaps to `VolumeUnreachableBanner` (`smbGaveUp` variant); on `needs-auth` and `needs-host-key` it swaps
   to `../pane/RemoteConnectView.svelte`; on success the `onSuccess` callback re-runs `loadDirectory`. ❗ The four
   `show*` derivations are a POSITIVE list, so a FIFTH status without its own derivation renders a plain listing over a
   dead session rather than saying anything.

Auth-failure give-up → "Sign in", not "unreachable" (`needs-auth` status): when reconnect fails on an auth error the
saved password can't fix, the backend emits `state: "needs_credentials"`. The manager's `handleNeedsAuth` stops the
backoff (retrying a stale password is futile) and flips to `needs-auth`; FilePane shows `RemoteConnectView`'s
`signed_out`, whose button opens the one sign-in sheet as a REGISTERED place. The sheet's attempt calls
`reconnectVolumeWithCredentials(volumeId, …)`, which refreshes the stored password and reconnects; success arrives as a
`connected` event that clears the state and reloads. Pinned by `smb-reconnect-manager.svelte.test.ts`.

❗ **`handleNeedsAuth` also asks `getVolumeSignInState(volumeId)` and stores the answer on the entry**
(`getSignInShape(volumeId)` reads it back). It is asked HERE, at the flip, and ❌ never carried over from earlier: the
credential a remote volume comes back on is decided per dial, so an answer kept from the connect that opened it
describes a session that has since ended. On SFTP that is the difference between a volume the user can sign in to and
one with no way in at all; the reasoning and the per-rung table are `crates/cmdr-sftp/DETAILS.md` § "What the banner
shows, per rung". The flip itself stays synchronous with the event (the `await` comes after), so `runAttempt`'s
in-flight `needs-auth` check is unaffected.

The sheet reads the shape when it renders rather than taking this stored one, for the same reason the flip re-asks: it
describes THIS session. The stored value is what the pane's banner has to hand before the sheet opens.

❗ `needs_host_key_approval` is the fourth `volume-connection-changed` state, and it gets its OWN status
(`needs-host-key`), ❌ never the sign-in path: it only ever describes an SFTP volume whose host key stopped matching,
and a password box in front of a possible man-in-the-middle is how a password gets typed into one. `handleNeedsHostKey`
ends the backoff and flips; ❗ it asks no `getVolumeSignInState`, because nothing about this is a credential question and
asking one would be the first step toward putting a password box in front of it. The pane renders
`RemoteConnectView`'s `host_key_changed`, which offers Disconnect (`../pane/DETAILS.md` § the connect views says why
that, and not "Trust it"). `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

Lazy-nav path: opening a share that's already `Disconnected` (no fresh event in flight), the FilePane `$effect` notices
`currentVolumeInfo?.connectionState === 'disconnected'` and calls `manager.startCycle(volumeId)` directly.

Disconnect button: `disconnectSmbVolume(volumeId)` shells out to `diskutil unmount` (macOS) → FSEvents fires →
`SmbVolume::on_unmount` → volume removed from `VolumeManager` → `volumes-changed` removes it from the picker.

## Lazy mDNS trigger

`triggerNetworkDiscovery()`:

1. No-ops if `network.enabled === false`.
2. Calls `ensureNetworkDiscoveryStarted()` (idempotent backend command; the first call kicks off the mDNS daemon, firing
   the macOS "Cmdr wants to find devices on local networks" prompt the first time).
3. Sets `network.firstTriggerDone = true` so subsequent launches start mDNS eagerly (returning users get full speed
   without re-prompts).

Call sites: `ServersHub.onMount` and `VolumeBreadcrumb.handleSubmenuAction` (the OS-mount → direct-smb2 upgrade also
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
- **A reconnect is silent: no toast fires for one.** The recovery already shows itself where the user is looking, in the
  pane (`SmbReconnectingView`, then the give-up banner) and on the volume picker's dot, and every laptop lid-close drops
  the session, so a toast per reconnect would fire on routine sleep/wake and teach the user to dismiss it unread. The
  evidence stays available on demand instead: the debug window's SMB diagnostics dashboard reads the session counters
  through `src-tauri/src/commands/smb_diagnostics.rs`.

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
- **⌘R in `ServersHub` calls `stopPropagation()` too, and one round of shares depends on it.** The document-level
  dispatcher runs after this handler and has no `defaultPrevented` guard, so `pane.refresh` would ALSO dispatch into
  `refreshPane` → `refreshNetworkHosts()` → `ServersHub.refresh()` — the same `handleRefreshClick()` the local branch
  just ran, giving every host two `clearShareState` + `fetchShares` rounds per keypress. Pinned by `ServersHub.test.ts`;
  the general rule is in `$lib/shortcuts/DETAILS.md` § "Local handlers resolve through the registry too".
- **Neither browser's `handleKeyDown` returns a "handled" boolean** (`BrowserAPI` in `../pane/types.ts`). Nothing above
  them branches on one: `NetworkMountView` and `pane-key-router` hand the network view every key and return either way,
  so the claim is `preventDefault()` + `stopPropagation()` on the event.
- **The hub's MCP sync encodes metadata into the `name` field** as a flat string because MCP `PaneFileEntry` has only
  `name` / `path` / `isDirectory`; the encoding lets agents read what the UI shows without a schema change.
  `servers-hub-mcp.ts` owns it, and its status token stays untranslated on purpose.
- **The hub row's NAME has four spellings to keep in step**, and three of them are not in this directory: the catalog
  key `fileExplorer.navigation.networkVolume`, Rust's `volume_listing::SERVERS_VOLUME_NAME`,
  `../pane/volume-selection.ts::selectVolumeByName` (which special-cases the hub because it is synthetic and no
  `findIndex` over the volume list reaches it), and `DualPaneExplorer`'s `leftVolumeName` / `rightVolumeName`. ❗ A
  fifth spelling is not a wrong word on screen, it is a 30 s MCP timeout: `mcp/executor/nav.rs` waits for the
  frontend-pushed pane name to equal the const before it calls a `select_volume` done.

## Dependencies

- `$lib/tauri-commands`: `listNetworkHosts`, `resolveNetworkHost`, `listSharesOnHost`, `listSharesWithCredentials`,
  `prefetchShares`, `getSmbCredentials`, `saveSmbCredentials`, `deleteSmbCredentials`, `getUsernameHint`,
  `getKnownShareByName`, `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`, `connectToServer`,
  `removeManualServer`
- `$lib/settings/network-settings`: `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `$lib/utils/confirm-dialog`: `confirmDialog`
- `$lib/ui/toast`: `addToast`
- `../navigation/keyboard-shortcuts`: `handleNavigationShortcut`
- `../types`: `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
