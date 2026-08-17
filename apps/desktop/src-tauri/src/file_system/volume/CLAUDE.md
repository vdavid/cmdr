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
  `SmbVolume` with a `LocalPosixVolume`. Plain `register` is for explicit replacement only, and it replaces ONLY at the
  same root: two roots claiming one ID (a doubly-mounted filesystem) keeps the incumbent and logs, so registration
  order can't decide where a volume is rooted. `DETAILS.md` § "Key decisions".
- **A registry entry owns a SET of mount roots, one active.** An unmount drops a root via
  `VolumeManager::remove_root`, which promotes a survivor and unregisters ONLY on the last one; ❌ never `unregister` a
  volume just because one of its mounts went away, or a share mounted twice disappears on the first eject.
  `find_by_root` matches any known root, so compare `volume.root()` when you mean the ACTIVE one. `DETAILS.md` § "A
  volume ID owns a set of mount roots".
- **❌ Never probe a mount root for liveness.** Promotion is driven by evidence that arrives on its own: an unmount
  event, or `volume::note_root_failure` seeing a mount-is-gone errno. A `statfs`/NSURL probe on a wedged mount blocks
  30–120 s and once froze the app at launch (`volumes/DETAILS.md` § "Hung mounts").
- **Cross-volume copy flows only through `open_read_stream` / `write_from_stream`, chunk by chunk.** ❌ Never drain a
  `VolumeReadStream` or collect a remote file into a `Vec<u8>`; ❌ don't reintroduce `export_to_local` /
  `import_from_local`. `DETAILS.md` § "Streaming patterns".
- **Every mutation must call `notify_mutation`, `write_from_stream` included.** Its default is a no-op and SMB/MTP
  watcher events are lossy, so skipping it leaves a stale pane. `DETAILS.md` § "Mutation notification".
- **Capability flags default to the conservative answer** (`Err(NotSupported)` / `false`), so a new backend starts at
  `list_directory` + `get_metadata` and opts in. Read `DETAILS.md` § "Trait capability model" before overriding one:
  `create_directory_all`, `listing_watch_coverage`, `operations_are_local`, and `create_directory_errors_on_existing_dir`
  each gate behavior that breaks silently on a wrong answer. New backend? § "Building a new volume".
- **`capabilities()` is a pure fold of those predicates; ❌ never override it.** It publishes `is_writable` +
  `can_export` to the frontend as DATA, so an override would be the second source of truth it exists to retire. Grow the
  surface by adding a predicate and folding it in. `is_writable` reaches the user as button state, so a new writable
  backend owes both the override AND `conformance::assert_writability_matches_the_mutations_offered`.
- **`lane_key()` is the operation manager's serialization key** (default = the volume root). Override it when several
  `Volume`s share one physical resource.
- **On macOS, never size disk space from `statvfs` alone** (it ignores purgeable space). Use `get_space_info_for_path`,
  which falls back to `statvfs` on Linux.
- **A path from the UI is anchored by its CALLER (`cmdr_fs::volume::root_anchored`), ❌ never guessed at by the
  backend.** Panes send absolute paths, the transfer dialog's dest box volume-relative ones, and a leading `/` doesn't
  tell them apart. It's idempotent, so anchor without checking; skip it and `SmbVolume` fails a write into a subfolder
  before any I/O. `DETAILS.md` § "Path handling gotchas".
- **`LocalPosixVolume` has two fixed-order path hooks**, each silently serving the wrong directory when wrong:
  `resolve` (= `root_anchored`), then `.git` read delegation to the git module.
  `DETAILS.md` § "Git delegation hooks".
- **`eject.rs` stops a `LocalExternal` index BEFORE `diskutil` runs**: an open watcher or handle at unmount can wedge
  macOS FSKit (kernel-panic risk). `DETAILS.md` § "Eject".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
