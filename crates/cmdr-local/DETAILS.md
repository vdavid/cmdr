# cmdr-local details

Pull-tier docs for the local backend: decisions, the reader, space, the rename primitive, the tests, and the capped
surface. Must-knows live in `CLAUDE.md`. The trait shape, capability matrix, and streaming patterns every backend shares
are `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md`.

## Decisions

**Decision**: `LocalPosixVolume` is a crate of its own, like every other backend **Why**: so the write engine can depend
on a crate for every local primitive (the backend, the reader, the no-replace rename, space, the mount probes) without
depending on the app. Folding it into `cmdr-fs` was rejected: that crate is shared vocabulary every backend builds on,
and the local backend brings objc2/NSURL and the live-space memo with it.

**Decision**: `xattr` and `libc` are `cfg(unix)` dependencies **Why**: the backend is a POSIX one, and neither builds
for Windows the way it's used here. A Windows local disk is a sibling backend behind the same `Volume` trait, not `cfg`
arms through this one (`docs/notes/windows-port/crates-audit.md` § cmdr-local).

**Decision**: `LocalPosixVolume` takes a `VolumeHost` at construction, like every backend crate **Why**: its couplings
were the app's LOCAL machinery rather than a protocol: the pane patch after a mutation, the watch-coverage oracle, and
the panes' name order, plus space, macOS metadata, and the mount probes. The first three are `ListingHost` calls
(`directory_changed`, `local_watch_coverage`, `sort_listing`; the app answers them in
`apps/desktop/src-tauri/src/listing_host/backends.rs`), and the rest is OS-probe code that lives here, so the backend
names nothing in the app. `patch_listing_after_local_mutation` stays the backend's own: it `std::fs`-stats the changed
entry, which no backend on a protocol can do. The host is a required argument, ❗ never a default: a production volume
on `VolumeHost::detached()` would silently turn off the scan oracle's fast path and leave panes stale after every local
copy. The git portal is ❌ not a coupling: it reaches a repo's virtual trees through a route plus a listing overlay, and
`local_posix.rs` names git nowhere (`apps/desktop/src-tauri/src/file_system/git/DETAILS.md` § "Two seams, no hooks").
Seam rationale: `crates/cmdr-fs/src/volume/host/DETAILS.md`.

**Decision**: a `Volume` method's blocking work goes to an ambient `tokio::task::spawn_blocking`, not to
`self.host.runtime()` **Why**: a `Volume` method runs on its CALLER's runtime, as every backend's does (SMB, SFTP,
WebDAV, and archive spawn the same way): the trait is runtime-agnostic, and the caller (the write engine, a pane
listing) already chose which runtime polls the future. Routing only the blocking half to the host's would split one call
across two pools for nothing; in production both are the app runtime. The write engine's own spawn rule
(`crates/cmdr-operations/src/host/CLAUDE.md`) covers the engine, not the backends it drives.

**Decision**: the mount probes the engine and `volumes*/` ask live here, and their callers name this crate **Why**:
they're the same OS-probe code the backend's own answers need (the `getfsstat` snapshot in `mount_table.rs`, `statfs` in
`statfs.rs`, `/proc/mounts` in `linux_mounts.rs`, the filesystem behind a path and whether it's a network one in
`filesystem_kind.rs`), and none of them knows the app. The platform-free classification they feed (`FilesystemKind`,
`MaxFileSize`, `mount_is_local_disk`) is `cmdr_fs::filesystem_kind`. What stays app-side is policy over the snapshot:
which rows the switcher shows, and whose mount is another account's own
(`apps/desktop/src-tauri/src/volumes/platform/macos/mounts.rs::mount_is_private_to_another_user`).

**Decision**: `write_from_stream` hands each file to its drive (`cmdr_fs::durable::sync_to_drive`, a plain `fsync` on
macOS) and notes it and its parent directory in the volume's `DeviceBarriers`; `Volume::flush_writes` runs the closing
pass (each noted directory's `fsync`, then one `F_FULLFSYNC` per device written). **Why**: Every cross-volume copy/move
that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one method. A bare `file.flush()`
is a userspace no-op on a raw `std::fs::File`, so without a sync the bytes would sit only in the OS page cache when the
op reports "complete", letting the user eject or sleep and lose data (on a move, from both sides). A plain `fsync` stops
at the drive's cache on macOS, though, and the barrier past it costs ~4 ms a file on an internal SSD and far more on a
spinning disk, which made a many-small-file import ~30x slower than Finder (#1145). One barrier covers every file handed
over before it, so the volume barriers on an interval (2 s) and in `flush_writes`, and each directory gets one `fsync`
per pass however many files it gained (it used to be one per file). The caller owes `flush_writes` before it reports a
write done or deletes a source: the volume copy, the cross-volume move (before each sweep), archive move-out, fresh
compress, the remote archive upload, and the drag-out fulfillment all run it. The tracker lives on the volume, so two
operations writing through one volume share it, which only adds coverage: a barrier for one op covers the other's
earlier files, and each still runs its own close. The per-file sync and the directory syncs are best-effort on error (a
failure logs under `target: "write_durability"`); the closing barrier is what answers, and its refusal is an `IoError`
naming the item it went through. Non-local backends keep the default `Ok`: durability there is the device's or server's
concern once it answered. The rule and its measurements: `crates/cmdr-operations/src/transfer/DETAILS.md` § Durability.
Pinned by `local_posix_durability_test.rs` (content through the finish path, and no barrier until `flush_writes`, then
one). Unknown-length writes take the same path.

**Decision**: `LocalPosixVolume` overrides `is_directory` and `entry_kind` with a bare `lstat` **Why**: `get_metadata`
also stats a link's target and looks up owner names, none of which answers "what is at this path". Both answers see a
link as the link, which a move engine depends on (`crates/cmdr-operations/src/transfer/DETAILS.md` § "Symlinks are
opaque to a move"); ❌ don't route either through `get_metadata`, whose `is_directory` is true for a link to a folder.

**Decision**: `LocalPosixVolume` overrides `list_directory_as_source`, and `scan_subtree` filters its walk the same way,
leaving out the AppleDouble sidecars of a FAT or exFAT folder **Why**: the kernel serves `._name` as `name`'s xattrs
there, so a cross-volume copy that also streamed it landed a stray hidden file. The pane's `list_directory` still shows
them, since they're on disk. A level with no `._` name never leaves the async task. `with_sidecar_probe` lets a test
play a FAT volume on APFS. Rule: `crates/cmdr-fs/src/apple_double.rs`; who lists as a source:
`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A source is listed as a source".

## A consumer's test build is a test build here too

`cfg(test)` is set only while this crate compiles its OWN tests, so the app's tests would otherwise run the production
shape of everything below. The `testing` feature (the app turns it on through a dev-dependency) widens exactly what the
app's test build needs, the reach items with `any(test, feature = "testing")` and the behavior gates through
`TEST_FAST_PATHS` (the decision below):

- `space::get_volume_space` answers from `statvfs` (§ "Local volume space"): without it, the #357 stall comes back in
  the write engine's transfer and archive move-out suites and the app's ADB suites.
- `PROGRESS_SAMPLE_INTERVAL` is 1 ms (§ "The local reader").
- `LocalPosixVolume::with_sidecar_probe`, which the write engine's `copy_sidecar_tests.rs` plays a FAT folder with.
- `reading::list_directory_with` / `Lister` / `added_at_by_path`, which force a lister for the parity cells and
  `examples/listing_bench`.

The first two are `cfg!` branches rather than `#[cfg]` items, so both arms compile in every build and neither leaves
dead code behind in the other.

**Decision: the two behavior gates read `TEST_FAST_PATHS` (`lib.rs`), which also requires `debug_assertions`.** Why:
Cargo builds a bench target with its package's dev-dependencies, so the engine's Criterion bench
(`crates/cmdr-operations/benches/transfer_benchmarks.rs`) compiles this crate with `testing` on. Gated on `testing`
alone, the bench measured the fast paths, and `stream_large_file` ran 17% faster than what ships
(`docs/notes/transfer-extraction-baseline.md` § "Criterion, after"). A separate cargo feature can't make the cut: the
engine's own tests need the `statvfs` answer (#357), and one package's tests and benches share one dev-dependency
feature set. The `bench` profile has no `debug_assertions` and every test profile does, so the cut falls exactly between
them. The reach items (`with_sidecar_probe`, `list_directory_with`) stay on `testing` alone: the bench needs them to
compile, and they change no behavior. (verified with a temporary
`const _: () = assert!(cfg!(feature = "testing") && !TEST_FAST_PATHS)`:
`cargo bench -p cmdr-operations --bench transfer_benchmarks -- --test` built and passed,
`cargo check -p cmdr-operations --tests` refused it, 2026-10-17)

## The local reader

`reading.rs` is the local reader: `list_directory_core` (every entry of a folder, built into `FileEntry`s and sorted by
the host's `sort_listing`), `get_single_entry` (one path), and `supported_dates_of` (which dates a folder's listing
carries, `LocalPosixVolume::supported_dates`'s answer). `LocalPosixVolume` lists and stats through it, and so do the
paths that read a local directory with no volume in hand: the app's watcher (its unregistered-volume fallback and its
per-entry re-stat) and `tags.rs`. It's the local backend's reader, not the listing engine's, which is why it sits here
and `cmdr-listing` only sees the `FileEntry`s it returns. `mutation.rs` is the other half:
`patch_listing_after_local_mutation`, what a successful local mutation means for the listing cache
(`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Mutation notification"). The app's `create.rs` sends its
instant row for mkdir, mkfile, and paste-as-file through it too.

### Two listers, one entry

- **`read_dir`** (`read_dir_entries`): every name, then one `lstat` per entry, plus one `stat` of a link's target for
  `is_directory`. Linux, and every macOS filesystem but APFS and HFS+.
- **`getattrlistbulk`** (`crates/cmdr-local/src/reading/bulk.rs`, macOS): one syscall per 64 KiB batch, every attribute
  an entry needs plus the added date in each record, decoded through `cmdr_fs::attr_buf` (the one decoder of that
  layout; `crates/cmdr-fs/DETAILS.md` § "`attr_buf`: the packed attribute layout"). At 100k entries, 1.4× faster warm
  and 1.5–2.7× cold-ish depending on load (`docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md` § "Production
  lister").

**How one is picked.** `bulk::read_entries` opens the folder (`O_DIRECTORY`, the open `opendir` does, so access and TCC
errors are today's) and asks `fstatfs` on that fd: bulk iff `f_fstypename` is `apfs` or `hfs` (`bulk_eligible`).

- **Decision**: an allowlist of filesystem type names, per listing, no cache. **Why**: speed, not support, is the
  question. smbfs supports bulk (a `VOL_CAP_INT_ATTRLIST` probe says yes) but runs it 2.2× slower (it re-enumerates over
  the wire where `lstat` hits its attribute cache); `MNT_LOCAL` alone would admit exFAT, FAT, NTFS, and FUSE; exFAT and
  FAT get bulk through VFS emulation with nothing gained, and FAT's allocated size reads 0; an unknown filesystem keeps
  the proven path. `fstatfs` on the open fd is about a microsecond, answers the root volume listing a folder on another
  filesystem by construction, and can't go stale on a remount that reuses an `fsid`.
- `only_apfs_and_hfs_take_the_bulk_lister` is the only automated proof that FAT and smbfs stay on `read_dir` (no CI box
  mounts either, and ❌ no FAT image: an FSKit `msdos` unmount once panicked a Mac).

**Which dates a listing claims.** `supported_dates_of(dir)` is `LocalPosixVolume::supported_dates`'s answer, run on
`spawn_blocking` beside the listing: Added iff `bulk_eligible` (the lister's own predicate), Created iff the folder's
`stat` carries a birth time (macOS always; Linux when `statx` returns `STATX_BTIME`). The cross-backend matrix and the
conformance rule are `crates/cmdr-fs/DETAILS.md` § "Which dates each volume lists".

- **Decision**: claim by construction from what the listers fill, ❌ not from `getattrlist(ATTR_VOL_ATTRIBUTES)`.
  **Why**: a claim that disagrees with the filler either strips a Created date the lister shows (smbfs, exFAT) or claims
  an Added it never fills (an unknown filesystem that validates `ADDEDTIME` stays on `read_dir`). The benchmark's
  `volcaps` agrees with the claim on every filesystem measured
  (`docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md`). `FilesystemKind` is a test oracle, ❌ not the decider.

**One entry builder.** `entry_from_metadata` builds an entry from an `lstat`; `entry_from_lstat` (the `read_dir` path,
and a doubtful bulk record) and `get_single_entry` both go through it, so the listing and the watcher's re-stat can't
drift. `minimal_row` is the one shape for an entry nothing could be read about (name, link bit for the icon, nothing
else). Bulk builds its trusted entries from the record, with the same rules: `is_directory` from a link's target (one
`stat`), everything else from the link; `permissions = (ACCESSMASK & 0o7777) | type_bits(OBJTYPE)` (APFS's mask already
carries `S_IFMT`, HFS+'s may not); `physical_size` from `ALLOCSIZE`, which matches `st_blocks * 512` on APFS and HFS+
(compressed and sparse files included).

**❗ Never drop, invent, or reshape an entry.** Write paths act on this listing's fields: delete stops descending at
`is_symlink` (a link reported as a plain folder deletes its TARGET's children), hardlink dedup and rollback's
self-collision check trust `inode`. So the bulk path ends one of three ways:

- Every record trusted: their entries.
- A record in doubt (`ATTR_CMN_ERROR` set, or a required attribute's returned bit clear: type, mtime, owner, group,
  mode, flags, file id, a folder's mount status, a non-folder's link count, allocated size, and length): that ONE entry
  is `entry_from_lstat`, today's answer. If its `lstat` fails too, today's minimal row when an unerrored record still
  has a valid type (it picks the icon), else the stream is in doubt. Birth and added time are optional: invalid reads
  `None`, never 0.
- The stream in doubt (a `BadRecord`, a nameless or untyped unreadable record, `EINTR`, `EACCES`, `ENOTSUP`, `EINVAL`):
  discard every bulk entry, reset the progress tally, and list again through `read_dir` from scratch. `bulk::Failure`
  carries no entries, so a partial or merged listing can't be returned. `EACCES` is the common one: a readable but
  unsearchable (`0444`) folder fails the whole call where `read_dir` lists minimal rows. `EINTR` isn't retried in place:
  nothing says a resumed call can't skip a batch. Any other errno reaches the caller, as a `read_dir` iteration error
  does.

**Mount points and firmlinks** come back describing the folder they COVER (the stub's owner, mode, and times). Detected
by `ATTR_DIR_MOUNTSTATUS & DIR_MNTSTATUS_MNTPOINT` or `SF_FIRMLINK` in the flags, they're built wholly from
`get_single_entry` (added date included), the minimal row if that fails, ❌ never the stub. At `/`, 9 of 22 entries take
this path.

**The added date never depends on which lister ran.** On a bulk-eligible folder, the `read_dir` lister (forced, or the
re-list after a doubtful stream) and a doubtful record fill `added_at` with one `getattrlist(ATTR_CMN_ADDEDTIME)` each
(`bulk::added_at_by_path`, ~3.5 µs, paid only on those rare paths), and `get_single_entry` does the same when its PARENT
is bulk-eligible (`statfs` of the path itself would follow a final symlink). Elsewhere, no `added_at`. HFS+ records none
for a symlink (verified on macOS 27.0.1, `reading_parity_test` on a `testing::disk_images` HFS+ image, 2026-10-22).

- **Decision**: that parent `statfs` runs on every `get_single_entry`, ❌ no per-device eligibility cache. **Why**: a
  cache keyed on `st_dev` and cleared by a kqueue `EVFILT_FS` thread was built, measured, and reverted: it saved ~1 µs
  per call (APFS 4.5 → 3.3 µs; smbfs answers the warm `statfs` from its client cache, so no round trip either), which
  doesn't pay for a resident thread and device-number-reuse races. Numbers and the revisit condition:
  `docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md` § "What a re-stat costs, and why it keeps its `statfs`".

**Intended differences from a `read_dir` listing:**

- A sandbox-protected entry (`/private/var/db/...`, where `lstat` gets `EPERM`) lists as what it is from bulk, where
  `read_dir` shows a dateless "file" minimal row. Opening it still says "no permission".
- A name that vanishes mid-enumeration drops out (the kernel's next batch omits it), where `read_dir` shows a minimal
  row for a name that vanished between `readdir` and `lstat`. The watcher's event reconciles either way.

**What's unverified or untested here:**

- TCC (Desktop, Documents, Downloads, iCloud Drive): `open(O_DIRECTORY)` is the gate `opendir` hits, so a denial is the
  same `EPERM` at open, but neither lister has run under the app's own TCC identity. On David's QA list.
- Dataless (evicted iCloud) files: every requested attribute is metadata the kernel keeps for an evicted file, and
  nothing reads content or xattrs, so nothing should materialize. Unverified (no evicted file at hand, 2026-10-22).
  Enumerating a dataless FOLDER materializes it with either lister.
- Non-UTF-8 names never reach bulk (APFS and HFS+ refuse them with `EILSEQ`); the name decodes through `to_string_lossy`
  and the path joins the raw bytes, as `read_dir` does.

**Progress.** The pane's "Loaded N files…" readout needs `on_progress` fed while the stat loop runs (why it matters:
`crates/cmdr-listing/DETAILS.md` § "Local listing progress"), but the local backend can't take the callback where the
work happens. `on_progress` is `Sync` but not `Send`, and `LocalPosixVolume` runs its stat loop on `spawn_blocking`,
which demands `Send + 'static`. So the two halves are split:

- `list_directory_core_with_tally` publishes into a `ListingTally` as it stats, one relaxed atomic bump per entry,
  unthrottled. That's free next to the `stat` it accompanies.
- `LocalPosixVolume::list_directory` samples the tally every `PROGRESS_SAMPLE_INTERVAL` through
  `cmdr_fs::volume::listing_progress::report_while`, and calls `on_progress` from there. The callback never leaves the
  async task that owns it. The git portal's commit walk uses the same helper.

Two details the helper depends on. Its `select!` is `biased` so a listing that finished during a tick returns its real
result rather than spending another sample on an approximate count. And a snapshot of zero is dropped rather than
emitted, because the blocking pool may not have picked the task up yet and "Loaded 0 files..." is worse than the
"Opening folder..." it would replace.

Putting the throttle in the sampler rather than the stat loop means one place decides how often the number changes, and
it's the place that knows it's driving a UI. `PROGRESS_SAMPLE_INTERVAL` is 200 ms in production and 1 ms in a test
build. Even 1 ms guarantees no tick: under load the async side can wake only after a fast read is done (#755, 2 in 40
red under 32 CPU hogs). So `listing_a_local_directory_reports_progress_while_it_reads` pins the wiring by holding the
read after its first entry until a tick lands, through `LocalPosixVolume::with_listing_hook` (a `testing`-gated hook the
tally runs after each entry it counts). ❌ Don't pin it with a bigger folder: no size outlasts a starved sampler.

SMB and MTP wire `on_progress` through their own listing loops directly, having no `spawn_blocking` hop to cross.

**Metadata tiers.** Tiers 1-2 are fetched eagerly (stat + uid→name), tiers 3-4 deferred. With 50k+ files, each metadata
piece has a different cost: Tier 1 (name, size, dates, permissions) is free from a single `stat()`; Tier 2 (owner name,
symlink target) is ~1 μs and cacheable; Tier 3 (macOS Spotlight/NSURL metadata) costs ~50-100 μs/file; Tier 4 (EXIF,
PDF) costs 1-100 ms+ and reads file content. The full tier table: `docs/notes/file-metadata-tiers.md`.

The added date rides with the core listing (§ "Two listers, one entry"), and nothing reads NSURL per entry. Finder tags
are deferred, and `list_directory_core` never reads them: `crates/cmdr-listing/DETAILS.md` § "Finder tags".

## Local volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey`
(falls back to `NSURLVolumeAvailableCapacityKey`) on macOS, and `statvfs` on Linux. Returns `None` for non-existent
paths.

`space.rs` holds it, beside `get_space_info_for_path` (what `Volume::get_space_info` returns) and the `statvfs` reading
Linux uses and macOS falls back to.

**Decision**: on Linux, `get_volume_space` is THE `statvfs` call: the app's `volumes` seam re-exports it for the IPC
command, and the space poller's unregistered-path fallback calls it, so ❌ never write a second `statvfs` in the app.
**Why**: the app once kept its own copy, which derived `used` as `total - available` (counting root-reserved blocks as
used) while a `Volume`'s answer read `f_bfree`, so the switcher and a transfer's pre-flight could disagree on one disk
(#465).

**Decision**: a test build answers from `statvfs` (`space::get_space_info_statvfs`), and only the NSURL tests in
`space.rs` call the real `nsurl_volume_space`. **Why**: the first NSURL call in a process runs `CFBundleGetMainBundle`,
which `readdir`s the test binary's own directory. On a bloated `target/debug/deps` (457k entries) with ~16 nextest
processes doing that at once, transfer, archive move-out, and ADB tests reaching the copy pre-flight or
`LocalPosixVolume::get_space_info` blew their 5 s waits (#357). Every space path (`get_space_info_for_path`,
`live_volume_space`, the copy pre-flight, the IPC command) funnels through `get_volume_space`, so that one function is
the seam. "A test build" means this crate's own and the app's (through `testing`, § "A consumer's test build is a test
build here too"). `space::tests::a_test_build_answers_a_space_query_without_nsurl` pins it. Verified 2026-10-14 with a
tripwire on every NSURL resource lookup: none of the 54 tests in the five suites #357 named reaches one.

### Live volume space

`live_space.rs::live_volume_space(path)` is what the space poller reads every 2 s (through `Volume::get_live_space_info`
and the poller's unregistered-path fallback). It returns the same important-usage figure as `get_volume_space`, mostly
at `statfs` cost:

- **Why**: on the boot volume the important-usage query costs ~6.5 ms of CPU (5.9 of it system time, plus work in the
  `deleted` daemon), against ~0.01 ms for `statfs`. The poller asks for the boot volume permanently (the low-space
  check), so that alone was ~0.33% of a core. On external and SMB volumes the query is ~0.02 ms; the cost is
  CacheDelete's purgeable accounting on the boot container.
- **How**: important-usage free = `statfs` free + purgeable, and ordinary writes and deletes move `statfs` and the
  important-usage figure by the same bytes while purgeable holds still. So each filesystem (keyed by mount point) keeps
  an anchor: the last important-usage reading and the `statfs` taken beside it. A live reading is the anchor plus how
  far `statfs` moved since, clamped to the drive.
- **When it asks again** (`plan`): no anchor yet; the anchor is 60 s old; `statfs` drifted by 1/1,000 of the drive (one
  readout step: past that it may be macOS purging, which raises `statfs` free while the important-usage figure holds);
  the total changed; or a write operation settled since (`expect_space_change`, from the app's
  `TauriEventSink::emit_settled`), because deleting a file a Time Machine local snapshot still holds frees purgeable
  space `statfs` never sees.
- **Evidence** (verified on macOS 27.0, a Swift probe sampling all three figures every 2 s for 5 min, 2026-09-27):
  `statfs` free equals `NSURLVolumeAvailableCapacityKey` to the byte, purgeable held at 10.692 GB through 3.3 GB of
  writes, and the anchor-plus-delta figure stayed within 0.4 MB of the real one (one sample 10 MB off: a write inside
  the call). Numbers and before/after CPU: `docs/notes/performance/space-poll-cost-2026-09-27.md`.
- ❌ **Only for readouts that poll.** Copy validation and the transfer pre-flight act on the figure once, so they keep
  calling `get_volume_space`, which always asks.
- The anchors are a process-wide memo (`ANCHORS`), the one static here: a cache of an OS answer the poller and
  `expect_space_change` share, not engine state.

## Local renames are atomic-exclusive

`LocalPosixVolume` routes every non-forced rename through the shared atomic-exclusive primitive,
`rename_local_exclusive`. This applies equally to `/`, attached disks, Dropbox, iCloud, and other local POSIX roots
registered with non-root volume IDs. Forced renames retain normal POSIX replacement semantics because the caller
explicitly authorized replacement.

**There is exactly one implementation of it**, and the write engine's `overwrite::rename_no_replace` (`cmdr-operations`)
is a one-line delegation to it, as is the in-flight sweep's local landing. Both names have to stay one function: while
they were two, only the copy side degraded on a filesystem lacking the kernel flag, so the pane's own renames failed on
every mounted SMB share while a copy landing in the same folder succeeded (ERR-8RFN4, app 0.45.1).

**The kernel flag is not universal.** `renamex_np(RENAME_EXCL)` and `renameat2(RENAME_NOREPLACE)` are APFS, HFS+, and
ext4; macOS's smbfs answers `ENOTSUP` (errno 45, verified twice against Synology SMB mounts: macOS 26.6.2 from the
ERR-8RFN4 bundle's log, and macOS 27.0 by a direct `renamex_np` probe where APFS took the flag, the mount returned errno
45, and the check-then-rename fallback landed, 2026-09-17), and other non-native mounts answer `EINVAL` or `ENOSYS`.
Those three degrade to a `symlink_metadata` check then a plain `rename(2)`; ❌ every other errno reaches the caller
unchanged, or a refusal the filesystem meant becomes a second, quieter way to fail. The degradation is racy and
deliberately so: a lost race refuses, where a bare `rename(2)` would clobber silently and unrecoverably.

**FAT32 and exFAT are NOT on the degraded path**, which is worth knowing before sizing any work that calls itself "the
USB-stick case": both take the flag and answer `EEXIST` (verified on macOS 27.0 by a direct `renamex_np` probe against
freshly created FAT32 and exFAT disk images alongside HFS+ and APFS controls, all four identical, 2026-09-19). What
actually reaches the fallback is macOS-mounted SMB and FUSE mounts.

**The check is three-valued, and ❗ that is the whole point.** `name_state` folds the `symlink_metadata` result into
`Occupied` / `Free` / `Unknown(err)`, and `rename_if_free` renames on `Free` alone. A stat that failed for any reason
other than the name being free has proved nothing about the name, so `Unknown` refuses and carries the filesystem's own
errno out. Collapsing it into "free" (which `.is_ok()` did) turns every transient stat failure into a silent,
unrecoverable clobber, on exactly the flaky mounts that reach this path in the first place. The enum exists so the fold
can't be written as a bool again.

The `ENOTSUP` path can't be reached on a filesystem a test can mount, and `testing::disk_images` is not for minting a
FAT image, so `rename_exclusive_with` takes the attempt as a parameter and `local_posix_test.rs` injects the errno. The
`Unknown` arm needs no seam at all: a stat and the rename beside it resolve the same path through the same permissions,
so nothing a test can set up fails one without failing the other, and only a transient failure separates them.
`local_posix_test.rs` hands `rename_if_free` the verdict directly instead.

## Landing a link

`read_link_target` and `create_symlink` are the local half of `Volume`'s link seam, and they store and read a target
verbatim, ❌ never resolved (the transfer side: `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A copy
carries a link as the link"). `readlink`'s `EINVAL` means "no link here" (`None`). `can_hold_links` answers for the
backend, but exFAT under a local volume refuses with `EPERM`, `ENOTSUP`, or `EOPNOTSUPP`, so `create_symlink` turns
those into `NotSupported`. ❗ `ENOTSUP` and `EOPNOTSUPP` are one number on Linux and two on macOS, so that test compares
values: a `matches!` pattern naming both doesn't compile on Linux. The copy scan counts a nested link, so totals agree
with the preview and `scan_walk`.

## Testing

- `local_posix_test.rs`: real-FS cells (write ops, symlinks, copy, space info, the rename's degradation) on scratch
  dirs, against a detached or recording host.
- `local_posix_conformance_test.rs`: the shared `cmdr_fs::volume::conformance` assertions this backend can run, the way
  every crate backend keeps its own. The roster and what each defends: `crates/cmdr-fs/DETAILS.md` § "The shared
  assertions in `volume::conformance`".
- `reading_test.rs`: the reader on its own; the inline `tests` modules in `space.rs`, `live_space.rs`, `mount_table.rs`,
  and `linux_mounts.rs` pin the probes.
- `reading_parity_test.rs`: what the listers yield, field for field, each listed entry equal to its `get_single_entry`
  re-stat (every field, `added_at` included, through `Debug`). Each field a write path acts on also gets a cell against
  `lstat` itself, since parity alone can't see both readers going wrong together. Every cell runs once per entry of
  `LISTERS` (bulk and `read_dir` on macOS, `read_dir` on Linux) through `reading::list_directory_with`. The one known
  difference: `/.hidden` at a volume root, which the re-stat never applies. Its `real_image` module (a mount point's
  parent, the fixture on HFS+) runs in `pnpm check disk-images`.
- `crates/cmdr-local/src/reading/bulk_test.rs`: the bulk lister's own policy over hand-built `attr_buf::Record`s (a
  doubtful record is its `lstat`, a mount point its re-stat) and a broken stream (a real first batch, then a bad record
  or an errno, injected through `read_entries_with`'s batch source): the result must equal the `read_dir` listing
  exactly.
- **The cells that need the app's real host stay app-side**, in
  `apps/desktop/src-tauri/src/listing_host/tests/local_posix_host_test.rs`: the watch-coverage answer following the real
  FSEvents watcher, the panes' name order, and a mutation reaching the pane through the app's composer. They assert on
  `listing_host::backends::AppListings`, so they live beside it.

## The public surface is capped

`index-crate-isolation` holds this crate to its measured surface (13 root promises, ten public modules, 41 items, no
handle), with no headroom. A backend's API is the `Volume` trait, `cmdr-fs`'s promise, so everything counted serves an
app read of a local disk that doesn't go through a volume:

- **The root**: `LocalPosixVolume`, `rename_local_exclusive` (the engine's landings and the in-flight sweep), and
  `rename_volume_error` (the app's single rename, which names the right path in its error).
- **`reading`** (`list_directory_core`, `get_single_entry`; `list_directory_with`, `Lister`, and `added_at_by_path` are
  `testing`-gated, for the parity cells and `examples/listing_bench`) and **`mutation`**
  (`patch_listing_after_local_mutation`): the watcher, `tags.rs`, and `create.rs`'s instant row.
- **`space`** (`get_space_info_for_path`, `get_volume_space`) and **`live_space`** (`live_volume_space`,
  `expect_space_change`): the index's tier-2 progress, the copy pre-flight, the IPC command, and the space poller.
- **`mounts`** (`list`, `is_mount_point`): the one door to the mount table, per OS behind `mounts/platform/`, so the
  write engine's mount guards never name the OS. Raised the ceiling by one module and two items (2026-11-11): the wider
  surface is the better design, because every caller that moves onto it is a two-arm OS dispatch gone, and once the
  app's volume code reads its records through it too, `mount_table` and `linux_mounts` can go private.
- **The mount probes**: `filesystem_kind` (seven, counting `probe_mount_for_path`'s three platform arms), `mount_table`
  (four), `statfs` (three), `nsurl` (two), and `linux_mounts` (16, the `/proc/mounts` and `mountinfo` readers the app's
  Linux volumes arm builds on). The counter reads source, so each platform's module counts whatever the build target.

A new item names which of these callers it serves, or goes behind `testing`.
