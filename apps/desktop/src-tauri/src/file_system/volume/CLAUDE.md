# Volume abstraction

The `Volume` trait's app-side wiring: backends, the `VolumeManager` registry, eject. Every file system operation goes
through a `Volume`, with **paths relative to the volume root**.

## Module map

- `mod.rs` re-exports all of `cmdr_fs::volume`; the trait itself is `crates/cmdr-fs/src/volume/mod.rs`.
- `manager.rs` (+ `manager/archive_routing.rs`): the registry behind `get_volume_manager()`.
- `backends/` (`backends/CLAUDE.md`), `eject.rs` (macOS+Linux teardown by kind), `friendly_error/` (typed, word-free
  classification, in `crates/cmdr-fs`).

## Must-knows

- **A site passing a path calls `VolumeManager::resolve(volume_id, path).await`, ❌ never `get(volume_id)`.** `resolve`
  routes a `.zip`-crossing path to a read-only `ArchiveVolume`, path UNCHANGED; it's async because a remote `.zip`
  probes the network. `resolve_local_only` is for the ONE caller that can't `.await`.
- **Watcher-pre-registered volumes go in via `register_if_absent`**, else the FSEvents watcher overwrites an
  `SmbVolume` with a `LocalPosixVolume`. Plain `register` is for explicit replacement only.
- **Cross-volume copy flows only through `open_read_stream` / `write_from_stream`, chunk by chunk.** ❌ Never drain a
  `VolumeReadStream` or collect a remote file into a `Vec<u8>`; ❌ don't reintroduce `export_to_local` /
  `import_from_local`. `DETAILS.md` § "Streaming patterns".
- **Every mutation must call `notify_mutation`, `write_from_stream` included.** Its default is a no-op and SMB/MTP
  watcher events are lossy, so skipping it leaves a stale pane. `DETAILS.md` § "Mutation notification".
- **Capability flags default to the conservative answer** (`Err(NotSupported)` / `false`), so a new backend starts at
  `list_directory` + `get_metadata` and opts in. Read `DETAILS.md` § "Trait capability model" before overriding one:
  `create_directory_all`, `listing_watch_coverage`, `operations_are_local`, and `create_directory_errors_on_existing_dir`
  each gate behavior that breaks silently on a wrong answer. New backend? § "Building a new volume".
- **`lane_key()` is the operation manager's serialization key** (default = the volume root). Override it when several
  `Volume`s share one physical resource.
- **On macOS, never size disk space from `statvfs` alone** (it ignores purgeable space). Use `get_space_info_for_path`,
  which falls back to `statvfs` on Linux.
- **`LocalPosixVolume` has two fixed-order path hooks**, each silently serving the wrong directory when wrong: a
  three-way `resolve` branch for absolute paths (the frontend sends those), then `.git` read delegation to the git
  module. `DETAILS.md` §§ "Path handling gotchas", "Git delegation hooks".
- **`eject.rs` stops a `LocalExternal` index BEFORE `diskutil` runs**: an open watcher or handle at unmount can wedge
  macOS FSKit (kernel-panic risk). `DETAILS.md` § "Eject".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
