# Indexing transports details

Read this before any non-trivial work in `indexing/transports/`: editing, planning, reorganizing, or advising.
Must-know invariants are in [CLAUDE.md](CLAUDE.md).

Each transport reuses the ENTIRE registry, writer, aggregator, `dir_stats`, and read-path machinery. Only how a volume
is enabled and how live changes arrive differ. The scan itself (the `Volume`-trait BFS for SMB/MTP, the local guarded
walker for local-external), the freshness state machine, and the scan-completion handler are owned elsewhere and pointed
to below — this area owns ENABLE + WATCH.

## SMB (`smb/`)

### The direct-smb2 gate (`smb/index.rs`)

Indexing an SMB share requires Cmdr's own smb2 (`direct`) session, NOT the macOS `os_mount`: `CHANGE_NOTIFY` watching
runs over smb2 anyway, and smb2 parallelizes listing far better than per-`readdir` round trips through the kernel mount.
An `os_mount` share is registered as a `LocalPosixVolume` on an `smbfs` mount (its `smb_connection_state()` is `None`);
a direct one is an `SmbVolume` returning `Some(Direct)`. `ensure_direct_smb` therefore: `Direct` → index now;
`Disconnected` → refuse (reconnect first); `os_mount` → trigger/await `upgrade_to_smb_volume_inner`, then re-check.

Every refusal is a TYPED `SmbIndexGateReason` (`NotRegistered` / `NotAnSmbVolume` / `UpgradeFailed` /
`CredentialsNeeded` / `Disconnected`) that crosses IPC as a snake_case tag, never a message substring (per
`.claude/rules/no-string-matching.md`). FDA-independent: SMB paths aren't TCC-protected, so `start_indexing_for_smb`
never routes through `should_auto_start_indexing`. Volume access is global (`crate::file_system::get_volume_manager()`,
a `LazyLock`, no `AppHandle` needed). `smb_volume_id_for_path` probes the mount (`get_smb_mount_info`) and keys by
`(server, port, share)`.

### Auto-resume on (re)connect (`smb/index.rs`)

`enable_drive_index` is the only path that STARTS an SMB index from scratch, but must not be the only path that
RE-registers one: `indexing::init` doesn't re-open persisted SMB DBs, so after any disconnect or restart an enabled NAS
index would stay dark (registry-absent = gray) until re-enabled by hand. So every SMB session-install success calls
`resume_smb_index_if_enabled(volume_id)` (fired from the volume backend; see `backends/DETAILS.md` § "Backend-autonomous
reconnect and index resume" for the trigger sites). It's fire-and-forget (spawns, so the async start never runs under a
reconnect/registry lock), a no-op if already active, and gated on the PERSISTED per-volume state (never the live
registry) via `smb_index_was_enabled`, which requires BOTH:

- **A completed scan is recorded** — `IndexStore::persisted_scan_completed(db_path)`, a `scan_completed_at` marker read
  off a short-lived READ-ONLY connection (never the delete-and-recreate `open`). A never-enabled share has no such DB,
  so it's never indexed uninvited.
- **The user hasn't turned indexing OFF** — the sticky `user_disabled` meta marker is absent. `disable_drive_index`
  KEEPS the DB (so a re-enable resumes fast) but writes this marker via `state::disable_drive_index_persist_intent`, so
  a reconnect never turns back on what the user turned off. Enabling (`start_indexing_for_smb`, both the manual command
  and the auto-resume hook) clears it; `forget_drive_index` deletes the whole DB. **The marker is written ONLY at the
  explicit user-disable command, NEVER inside `stop_indexing`** (which also runs on eject, unmount, an interrupted
  network scan, and the memory watchdog — marking there would suppress resume after a transient teardown). Its only
  consumer is this SMB gate.

The resumed index loads Stale: we weren't watching while disconnected, so a rescan — not the reconnect — restores Fresh.

### Live SMB watch → index (`smb/watch.rs`)

The SMB watcher (`file_system/volume/backends/smb_watcher.rs`) already turns `CHANGE_NOTIFY` into `DirectoryChange`s for
the open pane. This layer is a SECOND consumer so the persisted index stays correct under live mutation.
`notify_directory_changed` calls `apply_smb_change(volume_id, parent_path, &change)` **first, before any pane-listing
work** — ahead of the "no listing matches, bail" early-return — because the watcher runs for the whole volume's
lifetime, not just while a pane shows the share, so the index must update even with zero open listings.

- **Path space.** The SMB index's `ROOT_ID` is the volume's MOUNT ROOT; entries are stored by `name` under their
  parent. The watcher delivers a MOUNT-ABSOLUTE parent path (`/Volumes/share/sub`), so `apply_smb_change` strips the
  mount root to a MOUNT-RELATIVE path (`/sub`) via `index_relative_path` before `store::resolve_path` (which walks from
  `ROOT_ID`). `index_relative_path` is defined here and reused by the read side (`paths::index_read_path`) and by
  `IndexPathSpace::resolve_abs` — the single mount-strip, never a second copy.
- **Translation (`resolve_change`, pure over DB state, unit-tested).** `Added`/`Modified` → `UpsertEntryV2` under the
  resolved `parent_id`; `Renamed` → upsert the new entry (same-dir, so ancestor totals hold); `Removed` → resolve the
  child id and `DeleteEntryById` (file) / `DeleteSubtreeById` (dir); `FullRefresh` → no targeted write (overflow is
  handled by the freshness path). A `Removed` for a name the index never had is a no-op — **resolve-deletes-against-the-
  index**, NOT a live stat: SMB does not stat the volume per delete, so a false removal (atomic-rename old name,
  coalesced delete-then-recreate) with no matching index row enqueues nothing, and a recreate heals via the separate
  `Added`. SMB entries carry no stable inode, so `inode`/`nlink` are `None` (no hardlink dedup). The writer
  auto-propagates the size/count delta on upsert AND delete, so the translator never sends a separate
  `PropagateDeltaById`.
- **Emit-after-write ordering.** The inline pane-enrich reads sizes BEFORE the index write would land, so the write is
  sequenced FIRST and the writer emits `index-dir-updated` for the affected dir (`EmitDirUpdated`, which rides the same
  writer channel so it fires only after the upsert/delete commits). The existing FE refresh path
  (`index-dir-updated` → `refreshIndexSizes` → `getDirStatsBatch`) re-reads the just-written sizes. The coupling is
  one-directional: the listing layer notifies the indexer, never the reverse.
- **Single-writer + reads-off-the-lock.** `apply_smb_change` only ENQUEUES on the volume's existing writer thread; it
  never opens a write connection. Id resolution uses the volume's `ReadPool`, never the registry lock. It's synchronous
  (all local DB reads + channel enqueues, no network round trip), so it's safe inline from the sync
  `notify_directory_changed`. It no-ops for `root` (local disk feeds its index from FSEvents) and any non-`Running`
  volume.
- **Watcher lifetime is already volume-index-scoped.** The watcher is spawned in `connect_smb_volume` (on the direct-
  smb2 upgrade) and canceled only by `on_unmount` or `do_attempt_reconnect` (which respawns it). Pane close
  (`list_directory_end`) can't reach it. Pinned by `smb_integration_pane_close_does_not_kill_index_watcher`.
- **Buffer-during-scan.** The smb2 watcher runs continuously, so changes are on the wire throughout a scan. A change to
  an already-walked dir can't be applied straight to the mid-scan index (the scan truncated the DB and is still
  inserting). So `start_volume_scan` flips `scanning` true BEFORE the truncate, and while set `apply_smb_change` BUFFERS
  the change (per-volume `SCAN_CHANGE_BUFFER`, bounded `MAX_BUFFERED_CHANGES = 50_000`) instead of applying it. The
  scan-completion handler calls `replay_buffered_changes` AFTER aggregation lands, then transitions to Fresh; overflow
  fires `OverflowUnrecoverable` ⇒ Stale; an interrupted scan calls `discard_buffered_changes`. The completion handler
  itself is owned by [`../lifecycle`](../lifecycle/DETAILS.md).
- **Overflow policy.** On `STATUS_NOTIFY_ENUM_DIR` the watcher keeps watching (the session is fine) and emits a
  root-scoped `FullRefresh`; the index path fires `OverflowUnrecoverable` ⇒ Stale (the watcher only ever signals
  overflow for the share ROOT, so a full rescan is the only honest repair). Overflow is a DIFFERENT code path from a
  disconnect (`WatcherDied`) — never conflated. `on_smb_watcher_died` / `on_smb_overflow` (in `smb/index.rs`) fire the
  freshness events and bump `current_epoch`; the freshness state machine is owned by
  [`../lifecycle`](../lifecycle/DETAILS.md).

## MTP (`mtp/`)

MTP storages index into their own per-volume DB exactly like SMB, reusing the whole registry / writer / aggregator /
`dir_stats` / read-path machinery and the `Volume`-trait scanner. `IndexVolumeKind::Mtp` joins `Smb` under
`is_trait_scanned()`. MTP differs from SMB in three places.

### Stable identity and `:`-safe parsing (`crate::mtp::identity`)

The MTP volume id keys the persisted index DB, so it must survive a replug. `device_id_for(serial, location_id)` prefers
the device's stable `serial_number` (`mtp-{serial}` — re-matches on a replug to ANY port) over the USB-topology
`location_id` (`mtp-{location_id}` — same-port-only fallback when no serial is reported). The volume id stays
`{device_id}:{storage_id}`.

**Parser audit (the riskiest part).** A serial CAN contain `:`, which a naive `split(':').nth(1)` mis-reads. The
storage id is ALWAYS the trailing numeric component, so `split_volume_id` splits on the LAST `:` (`rsplit_once`) and
parses the tail as a `u32`. Every former split routes through `identity`. The device id is OPAQUE — `connect()` resolves
it to a `location_id` by matching the live enumeration, never by decoding it. (`identity` lives in the top-level `mtp`
module, not here; the read-side `paths::routing` and this watch layer both consume it.)

### Enable (`mtp/index.rs`)

`start_indexing_for_mtp` needs only the device connected (the volume registered) — no smb2-style connection-upgrade
gate, and FDA-independent (USB isn't TCC-protected), so `enable_drive_index` routes `mtp-*` ids straight here. A clean
scan ⇒ Fresh while connected. `handle_device_disconnected` (any disconnect) flips EVERY indexed storage on the device to
Stale via `FreshnessEvent::WatcherDied` (matched by `device_id_of_volume`, so it doesn't over-match a different device).
A persisted MTP index loads Stale on launch (non-journaled). **MTP Fresh is as strong as SMB.**

### Live watch → index (`mtp/watch.rs`)

The mirror of `smb/watch.rs`, fed from the PTP event loop (`watch/event_loop`) instead of `notify_directory_changed`.
`feed_index_added_or_changed` keeps the index in sync per indexed storage of the device; `feed_index_removed` resolves
by the stored handle. PTP events are device-wide but storages are separate namespaces, so both fan out over
`state::registered_mtp_volume_ids_for_device`.

- **Per-entry object handle → removal lookup (inode reuse, no schema bump).** PTP `ObjectRemoved` carries only an opaque
  handle and the object is already gone, so `GetObjectInfo` can't map it to a path. The MTP scan and the live upserts
  store each entry's PTP object handle in the index's existing `inode` column. `ObjectRemoved{handle}` then resolves the
  row via `find_entry_by_inode(handle)` → `DeleteEntryById`/`DeleteSubtreeById`. Reusing `inode` avoids a schema bump;
  it's safe because MTP entries live only in the MTP index (the local rename pre-pass that also uses
  `find_entry_by_inode` runs only against `root`). A removal for an unseen handle is a no-op — the same
  resolve-against-the-index rule as SMB.
- **Gate BEFORE resolve (foreground-priority device scheduler).** `feed_index_added_or_changed` does NOT resolve the
  handle up front. It first calls `buffer_mtp_handle_if_scanning(volume_id, storage_id, handle)` — device-free, just the
  registry scanning flag. During a scan it buffers the RAW handle (`BufferedChange::UpsertHandle`, zero device I/O) and
  the caller skips resolution; only a non-scanning storage resolves live (`resolve_object_for_index`: the handle→path
  walk plus one `GetObjectInfo`, at foreground priority) and enqueues an `UpsertEntryV2` storing the handle. Before this
  gate, the resolve ran ahead of the scanning check, so every change hit the contended device mid-scan and timed out
  (the livelock). `replay_buffered_mtp_changes` applies the sync changes, then spawns one task to resolve the buffered
  raw handles post-scan (device idle); a failed resolve is dropped (the scan captured the object; a later change
  re-fires). A handle can be buffered for the wrong storage, but its replay resolve fails cleanly.
- **Path space — no mount-strip.** The MTP resolver produces storage-relative paths (`/DCIM/Camera`) and the index
  `ROOT_ID` is the storage root, so `apply_mtp_*` resolves against the index directly. The read-side `index_read_path`
  strips the `mtp://{device}/{storage}` scheme prefix off listing/dir-stats paths (owned by
  [`../paths`](../paths/DETAILS.md)).
- The rest matches SMB verbatim: enqueue on the volume's writer, index write before the `index-dir-updated` emit, reads
  off the `ReadPool`, buffer-during-scan (`SCAN_CHANGE_BUFFER`, 50,000, overflow ⇒ Stale) replayed after aggregation,
  discard-on-interrupt.

**Needs real-device QA** (no live MTP hardware in CI; the pure pieces are unit-tested): connect → scan → green badge;
add/delete a file on the phone → index reflects it while connected; unplug → yellow; rescan → green; a same-port vs.
different-port replug (serial id re-matches, location id rescans); a device that reports no serial (location fallback).

## Local external drives (`local_external/`)

### Enable + classification (`local_external/index.rs`)

`enable_drive_index` routes a per-drive "Turn on indexing" by id: `root` → local; an `mtp-*` id → MTP; then the
**local-external branch** (`start_indexing_for_local_external`); then the SMB fall-through. The local-external branch is
what a plain local external drive (USB stick, SD card, extra disk, mounted disk image) needs — before it existed, a
healthy local drive fell to the SMB path and was refused as `NotAnSmbVolume` (the reported bug). Unlike SMB it has NO
connection gate (a local mount is already directly readable) and NO typed refusal; unlike MTP it uses the local scanner.

**Classification (`classify`)** decides local-external vs fall-through from TYPED facts, never a volume-id/path
substring: resolve the volume in `VolumeManager`, read its mount root, and check two things — a live smb2 session
(`smb_connection_state().is_some()`) and whether the mount's filesystem is a network type (`is_network_fs_type` over the
fs-type from `detect_filesystem_for_path`). Either ⇒ fall through to the SMB gate (a network mount must never run the
local guarded walker). Neither ⇒ `LocalExternal`, indexed via `start_indexing_for_local_external_inner` →
`start_indexing_for(.., LocalExternal, inodes_trustworthy)`, then `enforce_external_index_cap` (retention, owned by
[`../resources`](../resources/DETAILS.md)). The pure routing decision (`routes_to_local_external`) is split from the
wiring so it's unit-testable without a `VolumeManager` or `AppHandle`. Disk images are INCLUDED: a mounted DMG is a real
local filesystem; the first-connect prompt stays `isDriveRow`-gated so a DMG is only ever indexed by an explicit enable.

**The fs-type probe is timeout-guarded** (2 s, on the blocking pool): a hung network mount's `statfs` must never stall
the IPC thread, and a timed-out/errored probe is treated as network → fall through (safe). The same probe also yields
the drive's inode-trust fact (`FilesystemKind::has_stable_inodes()`, false for FAT/exFAT), threaded to the scan as
`inodes_trustworthy` so its entries store `inode: None` (see [`../paths`](../paths/DETAILS.md) `trust_inode`, and the
FAT/exFAT rationale below).

`LocalExternal` is the first volume that is BOTH local-scanned AND mount-rooted, non-journaled, and non-search-feeding.
Journal replay is gated on `kind.has_event_journal()`, NOT `stored_event_id.is_some()`: the shared local event loop
persists `last_event_id` for ANY local-scanner volume, so a completed `LocalExternal` index carries one despite having
no `.fseventsd` journal to replay. Only the boot disk (`Local`) replays. This resume dispatch is owned by
[`../lifecycle`](../lifecycle/DETAILS.md).

### Why FAT/exFAT nulls the inode wholesale

FAT32/exFAT store no inode; macOS/Linux DERIVE `st_ino` from the file's first data cluster, so it's unstable — writing
content into an empty file changes it, and a delete+create ALIASES a fresh, unrelated file onto a freed cluster's inode.
The live rename pre-pass (`detect_renames_by_inode` → `MoveEntryV2`) keys off inode identity, so a derived inode would
both MISS real renames and, worse, FALSE-MATCH an inode-reused delete+create as a move — silently re-homing the deleted
entry's `dir_stats` onto the unrelated new file (index corruption). The fix keys off the volume's filesystem, not the
per-rename outcome: `has_stable_inodes()` is resolved ONCE per scan and threaded via `IndexPathSpace::trust_inode`, which
every local write path funnels a snapshot's inode through. With no stored inode `find_entry_by_inode` can never match, so
the pre-pass is inert and every change falls back to the safe delete+create path (a renamed DIRECTORY loses its
`dir_stats` and re-accrues them — rare, self-heals via verification). Rename-stable formats (APFS, HFS+, ext4/btrfs/XFS/
ZFS, NTFS) keep the real inode.

### Unmount/eject lifecycle (the wedge-safe ordering)

**The constraint this defends (the 2026-07-15 incident).** A `LocalExternal` index holds an FSEvents watcher and open
SQLite handles rooted at the drive's mount (`/Volumes/X`). On 2026-07-15, `diskutil unmount` on a physical FAT32 card
wedged macOS 26's userspace FSKit `msdos` service *mid-unmount*, held kernel vnode locks, and kernel-panicked the
machine. An open FSEvents stream / SQLite handle on the volume at the moment of unmount is exactly the kind of open
reference that can wedge the FSKit unmount. So the index MUST be stopped — watcher dropped, writer drained, handles
closed — while the filesystem is still healthy, i.e. BEFORE the unmount. The wedge happens *during* unmount, so no
post-unmount hook can undo it.

**Three hooks, only one reliable.** Each releases the watcher + handles and preserves the DB on disk (a later remount +
re-enable reconciles in place). Idempotent, so overlapping hooks are safe. All three act ONLY for a `LocalExternal`
index (`indexing::volume_kind(id) == Some(LocalExternal)`); SMB and MTP tear their indexes down through their own
disconnect paths, and stopping them here would fight that. (The hooks themselves live in `file_system/volume/eject.rs`
and `volumes/watcher.rs`; the `LocalExternal` gate — `stop_index_blocking` — lives here.)

- **Cmdr's own eject (`file_system/volume/eject.rs`) — the reliable wedge-safe point.** For a
  `DiskutilUnmount`/`DiskutilEject`, `stop_index_then_unmount` awaits `stop_index_blocking(volume_id)` and ONLY THEN
  runs `diskutil`. The ordering is unconditional and runs on the blocking pool. This is the one path where Cmdr controls
  the timing, so it's the guaranteed protection. SMB/MTP keep their own teardown and their offline-browsable Stale index
  across an eject, so the eject-stop is a no-op for them.
- **`NSWorkspaceWillUnmountNotification` — best-effort pre-unmount.** The earliest hook macOS offers for an OS/Finder
  eject, but RACY (the OS doesn't wait for our observer). Better than nothing; NOT a guarantee. Runs off-main.
- **`NSWorkspaceDidUnmountNotification` — CLEANUP only.** By the time it fires the volume is gone, so it can't prevent a
  wedge; it releases the now-dangling watcher + handles for a volume that unmounted without going through Cmdr's eject.

The DidUnmount/WillUnmount handlers do NOT flip freshness to Stale: for a physically removed drive the volume row leaves
the picker, so a `Fresh→Stale` transition would pop the one-time stale dialog about a drive that's simply gone.

**The drain is cooperative, so the ORDERING is what protects — not the drain.** `IndexManager::shutdown`'s cancel is
cooperative and does NOT join the scan thread, so a scan worker already blocked inside a wedged FSKit `read_dir` won't
block the drain (good) but also can't be interrupted and may still hold vnode locks. So the protection is the eject-stop
*ordering* (release the watcher + handles while the FS is healthy), NOT the drain making a mid-wedge unmount safe. Test
with synthetic disk images ONLY (never a real physical FAT card — that's what panicked the machine).
