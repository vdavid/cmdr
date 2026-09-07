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
`android-adb-ui.md` (wipe it when M5 lands; its eight decisions are restated below only where this plan changes them).
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
  first successful connect. "Unpin" lives in the row's context menu. ❌ No "Connect to server…" row in the switcher:
  adding lives in the hub, the command palette, and ⌘K. (Pins cover SFTP and WebDAV places in this effort; SMB shares
  are D4's "later".) ❗ The rule is the SWITCHER's, applied in `navigation/volume-grouping.ts` over the `pinned` field
  each row carries; the volume LIST holds every place, because an id with no row is one the app denies exists.
- **Every row in the switcher is a place. Opening a place that isn't live brings it to life in the pane, with a cancel
  button.** MTP already works this way (`MtpConnectionView`); SFTP, WebDAV, SMB, and ADB inherit it.
- **Dialogs are for entering data; panes are for waiting.** The sheet exists to type a new server, edit one, or answer a
  sign-in. Connecting, waiting for the phone's Allow tap, a refusal with retry, a changed host key, and "signed out" all
  render in the pane. This is what reconciles `android-adb-ui.md`'s "never a modal" with a connect dialog: both are
  right once split.
- **The sheet opens only on user intent.** Activating a row that needs a sign-in, pressing Add, pressing "Sign in…". ❌
  Never on its own when a session drops: a modal stealing focus during a lid-open wake is the wrong thing, and the SMB
  doc's "a reconnect is silent" promise stays. The pane's banner has the button.

## What exists (pointers, not restatement)

- Every SFTP and WebDAV command is typed and wired; the outcome enums are in `commands/sftp.rs` and
  `commands/webdav.rs`. Cancel ids are the caller's. The reconnect manager and `getVolumeSignInState` are
  backend-neutral despite SMB names (`network/DETAILS.md` § "SMB live-reconnect flow").
- The volume list is built by `volume_listing::complete` (device providers folded in, then
  `enrich_from_volume_registry`, which copies `capabilities` and the connection state FROM the registered volume; ❗
  anything appended after it ships `capabilities: None`, and anything whose `Volume` impl doesn't answer the
  connection-state accessor comes out `None`).
- Path resolution is `commands/volumes.rs::resolve_path_to_volume`, prefix-dispatched on `adb://`, `mtp://`, `smb://`,
  then the mount table, which on both platforms walks up to `/` and answers the root volume for ANY absolute path it
  doesn't know. ❗ SFTP and WebDAV paths carry no scheme today (`/srv/data/x`), so every un-hinted resolver call (tab
  restore, favorites, drag between panes, go-to-path, MCP, trash) would land them on the local root. D3 is the fix.
- The volume manager hands a volume the path as given (`manager/routing.rs::resolve` routes archives and the git portal,
  nothing else), and each crate maps app paths to backend paths itself: MTP is rooted at `mtp://<dev>/<storage>` and
  strips that; SFTP's `to_remote_path` treats an absolute path as a server path, and its `display_path_for` (the
  listing-cache patcher's spelling) is defined as the same string. ❗ Five app sites pass paths through
  `cmdr_fs::volume::root_anchored` first (`commands/file_system/listing.rs`, `write_operations/routing.rs` twice,
  `transfer/volume/copy.rs`, `transfer/volume/move.rs`), which JOINS a path that doesn't start with the root onto it.
- Three connection enums exist and are mapped in one place: the backend seam `host::events::VolumeConnection`, the wire
  `network::VolumeConnection` (both `connected | disconnected | needs_credentials | needs_host_key_approval`), and the
  listing's `SmbConnectionState`; `events/volume_mapping.rs::wire_state` joins the first two, and the frontend's
  `toSmbConnectionState` (`stores/volume-store.svelte.ts`) drops the last two wire variants to `null`. ❗ On the
  backend, `smb_connection_state().is_some()` is ALSO the app-wide "is this an SMB session" test, in about a dozen sites
  (`cmdr-index`'s SMB and local-external transports and cover bootstrap, `file_system/volume/eject.rs`,
  `file_viewer/media_session.rs`, `commands/file_system/listing.rs`, `mcp/resources/volumes.rs`). The Linux enrichment
  twin (`volumes_linux/smb.rs`) copies only `capabilities`, not the state.
- `KnownNetworkShare` is server-level in practice: its only writer stores `share_name: ''`, it carries no port, and a
  mounted share's id comes from statfs (which normalizes an mDNS name to an IP), so no id derivable from the store
  matches the mounted volume's.
- `VolumeUnmounted` is path-keyed (`volume_broadcast.rs`), emitted only by the two mount watchers, and its consumer in
  `DualPaneExplorer` finds the volume in the store by path before redirecting.
- `SmbVolume::reconnect_with_credentials` accepts a changed username and rewrites its params; SFTP's answers
  `NotSupported` for one. The two backends genuinely disagree, on purpose.
- The resolver's `adb://` arm DIALS the device as a side effect, and `initialization.ts::resolveVolumeId` calls the
  resolver for every restored tab at launch.
- Every native menu pops at the pointer (`menu.popup(window)`); nothing in the app opens a context menu from the
  keyboard.
- The `network.enabled` switch relabels the synthetic row "Network (disabled)" and sends its click to Settings; it
  exists for the macOS Local Network permission that mDNS needs, which SFTP and WebDAV never do.
  `test/e2e-playwright/network-toggle.spec.ts` asserts both behaviors.
- The frontend classifies a pane's kind in `pane/volume-capabilities.ts::volumeKindOf`, which delegates to the tint
  classifier `volume-tint.svelte.ts` (`category === 'network' || fsType === 'smbfs'` → `smb`). ❗ A `category: Network`
  row with `fs_type: sftp` is therefore an SMB pane today: SMB tint, SMB capability row, and `canOpenTerminalIn('smb')`
  is true, so Open terminal would fire with an `sftp://` path. That module's header rule stands: classify off `fsType`
  and category, ❌ never off the backend's published identity.
- `resolveValidPath` (`navigation/path-resolution.ts`, called from `listing-loader.ts`) walks a missing path up to its
  parents, then `~`, then `/`. On a scheme path every `pathExists` is false and the walk-up chops the scheme itself
  (`sftp:/`, then `sftp:`), so `~` wins and the pane lands on home. `path-navigation.ts::determineNavigationPath` never
  goes home; it falls back to the volume path.
- Go-to-path resolution is backend-owned and local-only (`commands/go_to_path.rs` → `go_to_path/mod.rs`, a
  `std::fs::metadata` resolver over a tilde-expanded, base-dir-joined path answering
  `directory | file | nearestAncestor | invalid`). A scheme input joins onto the pane's directory and answers `invalid`.
- There are THREE `LocationInfo` / `VolumeInfo` twins: `volumes/mod.rs`, `volumes_linux/mod.rs`, and `stubs/volumes.rs`
  (declared unconditionally, `smb_connection_state: Option<String>`).
- The browsers' command ids (`network.selectHost`, `share.back`, `share.selectShare`) are in `FIXED_KEY_COMMAND_IDS`, so
  nothing about them is persisted in anyone's shortcut settings; `rust-command-id-drift.test.ts` and the message keys
  are what pin them. Scopes are a typed record in `shortcuts/scope-hierarchy.ts` with `shortcut-vocabulary.test.ts` over
  it; `Main window/Network` is a sibling of `Main window/File list`, so a hub key never overlaps a file-list key. F8 and
  ⌘E are both free in that scope.
- Four saved-server stores with no shared type: `sftp_known_servers.rs`, `webdav_known_servers.rs`, `known_shares.rs`
  (SMB, keyed `(server, share)`), `manual_servers.rs` (SMB hosts typed by hand). ❗ SFTP's and WebDAV's `remember`
  replaces the whole entry on every connect, so any new field is clobbered unless the connect path reads the existing
  entry first.
- `SignInPrompt` (`crates/cmdr-fs/src/volume/types.rs`) is `Nothing | Password | KeyPassphrase`, a bare string union on
  the wire, read live through `Volume::sign_in_prompt`; the reconnect manager already stores the answer per volume and
  renders nothing. ❗ Its variant doc comments claim a password is persisted on sign-in and a passphrase never saved;
  both contradict the SFTP contract and get rewritten with the enum.
- The SMB sign-in form (`NetworkLoginForm.svelte`) is rendered from three sites: `ShareBrowser` (listing auth),
  `NetworkMountView` (mount-phase auth), `SmbReauthView` (reconnect re-auth); plus `FilePane`'s `smbUpgradeLogin`
  branch. `smb-login-hosts.ts` exists only to find a pane that can host it. The pane's `isSmbVolume` test is
  `smbConnectionState != null` (`smb-view-state.svelte.ts`), and the eject predicate is
  `isEjectable || smbConnectionState != null`.
- The MCP volumes resource derives `kind: Smb` from `smb_connection_state.is_some() || is_smb_fs_type` and hardcodes the
  synthetic row's name "Network" (`mcp/resources/volumes.rs`).
- Dialog registration is a three-step contract (`SOFT_DIALOG_REGISTRY`, `ModalDialog` `dialogId`, a gallery row +
  fixture). App-level dialogs mount from `routes/(main)/+page.svelte` or `+layout.svelte`; `ConnectToServerDialog` is
  the outlier, mounted inside `NetworkMountView`. The main window's capability file grants `dialog:allow-ask` only;
  `dialog:allow-open` is the settings window's.
- ⌘K is unbound. The palette is ⌘⇧P. Commands are data in `commands/sources/*.ts` plus a handler; four places per
  command. `network.selectHost` and `share.back` (Backspace, Escape, ⌘↑) are the browsers' commands, with user-visible
  scope labels in shortcut settings.
- Row context menus are native (`commands/menu.rs::show_volume_row_context_menu`) and answer on `volume-context-action`
  with `action: String`; eject disables itself while `busy_volume_ids()` names the volume; panes leave an unmounted
  volume through the `VolumeUnmounted` broadcast. There is no keyboard opener for a row menu.
- ADB: `AdbDeviceProvider::entries()` lists `Ready` devices only, at path `adb://<serial>`; `list_adb_devices` returns
  every state typed; `get_adb_install_status` / `recheck_adb_install` exist; the connect is not cancelable from the
  pane; no settings.
- The Playwright lane's `smb-e2e` feature is an in-process fake; the SFTP and WebDAV Docker fixtures are leased by the
  Rust integration lane only. No `fast-check` on the frontend; `proptest` is Rust-only.
- Toasts: `addToast` with `dismissal: 'persistent'`, dedup `id`, component content; the once-ever precedent is
  `behavior.doubleClickOnPaneNotificationSeen`.

## Decisions

### D1. Session health is one field; device presence is another

`LocationInfo.smb_connection_state: Option<SmbConnectionState>` becomes `connection_state: Option<ConnectionState>`, on
all three twins (`volumes/mod.rs`, `volumes_linux/mod.rs`, and `stubs/volumes.rs`; the latter two are `Option<String>`
today and become the enum) and the TS `VolumeInfo`:

- `direct`: a live session Cmdr owns (SMB smb2, SFTP, WebDAV, ADB dialed).
- `os_mount`: SMB's kernel-mount fallback. Unchanged meaning.
- `disconnected`: the session dropped; the backoff loop is running or gave up.
- `needs_sign_in`: the backend stopped retrying because a credential is what's missing.
- `needs_host_key_approval`: SFTP only; the row shows it, the pane sends the user to look at the key.
- `saved`: pinned, not connected, nothing in flight. The greyed row with the hollow dot.

The trait accessor `Volume::smb_connection_state()` becomes `Volume::connection_state()`, and SFTP, WebDAV, and ADB
implement it (SMB already does). ❗ This is what keeps `enrich_from_volume_registry` from wiping the servers arm's
`direct` back to `None`; the Linux enrichment twin starts copying the state too, so the Docker E2E lane sees what macOS
sees.

❗ **Every read of the old accessor was also a backend "is this SMB" test** (`is_some()` in most places, and
`matches!(…, Some(Direct))` in `network/smb_upgrade.rs`, which an `is_some()` grep never finds), and once three more
backends answer it, every one of those sites reclassifies an SFTP volume as an SMB session (the SMB indexer would
receive an `sftp://` root). So the same commit adds `Volume::backend_kind() -> BackendKind`
(`local | smb | sftp | webdav | mtp | adb | archive | git_portal`, with a `local` default so the test doubles keep
compiling), converts every read site to `backend_kind() == Smb` where it meant "SMB" and to the state where it meant the
state, and M0 step 1 lists them by path so nobody discovers them at `cargo build` time. The MCP volumes resource derives
`kind` from the same accessor; its token set grows from `local | smb | mtp | virtual` to
`local | smb | sftp | webdav | mtp | adb | virtual`, which is an agent-facing wire change pinned in
`mcp/resources/DETAILS.md` beside the `connectionState` rename.

**The frontend classifier is separate and stays keyed on `fsType` and category**, per its own header rule. `VolumeKind`
gains `sftp` and `webdav` members with their own capability rows (a real backend listing, write and export from the
backend's published answer, no system clipboard, no terminal, no index badge), and `volumeKindFor` learns the two
`fsType` values AHEAD of the `category === 'network'` arm. ❗ Nothing switches exhaustively over `VolumeKind`; every
consumer is a positive-list comparison, so the compiler reports nothing and three sites misclassify silently:
`pane/clipboard-operations.ts` (`kind === 'mtp' || kind === 'adb'` gates the system-clipboard refusal, so ⌘C on an SFTP
pane would put an `sftp://` path on the clipboard), `volume-tint.svelte.ts` (falls through to `'none'`), and
`search/search-target-volume.ts` (`=== 'smb'` picks the coverage voice). All three gain the two members. The tint reuses
the existing `appearance.tintSmb` setting, relabelled "Servers (SMB, SFTP, WebDAV)" (a copy change on its label key); ❌
no fourth tint setting, which would be three definition sites, a section row, and two parity tests for a color nobody
asked to set separately. Without this an SFTP pane is an SMB pane: SMB capability row and Open terminal firing with an
`sftp://` path.

**One name for the hub row.** "Network" is a literal in `mcp/executor/nav.rs` (the `is_virtual` test and the
available-volumes message), `commands/volumes.rs`'s `smb://` arm, `mcp/resources/volumes.rs`, eleven
`mcpSelectVolume('left', 'Network')` calls in `smb.spec.ts`, `i18n-capture-surfaces.ts`, and the parity test that pins
`fileExplorer.navigation.networkVolume` to "Network". A Rust `SERVERS_VOLUME_NAME` const feeds the three Rust sites, and
the four test sites retarget in the same commit as the label.

**The three connection enums stay three**, because they answer different questions: the backend seam and wire enums are
transitions an event carries, the listing enum is a row's standing state. What changes is the frontend mapper:
`toSmbConnectionState` becomes `toConnectionState` and maps all four wire variants (`needs_credentials` →
`needs_sign_in`, `needs_host_key_approval` → the same), so a live SFTP session dropping to a sign-in reaches the dot and
the pane between `volumes-changed` broadcasts, which is exactly what D8's `signed_out` state rides on.

Device presence is NOT a session and stays off this field: `LocationInfo.device_readiness: Option<DeviceReadiness>` with
`ready | waiting_for_authorization | unavailable { reason: offline | no_permissions }`, set by the device providers only
(D12).

**Why the split.** Every consumer of the old field asks "how live is this session" (the dot, the tooltip, the reconnect
subscription, the volume-store patch on `volume-connection-changed`), and the SMB view-state's `!= null` test enrolls
whatever is non-null into the reconnect manager. A phone waiting for its Allow tap must never start a backoff loop.
Consumers get explicit predicates in `navigation/connection-state.ts`: `hasReconnectLoop(state)` (`direct`, `os_mount`,
`disconnected`, `needs_sign_in`, `needs_host_key_approval`: the last so the pane keeps its subscription and can render
the key banner), `isLiveSession(state)`, `showsDisconnect(state)` (`direct` and `disconnected`; ❌ not `saved`, there is
nothing to disconnect). The eject predicate becomes `isEjectable || showsDisconnect(connectionState)` in the same
commit. The switcher's dot builds its CSS class from the state name, so the three added states get three style rules.

The MCP volumes resource exposes the field as `connectionState` and takes the synthetic row's name from the same source
the hub does. That field is an agent-facing contract; the rename is stated in `mcp/resources/DETAILS.md`.

### D2. Remote volumes are listed by a servers arm; the pin filters the switcher, not the list

`volume_listing::complete` grows a fold BEFORE `enrich_from_volume_registry`: one `LocationInfo` per SFTP or WebDAV
place the app knows, registered or merely saved (state `saved` when nothing is registered under its id).
`category: Network`, `fs_type: "sftp" | "webdav"`, `is_ejectable: false`, `supports_trash: false`, `name` the saved
`displayName`, `path` the volume's app root (D3), `id` from `cmdr_fs::volume::ids::sftp_volume_id` / `webdav_volume_id`,
which is also the id the registry uses, so a `saved` row and the volume it becomes share one id across the dial. ❗ That
identity is what makes a tab restorable: a tab stores `(volumeId, path)`, the switcher shows the same id greyed, and
activating either dials the same saved entry.

❗ **Every saved place gets a row, pinned or not**, and the row carries `pinned: Option<bool>` (`None` for anything the
cap was never about: a local disk, a favorite, a mounted SMB share). The volume list is the app's registry of what an id
MEANS, and four things resolve one: Enter on a hub row, a restored tab, a favorite inside a server, and the pane's own
lookup. An unpinned server with no row is an id that resolves to nothing, and the pane lands on the boot disk. So the
listing hides nothing and `navigation/volume-grouping.ts` applies the three-things rule itself, over `pinned` plus the
`isLiveSession` / `hasReconnectLoop` predicates. ❗ A row with no `pinned` shows unconditionally: a Linux CIFS mount
carries no connection state either, and a rule spelled "live or pinned" would drop it off the switcher.

The device-provider seam is deliberately NOT reused (`crates/cmdr-sftp/DETAILS.md` says why: it lists things that appear
and leave on their own). The servers arm reads the two stores plus the registry, all cached state; ❌ never the wire,
because the listing runs on every `volumes-changed`.

### D3. Remote paths carry a scheme, the way MTP's and ADB's do

An SFTP place's app-facing paths are `sftp://<user>@<host>:<port>/<server path>`; a WebDAV place's are
`webdav://<user>@<host>:<port>/<remote path>`. `cmdr_fs::volume::ids` mints the prefix beside the id it already mints
(`sftp_app_root(host, port, username)`), so the crate and the app spell it from one function. The crate's volume is
rooted at `<prefix><remote_root>`; `to_remote_path` strips the prefix before today's containment logic, and a new
`to_app_path` is its exact inverse, which `display_path_for` now uses so the listing-cache patcher spells paths the way
panes hold them (the crate's DETAILS paragraph claiming "no second spelling of the tree" is rewritten in the same step).
WebDAV mirrors it.

❗ **A bare server-absolute path is REFUSED on a prefixed volume**, not accepted as a courtesy. Five app sites run paths
through `root_anchored` before the volume sees them, and that helper joins anything not under the root ONTO the root:
`/srv/data/x` would become `sftp://…/srv/data/srv/data/x`, strip to a real, wrong server path, and never refuse. With
the scheme in place the app never spells a remote path bare, so the only thing leniency buys is that hole. The three
root aliases stay: `/`, `.`, and the empty path still mean the volume root (WebDAV's own `query.rs` passes a bare `/`
from `is_root` and `get_space_info_impl`, and those two callers are named in the `paths_test.rs` list). The refusal cell
lives at the routing level too (a `root_anchored`-fed copy against the fixture), not only in `paths_test.rs`.

**Why a scheme and not a hint.** The mount table answers the local root for any absolute path it doesn't know, on both
platforms, so a scheme-free remote path is wrong at every resolver site the plan doesn't reach (eleven today, and the
next one would forget). A scheme makes the path self-describing: `resolve_path_to_volume` gains an `sftp://` /
`webdav://` arm that answers the registered volume, or ANY saved server (pinned or not: pins govern the switcher, not
identity, and an unpinned server's restored tab must still find its way home), whose prefix matches, as a `saved` row
when nothing is registered. ❗ **The arm never dials.** It sits one line under the `adb://` arm, which connects the
device as a side effect of resolving, and copying that shape would make every restored server tab dial at launch, which
D14 forbids; a cell asserts the resolver performs no connect. Tab restore's `initialization.ts::resolveVolumeId` (which
distrusts the stored id and re-resolves by path) lands on the right volume without an exemption; a favorite inside a
server is unambiguous by path alone; copy-path yields something a person can paste back; MCP rows are unambiguous. It is
also the convention the app already has for every volume without a local mount. **Why not resolve by the registry
alone**: two servers can both have `/home/ada`, and a local disk certainly does.

The frontend gets `servers/server-path-utils.ts` (parse, construct, `isServerPath`) beside `adb-path-utils.ts`, and
`navigate.ts` gets a server twin of `validateAdbNavigation`. The `network` volume's sentinel stays `smb://`; the label
changes, the path does not (nine sites and every user's persisted tab state depend on it, and a rename buys nothing the
label doesn't).

### D4. One command family for "a server", protocol-agnostic where the UI is

A new `commands/servers.rs` (app side, over the existing per-protocol wiring):

- `list_saved_servers() -> Vec<SavedServer>`: the union of the SFTP store, the WebDAV store, and SMB hosts derived from
  `known_shares.rs` (server-level rows: it stores no share rows today) and `manual_servers.rs`, each as
  `{ id, protocol, displayName, address, username?, pinned, lastConnectedAt?, places: Vec<SavedPlace> }` where a place
  is `{ volumeId, name, pinned, connected }`. SFTP and WebDAV have one place. ❗ SMB hosts list NO places here and
  cannot be pinned in this effort: the store has no per-share rows and no port, and a mounted share's id comes from
  statfs (an IP where the store holds an mDNS name), so no id derivable from the store would match the mounted volume.
  SMB places keep reaching the switcher the way they do today (mounted shares as real volumes) and the hub opens an SMB
  host into its live places list. Pinnable SMB shares need a share-level writer at mount time storing the server as
  statfs spells it plus the port; that is recorded in `later/` at M6 rather than half-built here.
- `connect_saved_place(volume_id, attempt_id, secret: Option<SecretOffer>) -> ServerConnectOutcome`: dials by saved
  entry, so the frontend never re-plumbs eight target fields to reopen a server. ❗ Only for a place with NO registered
  volume; a registered volume that dropped is mended by `reconnect_volume_with_credentials` (D8), which is what enforces
  the read-only username rule and the never-seeds rule, and re-dialing it would register a second volume.
- `connect_server(target: ServerTarget, attempt_id, secret: Option<SecretOffer>) -> ServerConnectOutcome`: the add-mode
  dial. `ServerTarget` is a tagged union of the two existing param shapes (`sftp { … }`, `webdav { … }`); SMB stays on
  its own commands (its connect is a share mount, not a session).
- `cancel_server_connect(attempt_id)`, `disconnect_place(volume_id)`, `forget_server(id)`, `forget_server_secret(id)`,
  `set_place_pinned(volume_id, pinned)`, `update_saved_server(SavedServerPatch)`. Pin and forget emit `volumes-changed`.
- `reconnect_smb_volume` / `reconnect_smb_volume_with_credentials` are renamed `reconnect_volume` /
  `reconnect_volume_with_credentials` on both sides (they were backend-neutral with the wrong name; an adjacent win).
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
user's choice. So the wiring gets a `SecretOffer { secret, remember }`:

- `remember: true` → `save_*_credentials` first, then dial. Identical to today's flow, one round-trip shorter.
- `remember: false` → the dial runs with a `CredentialStore` wrapper that answers this one `(service, scope)` from
  memory and forwards everything else. The wrapper is built per attempt and dropped with it. ❗ The volume never holds
  the secret: a later drop asks again, which is what "not remembered" means (`crates/cmdr-sftp/DETAILS.md` § "The two
  switches" is the contract, unchanged).

Where the wrapper lives: `network/one_shot_credentials.rs`, generic over the seam in
`cmdr_fs::volume::host::credentials`. ❗ The wrapper is the ONLY way a secret reaches a dial without the store; ❌ no
`password` argument on the crate-level params.

### D6. Servers say "Disconnect"; disconnecting keeps the row

The eject slot on a remote row is a Disconnect control (an unplug icon, tooltip "Disconnect"), shown by
`showsDisconnect` (D1), mirroring `android-adb-ui.md` decision 8: "Eject" promises safe-to-unplug and a server has
nothing to unplug. It is disabled while `busy_volume_ids()` names the volume, exactly like Eject, and so are the
Disconnect and Forget items in the menu. Disconnecting a pinned place leaves it as a `saved` row; "Forget server"
(removes the entry, its places, and their pins) and "Forget saved password" are separate context-menu items, both
confirmed. MTP keeps "Eject": it earns it by closing the device session.

**What the panes do.** Both Disconnect and Forget emit the `VolumeUnmounted` broadcast (a new emit site: today only the
two mount watchers raise it), so a pane sitting on the volume goes home the way it does after an eject, in both panes,
and a tab on a forgotten server becomes a home tab. ❗ The payload gains `volumeId` and the consumer matches on it: it
matches on path today, by finding the volume in the store first, and a Forget removes the row, so if the
`volumes-changed` refresh landed first the pane would never be redirected. Forget emits `VolumeUnmounted` BEFORE
`volumes-changed`, and the consumer no longer needs the row to exist. ❌ No new "you disconnected" pane state: a `saved`
row dials on activation, always, and a pane left standing on one would either dial again (wrong right after a
Disconnect) or need a third state nobody asked for.

The row's context menu, in order: Open, Disconnect (when live), Pin to switcher / Unpin, Edit…, Forget saved password
(when one exists), Forget server. It opens from right-click in the switcher and in the hub. **Keyboard reach is through
commands, not a menu opener**: no native menu in the app pops anywhere but the pointer and macOS has no keyboard
context-menu gesture to borrow, so each item is also a command acting on the hub's cursor row or the focused pane's
volume (F8 "Forget server" as today, ⌘E "Edit server…", and palette-only "Pin / unpin server", "Disconnect server",
"Forget saved password"). `VolumeContextAction.action` becomes a typed enum on both sides (a wire change with existing
consumers: `eject`, `rename-favorite`, `remove-favorite`, plus the six above).

### D7. The hub is a pane state, and it is the "Network" row grown up

The synthetic `network` volume keeps its id and its `smb://` sentinel; its label becomes "Servers". `NetworkBrowser`
becomes `ServersHub.svelte`: a table with columns Name, Type (SFTP / WebDAV / SMB), Address, Status, Last used, sourced
from `list_saved_servers()` merged with the discovered SMB hosts in `network-store`, plus an "Add server…" row at the
bottom (the `+` pseudo-row today). Status words: Connected, Saved, Found nearby, Signed out, Waiting for the key.

- Enter on an SFTP or WebDAV account navigates the pane to its one place (dialing if needed, D8). Enter on an SMB host
  opens its places list (`ShareBrowser`, renamed `PlacesBrowser` and given an `account` prop rather than a `host`, so an
  S3 account's buckets render through the same component later).
- Keyboard: the command ids `network.selectHost`, `share.back`, and `share.selectShare` stay (they are fixed-key ids
  pinned by `rust-command-id-drift.test.ts` and their message keys; renaming buys nothing); their scopes become "Main
  window/Servers" and "Main window/Places" in `shortcuts/scope-hierarchy.ts` (the typed record and the `CommandScope`
  union) with `shortcut-vocabulary.test.ts` updated. In the hub, Backspace and Escape do nothing (it is a volume root;
  ⌘↑ likewise). In the places list they go back to the hub, as today. F8 forgets a saved server (confirmed) or toasts
  "Can't remove discovered hosts" as today. ⌘E and the D6 menu edit. Both keys are free in that scope, and the scope is
  a sibling of the file list's, so a hub F8 never meets `file.delete`'s.
- The hub row pushes MCP rows the way `NetworkBrowser` does today, one encoded `name` per row with `protocol=`,
  `status=`, `address=` tokens, and the add row as `+ Add server…` at `smb://add`.
- A pane on the hub keeps `syncsToMcp: false`, `canWrite: false`, and the rest of the `network` capability row.
- `network.enabled` off no longer relabels or redirects the row: it gates mDNS discovery and SMB, which is what the
  macOS Local Network permission is about, and SFTP and WebDAV need none of it. The hub shows its saved servers as usual
  and, in place of the discovered hosts, one line: "Local network discovery is off" with a link to the setting. That
  deletes the "Network (disabled)" label (`fileExplorer.navigation.networkVolumeDisabled`, removed from every locale
  catalog) and `handleVolumeSelect`'s Settings early return, and `network-toggle.spec.ts` is rewritten to assert the
  hub's line and link instead.

**Why a pane state and not a settings list.** David is considering settings-as-pane-state generally; this is the pilot,
with one rule carried over: a pane state may be a list, never a form. The saved-server list in Settings is dropped; a
Settings card keeps only what the hub can't show (D11).

### D8. One connect flow, one pane view, one sheet

Three new modules under `apps/desktop/src/lib/servers/`:

- **`connect-flow.ts`**: the orchestrator. Given a saved place or an add-mode target, it loops: dial → on
  `needs_host_key_approval` open the sheet's key step → approve (re-check semantics per the SFTP doc) → dial again; on
  `needs_credentials` or `authentication_rejected` open the sheet's sign-in step → dial with the offer → loop; terminal
  outcomes return. The attempt id is minted here before the first dial, so cancel is armed from the first millisecond.
  ❗ It is the only caller of `connect_server` / `connect_saved_place` / `reconnect_volume_with_credentials`; the hub,
  the switcher, the sheet's Connect button, and the pane banner all go through it. ❗ It picks its move by the volume's
  standing, three arms: registered and `disconnected` → subscribe to the reconnect manager (which starts the backoff
  cycle if none is running, the lazy-nav path `smb-view-state` already documents for SMB) and render `connecting`, ❌
  never a dial; registered and `needs_sign_in` → `reconnect_volume_with_credentials` (refreshes but never seeds a
  secret; whether the username is editable is the shape's call, D10); absent (`saved`) → a dial. A `cancelled` outcome
  returns silently: the user pressed the button.
- **`RemoteConnectView.svelte`** (in `file-explorer/pane/`): the non-interactive pane states, replacing
  `SmbReconnectingView`, `SmbReauthView`, `MtpConnectionView`'s connecting branch, and the ADB view the spec never
  built. Typed `state` prop: `connecting { cancel }`, `waiting_for_device { reason }`, `signed_out { shape, signIn }`,
  `host_key_changed { lookAtKey }`, `refused { refusal, retry, disconnect }`, `gave_up { retry, disconnect }`. Every
  state names what is happening in one sentence (design principle: radical transparency), and every one with a process
  behind it has a cancel.
- **`SignInSheet.svelte`** (`servers/`): one `ModalDialog`, dialog id `server-sign-in`, three modes. `add` (address
  first, D9), `sign-in` (endpoint as a read-only header, credential fields only), `edit` (the add form prefilled, the
  two switches, the `needs_stored_secret` warning when the backend reports it). The host-key approval is a step inside
  the sheet that replaces the body, ❌ not a second dialog. It mounts from `+layout.svelte` behind a
  `sign-in-sheet-state.svelte.ts` singleton whose `openSignInSheet(request): Promise<SignInSheetResult>` any caller can
  await. This retires `smb-login-hosts.ts` entirely.

**The sheet takes an `attempt` callback rather than dialing itself.** `attempt(credentials) → Promise<AttemptResult>`
where `AttemptResult = { ok: true } | { ok: false; refusal: SignInRefusal }` and `SignInRefusal` is typed
(`authentication_rejected | needs_credentials | auth_method_unsupported | unreachable | timed_out | other`). The sheet
owns the form, the inline refusal under the field it is about, and the retry; the caller owns the protocol. That is what
lets SMB's three sites (a share listing, a share mount, a reconnect) reuse it with their own commands, and what lets S3
plug in with one more renderer.

**"Remember in Keychain" per mode.** `add`: default on. `sign-in` and `edit`: seeded from `has*Credentials`, and a
change is written explicitly through `save_*` / `delete_*` when the user flips it, never as a side effect of the dial.
❗ An attended sign-in refreshes a remembered secret and never seeds one (the SFTP contract); a default-on box in
sign-in mode would seed a secret the user already declined.

**The backend tells the sheet what to ask.** `SignInPrompt` widens into `SignInShape` (D10), and the sheet has one
renderer per variant. ❌ The frontend never derives the shape from a rung, a protocol, or a connect result: the SFTP
doc's "asked when the banner renders" rule stands.

### D9. Add mode is address-first

People paste what they have: an `ssh` line, `user@host`, `sftp://host:2222/srv`, a Nextcloud URL, `https://nas:5006/`,
`smb://naspolya`. `servers/address-parser.ts` (pure, tested against an example table; there is no property-testing
library on the frontend and adding one is not this effort's call) turns one string into
`{ protocol, host, port, username?, path? }` and the sheet flips a `ToggleGroup` (SMB / SFTP / WebDAV) to match, leaving
it editable. Fields per protocol, in order: Address, Username, Password (with "Remember in Keychain"), then an
"Advanced" disclosure: Name, Remote folder, and for SFTP a Key file (a path field with a Browse button through
`@tauri-apps/plugin-dialog`'s `open()`, which needs `dialog:allow-open` added to the main window's capability file) plus
"Use ssh-agent" (default on), and "Reconnect automatically" (default on). The Connect button arms cancel before the call
and the sheet stays open while dialing so a refusal lands beside the field it is about.

Two refusals get a remedy button because they are where real users stall:

- `not_a_webdav_server` on a bare origin: "Try the Nextcloud address" retries with `/remote.php/dav/files/<username>/`
  appended (ownCloud shares the path). Nobody knows that path.
- `certificate_untrusted`: worded honestly (the certificate isn't trusted by macOS; add it in Keychain Access) with no
  button that can't work. Trust-on-first-use is `webdav-backend-follow-ups.md` § 2 and stays backend work.

SMB in add mode keeps today's behavior behind the same field: `connectToServer(address)` injects a manual host and opens
its places; no credentials are asked until a listing or mount refuses.

### D10. `SignInShape` replaces `SignInPrompt`

`crates/cmdr-fs/src/volume/connection.rs` (M0 step 1 grew `types.rs` past its size bar, so the five remote-vocabulary
types moved to a module of their own; `mod.rs` re-exports them, so every import path is unchanged), internally tagged
the way the outcome enums are (`#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]`), so
the TS side is a discriminated union on `kind`:

```
SignInShape =
  | { kind: 'nothing' }
  | { kind: 'password' }                                   // SFTP password / keyboard-interactive, WebDAV
  | { kind: 'key_passphrase' }                             // SFTP encrypted key file
  | { kind: 'username_password', guestAllowed: boolean }   // SMB (username editable; the share is the identity)
```

Reserved, documented here and in the type's doc comment, ❌ not added until a producer exists:

- `access_keys { sessionToken: bool }`: S3. Access key id, secret access key, optional session token.
- `oauth { provider }`: a "Continue in your browser" button and a waiting state; the callback comes home on a loopback
  listener on a random high port or a `cmdr://` deep link, backend-owned; "remember" is implicit (the refresh token is
  the only sane state), and a revoked token surfaces as `needs_sign_in` with the same banner.

**Username editability is a property of the variant, not of the sheet's mode.** `password` and `key_passphrase` render
the username as a read-only header: SFTP's `reconnect_with_credentials` refuses a changed username because the volume id
IS the account, and WebDAV's does the same. `username_password` renders it editable: SMB's accepts a new username and
rewrites its params, which is how re-auth-as-someone-else works today and must keep working. One implementer reading
"read-only" as a mode rule would break SMB; one reading "editable" as a mode rule would break SFTP.

SMB's `SmbVolume::sign_in_prompt` returns `username_password` with `guestAllowed` from its known auth options. The
default stays `password` (the safe way to be wrong, per the trait doc). The variant doc comments are rewritten to match
the SFTP contract. The reconnect manager's stored answer becomes the `shape` the `signed_out` pane state hands the
sheet; its test compares on `kind`.

### D11. Settings

`Settings > File systems` gets, beside "SMB/Network shares" and "MTP":

- **Servers (SFTP, WebDAV)**: one card, "Trusted host keys" listing `listTrustedSftpHostKeys()` rows with a Forget
  button each. Nothing else: saved servers live in the hub.
- **Android (ADB)**, exactly `android-adb-ui.md` decision 5's four controls (enable switch default on, status, Re-check,
  `adb` location with Browse), plus one addition: when the status is "Not found", a copyable
  `brew install android-platform-tools` line the way `PtpcameradDialog` shows its command. Settings ids:
  `fileOperations.adbEnabled`, `fileOperations.adbBinaryPath`; both follow the five-place `mtpEnabled` plumbing
  (`settings/loader.rs` hand-parsed dot keys, startup seed, live apply through a `set_adb_settings` command that
  restarts the tracker).

### D12. ADB rows and the phone's Allow tap

`DeviceVolumeEntry` grows `device_readiness: Option<DeviceReadiness>` (D1) and `AdbDeviceProvider::entries()` lists
every device except `recovery`, `bootloader`, and `sideload`: `Ready` as today, `unauthorized` / `authorizing` /
`connecting` as `waiting_for_authorization`, `offline` / `no permissions` as `unavailable`. This resolves the
contradiction between `android-adb-ui.md` decisions 2 and 3: `waiting_for_authorization` rows ARE openable and lead to
`RemoteConnectView`'s `waiting_for_device` state, which subscribes to `volumes-changed` and navigates on its own the
moment the row turns `ready`; `unavailable` rows are disabled with the reason as tooltip.

Also from that spec: the provider's root stays `adb://<serial>`, and the FIRST navigation onto a device (no last-used
path) lands on `adb://<serial>/sdcard`, a `path-navigation.ts` rule rather than a root change; ADB is not indexed; the
connect becomes cancelable (`connect_adb_device` takes the caller's attempt id like SFTP does). The merged
one-row-per-phone item (decision 1) is M8, last, and `adb.volumeLabelWithSuffix` dies with it.

**Discoverability**, the one thing that spec left out: a phone with USB debugging off is a plain MTP row, and nobody
will find ADB. The MTP pane header gets one quiet dismissible line, "Want the whole filesystem? Turn on USB debugging",
linking to a short help page, gated by `behavior.adbHintDismissed`.

### D13. Reach

- **⌘K** opens the sheet in add mode (Finder's binding for the same thing). Palette: "Connect to server…" and "Show
  servers" (navigates the focused pane to the hub). Both via the four-place command contract.
- **Go to path** (⌘G) accepts a pasted `sftp://`, `ssh://`, `webdav://`, `davs://`, `https://`, or `smb://` address and
  opens the sheet prefilled; an `sftp://` or `webdav://` path that matches a saved place navigates to it instead.
  `adb://` and `mtp://` navigate straight to the device (`android-adb-backend-follow-ups.md` § 3, an adjacent clear win,
  taken here). ❗ All of it is a frontend intercept in `go-to-path.ts` BEFORE `resolveGoToPath` is called: the Rust
  resolver is local-only by design (a `std::fs::metadata` walk), a scheme input joins onto the pane's directory there
  and answers `invalid`, and teaching it schemes would mean a resolver that consults stores it has no business reading.
  The intercept parses the scheme, asks `list_saved_servers()` for a match, and hands `navigate()` a `Location`; the
  pane's own scheme guards decide reachability. ❗ The dialog calls the Rust resolver from THREE sites (`goToPath` in
  `go-to-path.ts`, and `GoToPathDialog.svelte`'s own `resolveOrNull` for both the debounced preview and the clipboard
  prefill), so the intercept is one shared function all three call, ❌ not a branch inside `goToPath`. The dialog closes
  only on a resolution whose kind isn't `invalid`, so the intercept returns a resolution-shaped `{ kind: 'directory' }`
  for a navigation and `{ kind: 'handed_off' }` when it opened the sheet; `GoToPathResolution` is Rust-generated, so the
  frontend declares `GoToPathOutcome = GoToPathResolution | { kind: 'handed_off' }`, widens `onGo` and `goToPath`'s
  return to it, and keeps `shouldPrefillClipboard` on the narrow type. A scheme input previews "Opens {name}" or "Adds a
  server" instead of "not found". One cell per scheme, one per return shape, one per call site.
- **Educational toast**: when the pinned count first reaches five, one persistent toast (`behavior.serversPinHintSeen`)
  teaches right-click → Unpin and the hub, and adds a line about favorites when the user has three or more. Dismiss with
  "Got it".
- **A favorite inside a server** is a path favorite whose path carries the scheme (D3), so it resolves to the saved
  place by prefix and opening it dials. One E2E.

### D14. Restore behavior

A restored tab on a remote place comes back as `saved` and dials when ACTIVATED (the pane shows `connecting` with
cancel). ❌ Launch never dials a server in the background: four servers dialing at startup is four Keychain reads and
four network waits nobody asked for. `initialization.ts::resolveVolumeId` re-resolves the stored path and lands on the
`saved` id through the D3 arm. ❗ The walk-to-home is `navigation/path-resolution.ts::resolveValidPath`, which has six
callers, and restore is not the `listing-loader.ts` one: it is `app-status-store.ts::resolvePersistedPath`, applied to
every persisted tab and both pane paths before the volume list exists, with a single `volumeId === 'network'` exemption
at four sites. So the rule is PATH-shaped, not state-shaped: `resolvePersistedPath` skips probing any `<scheme>://` path
(beside each of the four `network` exemptions), and `resolveValidPath` itself gains a guard for the other five callers:
on a scheme path the parent walk stops at the scheme root and RETURNS THAT ROOT, never `null` and never `~` or `/` (four
callers hand a `null` to `navigateToFallback`, which turns it into `~` on the root volume, the exact thing this decision
exists to prevent). The cell is a `loadPaneTabs` cell restoring `sftp://…/srv/data/photos` and asserting the tab keeps
the subpath, plus a `resolveValidPath` cell asserting the scheme root comes back.

## Milestones

Sequential, one worktree. Each ends green on the named checks, committed, with docs updated. "TDD" marks a real red →
green sequence; "after" marks tests written once the shape settles.

❗ **Every milestone that adds English copy ends with a translation pass** into the ten full locales (`de`, `es`, `fr`,
`hu`, `nl`, `pt`, `sv`, `vi`, `zh`, `zh-Hant`) per `docs/guides/i18n-translation.md`: `desktop-i18n-coverage` is an
error-level FAST check with no exemption for a missing key, so a milestone without the pass cannot end green. A new
catalog file (`servers.json`) needs its sibling in every locale dir. The pass is the milestone's last step, by a
translator agent following that guide, ❌ never a hand-typed placeholder.

### M0. Backend groundwork (no UI change)

1. D1: `ConnectionState` and `DeviceReadiness` on all three twins and `VolumeInfo`; `Volume::connection_state()`
   implemented by SFTP, WebDAV, ADB; `Volume::backend_kind()` and the conversion of EVERY read of the old accessor AND
   field (`rg "smb_connection_state" --type rust` is the sweep, no paren, because two agent surfaces read the FIELD; the
   doc comment in `file_system/volume/manager.rs` goes too): `crates/cmdr-index/src/indexing/transports/smb/index.rs`,
   `transports/local_external/index.rs`, `lifecycle/cover/bootstrap.rs`, `file_system/volume/eject.rs`,
   `file_system/mod.rs`, `file_viewer/media_session.rs`, `commands/file_system/listing.rs`, `commands/network.rs` (the
   three direct-upgrade short-circuits), `network/smb_upgrade.rs` (the `matches!(Direct)`), `mcp/resources/volumes.rs`
   (with its widened token set), `agent/tools/read/volumes.rs` (the Ask Cmdr `list_volumes` field) and
   `agent/chat/session.rs` (the `EnvelopeConnectivity` mapping, widened to the new states so an SFTP session stops
   reading as "not a network volume" to the in-app agent); the Linux enrichment twin copying the state; the frontend
   predicates in `navigation/connection-state.ts`; `toConnectionState` mapping all four wire variants; the `sftp` /
   `webdav` members of the frontend `VolumeKind` with their capability rows, `volumeKindFor`, and the tint; every
   consumer renamed (store, breadcrumb, `FilePane`, `smb-view-state`, eject predicate, `direct-connect`,
   `os-mount-notice-bridge.ts`, `TransferDialog.svelte`, `TransferProgressDialog.svelte`, `clipboard-operations.ts`,
   `search-target-volume.ts`, tests, the two E2E specs keyed on the dot); the dot's three new style rules; `bindings.ts`
   regenerated. TDD: a listing cell proving a registered SFTP volume survives enrichment as `direct` on both platform
   twins; a cell proving the SMB indexer transport refuses an SFTP volume; a `volumeKindOf` cell for an `sftp` row, a
   clipboard-refusal cell, and a terminal-refusal cell; the predicate table; the eject predicate on `saved`; the store
   mapper on `needs_credentials`. After: the rest.
2. D10: `SignInShape`; SMB's producer; `get_volume_sign_in_state` answers it. TDD: `commands/network_test.rs` default
   cell, an SMB cell for `guestAllowed`, and the SFTP rung table in `reconnect_test.rs` updated by variant.
3. `pinned` on `KnownSftpServer` and `KnownWebdavServer` (`#[serde(default)]`, default `false`; the connect path sets
   `true` only when the entry is NEW, and otherwise preserves the stored value). TDD: the "field-absent file" cell each
   store already has, copied for `pinned`, plus a "reconnect preserves an unpinned entry" cell in each wiring test.
4. D5: `one_shot_credentials.rs` and `SecretOffer` on both wirings. TDD: a wrapper cell (answers the one key, forwards
   the rest, never writes), a wiring cell per backend against the Docker fixture proving a `remember: false` dial leaves
   the store empty afterwards. ❗ Every app-crate fixture cell is named with the `sftp_integration_` /
   `webdav_integration_` prefix, or `desktop-fixture-lane-coverage` fails it and no lane runs it.
5. D3 backend: `sftp_app_root` / `webdav_app_root` in `ids.rs`; the crates' roots, `to_remote_path` prefix stripping,
   `to_app_path`, and `display_path_for` on it (TDD in each crate's `paths_test.rs`: prefixed and relative spellings
   land on the same server path and round-trip through `to_app_path`; a bare server-absolute path and a prefixed path
   outside the root are both refused); a routing-level cell against the fixture proving a copy whose destination went
   through `root_anchored` lands where the pane says (a new `write_operations/sftp_transfer_semantics_test.rs` modelled
   on the SMB twin's `root_anchored` destination cell, prefixed per step 4); the `sftp://` / `webdav://` arm in
   `resolve_path_to_volume` (TDD: registered volume, pinned saved server, UNPINNED saved server, unknown prefix →
   `None`, no connect performed, and the existing local cells untouched).
6. D4: `commands/servers.rs` and the reconnect-command rename. TDD: `commands/servers_test.rs` cells for the union
   listing (three stores, one pinned, one connected), the outcome mapping (every WebDAV and SFTP variant lands on its
   superset twin, `AuthMethodUnsupported` included), pin round-trips emitting `volumes-changed`, and
   `connect_saved_place` refusing a registered id.
7. D2: the servers arm in `volume_listing::complete`. TDD: a listing cell with one registered SFTP volume, one pinned
   WebDAV entry, one unpinned entry (absent), asserting id equality between the `saved` row and the id `sftp_volume_id`
   mints, and `capabilities: Some` on the registered one.
8. D12 backend half: `DeviceVolumeEntry.device_readiness`, the provider listing all states, `connect_adb_device` taking
   an attempt id. TDD: `adb/device_provider.rs` cells per state, a cancel cell against `FakeAdbServer`.
9. D11 backend half: `adb_enabled` / `adb_binary_path` settings plumbing and `set_adb_settings`. After: the loader parse
   cell, a tracker restart cell.
10. D6 backend half: the typed `VolumeContextAction.action`, the Disconnect / Forget items with the busy guard,
    `VolumeUnmounted` gaining `volumeId` and being emitted on disconnect and on forget (before `volumes-changed`).
    After: `commands/menu.rs` cells, a `volume_broadcast` ordering cell.
11. Docs: `crates/cmdr-fs` (the shape, the app roots), each crate's `DETAILS.md` § paths, `network/DETAILS.md` (servers
    family, one-shot wrapper), `volumes/DETAILS.md` (the arm, the scheme arm), `adb/DETAILS.md`, `commands/DETAILS.md`,
    `mcp/resources/DETAILS.md`. Wipe the "not wired yet" bullets these close.

Checks: `pnpm check rust` per step, `pnpm check` after step 1 (it touches the frontend), `pnpm check --include-slow`
once at the end of M0 (the fixture lanes).

### M1. Switcher rows and the hub

1. D7: `ServersHub.svelte` replacing `NetworkBrowser.svelte` (rename, then edit, to keep file history), `PlacesBrowser`
   (renamed `ShareBrowser` with an `account` prop), the MCP row encoding, the status column, the scope rename in
   `scope-hierarchy.ts`, the `SERVERS_VOLUME_NAME` const and its four test retargets, the discovery-off line with the
   "Network (disabled)" label and the Settings early return deleted and `network-toggle.spec.ts` rewritten (it already
   drives settings through the `mcp-set-setting` event and keys on `.volume-name`; the new assertions key on the hub's
   line and its link). ❗ The hub's "Connect to server…" row keeps opening today's `ConnectToServerDialog` until M2
   replaces it, so a user can add an SMB host at every boundary. `volume-grouping.ts` applies the three-things rule; the
   switcher row renders the `saved` dot, the protocol in the `volume-fs` slot, and the Disconnect control (D6). The
   context menu (D6) and the hub commands that mirror it.
2. D3 frontend: `server-path-utils.ts`, the `navigate.ts` guard, `initialization.ts::resolveVolumeId` verified against a
   `saved` id (TDD: a restore cell with an `sftp://` path), and D14's two rules in `path-resolution.ts` (TDD: the
   restored-subpath cell, and a scheme path never walking above its root).
3. **M1 deliverables from D8**: `RemoteConnectView.svelte` with the `connecting` and `refused` states only, and
   `connect-flow.ts`'s dial half (the three-arm dispatch and the terminal outcomes; the sheet-driven loop is M2).
   `handleVolumeSelect` on a `saved` row navigates to the volume id; `FilePane` gates `connectionState === 'saved'` in
   front of kind and renders `connecting`. Until M2 delivers the sheet, a `needs_credentials` or
   `authentication_rejected` outcome renders `refused` with the explanation and "Try again" only; ❌ no inert "Sign in…"
   button. M1 is demoable on a server with a stored secret.

Tests: TDD for `volume-grouping` (three-things rule: hub row always, connected-unpinned appears, pinned `saved` appears,
unpinned-disconnected absent), hub sorting; after: `ServersHub.test.ts` (rename of `NetworkBrowser.test.ts`, with new
rows), `PlacesBrowser.test.ts`, breadcrumb tests (Disconnect on `direct`, nothing on `saved`), MCP encoding test, the
hub command handlers. E2E: `servers.spec.ts` on the Playwright lane drives the hub, the switcher rows, and the pane view
through `emitBackendEvent('volumes-changed', …)` with a `saved` SFTP row (using an id no real backend can claim, and
asserting before any real broadcast can clobber it) and through the `smb-e2e` fake for a real mount; ❗ the SFTP wire
itself is proven in the Rust integration lane, not in Playwright (no SFTP fixture is leased there). Docs:
`network/CLAUDE.md` + `DETAILS.md` (the dir keeps its name; a move is churn without a win), `navigation/CLAUDE.md`,
`pane/CLAUDE.md`.

Checks: `pnpm check --fast` per step, `pnpm check` at the end, `pnpm check desktop-e2e-playwright` for the spec.

### M2. The sign-in sheet and the connect flow

1. `address-parser.ts` (TDD, example table: every shape in D9 round-trips; `user@host` is SFTP; a bare `https://` origin
   is WebDAV; `smb://` is SMB; garbage is `unparsed`).
2. `SignInSheet.svelte` with the four renderers (`nothing` never opens the sheet), the host-key step (both `kind`
   values, revoked, `superseded` restart, `unreachable` no-write), the add mode fields, `dialog:allow-open` in the main
   capability file, the `servers` area added to `messageKeyKnownAreas` in
   `scripts/check/checks/desktop-message-key-naming.go` (an unknown first segment is an error-level violation), the edit
   mode with the two switches and the `needs_stored_secret` warning (`getSftpUnattendedReconnect` / the WebDAV twin,
   asked when the sheet renders), the per-mode Remember seeding. Dialog registry, gallery rows (one per mode, fixtures),
   a11y block in `network.a11y.test.ts`.
3. `connect-flow.ts` (TDD with `installIpcMock`: the three-round first connection, `superseded` restart, cancel from
   each phase returns `cancelled` silently, `remember: false` sends the offer and never `save_*`, a registered
   `needs_sign_in` volume takes `reconnect_volume_with_credentials` and never a dial).
4. Wire: hub Add row, ⌘K, palette, the go-to-path scheme intercept (D13, TDD: one cell per scheme, one per return shape,
   one per call site, and a cell that `resolveGoToPath` is never called for one), `RemoteConnectView`'s remaining states
   and `signed_out`'s button, the switcher row's Edit…. The reconnect manager gains a `needs-host-key` status with the
   same hand-off `needs-auth` gets, PLUS the fourth `show*` derivation in `smb-view-state` and the matching `FilePane`
   branch rendering `host_key_changed` (the three existing ones are a positive list, so a fourth status otherwise
   compiles and shows a plain listing over a dead session; the manager's "ignored on purpose" comment and branch go).
   `servers.refusal.` joins `unusedKeyDynamicPrefixes` in `desktop-message-keys-unused.go`, since the keys are built
   from the outcome kind. Delete `ConnectToServerDialog.svelte`.
5. The two remedy buttons (D9).

Tests: above, plus the sheet's component tests (wrong password renders inline and keeps the field's focus; Tab moves
between fields, never switches panes). Docs: `servers/CLAUDE.md` + `DETAILS.md` (new dir: the flow, the sheet contract,
the renderer table, the reserved shapes), `docs/guides/building-ui.md` gains one line pointing at the sheet for any
credential ask, `docs/architecture.md` row.

### M3. SMB moves onto the sheet and the pane view

❗ **What M2 already took off this list.** `SmbReauthView.svelte` is gone: the `needs-auth` branch renders
`RemoteConnectView`'s `signed_out`, whose button opens the sheet as a REGISTERED place. `needs_host_key_approval` has
its own manager status and its own pane state. So step 2 below is now `SmbReconnectingView` and
`VolumeUnreachableBanner`'s `smbGaveUp` variant only, and `gave_up` lands with that variant's retirement.

❗ **The sheet's vocabulary is wider than D8 drafted.** `SignInRefusal` is `connect-refusals.ts`'s full
`ConnectRefusalKind` (one vocabulary for a refusal across the pane and the sheet, so the two can't drift), the attempt
outcome keeps the two host-key payloads, and it gained `handed_off` for SMB's add path, whose connect is a share mount
rather than a session. `servers/DETAILS.md` § "The sheet contract" is canonical.

1. `ShareBrowser`'s listing auth, `NetworkMountView`'s mount auth, `SmbReauthView`, and `FilePane`'s `smbUpgradeLogin`
   branch all call `openSignInSheet({ mode: 'sign-in', shape: username_password, attempt })` with their own `attempt`.
   `direct-connect.ts`'s `raiseCredentialsForm` becomes a call into the sheet; `smb-login-hosts.ts` and
   `NetworkLoginForm.svelte`'s in-pane rendering are deleted (the form's field logic moves into the renderer, including
   the username-hint lookup and the guest radio, which becomes a `RadioGroup` inside the renderer with the same
   `$derived.by` rule; the in-pane focus-trap `eslint-disable` goes rather than being carried over).
2. `SmbReconnectingView` and `SmbReauthView` are replaced by `RemoteConnectView` states; the reconnect manager's
   `needs-auth` maps to `signed_out { shape }`. `VolumeUnreachableBanner`'s `smbGaveUp` variant becomes `gave_up`.
3. Wipe `servers-in-the-sidebar.md` from `docs/specs/` (its intent now lives in `servers/DETAILS.md`).

Tests: the three site tests rewritten to assert the sheet request (not a rendered form); `smb-reconnect-manager` test
for the shape hand-off; a11y blocks updated; the SMB E2E spec's login steps retargeted to the sheet. Docs:
`network/CLAUDE.md` loses the Tab guard, `connectionMode`, and login-hosts must-knows (the never-pre-prompt rule stays),
`pane/CLAUDE.md`, `DETAILS.md`s.

### M4. The toast and Settings

1. The educational toast (D13). TDD: a pure `should-show-pin-hint.ts` (count ≥ 5, seen flag, favorites ≥ 3 adds the
   line).
2. D11: the Servers card and the Android (ADB) section, `settings-i18n-parity`, `settings-registry` tests.

Docs: `settings/CLAUDE.md`, `navigation/DETAILS.md`. Checks: `pnpm check`.

### M5. ADB in the pane

1. D12 frontend: non-ready rows, `waiting_for_device` auto-proceed, cancel, a new `adb/adb-connect-errors.ts` wording
   every `AdbConnectOutcomeError` variant from the `adb.connect.*` keys (nothing words the enum today; the app-side ADB
   doc says otherwise and is corrected), `RemoteConnectView.refused` rendering them, Disconnect wording, the `/sdcard`
   first-path rule.
2. The MTP-header hint.
3. Wipe `android-adb-ui.md` except decision 1 (moves to M8's entry in `later/` if M8 slips).

Tests: `connection-views.a11y.test.ts` blocks, an E2E driving `emitBackendEvent` for `volumes-changed` with a
`waiting_for_authorization` → `ready` transition (the real-device pass stays David's:
`android-adb-backend-follow-ups.md` § 1). Docs: `adb/CLAUDE.md` (frontend), `pane/CLAUDE.md`.

### M6. Reconcile, docs pass, spec wipe

1. Rebase on `main`, resolve, full `pnpm check --include-slow`.
2. `docs/architecture.md`, every touched `C.md` at 300–400 words, `docs/specs/index.md` updated, the two superseded
   specs wiped per `docs/specs/DETAILS.md` § "Wiping a shipped spec", `later/sftp-follow-ups.md` § 1 and
   `webdav-backend-follow-ups.md` § 1 rewritten to point at `servers/DETAILS.md`, the `~/.ssh/config` autocomplete idea
   recorded in `later/sftp-follow-ups.md`, and pinnable SMB shares (the share-level writer keyed the way statfs spells
   the server, plus the port) recorded as a `later/` spec of its own.

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
`friendly-error-style.test.ts` enforces those rules over the error catalog only, so M2 extends it to the
`servers.refusal.*` and `servers.paneState.*` keys.

- Hub row: "Servers". Hub columns: "Name", "Type", "Address", "Status", "Last used". Add row: "Add server…".
- Statuses: "Connected", "Saved", "Found nearby", "Signed out", "Waiting for you to check the key".
- Sheet titles: "Add server", "Sign in to {name}", "Edit {name}". Buttons: "Connect", "Sign in", "Save", "Cancel".
- Remember: "Remember in Keychain". Advanced: "Name", "Remote folder", "Key file", "Use ssh-agent", "Reconnect
  automatically".
- Refusals: `authentication_rejected` "That password didn't work for {username}." `needs_credentials` "This server asks
  for a password." `auth_method_unsupported` "This server uses a sign-in method Cmdr doesn't support yet."
  `certificate_untrusted` "macOS doesn't trust this server's certificate. Add it in Keychain Access, then try again."
  `not_a_webdav_server` "Nothing at this address answers WebDAV." + "Try the Nextcloud address". `unreachable` "Cmdr
  couldn't reach {host}." `timed_out` "{host} didn't answer in time." `invalid_url` "That doesn't look like a server
  address."
- Host key, first contact: "First time connecting to {host}. Its key fingerprint is {fingerprint}. Trust it?" Button
  "Trust and connect". Changed: "{host}'s key changed. This can mean the server was reinstalled, or that something is
  sitting between you and it. Check the fingerprint with the server's owner before trusting it." Secondary, behind a
  disclosure: "I've checked it. Trust the new key". Revoked: "{host}'s key is marked as compromised in your SSH settings
  on this Mac. Cmdr won't connect to it."
- Pane states: "Connecting to {name}…", "Waiting for you to tap Allow on your phone", "Signed out of {name}", "Couldn't
  reach {name}", "Cmdr stopped trying to reconnect to {name}". Buttons: "Cancel", "Sign in…", "Try again", "Disconnect",
  "Look at the key".
- Context menu: "Open", "Disconnect", "Pin to switcher", "Unpin", "Edit…", "Forget saved password", "Forget server".
- Toast at five pins: "Your Network group is getting long. Right-click a server and choose Unpin, or use "Pin / unpin
  server" from the command palette; it stays in the Servers list." Extra line with ≥3 favorites: "Favorites work the
  same way." Button "Got it".
- Hub, discovery off: "Local network discovery is off." Link "Turn it on in Settings".
- ADB hint: "Want the whole filesystem? Turn on USB debugging." Link "How".
- ADB refusals (`adb.connect.*`): `adbNotInstalled` "Cmdr couldn't find the Android platform tools." + "Open Settings";
  `serverUnreachable` "The Android tools on this Mac didn't answer." + "Try again"; `deviceGone` "Your phone isn't
  connected any more."; `unauthorized` "Check your phone and tap Allow."; `deviceTooOld` "This phone's Android version
  is too old for Cmdr to browse."; `timedOut` "Your phone didn't answer in time." + "Try again"; `transport` "Cmdr lost
  the connection to your phone." + "Try again"; `cancelled` says nothing.
- Settings: "Servers (SFTP, WebDAV)", "Trusted host keys", "Forget"; "Android (ADB)", "Enable Android debugging (ADB)",
  "Status", "Found at {path}" / "Not found", "Re-check", "adb location", "Browse…". Tint label: "Servers (SMB, SFTP,
  WebDAV)".
- Go to path preview: "Opens {name}", "Adds a server".

❌ Never expose "adb server", "sync service", "transport", "rung", "PROPFIND", "Basic", "Digest", or a backend
diagnostic.

## Not in this effort

- Trust-on-first-use certificates, Digest, Nextcloud chunked uploads, quota: `webdav-backend-follow-ups.md` § 2–5. TOFU
  is the day-one wall for the NAS audience and should follow this effort directly.
- `~/.ssh/config` host aliases as autocomplete in the address field. Backend follow-up; recorded in
  `later/sftp-follow-ups.md` at M6.
- Wireless ADB pairing. Decided out (`android-adb-backend-follow-ups.md` § 5).
- S3 and OAuth code. Their contracts are D4 (`ServerTarget` arm), D8 (one renderer), D10 (the reserved shapes).
- A property-testing library on the frontend.

## Parallelism

None worth the risk. M0's steps 3, 4, 8, and 9 are independent of each other and could run as subagents in one worktree
touching disjoint files, but the `bindings.ts` regeneration serializes them anyway. Run everything in order.
