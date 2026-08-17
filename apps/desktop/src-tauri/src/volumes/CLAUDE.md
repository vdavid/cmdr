# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace`. Distinct from
`file_system/volume/` (the `Volume` trait + `VolumeManager`). Linux twin: `volumes_linux/`.

## Module map

Everything re-exports from `mod.rs` (`LocationInfo` / `LocationCategory`, consts, orchestrators), so
`crate::volumes::X` stays stable: `ids.rs` (which identity a mount is keyed by), `fs_type.rs` / `nsurl.rs`
(non-blocking `statfs` / blocking NSURL enrichment), `smb.rs` / `cloud.rs`, `mounts.rs` (`getfsstat` enumeration),
`watcher.rs` (the `NSWorkspace` observer behind `volume-mounted` / `volume-unmounted`).

## Must-knows

- **❌ Never derive a volume ID yourself; call `ids::volume_id_for`** (or `volume_id_for_mount` given only a path). An
  ID keys the index DB, `lastUsedPaths`, tab state, and routing, so a lossy one sends reads and deletes to the wrong
  disk. DETAILS § "A volume ID is derived from the volume's IDENTITY".
- **One volume ID publishes ONE location, at ONE canonical root**: a filesystem mounted twice collapses to the
  shortest path (`cmdr_fs::volume::canonical_root::collapse_by_volume_id`, shared with `volumes_linux/`: ❌ never
  re-copy it here), and `list_locations` dedupes on ID, ❌ never on path alone. Publishing one location doesn't forget
  the others; the registry keeps them. DETAILS § "One volume ID publishes one mount root".
- **The unmount path can't use `volume_id_for_mount`** — neither `statfs` nor NSURL can identify a gone mount, so it
  falls back to the wrong id. Use `VolumeManager::remove_root(volume_path)`: it promotes a sibling mount and
  unregisters only on the last root. See `handle_volume_unmounted`.
- **`resolve_path_volume_fast()` checks cloud-drive prefixes BEFORE `statfs`**: cloud drives are plain folders on the
  data volume, so `statfs` resolves any path inside them to `/` and mis-highlights "Macintosh HD". It shares
  `cloud_volume_info()` with `get_cloud_drives()`, so IDs and categories can't drift.
- **Volume discovery must never block on a hung mount** (a wedged NAS once froze launch): enumerate with
  `getfsstat(MNT_NOWAIT)`, never NSFileManager; run blocking NSURL / NSWorkspace / DiskArbitration enrichment for
  LOCAL mounts only (❌ never ask a network mount for its UUID); never discover on the main thread. DETAILS § "Hung
  mounts".
- **Launch-time `NSWorkspace` icon or LaunchServices lookups and TCC-protected `read_dir` need the FDA gate**
  (`crate::fda_gate::is_fda_pending_runtime()`), or onboarding stacks 5-10 TCC popups. While pending,
  `get_icon_for_path()` returns `None` and `get_cloud_drives()` is empty; both re-emit after the decision.
  `lib/onboarding/CLAUDE.md` § "FDA gate".
- **Detect SMB with `is_smb_fs_type()`**, never raw `"smbfs"` / `"cifs"` comparisons: one place covers both platforms.
- **`is_read_only` (`MNT_RDONLY`) and `is_disk_image` (DiskArbitration) are set in BOTH `get_attached_volumes` and
  `resolve_path_volume_fast`, or they drift.** Gate the disk-image probe to local mounts (it resolves the path, so a
  hung mount stalls it), and don't read read-only as a disk-image proxy: a writable `.dmg` is read-write.
- **`LocationInfo` enrichment from `VolumeManager` lives only in `enrich_from_volume_registry`** (three callers); new
  enrichment fields go there once. It fills `capabilities` (the backend's `Volume::capabilities()`) and
  `smb_connection_state`; a location with no registered volume keeps `capabilities: None` and the frontend falls back to
  its per-kind defaults. ❌ Never fill `capabilities` from a discovery constructor: discovery knows the mount, the
  registry knows the backend.
- **`append_mtp_volumes` is duplicated** across `commands/volumes.rs` and `volume_broadcast.rs` (plus Linux twins), so
  set every MTP-derived field (like `usb_speed`) in BOTH or the bootstrap ships volumes missing it.
- **`get_main_volume` / `get_attached_volumes` / `get_volume_space` wrap their bodies in
  `objc2::rc::autoreleasepool`** (they run in `spawn_blocking`, so the per-call objc objects would leak), and
  `start_volume_watcher`'s observer block runs on the main thread: keep it cheap, no blocking I/O.
- **Location IDs**: `root`, `cloud-*`, and `fav-*` are literal; the rest are `{scheme}-{slug}-{digest}` from
  `cmdr_fs::volume::ids` (`vol-` = UUID-keyed local, plus `path-`, `smb-`, `mtp-`). Only the scheme prefix means
  anything: ❌ never match on the slug or rebuild an ID from parts. DETAILS § "Location IDs" also names the two
  `friendly_error.rs` sync points.

Decisions, edge cases, and the `Retained::cast_unchecked` contract: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
