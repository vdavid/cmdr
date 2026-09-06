# Servers: one front end for SFTP, WebDAV, SMB, and phones over ADB

**The problem.** Three finished backends and nobody can reach two of them. `crates/cmdr-sftp` and `crates/cmdr-webdav`
connect, list, read, write, and reconnect, and nothing puts either on screen. `crates/cmdr-adb` lists a phone only once
it is already authorized, so the one moment the user needs feedback (the "Allow USB debugging?" prompt) is silent. And
the one remote backend that IS on screen, SMB, renders its sign-in form inside a pane, where Tab means "switch panes" in
Cmdr's own mental model and "next field" in macOS's, and the form loses either way.

**The shape of the fix.** One model for every remote thing (an account holds places, the switcher shows pinned places),
one modal sheet for every sign-in (backend tells the sheet what to ask, the sheet asks), one pane view for every wait
(connecting, waiting for the phone, signed out, refused), and a hub pane state that lists what the user has. SMB
migrates onto all four so the next backend (S3, then an OAuth provider) inherits them instead of adding a fifth dialog.

This plan supersedes `servers-in-the-sidebar.md` (wipe it when M3 lands) and absorbs the frontend half of
`android-adb-ui.md` (wipe it when M6 lands; its eight decisions are restated below only where this plan changes them).
The backend contracts this builds on are canonical elsewhere and ❌ not restated here: `crates/cmdr-sftp/DETAILS.md` §
"Connecting from the frontend", `crates/cmdr-webdav/DETAILS.md` § the same, `apps/desktop/src-tauri/src/adb/DETAILS.md`,
`apps/desktop/src/lib/file-explorer/network/DETAILS.md` (the SMB flows being migrated).

David's calls, already taken (2026-09-06): the group stays "Network" and the hub row is "Servers"; places auto-pin on
first connect and unpin from the context menu; an educational toast fires at five pinned rows; every backend change the
frontend needs is in scope; S3 and OAuth are sketched here as contracts, not built.

## The model

Three levels. Every decision below is a consequence of them.

1. **Account**: an endpoint plus an identity. SMB host, SFTP `user@host:port`, WebDAV URL plus username, later an S3
   endpoint plus access key, later a Google account. It owns credentials, trust (host keys, certificates, refresh
   tokens), and the auto-reconnect switch. Saved, never itself navigable.
2. **Place**: the mountable thing under an account. An SMB share, an S3 bucket, an SFTP or WebDAV root, a shared drive.
   A place becomes a `VolumeInfo`; it is what tabs, favorites, and paths point at. SFTP and WebDAV have exactly one
   place per account; SMB and S3 have many. The SMB host → shares step stops being an oddity and becomes the general
   shape.
3. **Pin**: whether a place shows in the volume switcher.

**Why three levels and not "a server is a volume".** A server is not plugged in. Nothing else reminds the user it
exists, so the second use of a server has to cost one keystroke or the feature reads as "connect" rather than "my NAS is
right there". That needs saved things on screen, and saved things on screen need a cap, or a user with 12 buckets
scrolls past their own disks. Pins are the cap, and the user holds it.

### The four rules

- **The switcher's Network group holds three things and nothing else**: the hub row ("Servers"), every place that is
  connected right now, and every pinned place, greyed with a hollow dot when disconnected. A new place is pinned on its
  first successful connect. "Unpin" lives in the row's context menu. ❌ The "Connect to server…" pseudo-row leaves the
  switcher: it lives in the hub, the command palette, and ⌘K.
- **Every row in the switcher is a place. Opening a place that isn't live brings it to life in the pane, with a cancel
  button.** MTP already works this way (`MtpConnectionView`); SFTP, WebDAV, SMB, and ADB inherit it.
- **Dialogs are for entering data; panes are for waiting.** The sheet exists to type a new server, edit one, or answer a
  sign-in. Connecting, waiting for the phone's Allow tap, a refusal with retry, a changed host key, and "signed out" all
  render in the pane. This is what reconciles `android-adb-ui.md`'s "never a modal" with a connect dialog: both are
  right once split.
- **The sheet opens only on user intent.** Activating a row that needs a sign-in, pressing Add, pressing "Sign in…".
  ❌ Never on its own when a session drops: a modal stealing focus during a lid-open wake is the wrong thing, and the
  SMB doc's "a reconnect is silent" promise stays. The pane's banner has the button.

## What exists (pointers, not restatement)

- Every SFTP and WebDAV command is typed and wired; the outcome enums are in `commands/sftp.rs` and
  `commands/webdav.rs`. Cancel ids are the caller's. The reconnect manager and `getVolumeSignInState` are backend-neutral
  despite SMB names (`network/DETAILS.md` § "SMB live-reconnect flow").
- The volume list is built by `volume_listing::complete` (device providers folded in, then
  `enrich_from_volume_registry`; ❗ anything appended after enrichment ships `capabilities: None`).
- Path resolution is `commands/volumes.rs::resolve_path_to_volume`, prefix-dispatched on `adb://`, `mtp://`, `smb://`.
  ❗ SFTP and WebDAV paths carry no scheme (`/srv/data/x`), so prefix dispatch cannot work for them.
- Four saved-server stores with no shared type: `sftp_known_servers.rs`, `webdav_known_servers.rs`, `known_shares.rs`
  (SMB, keyed `(server, share)`), `manual_servers.rs` (SMB hosts typed by hand). ❗ SFTP's and WebDAV's `remember`
  replaces the whole entry on every connect, so any new field is clobbered unless the connect path reads the existing
  entry first.
- `SignInPrompt` (`crates/cmdr-fs/src/volume/types.rs`) is `Nothing | Password | KeyPassphrase`, read live through
  `Volume::sign_in_prompt`; the reconnect manager already stores the answer per volume and renders nothing.
- The SMB sign-in form (`NetworkLoginForm.svelte`) is rendered from three sites: `ShareBrowser` (listing auth),
  `NetworkMountView` (mount-phase auth), `SmbReauthView` (reconnect re-auth); plus `FilePane`'s `smbUpgradeLogin`
  branch. `smb-login-hosts.ts` exists only to find a pane that can host it.
- Dialog registration is a three-step contract (`SOFT_DIALOG_REGISTRY`, `ModalDialog` `dialogId`, a gallery row +
  fixture). App-level dialogs mount from `routes/(main)/+page.svelte` or `+layout.svelte`; `ConnectToServerDialog` is
  the outlier, mounted inside `NetworkMountView`.
- ⌘K is unbound. The palette is ⌘⇧P. Commands are data in `commands/sources/*.ts` plus a handler; four places per
  command.
- ADB: `AdbDeviceProvider::entries()` lists `Ready` devices only; `list_adb_devices` returns every state typed;
  `get_adb_install_status` / `recheck_adb_install` exist; the connect is not cancelable from the pane; no settings.
- Toasts: `addToast` with `dismissal: 'persistent'`, dedup `id`, component content; the once-ever precedent is
  `behavior.doubleClickOnPaneNotificationSeen`.

## Decisions

### D1. One connection-state field for every row

`LocationInfo.smb_connection_state: Option<SmbConnectionState>` (`direct | os_mount | disconnected`) becomes
`connection_state: Option<ConnectionState>`, on both platform twins and the TS `VolumeInfo`, with these variants:

- `direct`: a live session Cmdr owns (SMB smb2, SFTP, WebDAV, ADB dialed).
- `os_mount`: SMB's kernel-mount fallback. Unchanged meaning.
- `disconnected`: the session dropped and the backoff loop is running or gave up.
- `needs_sign_in`: the backend stopped retrying because a credential is what's missing. Replaces the manager's
  `needs-auth` derivation as the row's source of truth.
- `needs_host_key_approval`: SFTP only; the row shows it, the pane sends the user to look at the key.
- `saved`: pinned, not connected, nothing in flight. The greyed row with the hollow dot.
- `waiting_for_device`: an ADB row that is plugged in and not `Ready` (`unauthorized`, `authorizing`, `connecting`).
- `unavailable`: an ADB row in `offline` or `no permissions`. Disabled, with the reason in a tooltip.

**Why one enum and not a second field beside `smbConnectionState`.** Every consumer (the dot, the tooltip, the eject
predicate, the reconnect subscription in `FilePane`, the volume-store patch on `volume-connection-changed`) asks "how
live is this row", and two fields answering the same question is how the ADB row and the SFTP row end up with different
dots for the same state. The Linux twin currently types it as `Option<String>`; it becomes the enum there too.

The rename is mechanical and wide (`smbConnectionState` appears in the store, the breadcrumb, `FilePane`, the eject
predicate, `direct-connect`, tests, and two E2E specs). Do it in one commit with `bindings.ts` regenerated, before any
feature work, so every later diff is small.

### D2. Remote volumes are listed by a servers arm; saved-but-unpinned ones are not

`volume_listing::complete` grows a fold BEFORE `enrich_from_volume_registry`: one `LocationInfo` per registered SFTP
or WebDAV volume, plus one per pinned saved server that has no registered volume (state `saved`). `category: Network`,
`fs_type: "sftp" | "webdav"`, `is_ejectable: false` (D6 says what the row's action is), `supports_trash: false`, `name`
the saved `displayName`, `path` the volume's root (`remoteRoot`), `id` from `cmdr_fs::volume::ids::sftp_volume_id` /
`webdav_volume_id`, which is also the id the registry uses, so a `saved` row and the volume it becomes share one id
across the dial. ❗ That identity is what makes a tab restorable: a tab stores `(volumeId, path)`, the switcher shows the
same id greyed, and activating either dials the same saved entry.

The device-provider seam is deliberately NOT reused (`crates/cmdr-sftp/DETAILS.md` says why: it lists things that
appear and leave on their own). The servers arm reads the two stores plus the registry, all cached state; ❌ never the
wire, because the listing runs on every `volumes-changed`.

### D3. Path resolution takes the pane's volume as a hint

`resolve_path_volume(path)` and `resolve_location(path)` gain an optional `volume_hint: Option<String>`. When the hint
names a registered remote volume whose paths are scheme-free (SFTP, WebDAV) and `path` is absolute, the answer is that
volume. The frontend passes the pane's current volume id from the two call sites that matter: the breadcrumb's
`containingVolumeId` derivation and `navigate()`'s `goTo` arm. Everything else (favorites, go-to-path with a plain
absolute path on a local pane) passes nothing and resolves as today.

**Why a hint and not a scheme.** The crates spell remote paths exactly like local ones by design (the SFTP path doc
explains the `to_remote_path` stance), and inventing an `sftp://` display scheme would leak into every breadcrumb,
copy-path, and MCP row. A hint is the honest statement of what the pane already knows. **Why not resolve by the
registry alone**: two servers can both have `/home/ada`, and a local disk certainly does; the hint is what breaks the
tie.

### D4. One command family for "a server", protocol-agnostic where the UI is

A new `commands/servers.rs` (app side, over the existing per-protocol wiring):

- `list_saved_servers() -> Vec<SavedServer>`: the union of the SFTP store, the WebDAV store, SMB hosts derived from
  `known_shares.rs` and `manual_servers.rs`, each as `{ id, protocol, displayName, address, username?, pinned,
  lastConnectedAt?, places: Vec<SavedPlace> }` where a place is `{ volumeId, name, pinned, connected }`. SFTP and WebDAV
  have one place; SMB hosts list their known shares.
- `connect_saved_place(volume_id, attempt_id, secret: Option<SecretOffer>) -> ServerConnectOutcome`: dials by saved
  entry, so the frontend never re-plumbs eight target fields to reopen a server. `SecretOffer { secret, remember }`; see
  D5.
- `connect_server(target: ServerTarget, attempt_id, secret: Option<SecretOffer>) -> ServerConnectOutcome`: the
  add-mode dial. `ServerTarget` is a tagged union of the two existing param shapes (`sftp { … }`, `webdav { … }`); SMB
  stays on its own commands (its connect is a share mount, not a session).
- `cancel_server_connect(attempt_id)`, `disconnect_place(volume_id)`, `forget_server(id)`,
  `forget_server_secret(id)`, `set_place_pinned(volume_id, pinned)`, `update_saved_server(SavedServerPatch)`.
- `ServerConnectOutcome` is the superset, tagged on `outcome`:
  `connected { volumeId } | needs_host_key_approval { host, port, algorithm, fingerprint, kind } | host_key_revoked { algorithm, fingerprint } | authentication_rejected | needs_credentials | auth_method_unsupported | certificate_untrusted | not_a_webdav_server | invalid_url | timed_out | unreachable | cancelled`.
  ❗ `AuthMethodUnsupported` stops surfacing as `authentication_rejected` here: a Digest-only server never saw the
  password, and "check your password" is the wrong fix. The WebDAV command keeps its own enum; this one maps.

The per-protocol commands stay (their tests, their docs, their callers in the wiring); the family is a facade. **Why a
facade and not a rewrite**: the two backends' outcome enums are already correct and tested, and a frontend that branches
on a superset is one `switch` instead of two half-matching ones. The facade is also where S3 plugs in later: one more
`ServerTarget` arm, one more store, the same outcome enum plus whatever S3 adds.

### D5. A first connect may carry a one-shot secret

`connect_sftp_volume` and `connect_webdav_volume` deliberately take no secret; the dial reads the store. That leaves
"connect once without remembering" as save → dial → delete, and a Keychain entry that exists for a second is not the
user's choice. So the wiring gets a `SecretOffer`:

- `remember: true` → `save_*_credentials` first, then dial. Identical to today's flow, one round-trip shorter.
- `remember: false` → the dial runs with a `CredentialStore` wrapper that answers this one `(service, scope)` from
  memory and forwards everything else. The wrapper is built per attempt and dropped with it. ❗ The volume never holds
  the secret: a later drop asks again, which is what "not remembered" means (`crates/cmdr-sftp/DETAILS.md` § "The two
  switches" is the contract, unchanged).

Where the wrapper lives: `network/one_shot_credentials.rs`, generic over the seam in `cmdr_fs::volume::host::credentials`.
❗ The wrapper is the ONLY way a secret reaches a dial without the store; ❌ no `password` argument on the crate-level
params.

### D6. Servers say "Disconnect"; disconnecting keeps the row

The eject slot on a remote row is a Disconnect control (an unplug icon, tooltip "Disconnect"), mirroring
`android-adb-ui.md` decision 8: "Eject" promises safe-to-unplug and a server has nothing to unplug. Disconnecting a
pinned place leaves it as a `saved` row; "Forget server" (removes the entry and its places) and "Forget saved password"
are separate context-menu items, both confirmed. MTP keeps "Eject": it earns it by closing the device session.

The row's context menu, in order: Open, Disconnect (when live), Pin to switcher / Unpin, Edit…, Forget saved password
(when one exists), Forget server. The native menu is `commands/menu.rs::show_volume_row_context_menu`'s sibling, with
actions arriving on the existing `volume-context-action` event as typed `action` values (❌ never a label).

### D7. The hub is a pane state, and it is the "Network" row grown up

The synthetic `network` volume keeps its id; its label becomes "Servers" and its sentinel path becomes `network://`
(the hub lists non-SMB accounts, so `smb://` as its root is wrong; SMB hosts keep `smb://<host>` paths). `NetworkBrowser`
becomes `ServersHub.svelte`: a table with columns Name, Type (SFTP / WebDAV / SMB), Address, Status, Last used, sourced
from `list_saved_servers()` merged with the discovered SMB hosts in `network-store`, plus an "Add server…" row at the
bottom (the `+` pseudo-row today). Status words: Connected, Saved, Found nearby, Signed out, Waiting for the key.

- Enter on an SFTP or WebDAV account navigates the pane to its one place (dialing if needed, D8). Enter on an SMB host
  opens its places list (`ShareBrowser`, renamed `PlacesBrowser` and given an `account` prop rather than a `host`, so an
  S3 account's buckets render through the same component later).
- F8 forgets a saved server (confirmed) or toasts "Can't remove discovered hosts" as today. ⌘E and the context menu
  edit. Right-click shows the same native menu as D6 plus "Pin" per place.
- The hub row pushes MCP rows the way `NetworkBrowser` does today, one encoded `name` per row with `protocol=`,
  `status=`, `address=` tokens, and the add row as `+ Add server…` at `network://add`.
- A pane on the hub keeps `syncsToMcp: false`, `canWrite: false`, and the rest of the `network` capability row.

**Why a pane state and not a settings list.** David is considering settings-as-pane-state generally; this is the pilot,
with one rule carried over: a pane state may be a list, never a form. The saved-server list in Settings is dropped; a
Settings card keeps only what the hub can't show (D11).

### D8. One connect flow, one pane view, one sheet

Three new modules under `apps/desktop/src/lib/servers/`:

- **`connect-flow.ts`**: the orchestrator. Given a saved place or an add-mode target, it loops: dial → on
  `needs_host_key_approval` open the sheet's key step → approve (re-check semantics per the SFTP doc) → dial again; on
  `needs_credentials` or `authentication_rejected` open the sheet's sign-in step → dial with the offer → loop; terminal
  outcomes return. The attempt id is minted here before the first dial, so cancel is armed from the first millisecond.
  ❗ It is the only caller of `connect_server` / `connect_saved_place`; the hub, the switcher, the sheet's Connect
  button, and the pane banner all go through it. A `cancelled` outcome returns silently: the user pressed the button.
- **`RemoteConnectView.svelte`** (in `file-explorer/pane/`): the non-interactive pane states, replacing
  `SmbReconnectingView`, `SmbReauthView`, `MtpConnectionView`'s connecting branch, and the ADB view the spec never
  built. Typed `state` prop: `connecting { cancel }`, `waiting_for_device { reason }`, `signed_out { shape, signIn }`,
  `host_key_changed { lookAtKey }`, `refused { refusal, retry, disconnect }`, `gave_up { retry, disconnect }`. Every
  state names what is happening in one sentence (design principle: radical transparency), and every one with a process
  behind it has a cancel.
- **`SignInSheet.svelte`** (`servers/`): one `ModalDialog`, dialog id `server-sign-in`, three modes. `add` (address
  first, D9), `sign-in` (endpoint as a read-only header, credential fields only), `edit` (the add form prefilled, the two
  switches, the `needs_stored_secret` warning when the backend reports it). The host-key approval is a step inside the
  sheet that replaces the body, ❌ not a second dialog. It mounts from `+layout.svelte` behind a
  `sign-in-sheet-state.svelte.ts` singleton whose `openSignInSheet(request): Promise<SignInSheetResult>` any caller can
  await. This retires `smb-login-hosts.ts` entirely.

**The sheet takes an `attempt` callback rather than dialing itself.** `attempt(credentials) → Promise<AttemptResult>`
where `AttemptResult = { ok: true } | { ok: false; refusal: SignInRefusal }` and `SignInRefusal` is typed
(`authentication_rejected | needs_credentials | auth_method_unsupported | unreachable | timed_out | other`). The sheet
owns the form, the inline refusal under the field it is about, and the retry; the caller owns the protocol. That is what
lets SMB's three sites (a share listing, a share mount, a reconnect) reuse it with their own commands, and what lets S3
plug in with one more renderer.

**The backend tells the sheet what to ask.** `SignInPrompt` widens into `SignInShape` (D10), and the sheet has one
renderer per variant. ❌ The frontend never derives the shape from a rung, a protocol, or a connect result: the SFTP doc's
"asked when the banner renders" rule stands.

### D9. Add mode is address-first

People paste what they have: an `ssh` line, `user@host`, `sftp://host:2222/srv`, a Nextcloud URL, `https://nas:5006/`,
`smb://naspolya`. `servers/address-parser.ts` (pure, proptested) turns one string into `{ protocol, host, port,
username?, path? }` and the sheet flips a `ToggleGroup` (SMB / SFTP / WebDAV) to match, leaving it editable. Fields per
protocol, in order: Address, Username, Password (with "Remember in Keychain", default on), then an "Advanced"
disclosure: Name, Remote folder, and for SFTP a Key file (a path field with a Browse button through
`@tauri-apps/plugin-dialog`, which already has its capability) plus "Use ssh-agent" (default on), and "Reconnect
automatically" (default on). The Connect button arms cancel before the call and the sheet stays open while dialing so
a refusal lands beside the field it is about.

Two refusals get a remedy button because they are where real users stall:

- `not_a_webdav_server` on a bare origin: "Try the Nextcloud address" retries with `/remote.php/dav/files/<username>/`
  appended (ownCloud shares the path). Nobody knows that path.
- `certificate_untrusted`: worded honestly (the certificate isn't trusted by macOS; add it in Keychain Access) with no
  button that can't work. Trust-on-first-use is `webdav-backend-follow-ups.md` § 2 and stays backend work.

SMB in add mode keeps today's behavior behind the same field: `connectToServer(address)` injects a manual host and opens
its places; no credentials are asked until a listing or mount refuses.

### D10. `SignInShape` replaces `SignInPrompt`

`crates/cmdr-fs/src/volume/types.rs`:

```
SignInShape =
  | nothing
  | password                              // SFTP password / keyboard-interactive, WebDAV
  | key_passphrase                        // SFTP encrypted key file
  | username_password { guest_allowed }   // SMB (username editable; the share is the identity)
```

Reserved, documented here and in the type's doc comment, ❌ not added until a producer exists:

- `access_keys { session_token: bool }`: S3. Access key id, secret access key, optional session token.
- `oauth { provider }`: a "Continue in your browser" button and a waiting state; the callback comes home on a loopback
  listener on a random high port or a `cmdr://` deep link, backend-owned; "remember" is implicit (the refresh token is
  the only sane state), and a revoked token surfaces as `needs_sign_in` with the same banner.

SMB's `SmbVolume::sign_in_prompt` returns `username_password { guest_allowed }` from its known auth options. The
default stays `password` (the safe way to be wrong, per the trait doc). The reconnect manager's stored answer becomes
the `shape` the `signed_out` pane state hands the sheet.

### D11. Settings

`Settings > File systems` gets, beside "SMB/Network shares" and "MTP":

- **Servers (SFTP, WebDAV)**: one card, "Trusted host keys" listing `listTrustedSftpHostKeys()` rows with a Forget
  button each. Nothing else: saved servers live in the hub.
- **Android (ADB)**, exactly `android-adb-ui.md` decision 5's four controls (enable switch default on, status,
  Re-check, `adb` location with Browse), plus one addition: when the status is "Not found", a copyable
  `brew install android-platform-tools` line the way `PtpcameradDialog` shows its command. Settings ids:
  `fileOperations.adbEnabled`, `fileOperations.adbBinaryPath`; both follow the five-place `mtpEnabled` plumbing
  (`settings/loader.rs` hand-parsed dot keys, startup seed, live apply through a `set_adb_settings` command that
  restarts the tracker).

### D12. ADB rows and the phone's Allow tap

`DeviceVolumeEntry` grows `connection_state: Option<ConnectionState>` and `AdbDeviceProvider::entries()` lists every
device except `recovery`, `bootloader`, and `sideload`: `Ready` as today, `unauthorized` / `authorizing` / `connecting`
as `waiting_for_device`, `offline` / `no permissions` as `unavailable`. This resolves the contradiction between
`android-adb-ui.md` decisions 2 and 3: `waiting_for_device` rows ARE openable and lead to `RemoteConnectView`'s
`waiting_for_device` state, which subscribes to `volumes-changed` and navigates on its own the moment the row turns
`direct`; `unavailable` rows are disabled with the reason as tooltip.

Also from that spec, unchanged: panes open at `/sdcard`; ADB is not indexed; the connect becomes cancelable
(`connect_adb_device` takes the caller's attempt id like SFTP does). The merged one-row-per-phone item (decision 1) is
M8, last, and `adb.volumeLabelWithSuffix` dies with it.

**Discoverability**, the one thing that spec left out: a phone with USB debugging off is a plain MTP row, and nobody
will find ADB. The MTP pane header gets one quiet dismissible line, "Want the whole filesystem? Turn on USB debugging",
linking to a short help page, gated by `behavior.adbHintDismissed`.

### D13. Reach

- **⌘K** opens the sheet in add mode (Finder's binding for the same thing). Palette: "Connect to server…" and "Show
  servers" (navigates the focused pane to the hub). Both via the four-place command contract.
- **Go to path** (⌘G) accepts a pasted `sftp://`, `ssh://`, `webdav://`, `davs://`, `https://`, or `smb://` address and
  opens the sheet prefilled. `adb://` and `mtp://` short-circuit to the device (`android-adb-backend-follow-ups.md` § 3,
  an adjacent clear win, taken here).
- **Educational toast**: when the pinned count first reaches five, one persistent toast (`behavior.serversPinHintSeen`)
  teaches right-click → Unpin, and adds a line about favorites when the user has three or more. Dismiss with "Got it".
- **A favorite inside a server** is a path favorite whose containing volume is a place; opening it dials. No new
  concept; it falls out of D2 + D3 and gets one E2E.

### D14. Restore behavior

A restored tab on a remote place comes back as `saved` and dials when ACTIVATED (the pane shows `connecting` with
cancel). ❌ Launch never dials a server in the background: four servers dialing at startup is four Keychain reads and
four network waits nobody asked for. `path-navigation.ts`'s initial-path pick treats a `saved` volume as "path exists,
don't probe" so a restored tab doesn't fall back to home before the user has pressed anything.

## Milestones

Sequential, one worktree. Each ends green on the named checks, committed, with docs updated. "TDD" marks a real
red → green sequence; "after" marks tests written once the shape settles.

### M0. Backend groundwork (no UI change)

1. D1: `ConnectionState` on both platform twins and `VolumeInfo`; rename every consumer; `bindings.ts` regenerated.
   Tests after: the enrichment cells in `volumes/smb.rs`, `volume-store` patch test, breadcrumb tests, the two E2E specs
   keyed on the dot.
2. D10: `SignInShape`; SMB's producer; `get_volume_sign_in_state` answers it. TDD: `commands/network_test.rs` default
   cell, an SMB cell for `guest_allowed`, and the SFTP rung table in `reconnect_test.rs` updated by variant name.
3. `pinned` on `KnownSftpServer`, `KnownWebdavServer`, `KnownNetworkShare` (`#[serde(default)]`, default `false`; the
   connect path sets `true` only when the entry is NEW, and otherwise preserves the stored value). TDD: the
   "field-absent file" cell each store already has, copied for `pinned`, plus a "reconnect preserves an unpinned entry"
   cell in each wiring test.
4. D5: `one_shot_credentials.rs` and `SecretOffer` on both wirings. TDD: a wrapper cell (answers the one key, forwards
   the rest, never writes), a wiring cell per backend against the Docker fixture (`--run-ignored only` lane) proving a
   `remember: false` dial leaves the store empty afterwards.
5. D4: `commands/servers.rs` with `list_saved_servers`, `connect_saved_place`, `connect_server`, cancel, disconnect,
   forget, forget-secret, pin, update. TDD: `commands/servers_test.rs` cells for the union listing (three stores, one
   pinned, one connected), the outcome mapping (every WebDAV and SFTP variant lands on its superset twin,
   `AuthMethodUnsupported` included), and pin round-trips.
6. D2: the servers arm in `volume_listing::complete`. TDD: a listing cell with one registered SFTP volume, one pinned
   WebDAV entry, one unpinned entry (absent), asserting id equality between the `saved` row and the id
   `sftp_volume_id` mints.
7. D3: the `volume_hint`. TDD: `commands/volumes.rs` cells: hint names a registered SFTP volume + absolute path → that
   volume; hint names a local volume → today's answer; no hint → today's answer.
8. D12 backend half: `DeviceVolumeEntry.connection_state`, the provider listing all states, `connect_adb_device`
   taking an attempt id, `/sdcard` as the provider's `path`. TDD: `adb/device_provider.rs` cells per state, a cancel
   cell against `FakeAdbServer`.
9. D11 backend half: `adb_enabled` / `adb_binary_path` settings plumbing and `set_adb_settings`. Tests after: the
   loader parse cell, a tracker restart cell.
10. Docs: `crates/cmdr-fs` (the shape), `network/DETAILS.md` (servers family, one-shot wrapper), `volumes/DETAILS.md`
    (the arm, the hint), `adb/DETAILS.md`, `commands/DETAILS.md`. Wipe the "not wired yet" bullets these close.

Checks: `pnpm check rust`, `pnpm check --include-slow` once at the end of M0 (the fixture lanes).

### M1. Switcher rows and the hub

1. D7: `ServersHub.svelte` replacing `NetworkBrowser.svelte` (keep the file history: rename, then edit), the
   `network://` sentinel, `PlacesBrowser` (renamed `ShareBrowser` with an `account` prop), the MCP row encoding, the
   status column. `volume-grouping.ts` applies the three-things rule; the switcher row renders the `saved` dot, the
   protocol in the `volume-fs` slot, and the Disconnect control (D6). The context menu (D6) with its Rust sibling.
2. `handleVolumeSelect` on a `saved` row navigates to the volume id; `FilePane` gates `connectionState === 'saved'` in
   front of kind and renders `RemoteConnectView` `connecting`, which calls `connect-flow` (M2 delivers the sheet; until
   then a `needs_credentials` outcome renders `refused` with a "Sign in…" button that does nothing but is present, so
   M1 is demoable on a server with a stored secret).
3. D14 restore.

Tests: TDD for `volume-grouping` (three-things rule, pinned-only, connected-unpinned appears, "Connect to server…" row
gone), `address-free` hub sorting; after: `ServersHub.test.ts` (rename of `NetworkBrowser.test.ts`, with new rows),
`PlacesBrowser.test.ts`, breadcrumb tests, MCP encoding test, an E2E happy path against the Docker SFTP fixture (pin →
restart → greyed row → activate → connected), added to `smb.spec.ts`'s sibling `servers.spec.ts` under the existing
`--features playwright-e2e,smb-e2e` gate widened to the SFTP fixture. Docs: `network/CLAUDE.md` + `DETAILS.md` (rename
the module to what it is: `file-explorer/servers/`? ❌ no: keep `network/` and rename the files; a directory move is
churn without a win), `navigation/CLAUDE.md`, `pane/CLAUDE.md`.

Checks: `pnpm check --fast` per step, `pnpm check` at the end, `pnpm check desktop-e2e-playwright` for the spec.

### M2. The sign-in sheet and the connect flow

1. `address-parser.ts` (TDD, proptest-style with `fast-check` if present, else example tables: every shape in D9
   round-trips; `user@host` is SFTP; a bare `https://` origin is WebDAV; `smb://` is SMB; garbage is `unparsed`).
2. `SignInSheet.svelte` with the four renderers (`nothing` never opens the sheet), the host-key step (both `kind`
   values, revoked, `superseded` restart, `unreachable` no-write), the add mode fields, the edit mode with the two
   switches and the `needs_stored_secret` warning (`getSftpUnattendedReconnect` / the WebDAV twin, asked when the sheet
   renders). Dialog registry, gallery rows (one per mode, fixtures), a11y block in `network.a11y.test.ts`.
3. `connect-flow.ts` (TDD with `installIpcMock`: the three-round first connection, `superseded` restart, cancel from
   each phase returns `cancelled` silently, `remember: false` sends the offer and never `save_*`).
4. Wire: hub Add row, ⌘K, palette, go-to-path URL branch (D13), `RemoteConnectView.signed_out`'s button, the switcher
   row's Edit…. Delete `ConnectToServerDialog.svelte`.
5. The two remedy buttons (D9).

Tests: above, plus an E2E: add an SFTP server through the sheet against the fixture with a wrong password first
(`authentication_rejected` renders inline, the field keeps focus), then the right one, then `remember` off leaves
`hasSftpCredentials` false. Docs: `servers/CLAUDE.md` + `DETAILS.md` (new dir: the flow, the sheet contract, the
renderer table, the reserved shapes), `docs/guides/building-ui.md` gains one line pointing at the sheet for any
credential ask, `docs/architecture.md` row.

### M3. SMB moves onto the sheet and the pane view

1. `ShareBrowser`'s listing auth, `NetworkMountView`'s mount auth, `SmbReauthView`, and `FilePane`'s `smbUpgradeLogin`
   branch all call `openSignInSheet({ mode: 'sign-in', shape: username_password, attempt })` with their own `attempt`.
   `direct-connect.ts`'s `raiseCredentialsForm` becomes a call into the sheet; `smb-login-hosts.ts` and
   `NetworkLoginForm.svelte`'s in-pane rendering are deleted (the form's field logic moves into the renderer, including
   the username-hint lookup and the guest radio, which becomes a `RadioGroup` inside the renderer with the same
   `$derived.by` rule).
2. `SmbReconnectingView` and `SmbReauthView` are replaced by `RemoteConnectView` states; the reconnect manager's
   `needs-auth` maps to `signed_out { shape }`. `VolumeUnreachableBanner`'s `smbGaveUp` variant becomes `gave_up`.
3. Wipe `servers-in-the-sidebar.md` from `docs/specs/` (its intent now lives in `servers/DETAILS.md`).

Tests: the three site tests rewritten to assert the sheet request (not a rendered form); `smb-reconnect-manager` test
for the shape hand-off; a11y blocks updated; the SMB E2E spec's login steps retargeted to the sheet. Docs:
`network/CLAUDE.md` loses five must-knows (Tab guard, `connectionMode` rule, login-hosts, pre-prompt rule stays),
`pane/CLAUDE.md`, `DETAILS.md`s.

### M4. Pins, the toast, Settings

1. SMB places pinned on first mount (`known_shares.rs` `pinned` from M0); the switcher shows pinned shares greyed;
   activating one mounts with stored credentials or opens the sheet (D8's flow with the SMB `attempt`).
2. The educational toast (D13). TDD: a pure `should-show-pin-hint.ts` (count ≥ 5, seen flag, favorites ≥ 3 adds the
   line).
3. D11: the Servers card and the Android (ADB) section, `settings-i18n-parity`, `settings-registry` tests.

Docs: `settings/CLAUDE.md`, `navigation/DETAILS.md`. Checks: `pnpm check`.

### M5. ADB in the pane

1. D12 frontend: non-ready rows, `waiting_for_device` auto-proceed, cancel, the refusal words (`adb-connect-errors.ts`
   already words the enum; `RemoteConnectView.refused` renders them), Disconnect wording, `/sdcard`.
2. The MTP-header hint.
3. Wipe `android-adb-ui.md` except decision 1 (moves to M8's entry in `later/` if M8 slips).

Tests: `connection-views.a11y.test.ts` blocks, an E2E driving `emitBackendEvent` for `volumes-changed` with a
`waiting_for_device` → `direct` transition (the real-device pass stays David's: `android-adb-backend-follow-ups.md`
§ 1). Docs: `adb/CLAUDE.md` (frontend), `pane/CLAUDE.md`.

### M6. Reconcile, docs pass, spec wipe

1. Rebase on `main`, resolve, full `pnpm check --include-slow`.
2. `docs/architecture.md`, every touched `C.md` at 300–400 words, `docs/specs/index.md` updated, the two superseded
   specs wiped per `docs/specs/DETAILS.md` § "Wiping a shipped spec", `later/sftp-follow-ups.md` § 1 and
   `webdav-backend-follow-ups.md` § 1 rewritten to point at `servers/DETAILS.md`.

### M7. Real-server pass (David, by hand; recorded here for the QA)

Hetzner storage box (SFTP and WebDAV both), a Nextcloud, the Synology, a VPS with key auth, Fastmail, a phone with USB
debugging off then on. Each records its evidence line in the crate `DETAILS.md`.

### M8. One row per phone (ADB decision 1)

Fold MTP and ADB entries by serial in `device_volumes.rs`, a per-row active protocol the pane remembers, "Show the full
filesystem" in the row menu and the pane header. Last, so nothing else waits on it. Delete `adb.volumeLabelWithSuffix`
in the same commit.

## Copy

Every string below is a draft for David's pass (principle 4). Rules: `docs/style-guide.md` and
`docs/guides/error-handling.md` § "Writing rules" (no "error", no "failed", active, contractions, sentence case, no
"just"). Keys go in `en/servers.json` (new area), `en/adb.json`, and the existing `fileExplorer` / `settings` files.

- Hub row: "Servers". Hub columns: "Name", "Type", "Address", "Status", "Last used". Add row: "Add server…".
- Statuses: "Connected", "Saved", "Found nearby", "Signed out", "Waiting for you to check the key".
- Sheet titles: "Add server", "Sign in to {name}", "Edit {name}". Buttons: "Connect", "Sign in", "Save", "Cancel".
- Remember: "Remember in Keychain". Advanced: "Name", "Remote folder", "Key file", "Use ssh-agent", "Reconnect
  automatically".
- Refusals: `authentication_rejected` "That password didn't work for {username}." `needs_credentials` "This server asks
  for a password." `auth_method_unsupported` "This server only accepts a sign-in method Cmdr doesn't speak yet (Digest).
  Basic auth over HTTPS works." `certificate_untrusted` "macOS doesn't trust this server's certificate. Add it in
  Keychain Access, then try again." `not_a_webdav_server` "Nothing at this address answers WebDAV." + "Try the
  Nextcloud address". `unreachable` "Cmdr couldn't reach {host}." `timed_out` "{host} didn't answer in time."
  `invalid_url` "That doesn't look like a server address."
- Host key, first contact: "First time connecting to {host}. Its key fingerprint is {fingerprint}. Trust it?" Button
  "Trust and connect". Changed: "{host}'s key changed. This can mean the server was reinstalled, or that something is
  sitting between you and it. Check the fingerprint with the server's owner before trusting it." Secondary, behind a
  disclosure: "I've checked, trust the new key". Revoked: "{host}'s key is marked as compromised in your known_hosts
  file. Cmdr won't connect to it."
- Pane states: "Connecting to {name}…", "Waiting for you to tap Allow on your phone", "Signed out of {name}",
  "Couldn't reach {name}", "Cmdr stopped trying to reconnect to {name}". Buttons: "Cancel", "Sign in…", "Try again",
  "Disconnect", "Look at the key".
- Context menu: "Open", "Disconnect", "Pin to switcher", "Unpin", "Edit…", "Forget saved password", "Forget server".
- Toast at five pins: "Your Network group is getting long. Right-click a server to unpin it; it stays in Servers."
  Extra line with ≥3 favorites: "Favorites work the same way." Button "Got it".
- ADB hint: "Want the whole filesystem? Turn on USB debugging." Link "How".
- Settings: "Servers (SFTP, WebDAV)", "Trusted host keys", "Forget"; "Android (ADB)", "Enable Android debugging
  (ADB)", "Status", "Found at {path}" / "Not found", "Re-check", "adb location", "Browse…".

❌ Never expose "adb server", "sync service", "transport", "rung", "PROPFIND", or a backend diagnostic.

## Not in this effort

- Trust-on-first-use certificates, Digest, Nextcloud chunked uploads, quota: `webdav-backend-follow-ups.md` § 2–5.
  TOFU is the day-one wall for the NAS audience and should follow this effort directly.
- `~/.ssh/config` host aliases as autocomplete in the address field. Backend follow-up; recorded in
  `later/sftp-follow-ups.md` at M6.
- Wireless ADB pairing. Decided out (`android-adb-backend-follow-ups.md` § 5).
- S3 and OAuth code. Their contracts are D4 (`ServerTarget` arm), D8 (one renderer), D10 (the reserved shapes).

## Parallelism

None worth the risk. M0's steps 3, 4, and 8 are independent of each other and could run as three subagents in one
worktree touching disjoint files, but the `bindings.ts` regeneration serializes them anyway. Run everything in order.
