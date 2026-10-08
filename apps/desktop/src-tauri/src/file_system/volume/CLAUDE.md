# Volume abstraction

App-side backend, registry, and eject wiring. Every operation goes through a `Volume`, **relative to its root**.

## Module map

- `mod.rs` re-exports all of `cmdr_fs::volume`; the trait is `crates/cmdr-fs/src/volume/mod.rs`.
- `manager.rs` (+ `manager/`: `routing.rs`'s two routes, the mount-root set): the registry behind
  `get_volume_manager()`.
- `mount_registration.rs`: the one way a MOUNT becomes a registered volume (startup sweep, mount watcher, adoption).
- `backends/` (own `CLAUDE.md`), `eject/` (teardown by kind, macOS+Linux), `drive_release/` (the index stop/start
  gate), `friendly_error/` (in `cmdr-fs`).

## Must-knows

- **A site passing a path calls `VolumeManager::resolve(volume_id, path).await`, ❌ never `get(volume_id)`.** It routes
  `.zip`-crossing and `.git/<category>/` paths to their backends, path UNCHANGED, and answers `is_routed()`. The sync
  `resolve_local_only` serves the ONE caller that can't `.await`.
- **❗ Registration must never be cleverer than resolution.** A mount is registered through `mount_registration`, ❌
  never a caller's own `LocalPosixVolume::new` + `register`: resolution mints an ID for any mount this account can reach,
  so the sweep takes all of those (switcher rows are a separate, stricter question). Its net, adoption, takes a live
  mount ONLY when that mount derives exactly the requested ID, ❌ never on a prefix or path-shape test.
- **Watcher-pre-registered volumes use `register_if_absent`** (else FSEvents overwrites an `SmbVolume`). `register`
  replaces only at the SAME root; an edited root uses `replace_root_in_place`.
- **A registry entry owns a SET of mount roots, one active.** `remove_root` promotes a survivor and unregisters ONLY
  on the last; ❌ never `unregister` over one mount, or a share mounted twice vanishes on the first eject.
  `find_by_root` matches ANY known root, so compare `volume.root()`. A REMOVED volume is retired
  (`Volume::retirement`), standing its watcher and reconnect loop down; a replace isn't.
- **❌ Never probe a mount root for liveness**: one on a wedged mount blocks 30–120 s and once froze the app at launch.
  Promotion runs on evidence that arrives on its own: an unmount event, or `note_root_failure`'s mount-is-gone errno
  (`volumes/DETAILS.md` § "Hung mounts").
- **Work that must WAIT for a volume subscribes with `on_volume_arrival`, ❌ never polls the registry.** A listener
  gets the ID only and runs INSIDE the registration: return at once, hand work to a task.
- **Cross-volume copy flows only through `open_read_stream` / `write_from_stream`, chunk by chunk**, ❌ never draining
  a `VolumeReadStream` or collecting a remote file into a `Vec<u8>`. Every mutation must call `notify_mutation`,
  `write_from_stream` included: its default is a no-op and SMB/MTP events are lossy, so skipping it strands the pane.
- **Generated streams use `StreamLength::Unknown`, never `Known(0)`; unopted backends refuse before I/O.**
- **Capability flags default conservative** (`Err(NotSupported)` / `false`), so a backend opts in; several break
  silently when answered wrong (`is_writable` is button state). `capabilities()` is a pure fold, ❌ never overridden.
- **Two more rules each own a `DETAILS.md` section; read it before touching that area**: a row a pane sees that no
  volume holds is a `ListingOverlay`, ❌ never a `Volume` impl (§ Architecture), and a path from the UI is anchored by
  its CALLER, ❌ never the backend (§ Path handling gotchas).
- **Every app-side index stop of a removable drive and every non-root index start goes through `drive_release`**, ❌
  never a bare `Index::start_volume` / `rescan_volume` / `cover` / `stop_removable_volume`; every teardown goes through
  `run_teardown`. `eject/` stops through the gate BEFORE `diskutil` runs (FSKit wedge: kernel-panic risk), and an eject
  is **per PHYSICAL disk, ❌ never per volume**. ❗ `DiskMounts` and `HolderScan` each carry a "couldn't tell", ❌ never
  an empty one.

Every rule above owns a `DETAILS.md` section carrying its rationale and edge cases. Read the one you're touching, and
the rest before any non-trivial work here: editing, planning, reorganizing, or advising.
