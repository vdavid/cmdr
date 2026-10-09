# Favorites (backend) details

User-editable favorites. The frontend's favorites menu (⌃D) is fully user-owned: add, remove,
rename, reorder, and assign letter shortcuts. This module owns the ordered `favorites.json` store; the IPC layer
(`commands/favorites.rs`) is a thin pass-through. Read `CLAUDE.md` first for the
must-knows.

## What it replaces

Favorites used to be a hardcoded `Vec<LocationInfo>` of four folders, computed fresh on every
`get_favorites()` call with no write path. Now `get_favorites()` (both the macOS `volumes/mod.rs` and
the Linux `volumes_linux/mod.rs` twins) reads `favorites::store::list()` and maps each entry to a
`LocationInfo` with `category: Favorite`. The frontend already tells favorites apart from volumes via
`category`, so no extra "user-removable" flag is needed: every favorite is now a user favorite.

## On-disk shape

`favorites.json` in the app data dir:

```json
{
  "_schemaVersion": 1,
  "favorites": [
    { "id": "9f1c…", "path": "/Applications", "name": "Applications", "shortcut": "A",
      "volume": { "id": "root", "root": "/", "name": "Macintosh HD" } },
    { "id": "a83e…", "path": "/Volumes/naspi/docs", "name": "docs",
      "volume": { "id": "smb-naspi-…", "root": "/Volumes/naspi", "name": "naspi on nas.local" } }
  ]
}
```

- `id`: a random UUID minted on add, never derived from `path`. The frontend exposes it as
  `LocationInfo.id = "fav-<id>"`.
- `path`: the folder as an app path, spelled the way it was when last seen live: an OS path for a
  mounted volume, a `scheme://` path for a server or a phone.
- `name`: the display label. Defaults to the path's file name on add; the user can override via
  rename.
- `shortcut`: an optional uppercase ASCII letter. Omitted for unassigned and older entries.
- `volume`: which volume the folder lives on (`FavoriteVolume { id, root, name }`), as it was when
  the favorite was added or last claimed. The id is the registry id, the same identity a tab's
  `volumeId` carries, so a saved share, server, or phone names its row even while offline. `root` is
  what `path` sits under, which is what lets the listing rebase a mount-rooted favorite onto a moved
  mount point. `name` words a pick when the volume has no row at all. Absent on an entry written
  before the field existed until a listing claims it (§ Claiming legacy entries).

Order in the array is the display order.

**No schema bump for `volume`** (stays 1): the field is additive and optional, exactly like
`shortcut`. A bump would be actively harmful: `read_store_from_path` QUARANTINES a version mismatch,
so a beta user who downgrades would lose their whole list. An older build ignores `volume` (serde
ignores unknown fields; there's no `deny_unknown_fields`), drops it on its next write, and the next
newer build re-claims it.

## Seed-once contract

`favorites.json` existing means "already initialized." Four states:

- **File absent** (`read_store_from_path` returns `None`): first launch. Seed the platform defaults
  and write them. This is the only path that ever writes defaults.
- **File present, non-empty**: read verbatim.
- **File present, empty list**: the user cleared every favorite. Read verbatim (stays empty). Never
  re-seed.
- **File present, corrupt or wrong `_schemaVersion`**: quarantine to `<name>.broken`, then read as
  `Some(empty)` (NOT `None`). A broken file is "initialized but unreadable," so we must not re-seed
  the defaults over a user who had intentionally cleared their list.

Existing beta users (data dir present, no `favorites.json` yet) hit the absent branch on the first
launch after the update and get the platform defaults seeded, so there's no regression from the old
hardcoded behavior.

Platform defaults (`default_favorites`, platform-native per `design-principles.md`):

- macOS: `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads` (the previous hardcoded four).
- Linux: Home, `~/Desktop`, `~/Documents`, `~/Downloads` (matching the old `volumes_linux`
  favorites).

## Operations (pure core)

All in `store.rs`, unit-tested without disk or an `AppHandle`:

- `add(path, name?, volume)`: dedups by `(volume.id, path under volume.root)` when both sides know
  their volume, else by normalized path, so one share mounted at `/Volumes/naspi` and later at
  `/Volumes/naspi-1` is one favorite, and a legacy entry meets its own re-add. A re-add moves the
  existing entry to the end, keeps its id (applying a `name` override if given), and takes the add's
  `path` and `volume` (the root and name may have moved). A fresh add appends with a UUID id and a
  label defaulting to the path's file name. Move-to-end (not move-to-top) because favorites are a
  curated, ordered list, not a recency stack: a re-add shouldn't reshuffle the user's deliberate
  ordering more than necessary.
- `claim_volumes(claims)`: fills the `volume` of each named entry ❗ only where it's still `None`
  (§ Claiming legacy entries). Not a user gesture: `mutate_and_persist` takes a `StoreChange`, and
  `Claimed` reports nothing to analytics.
- `remove(id)`: drops by id. No-op if absent.
- `rename(id, name)`: updates the label by id. No-op if absent.
- `reorder(ordered_ids)`: reorders to match the given id list. Unknown ids are ignored; favorites
  whose ids are missing from the list are appended in their current relative order, so a partial or
  stale order from the frontend never drops an entry.
- `set_shortcut(id, shortcut)`: accepts one ASCII letter or `None` to clear. Letters are stored
  uppercase. Assigning one already owned by another favorite transfers it, keeping keyboard picks
  unambiguous. No-op if the id is absent.

`normalize_for_dedup` strips a single trailing `/` (but keeps root `/`), so `sftp://u@h:22/` and
`sftp://u@h:22` are one root while `…/srv/data` and `…/srv/data-1` stay two. Case-sensitivity is a known
limitation, same as `go_to_path/history.rs`: on case-insensitive APFS `/Users/x/Foo` and
`/Users/x/foo` compare unequal (worst case: a duplicate-looking row). We don't `canonicalize()` (it
would resolve symlinks and require the path to exist).

## Persistence and concurrency

- In-memory cache: `OnceLock<Mutex<Option<FavoritesStore>>>`. `None` until first access loads (and
  lazily seeds) from disk.
- `DISK_LOCK` (a separate `Mutex<()>`) serializes the read-modify-write cycle so concurrent commands
  can't clobber each other. `load_or_seed` re-checks the cache under the disk lock so two concurrent
  first-access callers can't both seed.
- Atomic writes via `config::durable_write_json` (write-tmp + fsync + rename + parent-dir fsync),
  the same data-loss-class discipline the rest of the app uses.
- The disk lock is never held across an `.await`; the in-memory guard is always dropped before any
  `fs` call. (The commands themselves run the store calls inside `spawn_blocking`, so even the
  synchronous store API never blocks the IPC thread.)
- `list_cached()` is the never-waiting read: a `try_lock` on the in-memory cache alone, answering
  `None` when the cache is still cold or another thread is mid-mutation. It exists for the Dock tile
  menu, which AppKit builds on the main thread while the Dock holds the user's mouse-down, so ❌ it
  must not fall back to `list()` on a `None` — that seeds the file behind the disk lock. The
  guard-never-held-across-I/O rule above is exactly what makes the `try_lock` sound. Full contract:
  `../dock/menu/DETAILS.md`.

## IPC contract (`commands/favorites.rs`)

Thin async pass-throughs, each `blocking_typed_result_with_timeout` (5 s, the write tier) since the
store write touches the filesystem. After persisting, each re-emits `volumes-changed` via
`volume_broadcast::emit_volumes_changed()` so both panes' menus refresh live
(subscribe-don't-poll). Listing rides the existing `list_volumes` / `volumes-changed` path, so
there's no `list_favorites` command.

- `add_favorite(path: String, name: Option<String>) -> Result<(), AddFavoriteError>`
- `remove_favorite(id: String) -> Result<(), DeadlineError>`
- `rename_favorite(id: String, name: String) -> Result<(), DeadlineError>`
- `reorder_favorites(ordered_ids: Vec<String>) -> Result<(), DeadlineError>`
- `set_favorite_shortcut(id: String, shortcut: Option<String>) -> Result<(), SetFavoriteShortcutError>`

Remove, rename, and reorder answer in `DeadlineError` (`deadline/mod.rs`) rather than a vocabulary of their own,
because the store swallows its own write errors: a favorite that doesn't reach disk still applies in
memory, so a missed deadline (or a panicked blocking task) is the only thing they can report.
`add_favorite` can also REFUSE (§ The add gate), so it owns `AddFavoriteError`: the same two
deadline variants plus `NotAPlace` and `PlaceNotConnected`. The error-type map is
`docs/guides/error-handling.md`.
`set_favorite_shortcut` owns `SetFavoriteShortcutError` because it also rejects anything other than
one ASCII letter (or `None`), including untrusted IPC callers.

Registered in the `ipc.rs` manifest, which feeds both runtime dispatch and the specta types.

## The read side

The listing builds a favorite row in two stages (`target.rs` header), because neither alone sees
enough. ❗ Every stored favorite publishes a row, a missing or offline one included: hiding is what
made favorites on an unmounted share silently vanish.

**Stage 1, discovery** (`volumes::favorite_location`, `volumes_linux::favorite_location`). Blocking,
under `discover_local`'s 2 s timeout, so it may look at the disk but ❌ never at a network path:

- The containing mount's fs type comes from the mount-table snapshot the listing already took
  (lexical, can't hang), so there's no per-favorite `statfs`.
- `target::on_disk` decides whether the folder may be stat'd: a `scheme://` path and a folder under
  a network mount are `Unchecked`; (macOS) a TCC-protected folder while the FDA gate is pending is
  `Assumed`; only a folder on a local disk gets `exists()` → `Yes` / `No`. The NSWorkspace icon is asked only
  for `Yes` (it's a call on the path too).
- The row's `favorite_target` is seeded from the store (`FavoriteTarget::discovered`) with a
  provisional reach (`NotFound` for `No`, `Ready` otherwise). The stored volume and the `OnDisk`
  evidence ride along in a `#[serde(skip)]` field for stage 2; they never cross IPC.

**FDA-pending skip (macOS).** Even `Path::exists()` trips a macOS TCC popup for the
protected-folder service once the bundle is registered with tccd, which is exactly the
onboarding-flood the FDA modal exists to prevent. So while the gate is pending, a folder where
`restricted_paths::tcc_paths::is_potentially_tcc_restricted(path)` is true is taken on trust
(`Assumed`: reach `Ready`, and enough evidence for a boot-volume claim, since these are home
folders on the local disk). ❗ Without that, the seeded `~/Desktop`, `~/Documents`, and
`~/Downloads` (no stored volume yet) read "not found" all through onboarding. The gate is pending
only until the person answers the FDA question (a decline clears it too). Non-protected paths are still checked (for example `/Applications`
can be absent on slim systems). Linux has no TCC, so its twin has no gate.

**Stage 2, the reach pass** (`reach.rs::annotate`). Runs inside `volume_listing::complete` AFTER
registry enrichment, so every row's `connection_state` and `capabilities` are final, and reads rows
plus two in-memory settings (`ReachFacts`: MTP's manager bit, `adb::volume_wiring::is_adb_enabled`),
❌ never a disk. Per favorite: the target is the stored volume id, or a claim (§ Claiming legacy
entries). With a row for that id, `volume_root` is the row's path, a mount-rooted volume
(`VolumeScheme::is_mount_rooted`: root, local, path, SMB, cloud) has the favorite's `path` REBASED
from the stored root onto the row's (a share back at `/Volumes/naspi-1`, a renamed drive), and a
server or phone path stays verbatim (it's in that namespace; an edited remote root doesn't move it).
Discovery's `OnDisk::No` counts only when no rebase happened, since it probed the stored path.

The reach table, one decision site:

- Row live (`connection_state` `None`, `Direct`, or `OsMount`), or a device row that's registered and
  ready → `Ready`, unless discovery saw the folder gone → `NotFound`. A path no longer under the
  row's root → `NotFound`.
- Row `Saved`, `Disconnected`, `NeedsSignIn`, `NeedsHostKeyApproval` → `Connects`: the pane's own
  views (place-connect, the reconnect banner, the sign-in and host-key sheets) take it from there.
- Device row listed but unregistered (an ADB phone not dialed yet), or `WaitingForAuthorization` →
  `Connects` (the pane's device view dials or waits for the tap).
- Device row `Unavailable { reason }` → `Unplugged { device: phone, reason }`.
- No row, MTP or ADB with its setting off → `AccessOff { backend }`; on → `Unplugged { phone }`, or
  `Unplugged { storage }` when the same MTP device is listed under another storage id (a swapped
  card, `mtp_ids::device_id_of_volume`).
- No row, root / local / path / cloud → `Unplugged { drive }` (also an NFS or AFP mount that's gone).
- No row, SMB / SFTP / WebDAV / S3 → `Forgotten`: nothing saved knows the id. Forget keeps the
  favorite, and the same place coming back mints the same id and revives it.
- No target at all (an unclaimable legacy entry) → `NotFound`.

## Claiming legacy entries

An entry written before `volume` existed (`volume: None`) is claimed lazily by the reach pass, ❌
never at load: load stays syscall-free and `AppHandle`-free.

- The claim is the NON-favorite row whose path is the deepest whole-segment prefix of the
  favorite's path (mount rows, the boot volume, cloud folders, saved SMB rows at their last mount
  path, server places, device storages).
- ❗ **Claim only from evidence.** The boot volume claims a folder only when discovery SAW it
  (`OnDisk::Yes`) or took a TCC-protected home folder on trust while the FDA gate is pending
  (`Assumed`). Otherwise `/Volumes/naspi/docs` on an unmounted share nobody saved would be
  claimed by `/`, written down, and read "not found" forever after the share comes back. Any other
  row names the volume by being there.
- Claims persist through `store::claim_volumes`, which fills `None` only, reports nothing to
  analytics, and emits no `volumes-changed` (the published row already shows the claimed facts). It
  writes a file, so `complete` runs it on the blocking pool, off the listing path; one that doesn't
  land is re-made by the next listing.
- A legacy favorite nothing can claim stays `volume: None` and reads `NotFound`, visible and greyed,
  and claims itself the first time its share is mounted.

## The add gate

A favorite points at a folder on a VOLUME the app can name: a local drive, an SMB share, an SFTP /
WebDAV / S3 place, or an MTP / ADB phone. `add_favorite` enforces that and records which volume,
and `commands/favorites.rs::favorite_volume_for` is the one place the rule lives.

**Why it has to exist.** The read side resolves every favorite through its stored volume id, the
same identity a tab carries, so a favorite with no volume behind it could never say where it
points or dial anything. Without a gate a `search-results://`, archive-inner, `.git`-portal, or
servers-hub path would be written down with nothing able to reopen it.

**What it reads**, typed questions only, ❌ not one test on the path string:

1. `VolumeManager::path_routes_over_its_parent()`. An archive-inner or `.git`-portal path resolves to
   the parent DRIVE while the path itself has no folder of its own there. Same reading the
   write-op router and the agent's `WritableDestination` take.
2. The canonical resolver (`resolve_path_volume_with_timeout`): protocol arms for `mtp://`,
   `adb://`, `sftp://`, `webdav://`, `s3://` (each from cached state, never dialing), `smb://` → the
   servers hub, the mount table otherwise.
3. **The volume must CONTAIN the path**, by whole segments (`cmdr_fs::volume::app_paths`). A scheme
   with no protocol arm (`search-results://`) reaches the mount table, which walks UP to `/`, and `/`
   doesn't contain it. That keeps the rule generic instead of a list of schemes to extend.
4. **The volume must be REGISTERED, and its backend admitted** by `backend_holds_favorites`, an
   EXHAUSTIVE match on `BackendKind` (❌ no `_` arm, so a new backend is a compile error here
   rather than a silent yes): `Local | Smb | Sftp | Webdav | S3 | Mtp | Adb` yes, `Archive |
   GitPortal` no. Registered means the folder was seen live. An unregistered saved place or listed
   device (a server that isn't connected, a phone not dialed yet) answers
   `AddFavoriteError::PlaceNotConnected`, so MCP can say "connect first"; anything else
   unregistered (the servers hub) answers `NotAPlace`.

It answers `FavoriteVolume { id, root, name }` from the resolved row. An OS-mounted SMB share
served by `LocalPosixVolume` reports `BackendKind::Local`, which is admitted like any drive.

**A folder on a share nothing saved saves the share** (D2, `docs/specs/saved-smb-shares.md` writer
4). A Finder-mounted share has no `known-shares.json` row, so once it unmounts nothing could dial the
favorite. For an SMB-scheme volume, `add_favorite` calls
`network::smb_saved_shares::remember_favorited_share` before persisting: the server, port, share, and
account from the live mount's source, the gate's volume id, `pinned: false`, and ❗ an existing row
left exactly as it is (pin and account included). Visible effect: the share appears in the servers
hub, unpinned. Its password lives in Finder's Keychain item, so the first offline pick asks once
through the sign-in sheet. A GVFS share (Linux) has no mount-table source, so it isn't saved.

**A resolver timeout is not a refusal.** It means the gate could not classify the path, so
`add_favorite` returns `AddFavoriteError::TimedOut` and does not persist anything. Resolver-correctness
tests inject a generous filesystem timeout because a saturated blocking pool can spend the production
two-second budget waiting to schedule an otherwise fast mount-table read; a separate synthetic resolver
result pins the typed timeout outcome without depending on scheduler timing. The classification
itself (`favorite_volume_from`) takes the registry lookup as a closure, so its cells run without
touching the process-wide registry.

**Every add surface meets it**, because they all route through the command: the `favorites.add`
palette / Go-menu handler, the folder-row and `..` context menus (`menu/menu_handlers.rs` calls
`commands::favorites::add_favorite`, ❌ never `store::add`), and the MCP `favorites` tool, which maps
both refusals to `invalid_params` with words naming the rule rather than an internal problem. `rename_favorite` takes no path and
can't move one, so it needs no gate.

**The frontend predicate beside it is a different job, ❌ not duplication.** The favorites menu greys
its "Add current folder to favorites" row out on the pane's own capability reading, which is an
AFFORDANCE: it tells someone up front that this pane can't be favorited. This one is the
ENFORCEMENT, and it's authoritative — the MCP tool and the native menus never go near the frontend.
The frontend's half is `kindCanBeFavorited` / `paneFolderCanBeFavorited` in
`src/lib/file-explorer/pane/volume-capabilities.ts`, reading the pane's ROUTED kind. The two give
the same answer set from different readings (pane kind vs backend kind), so a change to one is a
prompt to check the other.

## MCP consumer

The MCP `favorites` tool wraps the `commands::favorites` pass-throughs (add / rename / remove / reorder), and
`cmdr://state` `favorites:` reads `store::list()` for id discovery. See `mcp/DETAILS.md`.

## Analytics

`mutate_and_persist` reports `favorite_changed` past its no-op guard, with the `FavoriteAction` of a
`StoreChange::Gesture` and the list's size after the change. The change is a required PARAMETER rather than something
inferred from the closure, so a new mutation can't be added without deciding what it reports; `StoreChange::Claimed`
(the listing's legacy-entry claims) reports nothing, since nobody did anything. The other half of the story, `favorite_opened`, is the frontend's
(`src/lib/file-explorer/navigation/favorites-analytics.ts`): this store can't see a navigation. Props and rationale:
`src-tauri/src/analytics/DETAILS.md` § "Starter event set".
