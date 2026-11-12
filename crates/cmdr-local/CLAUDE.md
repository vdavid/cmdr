# cmdr-local

The local disk: `LocalPosixVolume` (the `Volume` over a `std::fs` root), the local reader, the one no-replace rename,
local volume space, and the mount probes. Tauri-free; the app builds every local volume with its `VolumeHost`.

## Module map

- **local_posix.rs** (the struct, the rename primitive, the query and mutation half of `impl Volume`), with
  **local_posix/scan.rs** (the copy-scan family), **local_posix/streams.rs** (bytes in and out), and
  **local_posix/links.rs** (reading and landing a symlink, `DETAILS.md` § "Landing a link"). A trait impl can't span
  files, so a moved method stays a one-line delegation to a `pub(super)` inherent body.
- **reading.rs** (the local reader the watcher and `tags.rs` read through too; `read_dir` + `lstat`) with
  **reading/bulk.rs** (macOS: the `getattrlistbulk` lister APFS and HFS+ take), **mutation.rs** (the listing patch a
  local mutation sends), **space.rs** + **live_space.rs**, and the OS probes: **filesystem_kind.rs**,
  **mount_table.rs**, **statfs.rs**, **nsurl.rs**, **linux_mounts.rs** (Linux), and **mounts/**, the one door to the
  mount table (`list`, `is_mount_point`) that callers ask instead of naming the OS.

## Must-knows

- **It reaches the app only through its `VolumeHost`** (`host.listings()`: pane patches, watch coverage, name order), a
  required constructor argument. ❌ Never default it to `VolumeHost::detached()` in production: the scan oracle's fast
  path turns off and panes go stale after every local copy. ❌ Never depend on `cmdr-listing` or the app.
- **`rename_local_exclusive` is the ONE no-replace rename**, and it degrades on a filesystem with no kernel flag (smbfs
  answers `ENOTSUP`). ❌ Never write a second `renamex_np` call: while `overwrite::rename_no_replace` was a separate
  copy, only it degraded, and every rename on a mounted SMB share failed (ERR-8RFN4). DETAILS § "Local renames are
  atomic-exclusive".
- **`write_from_stream` hands each file to its drive; `flush_writes` makes it durable.** The per-file sync is a plain
  `fsync`, and the device barrier comes from the volume's `DeviceBarriers` (every 2 s, and in `flush_writes`). ❌ A
  caller that reports a write done, or deletes a moved source, without `flush_writes` first loses data on eject. DETAILS
  § the durability decision.
- **Force-rename is an atomic replacement**, so `supports_atomic_replace_rename()` is true. Network backends whose force
  path deletes first must not inherit that answer.
- **❗ The two listers yield the same entries, field for field** (`reading_parity_test.rs` runs every cell through
  both): delete trusts `is_symlink`, hardlink dedup `inode`. A doubtful bulk record re-stats through `entry_from_lstat`,
  a doubtful stream re-lists through `read_dir` from scratch: ❌ never drop, invent, merge, or return a partial listing.
  `added_at` is filled on APFS/HFS+ whichever lister ran, and by `get_single_entry`. DETAILS § "The local reader".
- **`FileEntry::is_hidden` folds in `UF_HIDDEN` and root-only `/.hidden`** on top of the dot check, from data the
  listing already has (no extra syscall).
- **Feed `on_progress` in `list_directory`**: the stat loop runs on `spawn_blocking`, so it publishes into a tally the
  async side samples. DETAILS § "The local reader".
- **A test build answers space from `statvfs`, never NSURL** (#357). Gate test scaffolding on
  `any(test, feature = "testing")`, ❌ never `cfg(test)` alone: the app's tests turn `testing` on and need the same
  behavior. A gate that changes BEHAVIOR reads `TEST_FAST_PATHS` instead, so the engine's bench measures what ships.

Decisions, the reader, space, the rename's degradation, the tests, and the capped surface: `DETAILS.md`. Read it before
any non-trivial work here: editing, planning, reorganizing, or advising.
