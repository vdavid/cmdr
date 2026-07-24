# Volume abstraction

The `Volume` trait (the core abstraction for all storage backends) plus the `VolumeManager` registry. Every file system
operation goes through a `Volume`, with **paths relative to the volume root**.

## Module map

- `mod.rs`: the `Volume` trait (mostly async methods returning `Pin<Box<dyn Future>>`) and the `VolumeScanner` /
  `VolumeWatcher` / `VolumeReadStream` sub-traits. Re-exports `types::*` and `ids::*`.
- `types.rs`: the data types the trait exchanges (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `LaneKey`, …).
- `ids.rs`: the volume ID helpers (`path_to_id`, `smb_volume_id`).
- `manager.rs`: `VolumeManager`, a thread-safe `RwLock<HashMap>` registry with a default volume.
- `backends/`: per-backend impls (`LocalPosixVolume`, `MtpVolume`, `SmbVolume` + watcher, `InMemoryVolume`). See
  `backends/CLAUDE.md`.
- `eject.rs` (macOS+Linux): volume teardown by kind; `commands::eject` delegates to it. See `DETAILS.md`.
- `friendly_error/`: typed, word-free error classification; the words live on the FE. See
  `friendly_error/CLAUDE.md`.

## Must-knows

- **Optional trait methods default to `Err(NotSupported)` / `false`**, so new backends start with `list_directory` +
  `get_metadata` and opt into capabilities incrementally. New backend? See `DETAILS.md` § "Building a new volume".
- **`lane_key()` is the operation manager's serialization key** (default = volume root): write ops sharing a lane run
  one at a time, disjoint lanes in parallel. Override when multiple `Volume` instances share one physical resource.
- **At a site that calls a `Volume` method with a path, use `VolumeManager::resolve(volume_id, path).await`, not
  `get(volume_id)`.** `resolve` routes a `.zip`-crossing path to a read-only `ArchiveVolume` (on-demand, LRU-capped),
  returning the path UNCHANGED; otherwise it's a plain `get`. It's **async**: a REMOTE `.zip` needs a network probe,
  not `std::fs`. The sync-only `resolve_local_only` is for the ONE caller that can't `.await` (the write-op
  fresh-listing oracle). See `backends/archive/DETAILS.md` § "Routing and lifecycle".
- **Register watcher-pre-registered volumes via `VolumeManager::register_if_absent`, not `register`.** Otherwise the
  FSEvents watcher overwrites a pre-registered `SmbVolume` with a `LocalPosixVolume`; `register` overwrites, for
  explicit replacement only (a reconnecting `SmbVolume`).
- **All cross-volume copy flows through `open_read_stream` / `write_from_stream`.** Don't reintroduce
  `export_to_local` / `import_from_local`. New backends implement those two methods for cross-volume copy.
- **Never buffer a whole file in a transfer path** — don't drain a `VolumeReadStream` or collect a remote file into a
  `Vec<u8>`. Stream chunk-by-chunk. See `DETAILS.md` § "Streaming patterns".
- **`write_from_stream` is a mutation: call `notify_mutation` on success** on backends with unreliable notifications
  (SMB watcher / MTP USB events are lossy under load). `LocalPosixVolume` is the exception (FSEvents is reliable).
- **On macOS, never use `statvfs` alone for disk space** (ignores purgeable space: APFS snapshots, iCloud caches). Use
  `NSURLVolumeAvailableCapacityForImportantUsageKey` (`get_space_info_for_path`; `statvfs` fallback on Linux).
- **`MtpVolume` reports `create_directory_errors_on_existing_dir() = false`**: MTP allows same-name siblings and
  `create_folder` silently duplicates, so the folder-merge walker pre-checks existence there — else a merge targets the
  wrong directory.
- **`listing_is_watched(path)` defaults `false`**: a backend without a real watcher must not claim freshness, or
  pre-flight scans reuse stale cache. `true` means "fresh as our latest observation"; honor the per-backend debounce
  window. See `DETAILS.md` § "Trait capability model".
- **`begin_scan_session` / `end_scan_session` (default no-op) bracket background bulk work** (index scan, media
  enrichment prefetch) for scan-scoped resources (SMB's refcounted extra-session pool; `backends/DETAILS.md`).
- **`LocalPosixVolume::resolve` has a three-way branch for absolute paths** (the frontend sends full absolute paths, not
  always root-relative). Getting it wrong silently serves the wrong directory. See `DETAILS.md` § "Path handling
  gotchas".
- **`LocalPosixVolume` delegates `.git` read paths to the git module after `resolve()`**; mutations reject virtual
  paths via `git::is_virtual`. The hook order (`resolve()` then `try_route_*`) is fixed. See `DETAILS.md` § "Git
  delegation hooks".
- **`eject.rs` stops a `LocalExternal` index BEFORE `diskutil` runs.** An open FSEvents watcher/handle at unmount can
  wedge macOS FSKit (kernel-panic risk). See `DETAILS.md` § "Eject".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
