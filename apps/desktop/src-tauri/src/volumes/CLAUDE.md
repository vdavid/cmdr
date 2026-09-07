# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace`. Distinct from
`file_system/volume/` (the `Volume` trait + `VolumeManager`). Linux twin: `volumes_linux/`.

## Module map

`mod.rs` holds the model types and orchestrators and re-exports everything, so `crate::volumes::X` stays stable:
`ids.rs` (ID derivation), `fs_type.rs` (non-blocking `statfs`) and `nsurl.rs` (blocking NSURL enrichment), `mounts.rs`
(`getfsstat` enumeration), `smb.rs`, `cloud.rs`, `disk_image.rs`, `watcher.rs` (the `NSWorkspace` observer behind
`volume-mounted` / `volume-unmounted`).

## Must-knows

- **❌ Never derive or parse a volume ID yourself; call `ids::volume_id_for`** (or `volume_id_for_mount` given only a
  path). An ID keys the index DB, `lastUsedPaths`, tabs, and routing, so a lossy one sends reads and deletes to the
  wrong disk. Only its scheme prefix means anything: ❌ never match on the slug or rebuild one from parts.
- **One volume ID publishes ONE location at ONE canonical root**: mounts sharing an ID collapse to the shortest path
  via `cmdr_fs::volume::canonical_root::collapse_by_volume_id` (shared with `volumes_linux/`: ❌ never re-copy it here),
  and `list_locations` dedupes on ID, ❌ never on path alone.
- **The unmount path can't use `volume_id_for_mount`**: nothing identifies a gone mount, so it falls back to the wrong
  id. Use `VolumeManager::remove_root(volume_path)` (`handle_volume_unmounted`).
- **Check cloud-drive prefixes BEFORE `statfs` in `resolve_path_volume_fast()`**: a cloud drive is a folder on the data
  volume, so `statfs` answers `/` for it and mis-highlights "Macintosh HD".
- **Discovery must never block on a hung mount** (a wedged NAS once froze launch): enumerate with
  `getfsstat(MNT_NOWAIT)`, ❌ never NSFileManager; run blocking NSURL / NSWorkspace / DiskArbitration enrichment for
  LOCAL mounts only; never discover on the main thread.
- **Launch-time icon, LaunchServices, and TCC-protected `read_dir` calls need the FDA gate**
  (`crate::fda_gate::is_fda_pending_runtime()`), or onboarding stacks 5-10 native TCC popups.
- **Detect SMB with `is_smb_fs_type()`**, ❌ never raw `"smbfs"` / `"cifs"`: one place covers both platforms.
- **`mount_is_read_only` and `is_disk_image` are set in BOTH `get_attached_volumes` and `resolve_path_volume_fast`**, or
  they drift. ❌ Read-only is not a disk-image proxy: a writable `.dmg` is read-write.
- **Only `enrich_from_volume_registry` copies registry state onto a `LocationInfo`** (`capabilities` +
  `connection_state`); a new field goes there once, in BOTH twins. ❌ Never from a discovery constructor.
- **A published volume list is assembled by `volume_listing::complete`**, which owns the order (device providers,
  servers arm, enrichment) and the only `append_device_volumes` call.
- **Wrap every objc-touching `spawn_blocking` body in `objc2::rc::autoreleasepool`**, or the objects leak. Keep
  `watcher.rs`'s observer block cheap: it runs on the main thread, so no blocking I/O.

Decisions, edge cases, the servers arm, and the `Retained::cast_unchecked` contract: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
