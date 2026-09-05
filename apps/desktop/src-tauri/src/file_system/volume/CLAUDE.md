# Volume abstraction

The `Volume` trait's app-side wiring: backends, the `VolumeManager` registry, eject. Every file system operation goes
through a `Volume`, **paths relative to the volume root**.

## Module map

- `mod.rs` re-exports all of `cmdr_fs::volume`; the trait itself is `crates/cmdr-fs/src/volume/mod.rs`.
- `manager.rs` (+ `manager/`: `routing.rs` and the two routes it dispatches to, the mount-root set): the registry
  behind `get_volume_manager()`.
- `backends/` (`backends/CLAUDE.md`), `eject.rs` (macOS+Linux teardown by kind), `friendly_error/` (in `crates/cmdr-fs`).

## Must-knows

- **A site passing a path calls `VolumeManager::resolve(volume_id, path).await`, ❌ never `get(volume_id)`.** It routes
  a `.zip`-crossing path to a read-only `ArchiveVolume` and a `.git/<category>/` path to a `GitPortalVolume`, path
  UNCHANGED, and answers `is_routed()`, which is what a reader wants unless it's about one backend. Match a
  `RoutedKind` only there. `resolve_local_only` is for the ONE caller that can't `.await`. `DETAILS.md` § "Resolving a
  path: the two routes".
- **Watcher-pre-registered volumes go in via `register_if_absent`**, else the FSEvents watcher overwrites an
  `SmbVolume` with a `LocalPosixVolume`. Plain `register` is for explicit replacement at the SAME root.
  `DETAILS.md` § "Key decisions".
- **A volume the registry REMOVES is retired** (`Volume::retirement`), so a backend's watcher and reconnect loop stand
  down. A replace deliberately doesn't. `DETAILS.md` § "Leaving the registry".
- **Work that must WAIT for a volume subscribes with `on_volume_arrival`, ❌ never polls the registry.** A listener gets
  the ID only and runs INSIDE the registration, so it must return at once and hand real work to a task. Today's one
  subscriber is the in-flight temp ledger's deferred sweep. `DETAILS.md` § "Telling someone a volume arrived".
- **A registry entry owns a SET of mount roots, one active.** An unmount drops one via `VolumeManager::remove_root`,
  which promotes a survivor and unregisters ONLY on the last; ❌ never `unregister` because one mount went away, or a
  share mounted twice disappears on the first eject. `find_by_root` matches ANY known root, so compare `volume.root()`
  for the active one. `DETAILS.md` § "A volume ID owns a set of mount roots".
- **❌ Never probe a mount root for liveness.** Promotion runs on evidence that arrives on its own: an unmount event,
  or `volume::note_root_failure` seeing a mount-is-gone errno. A probe on a wedged mount blocks 30–120 s and once froze
  the app at launch (`volumes/DETAILS.md` § "Hung mounts").
- **Cross-volume copy flows only through `open_read_stream` / `write_from_stream`, chunk by chunk.** ❌ Never drain a
  `VolumeReadStream` or collect a remote file into a `Vec<u8>`.

- **Every mutation must call `notify_mutation`, `write_from_stream` included.** Its default is a no-op and SMB/MTP
  watcher events are lossy, so skipping it leaves a stale pane.
- **Capability flags default to the conservative answer** (`Err(NotSupported)` / `false`), so a backend opts in.
  Several break silently when answered wrong, and `is_writable` reaches the user as button state: read `DETAILS.md`
  § "Trait capability model" before answering one. `capabilities()` is a pure fold published to the frontend as DATA;
  ❌ never override it.
- **A path from the UI is anchored by its CALLER (`cmdr_fs::volume::root_anchored`), ❌ never guessed at by the
  backend.** Panes send absolute paths, the transfer dialog's dest box volume-relative ones, and a leading `/` doesn't
  tell them apart. Idempotent, so anchor without checking. `LocalPosixVolume` then runs two fixed-order hooks,
  `resolve` (= `root_anchored`) and `.git` read delegation, each serving another directory when wrong. `DETAILS.md`
  §§ "Path handling gotchas", "Git delegation hooks".
- **`eject.rs` stops a `LocalExternal` index BEFORE `diskutil` runs**: an open watcher or handle at unmount can wedge
  macOS FSKit (kernel-panic risk). `DETAILS.md` § "Eject"; a new backend, § "Building a new volume".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
