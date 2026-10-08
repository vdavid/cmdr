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
2. `startPrefetchShares(host)`: while a Servers view is on screen, for a host the person SAVED, calls
   `prefetchSharesCmd` (backend caches result), then `fetchSharesSilent` to populate `shareStates`. With no Servers view
   up, or for any other host, it does nothing.
3. `holdDiscoveryForServersView()`: on the FIRST view shown, runs `startPrefetchShares` over every known host, so the
   saved ones list as the view opens (each row shows its own loading state until its list lands).

❗ **Nothing is listed at launch, and a host Cmdr only found is listed when the person opens it, and at no other time**
(#324). Listing a host's shares means connecting to it and signing in, as a guest where it lets one in, so prefetching
every resolved host made each launch sign in to every SMB machine on the network, and the hub then labelled a machine
the person never touched "as guest". A saved server's prefetch waits for a Servers view too: at launch nobody is about
to pick a share, and the browse window (`docs/notes/performance/mdns-browse-gating-2026-09-27.md`) would otherwise turn
into a connection per saved host per launch. Three places hold the found-host line, and a fourth caller of
`listSharesOnHost` has to hold it too:

- `startPrefetchShares`: no prefetch.
- `refreshAllStaleShares` (entering the Servers view): a saved host's stale list is re-read; a found host's is DROPPED,
  so opening it reads a fresh one (`PlacesBrowser` shows a cached list without asking how old it is).
- The hub's refresh (`ServersHub.handleRefreshClick`, ⌘R): every host's list is dropped, and only the saved ones are
  re-read.

"Saved" is one question with one answer: `servers-hub-rows.ts::savedSmbHostIds`, the match the hub's merge makes, so a
host that gets no prefetch is exactly a host the hub puts in its nearby group. The store reads `listSavedServers()`
fresh per decision (`readSavedServers`, one read shared by every host that asks while it's out), because a host becomes
the person's the moment they add it or mount one of its shares, and nothing tells the store when. A read that breaks
answers "none saved": not knowing whose a host is must never be what signs Cmdr in to it. Pinned by
`network-store.prefetch.test.ts`. The backend has no other caller: `smb_client::list_shares` is reached only through the
three listing commands, all frontend-invoked (verified by reading its call sites, 2026-09-30).

Key exported functions: `getNetworkHosts()` (sorted copy), `fetchShares(host)` (explicit, throws on error),
`refreshSharesIfStale(host)`, `refreshAllStaleShares()` (call on entering network view),
`checkCredentialsForHost(serverName)` (one-time Keychain probe, idempotent), `forgetCredentials(serverName)`,
`setCredentialStatus` / `getCredentialStatus` (in-memory only), `setShareState` / `clearShareState`,
`getDiscoveryState()`, `isHostResolving(hostId)`, `getShareState(hostId)`, `getShareCount(hostId)`,
`isListingShares(hostId)`, `isShareDataStale(hostId)`.

## `ServersHub.svelte`

The pane state behind the switcher's "Servers" row: a table of Name, Type, Address, Status, and Last used over every
server the user saved, then the hosts mDNS is seeing that they didn't, folded into one group (§ "The nearby group"),
with an "Add server…" pseudo-row at the bottom (keyboard navigable, "+" icon, italic), firing `onConnectToServer`.
Keyboard nav via `handleNavigationShortcut` (`../navigation/keyboard-shortcuts`); Left/Right jump to first/last.

❗ **The hub lists nothing until its first read of the saved list answers** (`servers-hub-list.svelte.ts`). Which
servers are saved decides where every row goes, so listing the discovered hosts first would put them all in the nearby
group, open it, and leave the cursor on a row that then moves. A read that breaks counts as answered, with none saved.

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
- **A saved SMB share** (a row right under its host, `docs/specs/saved-smb-shares.md`) takes the pane to its place when
  the volume list has one, exactly like a one-place server, so an unmounted one is mounted IN THE PANE. One no mount
  went through yet (Add named it) opens its host's places list and mounts that share for the visit (`onShareViaHost`).
- **An S3 place** (a bucket, or the account root, right under its account) takes the pane to its place, exactly like a
  one-place server. **The S3 account row** is no place, so Enter answers a toast pointing at the places under it.
- **The add row** opens the one sign-in sheet in add mode (`../../servers/open-sign-in.ts`). An SMB address comes back
  as a hand-off, and `NetworkMountView` opens the injected host's places.

`servers-hub-rows.ts::openMoveFor` is the one place that decides which of these a row gets; the component only carries
it out.

### The nearby group

A host Cmdr only FOUND (no saved server, no saved share, nothing the person added: `servers-hub-rows.ts::isNearbyOnly`)
sits under one header row, below everything saved. The reason: a saved server and a machine that happens to be on the
same Wi-Fi had the same weight in the list, and most people never connect to the second kind. A saved host that mDNS
also sees stays among the saved ones with its "Found nearby" status.

- **Open or collapsed** (`servers-hub-items.ts::isNearbyGroupExpanded`): what the person last chose, else collapsed for
  someone with a saved server and expanded for someone with none, for whom the nearby servers are the whole view. The
  choice lives in the hidden `network.nearbyServersGroup` setting (`auto` until the first toggle), read reactively
  through `getNearbyServersGroupChoice`, so the two panes' hubs fold together.
- ❗ **An untoggled group goes by whether a server was saved when the view OPENED, ❌ not by the live count.** A first
  save (Add, or mounting a found host's share) would otherwise fold the group away under the person looking at it. The
  next visit opens collapsed.
- **Keys**: the header is a row to the cursor. Enter and Space toggle it (Space only there), a click toggles it and
  takes the cursor, and Left/Right stay the list's first-and-last jump, as on every row. F8, ⌃⏎, and the palette's
  server commands find no row on it (`getRowUnderCursor()` is `null`), like on "Add server…".
- ❗ **Two index spaces** (`servers-hub-items.ts`). The cursor counts what is ON SCREEN (`visibleHubItems`), so no key
  can land it on a hidden row. `cmdr://state`, `findItemIndex`, `setCursorIndex`, and `getItemCount` count the FULL list
  (`hubItems`), which keeps the nearby servers whatever the group's state: an agent asking which servers exist gets the
  truth, and the group's entry says `state=collapsed`. `setCursorIndex` onto a hidden server opens the group for this
  view (`nearbyRevealed`) and ❌ doesn't record a choice; `selectServer` goes the same way.
- **A group that collapses over the cursor** (the other pane's hub toggled it, `set_setting` did) puts the cursor on the
  header: `servers-hub-keys.ts::cursorAcrossRebuild` treats the hidden row as one that left.
- **The status bar still counts every server**, hidden ones included; the header says how many of those are nearby.
- **Discovery off** empties the discovery list, so there is no group and the "discovery is off" line stands where it
  would be. **Searching** shows its line under an open group or when nothing was found yet; a collapsed group shows a
  spinner on its header instead.

### The pure modules beside it

`ServersHub.svelte` is the table, the cursor, and the keys. Everything that can go quietly wrong lives next door and is
unit-tested:

- **`servers-hub-rows.ts`**: the merge, the status derivation, and the order. ❗ The merge is the part that goes wrong:
  a manually-typed SMB host is BOTH a saved server and a discovered host (adding one injects it into the discovery
  state), so concatenating the two sources shows a person's NAS twice. The dedup matches on the id first, then on the
  name or resolved hostname, because `known_shares` files a host under the server name `statfs` reported while mDNS
  files the same machine under its Bonjour name. Status comes off the VOLUME LIST, ❌ never off `SavedPlace.connected`,
  which is a snapshot from when the listing was built; the switcher's dot reads the same field, and two surfaces
  disagreeing about whether a server is up is worse than either being briefly stale. Order: live sessions, then the ones
  asking something of the user (`signed_out`, `waiting_for_key`), then the rest of what they saved by recency, then the
  hosts Cmdr only found, which stay contiguous at the end whatever their name or recency because the nearby group's
  header sits in front of the first one. ❗ A saved SMB host is "Found nearby" only when a DISCOVERED host matches it,
  ❌ never because of its own manual entry in the discovery list: Cmdr injects every typed-in host there at startup,
  reachable or not. The Address column carries the port off 445 (`localhost:11482`), and `savedHostFor` reads it back
  when mDNS sees nothing.

  ❗ **A many-place server's places are rows right under it** (`hasManyPlaces`: an SMB host's saved shares, an S3
  account's saved buckets and root; `kind: 'place'`, id `share:<volume id>`), placed AFTER the sort so they never drift
  from their server. Each place's status comes off the volume list by its id like every place's. The server row keeps
  `volumeId: null` and `pinned: false`: its places are the place rows. A share row names the account it opens as; an S3
  place names none, since the account row above it shows the key.

  ❗ **An S3 account row carries the NAME, its place rows their buckets**, as an SMB host carries its name over shares
  that read as themselves: a bucket row reads as the bucket the provider spells, and the account ROOT's row reads
  `servers.hub.s3AllBuckets` ("All buckets") and leads the places. The backend labels the root as its account (what the
  switcher and a pane show), which under the account's own row would repeat the name; `isS3AccountRoot` reads the root
  off its app root's empty server path. The model: `src-tauri/src/network/DETAILS.md` § "The S3 twin, a place per
  entry".

  ❗ **An S3 account row is no place.** Its id is the account ROOT place's id whether or not the root is saved, so
  acting through it would dial or pin the root behind the person's back. Each S3 place row connects, pins, edits, and
  forgets through its own volume id with the regular server-place menu (`servers-hub-actions.ts`); the account row's
  Enter answers a toast pointing at its places (`openMoveFor`'s `account` move), its Edit opens the account's editor
  (rename it, change its secret: `$lib/servers/DETAILS.md` § "An S3 edit is the ACCOUNT's or a PLACE's"), it has no
  right-click menu, and F8 forgets every place plus, box checked, the secret they share (written FIRST, while a place
  still names the entry). The account's status is its most urgent place's. Forgetting one place offers that shared
  secret UNCHECKED (`server-row-actions.ts::forgetSavedServer`), since the account's other places sign in with it.

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
  publishes, ❌ never a guess at the string's shape. Without it a person's NAS renames itself the moment they use it: an
  unnamed SMB label is a stand-in, either the way `statfs` spells the server (written to `known_shares` on the first
  share listing, `smb-consumer-guest`) or the address typed into "Add server" with the Name field left empty
  (`manual_servers` derives `host` or `host:port`), and the friendly name they recognize (`SMB Test (Guest)`,
  `Naspolya`) would drop out of the column. A name typed into the add or edit sheet is `user` and wins. Four
  `smb.spec.ts` specs poll on the Bonjour name and are the regression guard.

  ❗ **A saved SMB server claims EVERY host that matches it, not the first.** One machine sits in the discovery list
  twice once a person types a host mDNS already found: the manual entry injects a `manual` host beside the `discovered`
  one. `primaryHost` then picks the DISCOVERED one for the row's name and address, because a manual host is named after
  the address that was typed.

- **`servers-hub-list.svelte.ts`**: the one stateful sibling. `createHubList()` holds what a hub lists as live state
  (the saved servers it read, the rows and items built from them, whether the nearby group is open), one instance per
  hub, so the component keeps only the cursor and the keys.
- **`servers-hub-items.ts`** (+ `servers-hub-keys.ts`): the list as the cursor walks it, over the rows: the nearby
  group's header, which items are on screen, the two index spaces, and where the cursor goes across a rebuild (§ "The
  nearby group").
- **`servers-hub-mcp.ts`**: the `name` encoding. MCP's `PaneFileEntry` has only `name` / `path` / `isDirectory`, so the
  columns are encoded as `protocol=` / `status=` / `address=` tokens (plus `shares=` on an SMB host whose shares were
  listed, which is what `smb.spec.ts` polls on). The nearby group's header is an entry too:
  `Found nearby  kind=group  state=<expanded|collapsed>  servers=<n>` at `smb://nearby`, which `move_cursor` finds by
  that name and `open_under_cursor` toggles. ❗ The status token is locale-independent even though the column beside it
  is translated: an agent parses these strings and a translation landing in the wire would break both silently. A
  one-place row's path is the place's `appRoot` (from `SavedPlace`, which Rust mints in one function); an SMB host keeps
  the `smb://<address>` spelling the host list publishes; an S3 account row publishes the account's own prefix
  (`s3://<key>@<host>:<port>`) and its place rows `kind=place` (a share row keeps `kind=share`); the add row is
  `+ Add server…` at `smb://add`.
- **`servers-hub-actions.ts`**: F8, the two row menus, and the SMB host menu's answers, behind live getters (❌ never
  snapshots: the rows change under a menu that is still open). ❗ A one-place row and an SMB host take different paths
  at every branch, which is why they live in one unit: a one-place row is a PLACE the servers family speaks for, an SMB
  host is a manual-server entry whose "disconnect" unmounts shares rather than dropping a session.
- **`../navigation/servers-hub-rows` consumers**: `../pane/types.ts`'s `NetworkCursorEntry` gains a `server` arm, which
  is how the palette's server commands reach the row under the cursor (`$lib/servers/server-command-target.ts` owns the
  rule: the hub IS a pane, so "the focused pane's volume" would answer the synthetic hub row). The `host` arm carries
  its row too, so Edit and Rename reach an SMB host (`editHubRow`, `$lib/servers/DETAILS.md` § "Which server a command
  acts on").

### Discovery off

`network.enabled` gates mDNS and SMB, which is what the macOS Local Network permission is about; SFTP, WebDAV, and S3
need none of it. So the hub opens either way, keeps listing saved servers, and shows one line plus a link to the switch
in place of the nearby hosts. ❗ No "(disabled)" label, and no redirect to Settings; `network-toggle.spec.ts` is the
regression guard.

### Context menu and F8

F8 forgets the SAVED server under the cursor: a one-place row through `forgetSavedServer` (so the hub asks exactly what
the switcher's menu asks), an SMB host through `forgetSavedSmbHost` (its manual entry, its sign-in history, and its
saved shares; nothing unmounted), a saved share through `forgetServer` on its id (the row and its pin only), an S3 place
like a one-place row, an S3 account as every place under it (§ "The pure modules beside it"), and a host only mDNS knows
about gets the "Can't remove discovered hosts" toast. A share row's right-click is an in-app menu too: Open, the pin,
and Forget share. Right-click on a one-place row opens the house `Menu` at the pointer, holding the same list the
switcher row's → submenu shows (`../navigation/row-menu.ts`; Open moves THIS pane, like Enter); an SMB host keeps its
own native host menu (`show_network_host_context_menu`, one group, only what does something: Edit for a saved host,
Disconnect while a share from it is mounted, Forget saved password when one is stored, Forget server for a typed-in
one), whose actions arrive on the `network-host-context-action` event. Cursor auto-clamps when a row disappears.

`⌃⏎` (`file.contextMenu`) opens the cursor row's menu from the keyboard, the same menu a right-click opens, placed just
under the row by `../pane/context-menu-anchor.ts` (the native host menu takes that point as its `anchor`; a right-click
sends none and macOS uses the pointer). The "Add server…" row has none. A host's places list has no row menu yet, so
`⌃⏎` there does nothing.

❗ **Every row action carries the row's own id, end to end.** The native menu sends `rowId` out and gets it back on the
answer, `runHostAction` finds the row by that exact id (a row gone meanwhile gets nothing), and the backend's
`forgetSavedSmbHost` / `updateSavedSmbHost` take that id alone and find the host in the same listing. ❌ Never resolve
"which row" from a host id, name, or address at action time: two rows can stand on one discovered host, and a lookup by
host picked the first, so "Edit server…" on the second row saved into the first (QA 2026-09-25).

Exports for parent: `setCursorIndex(index)`, `findItemIndex(name)`, `handleKeyDown(e)`, `refresh()`,
`getHostUnderCursor()`, `getRowUnderCursor()`, `getItemCount()`, `openCursorItem()`. The three that speak in indexes
count the full list (§ "The nearby group"). `refresh()` is `pane.refresh`'s entry point from the command layer and is
the same body ⌘R runs locally, which is why the local branch stops propagation (see § Gotchas).

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

The error state's sentence, and the servers list's tooltip for the same failure (`host-status.ts`), come from
`share-list-error-messages.ts`: one `errors.shareList.*` key per `ShareListError` type, naming the host. ❌ Never
`error.message`: the backend documents it as diagnostic detail for the log, and it's English, often a fallback tool's
own stderr. `listSharesOnHost` and `listSharesWithCredentials` throw the typed value as a `ShareListFailure`
(`share-list-error.ts`), `fetchShares` throws its own for a host with no hostname yet (`resolution_failed`), and every
catch reads the value back through `shareListErrorOf`. ❌ Never `e as ShareListError`: anything untyped that lands in
the catch (a runtime exception, an IPC call that broke) had no `type`, and the renderer threw on it; it reads as
`protocol_error` now, its text kept for the log. Pinned by `PlacesBrowser.test.ts`.

A "Forget saved password" button appears in the header once this session has READ a password for the server
(`authenticatedCredentials`, or the host's `has_creds` status), whichever way the listing went: a guest listing of a
host whose server-level password was in use offered no way to forget it. `noteCachedCredentials` sets that status from
the backend's in-memory credential cache (`has_cached_smb_credentials`), filled by the listing's own lookup or a mount.
❗ It never touches the Keychain, so an unread password shows no button until something reads it. Clicking it calls
`forgetCredentials`, which clears both. Shares sort case-insensitively. Escape/Backspace go back.

The `autoMountShare` prop fires once per distinct value (tracked via `lastAutoMountAttempt`), not once per instance, so
"Copy path between panes" can auto-mount a different share without forcing a remount when the source cursor moves to
another share on the same host.

❗ **A queued share is a one-shot for the visit it was queued with.** `../pane/NetworkMountView.svelte` owns it and
retires it (`retireAutoMount`, which also clears the pane's copy through `onAutoMountConsumed`) whenever that visit
ends: a mount went through, the person went back, or another host was picked. Opening a host from the servers list only
ever lists its shares. The bug this closes (cmdr-reports#7, "Cmdr tried to open the share on that server"): the value
outlived its visit, every new `PlacesBrowser` instance fired it again, and a host row the person meant to browse mounted
a share they never picked.

## `smb-sign-in.ts`

SMB's side of the one sign-in sheet. The sheet contract, the three SMB sites and what each `attempt` runs, and the
endpoint header per site live in `../../servers/DETAILS.md`; this section is what SMB alone decides.

`openSmbSignInSheet({ host, label?, shareName?, guestAllowed, initialUsername?, refusal?, attempt })` builds the request
(titled by `label`, the name the person gave the server, when there is one; `host.name` stays every lookup's key), and
the caller's `attempt` gets an `SmbCredentialAnswer` (`{ username, password, remember }`) rather than the sheet's
generic submission. ❗ `username: null` IS guest: all three SMB commands take a nullable username and read it that way,
so a separate flag could only disagree with it.

- **The username** is resolved in one order everywhere: what an earlier attempt tried, then `getKnownShareByName()`'s
  last username for this share, then `getUsernameHint()` (the account typed in the add or edit sheet, else the host's
  share history). ❗ Both lookups take the server BY NAME and match on its stable identity in Rust, so a hint saved
  under one spelling (`Naspolya`) is found when the sheet opens under another (`Naspolya._smb._tcp.local`). ❌ Don't
  rebuild the key in TypeScript: that is what made the two sides disagree once.
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
            └─ startPrefetchShares() → only with a Servers view up, a saved host only:
                 prefetchSharesCmd() → fetchSharesSilent()

User opens the Servers volume → NetworkMountView: holdDiscoveryForServersView()
       └─ first view shown → startPrefetchShares() over every known host
     → ServersHub mounts → listSavedServers() + refreshAllStaleShares()

User double-clicks an SMB host → PlacesBrowser mounts → loadShares()
       ├─ cache hit → render (a saved host's prefetch, or an earlier visit)
       └─ auth required → tryStoredCredentials() → the sign-in sheet if needed

User activates a one-place server → onVolumeChange → the pane lands on a `saved`
       volume → ../pane/place-connect dials, RemoteConnectView renders the wait

User activates the "Add server…" row → the sign-in sheet opens in add mode
       ├─ SFTP / WebDAV → connectServer(target) → the pane lands on the volume
       └─ SMB → connectToServer(address, name) → TCP check → save + inject host
            └─ the sheet answers `handed_off`
                 ├─ PlacesBrowser mounts (host set)
                 └─ if sharePath → autoMountShare triggers mount
```

## Connect directly

`direct-connect.ts::connectDirectly({ volumeId, shareName })` is the single implementation behind every "turn this
OS-mounted share into a direct smb2 session" affordance: the yellow-dot popup in
`../navigation/VolumeBreadcrumb.svelte`, checking the switcher's "Use Cmdr's fast direct connection" row on an
OS-mounted share (`../navigation/direct-connection-switch.svelte.ts`), the switcher's "Connect directly now" fix on a
share whose switch is on but still OS-mounted (`../navigation/row-menu.ts`'s `runRowFix`), and the retry button on the
OS-mount fallback notice.

The sequence, and who speaks at each step:

1. `triggerNetworkDiscovery()`, because the direct connect opens a TCP socket to a private IP, which fires the macOS
   Local Network prompt anyway, so this is the honest moment to also start mDNS.
2. A persistent "Connecting directly…" toast goes up and comes down on every exit path.
3. `upgradeToSmbVolume(volumeId)` answers a typed `UpgradeResult` and never throws for an outcome. `success` → success
   toast + `requestVolumeRefresh()`. `networkError` → the `upgrade-messages.ts` sentence for that `UpgradeFailure`, at
   `error`. `mountNotResponding` → `mountNotRespondingMessage` naming the share, also at `error` and `stillOnOsMount`:
   nothing was dialed and nothing is gone, so a later press is worth making. `volumeGone` / `notSmbMount` →
   `nothingToUpgradeMessage`, at `warn`: nothing broke, there was just no OS-mounted share left to connect (an unmount,
   an eject, or a network drop between the offer and the press).
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

**The gap it fills.** A direct connect that fails on an AUTO path someone is watching (the FSEvents mount watcher,
Cmdr's own mount) leaves the share working on the macOS kernel mount, at a fraction of the speed and outside Cmdr's
control, announced by nothing louder than a yellow dot. That silence once cost an evening of debugging a "slow"
transfer. The manual "Connect directly" path is deliberately NOT a source here: it already reports its own failure to
the person who clicked it. Nor is the startup pass over mounts macOS already made: nobody asked, and a notice at launch
reads as something breaking.

**The flow.** The backend emits `smb-fell-back-to-os-mount { volumeId, share, reason, displayName }` at most once per
SERVER per app run, and only for a caller that says someone is watching (who speaks, the ledger, and the rationale:
`src-tauri/src/network/DETAILS.md` § "Telling the user about a kernel-mount fallback"). `os-mount-notice-bridge.ts`,
mounted from `routes/(main)/+page.svelte` beside the other event bridges, turns it into a persistent INFO toast
rendering `SmbOsMountFallbackToastContent.svelte`, dedup id `smb-os-mount:<volumeId>`.

**A notice that can't offer a retry doesn't.** The bridge turns `reason` into one `retryable` prop, false only for
`shareNotOnServer`, and the toast then renders `osMountFallback.shareNotOnServer` with no button instead of
`osMountFallback.message` with one. The server has said it has no share by that name, so the same ask gets the same
answer and a button could only fail; the copy sends the reader to the server instead. ❌ Don't pass the raw `reason` to
the toast: the component's question is "is there anything to offer", and every other variant answers it the same way.
Which reasons are which, and the two look-alikes that never arrive as this one: `src-tauri/src/network/DETAILS.md` §
"Telling the user about a kernel-mount fallback".

**This Mac blocked the connection.** For `blockedByThisMac` (the backend's evidence rule and the ERR-XGS9X incident:
`src-tauri/src/network/DETAILS.md` § "This Mac refusing the route") the bridge also passes `blockedServer` (the event's
`displayName`), and the notice swaps its sentence for `directConnectionBlockedByThisMacToast` (names the server and the
OS-localized `systemStrings.localNetwork`), leads with an "Open {localNetwork} settings" button
(`openLocalNetworkSettings` in `$lib/tauri-commands`, the same opener Settings > Network uses), and keeps the retry
beside it as the secondary button: the switch works at once, so pressing the retry right after is the whole fix.
"Connect directly" words the same answer through `LocalNetworkBlockedToastContent.svelte` (sentence plus the settings
button), raised by `announceNoUpgrade` instead of a plain string toast, so the sign-in sheet's rounds get it too.

**Dismissal watches the volume list, not the button.** A share can reach a direct session five ways: this notice's
button, the chip's yellow dot, the switcher's direct-connection switch or its "Connect directly now" fix, and the pane's
credential form after a working password. All of them end in `register_replacing_predecessor`, which broadcasts the
volume list, so the bridge dismisses on any `volumes-changed` carrying that volume as `direct`. One rule covers every
route, and a new route can't forget it. A share that goes AWAY broadcasts the list too, so a notice whose volume is no
longer listed retires the same way: its button could only say the share is gone. The bridge asks the toast store which
notices are up (`getToasts`, matched by content component and `props.volumeId`) rather than keeping a list, so there's
no frontend ledger to fall out of step with the backend's or with the user closing one.

**Switching the share's direct connection off withdraws it too.** The notice's button would then do exactly what the
user just opted out of. The volume list doesn't carry the per-share switch, so the backend owns this one: it emits
`smb-os-mount-notice-withdrawn { volumeId }` from `smb_direct_switch`, where the switch goes off whatever route flipped
it, and the bridge dismisses `smb-os-mount:<volumeId>`. Switching ON leaves the notice up: the connect it starts may
still fail, and the `direct` broadcast retires the notice when it doesn't. Backend side:
`src-tauri/src/network/DETAILS.md` § "Telling the user about a kernel-mount fallback".

❗ **Only a listing that finished may retire a notice by absence.** A `timedOut` payload is the last complete list
standing in for a fresh one, and a `discoveryPending` one carries the cached local part beside fresh server rows while
discovery is still out (`src-tauri/src/volumes/DETAILS.md`, "Server rows never wait on local discovery"), so a share
missing from either proves nothing. The rule's one gap: a discovery that started before a brand-new mount and finished
slowly (inside its timeout) after that mount's fallback could retire the fresh notice. The share keeps its yellow dot
either way.

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
  reports it. Cancelling clears it through `handleMountErrorBack`, back to the share list.
- **`auth_required` gets a calm pane, ❌ never the red one.** Behind the sheet it reads "Sign in to open …" with a lock,
  Sign in, and Back, and no Try again (without an account it gets the same answer). `auth_failed` and
  `permission_denied` keep the red "Couldn't mount share" pane: there an answer the person gave was turned away.
  `PlacesBrowser`'s listing does the same for `auth_required` / `signing_required` ("Sign in to {host}", no Retry). Red
  is feedback on something the person did (cmdr-reports#10).
- **Only a credential refusal keeps the sheet open.** A retry that comes back `share_not_found`, `host_unreachable`, or
  `mount_missing` (the system reported the share connected and no mount of it is there:
  `src-tauri/src/network/DETAILS.md` § "A reported mount counts once it's there") answers `handed_off`, closing the
  sheet onto the pane's error state with its own "Try again" / "Back": the sheet has no words for a share that went
  missing. Non-auth failures never open it in the first place. Pinned by `../pane/NetworkMountView.test.ts`.

**The pane's words are the frontend's.** `NetworkMountView` renders `renderMountError(error, host.name)`
(`mount-error-messages.ts`): one `errors.mount.*` key per `MountError` variant, in a record typed over the variants so a
new one can't ship without words. It names the host the way the pane shows it, since the backend only knows the address
it mounted by. `mountNetworkShare` throws the typed value as a `MountFailure` (`mount-error.ts`), and a caught value
that isn't one (the IPC call itself broke) reads as `unexpected`, its text going to the log. The MCP mirror carries the
same sentence plus the typed `reason`. Why the variants are cut the way they are: `src-tauri/src/network/DETAILS.md` §
"A mount refusal is data, and the frontend words it".

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
`RemoteConnectView`'s `host_key_changed`, which offers "Check the key" and Disconnect (`../pane/DETAILS.md` § the
connect views says why, and why never "Trust it"). `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

Lazy-nav path: opening a share that's already `Disconnected` (no fresh event in flight), the `smb-view-state.svelte.ts`
subscription `$effect` notices `currentVolumeInfo?.connectionState === 'disconnected'` and calls
`connectPlace({ volumeId, connectionState: 'disconnected' })`. ❗ Through `$lib/servers/connect-flow.ts`'s arm 1 rather
than `manager.startCycle` directly, because that module documents itself as the ONE caller of the manager's lazy start
and a second caller makes the guardrail a lie. Arm 1 is that call and nothing else, and the manager is idempotent, so
landing on the same share twice still costs nothing. The signed-out banner's Sign in goes through the same flow's arm 2,
which is what carries the `needs_credentials` reason into the sheet's first round.

Disconnect button: `disconnectSmbVolume(volumeId)` shells out to `diskutil unmount` (macOS) → FSEvents fires →
`SmbVolume::on_unmount` → volume removed from `VolumeManager` → `volumes-changed` removes it from the picker.

## Lazy mDNS trigger, and the Servers view's hold

`triggerNetworkDiscovery()`:

1. No-ops if `network.enabled === false`.
2. Calls `noteNetworkAction()` (backend: reload the manual servers, run the existing-SMB-mount upgrade pass, which holds
   the browse while it resolves).
3. Sets `network.firstTriggerDone = true` so later launches warm the server list up briefly (returning users see known
   servers at once, without re-prompts).

Call sites: `ServersHub.onMount` and `VolumeBreadcrumb.handleSubmenuAction` (the OS-mount → direct-smb2 upgrade also
opens a private-IP socket).

The browse itself runs while a Servers view is on screen: `NetworkMountView` holds it through
`holdDiscoveryForServersView()` (an `$effect` whose cleanup releases it), and the store tells the backend only on the
first view shown and the last one gone (`setServersViewShown`). Two panes can each show one. `initNetworkDiscovery`
re-sends the store's count, so a reloaded page corrects what the one before it left. Between browses the backend keeps
the hosts it found, so a view opens on them at once and the fresh browse adds and drops hosts live. Backend side:
`src-tauri/src/network/DETAILS.md` § "Discovery runs only while something needs it".

## Key decisions

- **Lazy discovery on first user intent, not at startup**: avoids the macOS Local Network prompt on fresh installs
  before the user has context; `network.firstTriggerDone` persists so returning users get the launch warm-up.
- **Resolution and share prefetch are fire-and-forget**: hosts come and go, so a timeout / unreachable during prefetch
  is normal, not worth surfacing. The UI shows "Not checked" / "Waiting..." until data arrives; only user-initiated
  actions surface errors.
- **Prefetch is for saved servers only, and only once a Servers view opens** (#324's option (b), David's call): the
  first open of the Servers view may show a saved server's list loading, and the first open of a host Cmdr only found
  waits for its listing. The trade is those waits against a file manager that connects to SMB machines at each launch
  whether or not anyone looks.
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
  `getKnownShareByName`, `updateKnownShare`, `updateLeftPaneState`, `updateRightPaneState`, `connectToServer`
- `$lib/settings/network-settings`: `getNetworkTimeoutMs`, `getShareCacheTtlMs`
- `$lib/utils/confirm-dialog`: `confirmDialog`
- `$lib/ui/toast`: `addToast`
- `../navigation/keyboard-shortcuts`: `handleNavigationShortcut`
- `../types`: `NetworkHost`, `DiscoveryState`, `ShareInfo`, `ShareListResult`, `ShareListError`, `AuthMode`
