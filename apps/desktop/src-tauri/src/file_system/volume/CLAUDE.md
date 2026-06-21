# Volume abstraction

The `Volume` trait (the core abstraction for all storage backends) plus the `VolumeManager` registry. Every file system
operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`, with **paths relative to the
volume root**.

## Module map

- `mod.rs`: `Volume` trait (most methods async, returning `Pin<Box<dyn Future>>`; sync ones: `name`, `root`,
  `supports_*`, `local_path`, `space_poll_interval`), the `VolumeScanner` / `VolumeWatcher` / `VolumeReadStream`
  sub-traits, `MutationEvent`, and shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`,
  `SourceItemInfo`).
- `manager.rs`: `VolumeManager`, a thread-safe `RwLock<HashMap>` registry with a default volume.
- `backends/`: per-backend impls (`LocalPosixVolume`, `MtpVolume`, `SmbVolume` + watcher, `InMemoryVolume`). See
  [`backends/CLAUDE.md`](backends/CLAUDE.md).
- `friendly_error/`: typed, word-free error classification (errno → reason, provider detection); the words live on the
  FE. See [`friendly_error/CLAUDE.md`](friendly_error/CLAUDE.md).

## Must-knows

- **Optional trait methods default to `Err(NotSupported)` / `false`**, so new backends start with `list_directory` +
  `get_metadata` and opt into capabilities incrementally. Adding a backend? Read [DETAILS.md](DETAILS.md) § "Building a
  new volume" and § "Capability matrix" first; both are referenced from `docs/architecture.md`.
- **`lane_key()` is the operation manager's serialization key** (default = volume root): write ops sharing a lane run
  one at a time, disjoint lanes run in parallel. Override it when multiple `Volume` instances share one physical
  resource (MTP device, SMB server) so they don't thrash; see [DETAILS.md](DETAILS.md) § "Building a new volume".
- **Register watcher-pre-registered volumes via `VolumeManager::register_if_absent`, not `register`.** The FSEvents
  watcher would otherwise overwrite a pre-registered `SmbVolume` with a `LocalPosixVolume`. `register` (overwrite) is
  only for explicit replacement (SmbVolume replacing itself on reconnect).
- **All cross-volume copy flows through `open_read_stream` / `write_from_stream`.** Don't reintroduce
  `export_to_local` / `import_from_local`. New backends implement those two streaming methods to get cross-volume copy.
- **Never buffer a whole file in a transfer path.** Don't drain a `VolumeReadStream` into a `Vec<u8>` before writing,
  and don't collect a remote file into a `Vec<u8>` before yielding: an 8 GB copy would allocate 8 GB. Reads and writes
  must stream chunk-by-chunk. See [DETAILS.md](DETAILS.md) § "Streaming patterns".
- **`write_from_stream` is a mutation: call `notify_mutation` on success** on backends with unreliable out-of-band
  notifications (the SMB watcher and MTP USB events are lossy under load). `LocalPosixVolume` is the exception (FSEvents
  is reliable). The Tier 2 checklist's "call `notify_mutation` after each mutation" rule includes `write_from_stream`.
- **On macOS, never use `statvfs` alone for disk space.** It ignores purgeable space (APFS snapshots, iCloud caches),
  which over-blocks copies and disagrees with the status bar. Use `NSURLVolumeAvailableCapacityForImportantUsageKey`
  (`get_space_info_for_path` calls `crate::volumes::get_volume_space()` on macOS, falls back to `statvfs` on Linux).
- **`MtpVolume` reports `create_directory_errors_on_existing_dir() = false`**: the MTP protocol allows same-name sibling
  objects and `create_folder` silently duplicates, so the folder-merge walker pre-checks existence on MTP. A blindly
  created duplicate would make a merge target the wrong directory.
- **`listing_is_watched(path)` defaults `false`**: a backend without a real watcher must not claim freshness, or
  write-op pre-flight scans reuse stale cache. A `true` result means "fresh as our most recent observation", not
  byte-perfect; honor the per-backend debounce window. See [DETAILS.md](DETAILS.md) § "Trait capability model".
- **`LocalPosixVolume::resolve` has a three-way branch for absolute paths** (the frontend sends full absolute paths, not
  always root-relative). Getting it wrong silently serves the wrong directory. See [DETAILS.md](DETAILS.md) § "Path
  handling gotchas".
- **`LocalPosixVolume` delegates `.git` read paths to the git module after `resolve()`** (`list_directory`,
  `get_metadata`, `open_read_stream`); mutation methods reject virtual paths via `git::is_virtual`. The hook order
  (`resolve()` then `try_route_*`) is fixed. See [DETAILS.md](DETAILS.md) § "Git delegation hooks".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
