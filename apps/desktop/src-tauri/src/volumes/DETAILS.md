# Volumes details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Location categories

`LocationCategory` variants:

- **Favorite**: user-editable, from the `favorites/` store.
- **MainVolume**: root volume at `/`.
- **AttachedVolume**: `/Volumes/*` (skips System, Preboot, Recovery, CloudStorage).
- **CloudDrive**: iCloud at `~/Library/Mobile Documents/…`, providers at `~/Library/CloudStorage/`.
- **Network**: variant exists but is currently unconstructed.

`parse_cloud_provider_name` maps `~/Library/CloudStorage/` dir prefixes to display names (Dropbox, GoogleDrive→Google
Drive, OneDrive/Business, Box, pCloud, else the first `-`-segment).

## Location IDs (two cross-file sync points)

`DEFAULT_VOLUME_ID = "root"`; `ICLOUD_VOLUME_ID = "cloud-icloud"` is the only hardcoded cloud-drive ID (the others
derive from the `~/Library/CloudStorage/<provider>` dir name). Both the ID and the provider mapping are mirrored in
`friendly_error.rs`, which `crate::volumes` can't reach, being macOS-only: it matches the `ICLOUD_VOLUME_ID` literal
under a sync-point comment, and `parse_cloud_provider_name`'s provider list must stay in sync with
`friendly_error::enrich_with_provider`'s separate one.

Every other ID is minted by `cmdr_fs::volume::ids` (below).

## `list_locations()`

Aggregates all `LocationCategory` entries in order and deduplicates by path AND by volume ID, two `HashSet<String>`s.
The OS-level `/Network` browseable location doesn't surface as a sidebar entry yet, so `LocationCategory::Network` is
currently unconstructed.

### One volume ID publishes one mount root

**Decision**: `get_attached_volumes` collapses mounts that share a volume ID (`mounts.rs::collapse_by_volume_id`),
keeping the SHORTEST path and breaking ties lexicographically. `list_locations` then dedupes on ID as well as path.

**Why**: a volume ID is identity, and one filesystem can legitimately be mounted twice: macOS mounts the same SMB share
at `/Volumes/naspi` and `/Volumes/naspi-1`, and both derive the same ID (a share keys on `(server, port, share)`; a
local disk on its filesystem UUID). Deduplicating on path alone let both survive under one ID, and everything
downstream keys on the ID:

- `file_system::register_discovered_volumes` registered both, so the registry ended up rooted at whichever mount
  registered last. A pane restoring a saved `/Volumes/naspi/…` path then hit a backend rooted at `/Volumes/naspi-1` and
  the listing failed.
- The frontend builds keyed lists from the volume list, and duplicate keys throw during render (Svelte
  `each_key_duplicate`), which took the transfer dialog down with it.

The shortest path wins because macOS suffixes the LATER mount, so the shortest is the original: the root every saved
path, favorite, and index row already refers to. The choice is pure and order-independent, so discovery order can't
decide identity. Registration keeps the incumbent on a conflict too (`file_system/volume/DETAILS.md` § "Key
decisions"), so no single source has to get this right alone.

**Publishing one location is not the same as forgetting the others.** The registry keeps every mount root that carries
an ID and promotes a survivor when the active one dies, so the shortest-path rule here is the tie-break among live
roots rather than a permanent binding: `file_system/volume/DETAILS.md` § "A volume ID owns a set of mount roots". This
collapse stays purely about what the switcher SHOWS, and it still runs without touching any mount.

## Hung mounts

**The problem.** A network mount (SMB, NFS, …) can wedge so that every metadata syscall on it blocks in the kernel for
30s–forever (uninterruptible — even `SIGKILL` won't land until the mount is force-unmounted). Volume discovery is riddled
with such syscalls, and a single dead mount used to take the whole app down at launch: `init_volume_manager` ran
`get_attached_volumes` synchronously on the main thread (inside the Tauri `setup` closure), and NSFileManager's
`mountedVolumeURLsIncludingResourceValuesForKeys` `getattrlist`s every mount to build the URL array. On a wedged
`/Volumes/naspi` the main thread stuck in `__getattrlist` for 90s+ and the webview never recovered (its startup IPC piled
up behind the frozen process). The MCP `cmdr://state` resource hit the same wall through `list_locations`: reads took a
flat ~30s (one smbfs kernel timeout). (Incident: live NAS QA, 2026-07-13.)

**The fix — three layers.**

1. **Non-blocking enumeration.** `get_attached_volumes` enumerates via `getfsstat(MNT_NOWAIT)` (`enumerate_mounts`), not
   NSFileManager. `MNT_NOWAIT` returns the kernel's cached mount table (mount point, fs type, `MNT_RDONLY` flag, and the
   `f_mntfromname` SMB source) without ever round-tripping to a filesystem, so a wedged mount can't stall it — this is
   the difference between `df -n` and plain `df`. `getfsstat` was verified non-blocking on the exact wedged NAS state
   from the incident. Because fs type and read-only come straight from the snapshot, three former per-volume `statfs`
   calls (`get_fs_type`, `read_only_from_statfs`, `get_smb_mount_info`) are gone from this path.
2. **Skip blocking enrichment for network mounts.** `build_attached_location` runs the blocking NSURL / NSWorkspace /
   DiskArbitration enrichment (`resolve_local`) ONLY for local mounts. Network mounts (`is_network_fs_type`) derive
   everything from the getfsstat snapshot: id/name from `f_mntfromname` (SMB → "share on server"), `is_ejectable = false`
   (cosmetically moot — the eject affordance keys on `smbConnectionState` and `eject.rs` forces it true for SMB), no icon,
   never a disk image. So a dead network mount contributes its entry and never blocks discovery of the healthy volumes
   beside it.
3. **Off-main + timeout-guarded callers.** `init_volume_manager` registers root synchronously (cheap, `/` never hangs)
   and spawns attached/cloud discovery on the `volume-init` helper thread, then re-emits `volumes-changed`. Every caller
   of `list_locations` is wrapped in a ~2s `spawn_blocking` timeout (`volume_broadcast::do_emit`, the MCP
   `snapshot_volumes`, the `list_volumes` IPC via `blocking_with_timeout_flag`), so the remaining unguarded blocking
   paths inside `list_locations` — `get_favorites` and `get_cloud_drives`, which still `statfs`/icon per item and would
   hang on a favorite or cloud folder that lives on a wedged mount — degrade to a bounded 2s partial result instead of an
   infinite stall. `get_main_volume` no longer enumerates: it builds root directly from `/`.
4. **A timed-out listing publishes the LAST GOOD one.** `volume_broadcast` keeps the most recent successful
   `list_locations` and re-emits it (still flagged `timed_out`) when a later one misses the deadline. Publishing the
   empty list beside that flag told the frontend "you have no volumes", and since the picker's refresh button re-ran the
   same listing into the same timeout, nothing the user could do brought them back. Rationale and the staleness bound:
   `volume_broadcast.rs` § `LAST_GOOD_LOCAL`.

Note that the 2s deadline fires for reasons other than a hung mount: `list_locations` runs on the shared blocking pool,
so a subsystem that saturates the pool starves it just as effectively (`commands/CLAUDE.md` § `BlockingBudget`).

**Follow-up.** `get_favorites` and `get_cloud_drives` still do unguarded per-item `statfs`/icon; a favorite pointing at a
hung mount makes `list_locations` time out (2s), so that listing carries no fresh volumes at all and the broadcast falls
back to the last good set. Fully fixing "one dead mount never hides the others" here needs per-item timeouts for those
two, tracked separately.

## Global state in `watcher.rs`

- `APP_HANDLE: OnceLock<AppHandle>`: app handle for emitting events.
- `OBSERVER_INSTALLED: OnceLock<()>`: idempotency gate.

The observer `RcBlock` closures aren't kept in our own static; `addObserverForName:object:queue:usingBlock:` retains the
block for the lifetime of the registration, and we never remove the observer. Same pattern as
`file_system/open_with.rs`. `DualPaneExplorer` uses the `volume-unmounted` event (carrying the volume path) to redirect
panes off ejected volumes.

## Volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey`
(falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

## Key decisions

**Decision**: Detect mounted disk images (`.dmg`) via DiskArbitration's `DADeviceModel`, set on
`LocationInfo::is_disk_image` (see `disk_image.rs`).
**Why**: Disk images are transient install-style mounts, so the UI suppresses their index affordances and free-space
bars (the frontend reads `isDiskImage`). The reliable signal is DiskArbitration: `DADeviceModel == "Disk Image"` for any
`hdiutil`-attached image (verified on macOS 15.5, 2026-06-27). Read-only is NOT a usable proxy — a writable APFS `.dmg`
reports `is_read_only == false`, and conversely a locked SD card is read-only but not an image — so the two flags are
independent. `fs_type`/`f_mntfromname` don't disambiguate either (a `.dmg` can be APFS/HFS and present a normal
`/dev/diskNsM` source). The DA call is synchronous (no run loop) and cheap next to the per-volume NSURL/icon work, but it
resolves the volume path, so callers gate it to local (non-SMB) mounts to keep a hung network mount from stalling it.
Both `get_attached_volumes` (the switcher list) and `resolve_path_volume_fast` (highlight + transfer-source) set the flag
so they can't drift.

**Decision**: Populate `is_read_only` for attached volumes from the `statfs` `MNT_RDONLY` flag (`read_only_from_statfs`).
**Why**: It powers the 🔒 indicator and the copy/move write guard for ANY read-only mount (a read-only `.dmg`, a locked
SD card, an optical disc), not just MTP locked storage. The frontend guard machinery (`file-operation-commands.ts`,
`transfer-entry.ts`) already keys on `isReadOnly`, so populating the flag activates it with no frontend change; backend
`validate_destination_writable` (via `libc::access`) is the second line of defense.

**Decision**: A volume ID is derived from the volume's IDENTITY, never from the shape of its mount path, and every ID is
built by one funnel (`cmdr_fs::volume::ids`; `ids.rs` here picks the constructor).
**Why**: A volume ID keys the index DB, `lastUsedPaths`, tab `volumeId` fields, the `VolumeManager` registry, and
therefore operation routing. Deriving it by DELETING characters (strip everything outside `[a-z0-9-]`, then lowercase)
is a many-to-one map, so it hands two volumes one identity: `/Volumes/My Disk` and `/Volumes/My_Disk` both became
`volumesmydisk`, and a NAS's `Public` share and a Docker container's `public` share both became `volumespublic`. The
collision cross-contaminates every per-volume store, and the user-visible bug was a wrong-case path leaking from a
stale `lastUsedPaths` entry into `SmbVolume::list_directory`, where the case-sensitive `strip_prefix` against
`mount_path` failed and the smb2 path was built as `Volumes\Public` (relative under the share root), producing
`STATUS_OBJECT_PATH_NOT_FOUND` from Samba. Reads and destructive operations landing on the wrong disk is the same bug
with worse consequences.

Three properties make that unreachable rather than unlikely:

1. **Injective encoding.** Every derived ID is `{scheme}-{slug}-{digest}`, where the digest is 64 bits of BLAKE3 over
   the length-prefixed, scheme-separated canonical tuple. The slug is lossy and cosmetic (so a data dir stays
   eyeballable); nothing may key off it. Length-prefixing is what stops `("nas", "polyashare")` and `("naspolya",
   "share")` hashing the same bytes. Cryptographic rather than a fast hash because volume names are user-controlled, so
   a *chosen* collision has to be out of reach too. The bounded length matters: these are filename components
   (`index-{id}.db`).
2. **The best available identity source, per kind.** SMB keys on `(server, port, share)` with server and share
   lowercased (DNS hostnames and SMB share names are both case-insensitive, so that's canonicalization, not loss); MTP
   on the device serial; a local volume on its filesystem UUID; anything else on its mount path. `volume_id_for` is the
   single rule, shared by `get_attached_volumes` and `resolve_path_volume_fast` so they can't disagree about one
   volume.
3. **A loud registry.** `VolumeManager::register` logs an error when one ID would cover two different `root()`s. Not
   reachable from a derived ID, but a byte-for-byte volume clone reports the same UUID as its original, and so does a
   filesystem mounted twice; both are genuinely ambiguous, and neither may be silent.

**Decision**: A local volume's ID keys on its filesystem UUID (`NSURLVolumeUUIDStringKey`), not on its mount point.
**Why**: The mount point isn't stable. Plug a disk in while `/Volumes/Backup` is taken and macOS mounts it at
`/Volumes/Backup 1`; rename a volume and it moves too. A path-keyed ID therefore orphaned that volume's index and saved
paths and forced a full rescan, for the same physical disk. The UUID is read through `LocalVolumeMeta`, the existing
"blocking, local mounts only" seam, so the NSURL round-trip never happens for a network mount (it hangs on a dead one,
which is the whole point of § "Hung mounts"). A volume without a UUID (tmpfs, most FUSE mounts) falls back to its path.
`NSURLVolumeUUIDStringKey` is stringly-typed, and a typo would silently return `None` forever while every volume
quietly fell back to a path ID, so `nsurl::tests::the_boot_volume_reports_a_uuid` pins that the key still resolves
(verified on macOS 26.5.2, `getResourceValue:forKey:`, 2026-08-10).

The unmount path can't use any of this: it goes through `VolumeManager::remove_root` (root-keyed, like
`find_by_root`), because neither statfs nor NSURL recovers a gone mount's identity.

**Decision**: Index databases keyed by an ID from the retired scheme are deleted at launch (the reclaim half of
`Index::start_root_at_launch`, driven by `is_legacy_volume_id`), rather than migrated.
**Why**: They're disposable caches, so the cost of dropping one is a rescan, while the cost of a mis-targeted rename is
a corrupt index. Nothing can mint a legacy ID any more, so nothing will ever open these files again; left alone they'd
sit in the data dir until the LRU cap happened to reach them, which for a user under the cap is never. Persisted tab
IDs need no migration at all: `pane/initialization.ts` already re-resolves every tab's volume from its path at startup
precisely because a stored ID can go stale.

**Decision**: Gate launch-time icon fetches on the FDA decision (`crate::fda_gate::is_fda_pending_runtime()`).
**Why**: `NSWorkspace.iconForFile:` resolution touches LaunchServices and several adjacent TCC services beyond the input
path. On a fresh prod install with FDA off, calling it for `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`,
the iCloud root, and per-provider cloud-storage paths stacked 5-10 macOS native permission popups (MediaLibrary,
AppData, Desktop, Documents, Downloads, …) on top of the in-app FDA modal, exactly the onboarding flood the modal is
meant to replace. Returning `icon: None` from `get_icon_for_path()` while the gate is pending eliminates the class; the
frontend falls back to a generic folder icon, so the sidebar still shows favorite/volume entries (just generic for the
few seconds before the user decides). See `commands/indexing.rs::start_indexing_after_fda_decision` for the gate-clear +
re-emit on the deny path; the allow path requires a restart, so re-entering `setup()` sets the gate to `false` via the
OS probe.

**Decision**: Use `NSWorkspace` notifications, not an FSEvents watcher on `/Volumes`.
**Why**: FSEvents fires when the kernel writes a directory entry under `/Volumes`, which races the mount: `statfs` on the
new mount point still returns the root filesystem's `fsid` until the OS finishes mounting. Polling `fsid` to settle
times out on slow drives behind USB-C/Thunderbolt docks, and a timeout would filter the volume out until an app
restart. `NSWorkspace` notifications are posted by `diskarbitrationd` after the mount is fully settled and
`NSFileManager` metadata is ready, so there's no race, and they carry the volume URL directly in
`userInfo[NSWorkspaceVolumeURLKey]` (no diffing or polling). DiskArbitration would work too but needs a CFRunLoop
scheduled separately from Tokio; `NSWorkspace` rides on the AppKit runloop Tauri already runs.

**Decision**: Use `OnceLock` for `APP_HANDLE` and `OBSERVER_INSTALLED`.
**Why**: `start_volume_watcher` must be idempotent; `OnceLock::set` failing on the second call is the gate. `LazyLock`
would initialize eagerly, which doesn't work because the `AppHandle` isn't available at static-init time.

**Decision**: Use `NSURLVolumeAvailableCapacityForImportantUsageKey` with fallback to `NSURLVolumeAvailableCapacityKey`.
**Why**: The "ForImportantUsage" key accounts for purgeable space (iCloud, APFS snapshots), matching what Finder shows.
The plain key reports only physically free blocks, misleadingly low on APFS volumes with purgeable data. The fallback
handles older macOS versions lacking the key.

**Decision**: `supports_trash` defaults to `true` for unknown filesystem types.
**Why**: Optimistic default. Most local filesystems support trash; the exceptions (network mounts, FAT-family) are
explicitly listed. If an unknown fs type doesn't support trash, the op fails gracefully at trash time, better than
pessimistically disabling trash for a filesystem that supports it.

**Decision**: Use `libc::statfs` for filesystem type detection, not `NSURLVolumeLocalizedFormatDescriptionKey`.
**Why**: The NSURL key returns a locale-dependent human string ("APFS (Case-sensitive)"). `statfs.f_fstypename` returns
a stable machine identifier ("apfs", "smbfs", "nfs") that matches against the known network/non-trash list.

## Gotchas

**Gotcha**: `VolumeInfo` is a type alias for `LocationInfo`, not a separate type.
**Why**: The frontend sends/receives `VolumeInfo`, but locations also cover favorites and cloud drives. The alias keeps
IPC compatibility without a frontend migration.

**Gotcha**: The watcher registers/unregisters volumes with `VolumeManager` directly (tight coupling to
`file_system::volume::manager::get_volume_manager()`).
**Why**: A mounting volume must be immediately available for file operations. Emitting only a Tauri event and letting the
frontend trigger registration would open a race window where ops fail because the volume isn't registered yet. Direct
registration ensures that by the time the frontend gets `volume-mounted`, the volume is usable.

**Gotcha**: `get_main_volume`, `get_attached_volumes`, and `get_volume_space` wrap their bodies in
`objc2::rc::autoreleasepool`.
**Why**: Called from `spawn_blocking` threads. Without a pool, the per-call `NSFileManager`/`NSURL`/`NSString`/`NSNumber`
objects accumulate in a default pool that's never drained, leaking memory over hours.

**Gotcha**: The observer block in `watcher.rs::install_observers` runs on the main thread.
**Why**: With `queue: nil`, AppKit dispatches the block on the thread that posted the notification, and
`diskarbitrationd` posts on the main thread. Keep the body cheap: `register_volume_with_manager` is microseconds,
`try_upgrade_smb_mount` and `emit_volumes_changed` both `tauri::async_runtime::spawn`, and `app.emit` is non-blocking.
Don't add blocking I/O here without moving it onto a background task.

**Gotcha**: `userInfo` is downcast with `Retained::cast_unchecked` to `NSDictionary<NSString, NSURL>`.
**Why**: AppKit documents the value under `NSWorkspaceVolumeURLKey` as an `NSURL`. The unchecked cast trades a runtime
type check for a hard contract on Apple's side. A safer alternative (`cast::<NSDictionary>` plus a per-value
`downcast::<NSURL>`) costs an `isKindOfClass:` call per notification. We lean on the documented contract; revisit if a
future macOS version breaks it.

## Dependencies

- External: `dirs`, `objc2`, `objc2_foundation`, `objc2_app_kit` (`NSWorkspace`), `block2` (`RcBlock`).
- Internal: `crate::file_system::volume::{manager::get_volume_manager, LocalPosixVolume}`, `crate::icons::get_icon_for_path`.
