# Volume abstraction

The `Volume` trait's app-side wiring: the backends, the `VolumeManager` registry, and eject. Every file system
operation goes through a `Volume`, with **paths relative to the volume root**.

## Module map

- `mod.rs`: re-exports all of `cmdr_fs::volume` (the trait and its sub-traits, its data types, the ID helpers,
  `InMemoryVolume`, `friendly_error`), then declares what stayed. The trait: `crates/cmdr-fs/src/volume/mod.rs`.
- `manager.rs`: `VolumeManager`, a thread-safe `RwLock<HashMap>` registry with a default volume.
- `backends/`: per-backend impls (`LocalPosixVolume`, `MtpVolume`, `SmbVolume` + watcher, `InMemoryVolume`). See
  `backends/CLAUDE.md`.
- `eject.rs` (macOS+Linux): volume teardown by kind; `commands::eject` delegates to it. See `DETAILS.md`.
- `friendly_error/`: typed, word-free error classification, now in `cmdr-fs`; the words live on the FE. See
  `crates/cmdr-fs/src/volume/friendly_error/CLAUDE.md`.

## Must-knows

- **Optional trait methods default to `Err(NotSupported)` / `false`**, so new backends start with `list_directory` +
  `get_metadata` and opt in incrementally. New backend? `DETAILS.md` § "Building a new volume".
- **`lane_key()` is the operation manager's serialization key** (default = volume root): write ops sharing a lane run
  one at a time, disjoint lanes in parallel. Override when multiple `Volume` instances share one physical resource.
- **At a site that calls a `Volume` method with a path, use `VolumeManager::resolve(volume_id, path).await`, not
  `get(volume_id)`.** `resolve` routes a `.zip`-crossing path to a read-only `ArchiveVolume` (on-demand, LRU-capped),
  returning the path UNCHANGED; otherwise it's a plain `get`. It's **async**: a REMOTE `.zip` needs a network probe.
  The sync `resolve_local_only` is for the ONE caller that can't `.await`. See `backends/archive/DETAILS.md`
  § "Routing and lifecycle".
- **Register watcher-pre-registered volumes via `VolumeManager::register_if_absent`, not `register`.** Otherwise the
  FSEvents watcher overwrites a pre-registered `SmbVolume` with a `LocalPosixVolume`; `register` is for explicit
  replacement only (a reconnecting `SmbVolume`).
- **All cross-volume copy flows through `open_read_stream` / `write_from_stream`.** Don't reintroduce
  `export_to_local` / `import_from_local`. New backends implement those two methods for cross-volume copy.
- **Never buffer a whole file in a transfer path** — don't drain a `VolumeReadStream` or collect a remote file into a
  `Vec<u8>`. Stream chunk-by-chunk. `DETAILS.md` § "Streaming patterns".
- **`notify_mutation`'s trait default is a NO-OP, and every mutation (including `write_from_stream`) must call it.**
  A backend that doesn't override it leaves a stale pane after a copy, because SMB/MTP watcher events are lossy under
  load. `cmdr-fs` can't supply a default — it doesn't know the listing cache exists. `DETAILS.md` § "Mutation
  notification".
- **On macOS, never use `statvfs` alone for disk space** (ignores purgeable space: APFS snapshots, iCloud caches). Use
  `NSURLVolumeAvailableCapacityForImportantUsageKey` (`get_space_info_for_path`; `statvfs` fallback on Linux).
- **`MtpVolume` reports `create_directory_errors_on_existing_dir() = false`**: MTP allows same-name siblings and
  `create_folder` silently duplicates, so the folder-merge walker pre-checks existence there — else a merge targets the
  wrong directory.
- **`listing_is_watched(path)` defaults `false`**: a backend without a real watcher must not claim freshness, or
  pre-flight scans reuse stale cache. `true` means "fresh as our latest observation"; honor the per-backend debounce
  window. `DETAILS.md` § "Trait capability model".
- **`begin_scan_session` / `end_scan_session` (default no-op) bracket background bulk work** for scan-scoped resources
  (SMB's refcounted extra-session pool; `backends/DETAILS.md`).
- **`LocalPosixVolume::resolve` has a three-way branch for absolute paths** (the frontend sends absolute paths, not
  always root-relative). Getting it wrong silently serves the wrong directory. `DETAILS.md` § "Path handling gotchas".
- **`LocalPosixVolume` delegates `.git` read paths to the git module after `resolve()`**; mutations reject virtual
  paths via `git::is_virtual`. The hook order is fixed. `DETAILS.md` § "Git delegation hooks".
- **`eject.rs` stops a `LocalExternal` index BEFORE `diskutil` runs.** An open FSEvents watcher/handle at unmount can
  wedge macOS FSKit (kernel-panic risk). `DETAILS.md` § "Eject".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
