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

❗ **Why a pane state and not a Settings list**, and the rule that comes with it: the hub is this app's pilot for
putting a manageable list where the user already navigates, so the saved-server list does NOT also live in Settings (a
Settings card keeps only what the hub can't show, the trusted host keys). The rule the pilot carries: **a pane state may
be a LIST, ❌ never a FORM.** Enter, F8, ⌘E, and the row menus act on the row under the cursor; anything that asks a
person to TYPE opens the sign-in sheet (`../../servers/DETAILS.md` § "The four rules", rule 3). A form inside a pane is
what the SMB login form used to be, where Tab meant "switch panes" to Cmdr and "next field" to macOS and the form lost
either way.

### What Enter does, per row kind

- **An SMB host** opens its places list (`onHostSelect`). A saved host mDNS isn't seeing right now is handed over as a
  synthesized `NetworkHost` whose `hostname` is the saved address, the only spelling anything has for it.
- **A one-place server** (SFTP, WebDAV) takes the pane to its place (`onServerSelect` → `NetworkMountView` →
  `onVolumeChange` with the place's `appRoot`). ❗ The PANE does the dialing, not the hub: landing on a `saved` volume
  is what `../pane/place-connect.svelte.ts` watches for, so the connecting view and its Cancel render where every other
  wait does.
- **The add row** opens the one sign-in sheet in add mode (`../../servers/open-sign-in.ts`). An SMB address comes back
  as a hand-off, and `NetworkMountView` opens the injected host's places.

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

  ❗ **The merge also guarantees every row id is UNIQUE**, first writer wins. The hub keys its `{#each}` on `row.id`,
  and Svelte throws `each_key_duplicate` on a repeat, so a duplicate crashes the whole pane rather than showing a row
  twice. `listSavedServers()` unions three stores, so a server recorded in two of them arrives twice and the name/id
  dedup above doesn't catch it: `smb_hosts` (`commands/servers.rs`) dedupes by ADDRESS while the row id comes from a
  normalized server NAME, so two rows for one host can differ in address and agree on id. Seen for real in the Linux E2E
  suite: the hub went blank and every `move_cursor` onto a host afterwards reported the row missing.

  ❗ **The Name column ranks three names**, in `servers-hub-rows.ts::displayName`: a name a PERSON chose (an SFTP or
  WebDAV account someone named), then the DISCOVERED Bonjour name, and last a stand-in nobody chose (an unnamed
  account's derived `username@host`, which keeps its place because only SMB rows match a discovered host). The rank
  comes off `SavedServer.nameSource` (`commands/servers.rs`'s `ServerNameSource`), a fact the store that wrote the label
  publishes, ❌ never a guess at the string's shape. Without it a person's NAS renames itself the moment they use it:
  every SMB label is a stand-in, either the way `statfs` spells the server (written to `known_shares` on the first share
  listing, `smb-consumer-guest`) or the address typed into "Add server" (`manual_servers` derives `host` or `host:port`;
  SMB's add flow asks for nothing else), and the friendly name they recognize (`SMB Test (Guest)`, `Naspolya`) would
  drop out of the column. Four `smb.spec.ts` specs poll on the Bonjour name and are the regression guard.

  ❗ **A saved SMB server claims EVERY host that matches it, not the first.** One machine sits in the discovery list
  twice once a person types a host mDNS already found: the manual entry injects a `manual` host beside the `discovered`
  one. `primaryHost` then picks the DISCOVERED one for the row's name and address, because a manual host is named after
  the address that was typed.

- **`servers-hub-mcp.ts`**: the `name` encoding. MCP's `PaneFileEntry` has only `name` / `path` / `isDirectory`, so the
  columns are encoded as `protocol=` / `status=` / `address=` tokens (plus `shares=` on an SMB host, which is what
  `smb.spec.ts` polls on). ❗ The status token is locale-independent even though the column beside it is translated: an
  agent parses these strings and a translation landing in the wire would break both silently. A one-place row's path is
  the place's `appRoot` (from `SavedPlace`, which Rust mints in one function); an SMB host keeps the `smb://<address>`
  spelling the host list publishes; the add row is `+ Add server…` at `smb://add`.
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
place of the nearby hosts. ❗ No "(disabled)" label, and no redirect to Settings; `network-toggle.spec.ts` is the
regression guard.

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
   `authenticatedCredentials` is set and auth is transparent; otherwise `askForCredentials()` opens the sign-in sheet.
3. If cache shows another error (`host_unreachable`, `timeout`, ...): fall through to a fresh fetch (user-initiated host
   open is an implicit retry; the background prefetch may have run before the host was ready).
4. Otherwise (no cache or loading): `fetchShares(host)`, same auth fallback.

`authenticatedCredentials` is passed to `onShareSelect` so the caller mounts without re-prompting.

The stored-creds attempt matters because the share list often loads via the SYSTEM Keychain (`smbutil view -N`) without
exercising Cmdr's own creds, so `authenticatedCredentials` is null even when a working password is saved.

`PlacesBrowser` asks for a credential ONLY when the share **listing** needs one (`loadShares`, and the two "Sign in"
buttons on its error and empty states). ❗ Cancelling goes BACK to the host list: this sheet is up because the listing
itself can't be read, so "not now" leaves the user in a room with nothing in it. (Share-activation auth is the mount's
question; `../pane/NetworkMountView.svelte` asks it.) Its `attempt` is `listWithCredentials`, which answers `handed_off`
on success (nothing here connects a VOLUME, and the loaded list IS the result), keeps the sheet open on
`authentication_rejected` / `needs_credentials`, and closes it onto the pane's own error state for anything else,
because that is where the retry and a missing dependency's install command live.

When `authenticatedCredentials` is set, a "Forget saved password" button appears in the header; clicking it calls
`forgetCredentials` and clears `authenticatedCredentials`. Shares sort case-insensitively. Escape/Backspace go back.

The `autoMountShare` prop fires once per distinct value (tracked via `lastAutoMountAttempt`), not once per instance, so
"Copy path between panes" can auto-mount a different share without forcing a remount when the source cursor moves to
another share on the same host.

## `smb-sign-in.ts`

SMB's side of the one sign-in sheet. The sheet contract, the three SMB sites and what each `attempt` runs, and the
endpoint header per site live in `../../servers/DETAILS.md`; this section is what SMB alone decides.

`openSmbSignInSheet({ host, shareName?, guestAllowed, initialUsername?, refusal?, attempt })` builds the request, and
the caller's `attempt` gets an `SmbCredentialAnswer` (`{ username, password, remember }`) rather than the sheet's
generic submission. ❗ `username: null` IS guest: all three SMB commands take a nullable username and read it that way,
so a separate flag could only disagree with it.

- **The username** is resolved in one order everywhere: what an earlier attempt tried, then `getKnownShareByName()`'s
  last username for this share, then `getUsernameHint()`. ❗ Both lookups take the server BY NAME and match on its
  stable identity in Rust, so a hint saved under one spelling (`Naspolya`) is found when the sheet opens under another
  (`Naspolya._smb._tcp.local`). ❌ Don't rebuild the key in TypeScript: that is what made the two sides disagree once.
- **Remember starts ON, and is ❌ never probed**, because `has_smb_credentials` is `get_credentials(…).is_ok()` and
  asking costs the same Keychain prompt as reading. Why the other protocols seed the box differently:
  `../../servers/DETAILS.md` § "The sheet contract".
- **`guestAllowed` is a per-site call, ❌ not a property of SMB.** The listing offers guest where the host's `authMode`
  says one is allowed; the mount and the upgrade both pass `false`.
- **`refusalForShareError` / `refusalForMountError`** put every SMB failure in `../../servers/connect-refusals.ts`'s
  vocabulary, and ❗ `auth_required` stays distinct from `auth_failed`: telling someone who has never entered a password
  that theirs is wrong is what collapsing the two does. A mount's `permission_denied` is a third answer,
  `account_not_permitted`: the account signed in and the share turned it away, so the password isn't what to fix.

## Data flow

```
App startup
  └─ initNetworkDiscovery() → listNetworkHosts() + event listeners
       └─ startResolution() → resolveNetworkHost()
            └─ startPrefetchShares() → prefetchSharesCmd() → fetchSharesSilent()

User opens the Servers volume → ServersHub mounts → listSavedServers() + refreshAllStaleShares()

User double-clicks an SMB host → PlacesBrowser mounts → loadShares()
       ├─ cache hit → render
       └─ auth required → tryStoredCredentials() → the sign-in sheet if needed

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

`direct-connect.ts::connectDirectly({ volumeId, shareName })` is the single implementation behind every "turn this
OS-mounted share into a direct smb2 session" affordance: the yellow-dot popup and the dropdown submenu in
`../navigation/VolumeBreadcrumb.svelte`, and the retry button on the OS-mount fallback notice.

The sequence, and who speaks at each step:

1. `triggerNetworkDiscovery()`, because the direct connect opens a TCP socket to a private IP, which fires the macOS
   Local Network prompt anyway, so this is the honest moment to also start mDNS.
2. A persistent "Connecting directly…" toast goes up and comes down on every exit path.
3. `upgradeToSmbVolume(volumeId)` answers a typed `UpgradeResult` and never throws for an outcome. `success` → success
   toast + `requestVolumeRefresh()`. `networkError` → the `upgrade-messages.ts` sentence for that `UpgradeFailure`, at
   `error`. `volumeGone` / `notSmbMount` → `nothingToUpgradeMessage`, at `warn`: nothing broke, there was just no
   OS-mounted share left to connect (an unmount, an eject, or a network drop between the offer and the press).
4. `credentialsNeeded` → `systemHasSavedSmbPassword` (a prompt-free probe). If macOS/Finder saved one, a native primer
   dialog ("Use the saved password?") cushions the system Keychain consent dialog, whose own text we can't customize. On
   "Use saved password", `upgradeToSmbVolumeUsingSavedPassword` reads consent → direct smb2 → copies the password into
   Cmdr's store.
5. Anything still short of a session goes to the one sign-in sheet.

Three properties callers rely on:

- **It never resolves without having said something.** Every branch, a thrown IPC error included, raises a toast first.
  That's what lets a button call it bare and be sure a press can't look inert. A throw is the one case with no typed
  reason: `announceBreakdown` logs it and toasts the unnamed sentence.
- **The returned `DirectConnectOutcome` describes the volume, not the call.** `connected` / `askingForCredentials` /
  `stillOnOsMount` / `gone` is exactly the distinction a notice needs to decide whether it still has anything to say.
- **The caller names the share.** A volume that's gone has no name left for the backend to look up, so `shareName` is
  what the pressed control showed: the notice's `share`, or the breadcrumb row's name read at click time.

The credential ask is the one app-global sign-in sheet, so the flow raises it itself. ❗ It returns
`askingForCredentials` as soon as the sheet is UP, ❌ not when the user is done with it: the OS-mount notice retires on
that outcome, and awaiting the sheet would leave the notice stacked under it. The sheet's `attempt` is
`upgradeToSmbVolumeWithCredentials`; a server that stopped answering or a share that went away closes it with the same
typed sentence the flow's own paths toast, because no credential can fix either.

**What the sheet says comes from the answer's typed `reason`**, through `REFUSAL_FOR_REASON`, on the opening round and
on every retry: `noCredential` → `needs_credentials`, `credentialRejected` → `authentication_rejected`, and
`accountNotPermitted` → `account_not_permitted`. The backend reads which one from who the attempt went out as and which
step refused it (`src-tauri/src/network/DETAILS.md` § "An auth rejection says what was actually rejected"). ❌ Never
collapse the last two: an account the share turns away SIGNED IN, and answering "wrong password" kept the sheet asking
for a password that worked (ERR-SHUSC). A refusal opens the sheet on the account it turned away (`usernameHint` as
`initialUsername`), because the sentence names that account; with nothing offered, the remembered username pre-fills
instead.

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
carrying that volume as `direct`. One rule covers every route, and a fifth route can't forget it. A share that goes AWAY
broadcasts the list too, so a notice whose volume is no longer listed retires the same way: its button could only say
the share is gone. The bridge asks the toast store which notices are up (`getToasts`, matched by content component and
`props.volumeId`) rather than keeping a list, so there's no frontend ledger to fall out of step with the backend's or
with the user closing one.

❗ **Only a listing that finished may retire a notice by absence.** A `timedOut` payload is the last complete list
standing in for a fresh one, so a share missing from it proves nothing. The rule's one gap: a discovery that started
before a brand-new mount and finished slowly (inside its timeout) after that mount's fallback could retire the fresh
notice. The share keeps its yellow dot either way.

**What the button does on a second failure.** It runs `connectDirectly`, which raises its own error toast naming the
typed reason, and the notice STAYS UP with the button live again: the situation it describes hasn't changed, and the
next press is worth making once the server wakes or the password is fixed. It retires itself on `connected` (the share
is fast now), on `askingForCredentials` (the form is a better surface for the same job, and two stacked prompts for one
share is noise), and on `gone` (the share unmounted before the press, so no retry can help). A press while an attempt is
in flight is ignored.

## Mount-phase auth failures

`NetworkMountView.svelte` (in `../pane/`) opens the sign-in sheet instead of dead-ending in its error pane whenever
`mountNetworkShare` rejects with a failure another credential can answer (`smb-sign-in.ts::isMountSignInQuestion`):
`auth_failed` (including the NetAuth -6600 code the backend maps), `auth_required`, and `permission_denied`. The sheet
opens carrying that refusal, pre-filled with the username the failed attempt tried; what its `attempt` then runs is
`../../servers/DETAILS.md` § "The sheet contract".

A share guests can list but not open reaches here as `auth_required`, not as the "not found" NetFS reports: the backend
asks the server itself which it was (`src-tauri/src/network/DETAILS.md` § "A share that says not found"). That is what
turned ERR-SHUSC's dead-end pane into a sign-in. A signed-in account the share refuses comes back `permission_denied`
and keeps the sheet open on `account_not_permitted`, so the user can try another account.

Two properties are load-bearing:

- **`mountError` is set for an auth failure too**, so the pane behind the sheet holds the failure and the MCP mirror
  reports it. The error pane is what cancelling lands back on, cleared by `handleMountErrorBack`.
- **Only a credential refusal keeps the sheet open.** A retry that comes back `share_not_found`, `host_unreachable`, or
  `mount_missing` (the system reported the share connected and no mount of it is there:
  `src-tauri/src/network/DETAILS.md` § "A reported mount counts once it's there") answers `handed_off`, closing the
  sheet onto the pane's error state with its own "Try again" / "Back": the sheet has no words for a share that went
  missing. Non-auth failures never open it in the first place. Pinned by `../pane/NetworkMountView.test.ts`.

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
   (both panes on one share share a cycle), and on success the `onSuccess` callback re-runs `loadDirectory`.
   `../pane/smb-view-state.svelte.ts` turns the manager's status into the ONE `RemoteConnectState` the pane renders, in
   a single exhaustive `switch`: `waiting` / `attempting` become `connecting` carrying the cycle's own lines, its
   countdown to the next attempt, and its Try now / Disconnect; `needs-auth` becomes `signed_out`; `needs-host-key`
   becomes `host_key_changed`. ❗ `gave-up` maps to `null` on purpose, because `VolumeUnreachableBanner`'s `gaveUp`
   variant is the app's one "couldn't reach this" surface, and two renderers for one state is worse than one in the file
   next door. The exhaustive switch replaced a list of per-status booleans, where a new status silently rendered a plain
   listing over a dead session.

Auth-failure give-up → "Sign in", not "unreachable" (`needs-auth` status): when reconnect fails on an auth error the
saved password can't fix, the backend emits `state: "needs_credentials"`. The manager's `handleNeedsAuth` stops the
backoff (retrying a stale password is futile) and flips to `needs-auth`; FilePane shows `RemoteConnectView`'s
`signed_out`, whose button opens the one sign-in sheet as a REGISTERED place, ❗ or offers no button at all when the
stored shape is `nothing`, since no secret a person could type would bring that session back. The sheet's attempt calls
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
ends the backoff and flips; ❗ it asks no `getVolumeSignInState`, because nothing about this is a credential question
and asking one would be the first step toward putting a password box in front of it. The pane renders
`RemoteConnectView`'s `host_key_changed`, which offers Disconnect (`../pane/DETAILS.md` § the connect views says why
that, and not "Trust it"). `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

Lazy-nav path: opening a share that's already `Disconnected` (no fresh event in flight), the `smb-view-state.svelte.ts`
subscription `$effect` notices `currentVolumeInfo?.connectionState === 'disconnected'` and calls
`connectPlace({ volumeId, connectionState: 'disconnected' })`. ❗ Through `$lib/servers/connect-flow.ts`'s arm 1 rather
than `manager.startCycle` directly, because that module documents itself as the ONE caller of the manager's lazy start
and a second caller makes the guardrail a lie. Arm 1 is that call and nothing else, and the manager is idempotent, so
landing on the same share twice still costs nothing. The signed-out banner's Sign in goes through the same flow's arm 2,
which is what carries the `needs_credentials` reason into the sheet's first round.

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
  pane (the reconnect cycle's countdown, then the give-up banner) and on the volume picker's dot, and every laptop
  lid-close drops the session, so a toast per reconnect would fire on routine sleep/wake and teach the user to dismiss
  it unread. The evidence stays available on demand instead: the debug window's SMB diagnostics dashboard reads the
  session counters through `src-tauri/src/commands/smb_diagnostics.rs`.

## Gotchas

- **`currentNetworkHost` lives in both `NetworkMountView` (local) and `FilePane` (via `initialNetworkHost` +
  `onNetworkHostChange`).** When `NetworkMountView` mutates its copy (mount success, back), it must propagate via
  `onNetworkHostChange`, or switching away from Network and back re-mounts with a stale host (bit E2E test 436 "unicode
  shares render" where a prior guest-share mount left FilePane stuck on guest).
- **`network` volume ID is virtual**: the discovery UI has no real mount point until a share is mounted via
  `mount_smbfs`. Mounted shares then appear as separate `VolumeInfo` entries with real IDs.
- **Credential status keyed by lowercase `host.name`**: the same physical host can change IP (DHCP) and hostname (mDNS
  vs DNS); the Bonjour service name is the stable identifier. Lowercasing avoids case mismatches.
- **⌘R in `ServersHub` calls `stopPropagation()` too, and one round of shares depends on it.** The document-level
  dispatcher runs after this handler and has no `defaultPrevented` guard, so `pane.refresh` would ALSO dispatch into
  `refreshPane` → `refreshNetworkHosts()` → `ServersHub.refresh()`, the same `handleRefreshClick()` the local branch
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
