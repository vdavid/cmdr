# Local external drive indexing (USB sticks, SD cards)

Status: planned, execution pending. Worktree: `david/local-drive-indexing`. Reviewed once (fresh-eyes Opus); this
revision folds in that review — the live-pipeline path-space gap (#1), the disk-image fixture contradiction (#2), the
watcher-lifecycle gap (#3/#5), the event-loop exclusion sites (#4), and the FE predicate location (#6).

## Why, and the guiding intent

Cmdr indexes each volume into its own SQLite DB so listings show recursive directory sizes. It supports the boot disk
(`root`), SMB shares, and MTP devices — but **not** a plain local external drive (USB stick, SD card, extra local disk).
Insert one, accept the first-connect "index this drive?" prompt, and it fails with a nonsense toast ("Cmdr can't index
{name} right now. Reconnect the drive and try again.") on a perfectly healthy, mounted drive.

The prompt is right to fire; the backend branch is simply missing. `enable_drive_index` routes `root → local`,
`mtp:// → MTP`, **everything else → the SMB path**, where a local drive hits the explicit non-SMB guard and is refused
as `SmbIndexGateReason::NotAnSmbVolume`. (Reproduced live: `"Can't index volumesnoname: not an SMB volume"`.)

**Guiding intent for the implementer:**

- **No reimplementation.** The per-volume machinery already exists and is the right machinery: the registry
  (`INDEX_REGISTRY`), `start_indexing_for(app, vid, root, kind)`, the guarded local walker, the FSEvents `DriveWatcher`,
  freshness, retention, the read-path mapping, and — crucially — `smb_watch::index_relative_path`, the mount-relative
  path strip. A local external drive is a **new combination** of existing capabilities, not a new subsystem.
- **The real shape of the work.** `LocalExternal` is the **first volume that is BOTH mount-rooted (index `ROOT_ID` =
  `/Volumes/X`, not `/`) AND scanned/watched by the LOCAL jwalk + FSEvents pipeline.** Today "mount-rooted" and
  "trait-scanned (SMB/MTP)" are welded together, and the local pipeline has only ever run on `root` where an absolute
  path already equals the index-relative path. So the bulk of the work is **teaching the local scan/reconcile/live-event
  pipeline to speak a mount-relative path space** (which the SMB *read* side already does via `index_read_path`, and the
  SMB *write* side via `smb_watch::index_relative_path`) — plus splitting one conflated axis and wiring the enable branch.
- **The elegant core is splitting two orthogonal capabilities that today coincide.** `IndexVolumeKind` conflates "which
  scanner" (jwalk vs `Volume`-trait) with "has an event journal" (FSEvents replay vs not) with "mount-rooted vs
  `/`-rooted". For the three existing kinds these move together; `LocalExternal` forces them apart. Making the latent
  orthogonality explicit is a strict improvement.
- **Honesty over cleverness (principles #3, #4).** A drive we weren't watching while unplugged is *stale*, and we say so;
  we never fabricate Fresh. A drive yanked mid-scan surfaces an honest aborted state, not a stuck spinner.
- **Respect the machine (principle #5) — and never repeat the incident below.**

## ⚠️ SAFETY: the FAT-unmount kernel-panic incident (READ THIS)

On 2026-07-15, a research subagent ran `diskutil unmount` on a physical, 97%-full FAT32 SD card during
inode-stability probing. macOS 26's userspace **FSKit `msdos` service wedged mid-unmount**, held kernel vnode locks,
and the pile-up eventually blocked WindowServer; the userspace watchdog force-crashed it and **rebooted the machine**
(panic: `userspace watchdog timeout: no successful checkins from WindowServer`). The card's data survived but the FAT
was left dirty.

Constraints this burns into the work:

- **Never mount/unmount/probe a physical removable card in tests or research.** Use a **synthetic disk image**
  (`hdiutil create -fs "MS-DOS FAT32"`) — same FSKit `msdos` code path, disposable, no real data (M0).
- **Every `hdiutil`/`diskutil` call in test/tooling is hard-timeout-guarded** (`timeout --signal=KILL 30 …`) so a wedged
  FSKit service is killed, never awaited. No mount/unmount *cycling*; attach once, detach once via `hdiutil detach`
  (with a `-force` fallback behind the timeout). This is verified working in `local-drive-indexing-probes/fat32-probe.sh` (attach+probe+clean
  detach in ~2 s).
- **The wedge happens DURING unmount, so it cannot be fully prevented from a post-unmount hook.** `volumes/watcher.rs`
  observes `NSWorkspaceDidUnmountNotification` — *after* the unmount. Stopping the index there is **cleanup, not
  wedge-prevention**. The only path where Cmdr can stop the index *before* the unmount is its **own eject command**, and
  a best-effort `NSWorkspaceWillUnmountNotification` subscription (racy, but better than nothing) for OS/Finder-initiated
  ejects. M6 is framed around this honestly — do not oversell the DidUnmount hook as safety-critical.
- A hung userspace FS taking down WindowServer is ultimately a macOS bug worth a Feedback Assistant report (the
  `panic-full-2026-07-15-101430` file is the evidence); our job is to never trigger it, and to hold no FSEvents stream or
  open SQLite handle on a drive we're about to eject.

## Verified facts this plan rests on (don't re-derive)

Empirical, synthetic FAT32 image, macOS 26.5.2, 2026-07-15 (`local-drive-indexing-probes/fat32-probe.sh`):

- **FAT/exFAT inodes are derived, not stored, and not stable.** An empty file gets a sentinel inode (`2^64−14`) that
  **changes when content is written** (`…602 → 20`). Rename/move preserve the inode (first data cluster is stable), but
  delete+create **aliases** onto freed clusters over time. `nlink` is always `1` for files and dirs. ⇒ **Inode identity
  is untrustworthy on FAT/exFAT**: the live rename pre-pass (`detect_renames_by_inode` → `MoveEntryV2`) would miss real
  renames AND, worse, **false-match a delete+create as a move** (inode reuse) — corrupting the index. Hardlink dedup
  (`nlink > 1`) never fires (safe).
- **No `.fseventsd` on the volume ⇒ no FSEvents journal ⇒ no `sinceWhen` replay** — BUT **live FSEvents DO fire.**
  Verified with an `FSEventStreamCreate` probe (`local-drive-indexing-probes/fsevents-probe.swift`) on both FAT32 and exFAT synthetic images:
  a file write / mkdir on the mount delivers a live callback with the path (`FSEVENT-FIRED /Volumes/…`). So the journal's
  absence costs *replay* (historical `sinceWhen` catch-up), not *live delivery*. ⇒ `LocalExternal` is **non-journaled but
  live-watchable**: it can't resume via replay the way `root` does, but a running `DriveWatcher` keeps it current while
  mounted. This is the empirical fact the whole live-watch design (Decision #2, M3, M5) rests on — it holds.
- **Volume UUID is a synthesized MD5 name-based (v3) UUID**, not stored — collides for same-named cards. Use the existing
  mount-path-derived id (`volumesnoname` via `path_to_id`), matching how the drive is registered in `VolumeManager`.
- **`FilesystemKind`** (`file_system/filesystem_kind.rs`) already classifies `msdos`/`vfat` → `Fat32`, `exfat` → `ExFat`.
  Reuse it for the inode-trust decision — add a method, don't build a classifier.

Code facts (file:line), verified this session:

- **`should_exclude` lists `/Volumes/`** (`scanner/exclusions.rs:22`), gated by `is_volume_root && should_exclude` per
  child (`scanner/mod.rs:549`). A jwalk scan rooted at `/Volumes/X` (which IS `is_volume_root`) `continue`s **every
  child** → 0 rows → `scan_completion.rs:283` fires `ScanCompleted ⇒ Fresh` + writes `scan_completed_at`. Silent
  false-complete, same shape as the "rescan does nothing to the NAS" bug (`manager.rs:124`). **`should_exclude` is ALSO
  called on the LIVE path** (`event_loop.rs:1069,1120,1360`) and in the reconciler/verifier — the fix must cover all
  sites, and scope **cannot** be derived from `is_volume_root` (the boot `root` scan is also `is_volume_root`); the
  volume kind/scope must be plumbed.
- **The LOCAL live pipeline resolves ABSOLUTE paths from `ROOT_ID`.** `reconciler.rs:14` documents "All path resolution
  uses `store::resolve_path(conn, path)`"; `event_loop.rs` and `reconciler.rs` call `firmlinks::normalize_path(&event.path)`
  then `store::resolve_path(conn, …)` at `event_loop.rs:227,700,711,918,1145,1284` and
  `reconciler.rs:320,765,772,823,897,1006,1077,1127,1147`, plus `local_reconcile.rs:303-310` for the reconcile root.
  For a mount-rooted index these walk `Volumes`→`X`→… from the mount root and **miss every event / the reconcile root**.
  `firmlinks::normalize_path` is boot-disk-only and must not run on external paths. **This is the core work, unbudgeted in
  the first draft.** The SMB read side solved the analogous gap with `index_read_path`; the SMB write side with
  `smb_watch::index_relative_path`.
- **`resolve_scan_root` already maps a volume-root scan → `ROOT_ID` kind-agnostically** (`store/mod.rs:129`) — works for
  `/Volumes/X`. **`index_read_path_pure` already strips a `mount_root` generically** (`routing.rs:82`) — the read side is
  genuinely nearly free once the volume resolves to a mount root.
- **`resume_or_scan` branches on `is_trait_scanned()`** (`manager.rs:233`); non-trait kinds fall into the FSEvents
  journal-replay path using the **global** `watcher::current_event_id()` + `start_replay`. **`resume_or_scan_network`
  starts NO watcher** (`network_scan.rs:71` returns `Ok(())` on a completed index; the SMB/MTP live watcher is
  connection-scoped, started elsewhere on connect). **The FSEvents `DriveWatcher` starts ONLY inside `start_scan`**
  (`manager.rs:333` replay, `624` fresh). ⇒ Neither existing resume path gives `LocalExternal` "loads Stale + live watch"
  for free; see the M4 decision.
- **`volume_id_for_local_path` returns `root`** for any non-SMB/non-`mtp://` path (`routing.rs:37,45`) → dir-stats/status
  and `cmdr://state` `indexStatus` (`mcp/resources/volumes.rs`) for an external drive route to `root` (the "reports Fresh
  when unindexed" bug).
- **Retention already treats any non-root `index-*.db` as external/evictable** (`retention.rs:145`, keyed on
  `!= ROOT_VOLUME_ID`) — **no change needed**, but `enforce_external_index_cap` is called only from the SMB/MTP enable and
  must also fire from the local-external enable.
- **`local_reconcile`'s root-miss returns `ScanError::Io("local reconcile: root is not in the index")`** (not
  `EmptyRoot`), routed through the completion failure arm (`scan_completion.rs:320`). (Label fix vs first draft.)
- **`LocationInfo` (with `is_disk_image`, `category`, `is_read_only`) is built by `list_locations`/`volume_broadcast`,
  NOT carried on the `Volume` trait** that `VolumeManager.get(id)` returns. So classifying the drive at the
  `enable_drive_index(app, id)` call site needs plumbing (or a timeout-guarded `statfs`/`detect_filesystem_for_path`
  probe, obeying the `src-tauri/CLAUDE.md` FS-timeout rules). Real work, not a one-liner.
- **`is_disk_image_mount` (`volumes/disk_image.rs:31`) flags an `hdiutil`-attached image as a disk image** (proven by its
  own `is_disk_image_mount_detects_real_dmg` test). ⇒ The M0 fixture *is* a disk image — so an "exclude disk images"
  enable gate would reject the fixture. **Disk-image policy decision below resolves this.**
- **The FE `isNetwork` predicate is `activity.volumeId !== ROOT_VOLUME_ID` at `IndexingDriveRow.svelte:180`** (and
  `IndexingDriveSummary.svelte`, `DETAILS.md:115`), feeding `runKind`. **`indexing-steps.ts` already branches on
  `runKind`** (`:108` replay, `:111` network) and needs **no** change. First draft named the wrong file.
- **`SmbIndexGateReason` is consumed only by `VolumeBreadcrumb.svelte::handleIndexRefusal`** (badge and
  `drive-index-status.ts` don't branch on it) — small blast radius to generalize.
- **The local scan path emits no `index-scan-aborted`** (only `network_scan.rs:401,420`) → yanked-mid-scan leaves a stuck
  "scanning" row.
- **i18n: English-only ships today**; translations self-heal via a sync step (`docs/guides/i18n-translation.md`).

## Decisions taken (resolving the review's open questions)

1. **Disk-image policy.** `enable_drive_index` will index **any local, non-network, non-root, readable volume, INCLUDING
   disk images** — a mounted DMG is a real local filesystem whose contents (under `/Volumes/X`) are excluded from `root`'s
   index, so indexing them is harmless and useful, and retention caps accumulation. The **first-connect prompt keeps
   following `isDriveRow`** (`drive-index-manager.svelte.ts:40`, false when `isDiskImage`), so a DMG is never
   auto-prompted; it's only indexed if explicitly enabled (menu/MCP). This removes the M0 *capability* contradiction: the
   disk-image fixture is legitimately indexable via the direct enable path, exercising the exact production scan/watch
   pipeline. **But it means the fixture can NOT drive the first-connect prompt** (an attached image is a disk image →
   `isDriveRow` false), so M0/M9 tests call `enable_drive_index` directly; prompt coverage stays unit-level
   (`first-connect-trigger.test.ts`). (If we later want to hard-exclude disk images from the *capability*, that's a
   separate policy flag — not now.)

2. **Relaunch of a persisted `LocalExternal` index.** It has no journal to replay, and no connection event to hang a
   watcher on. Rather than invent a "start live watch without scanning" path, **`LocalExternal` always routes through the
   LOCAL scan path, which already starts the `DriveWatcher`** (`manager.rs:624`): on first enable → fresh scan; on
   relaunch/remount with a populated DB → **reconcile-in-place** (the hang-tolerant `local_reconcile`, which diffs rather
   than re-walks-blind and starts the watcher). It loads **Stale** and the reconcile brings it to **Fresh** — honest,
   because the reconcile actually verifies the tree we couldn't watch while unplugged. This reuses existing machinery
   end-to-end and sidesteps the "network resume starts no watcher" gap. A "resume watch without a full reconcile" fast
   path is a documented future optimization (`docs/specs/later/`), not v1.

   **Load-bearing WHY for M4's replay gate (don't let anyone "simplify" it away):** the shared local event loop and
   completion handler persist `last_event_id` for ANY local-scanner volume (`event_loop.rs:422`, `scan_completion.rs:255`).
   So a completed `LocalExternal` index WILL have both `last_event_id` AND `scan_completed_at` set — and today's
   `resume_or_scan` would therefore take the `start_replay` branch (`manager.rs:244`) and try to replay a journal the
   volume doesn't have. M4 gates replay on `has_event_journal()`, NOT on `last_event_id.is_some()`. A future cleanup that
   re-collapses the gate to `last_event_id.is_some()` silently routes `LocalExternal` into an empty/garbage replay — so
   the gate stays kind-based, with this comment at the call site.

3. **Capability axes.** Split `IndexVolumeKind`'s conflated predicates into explicit methods; the new variant reuses the
   local scanner + FSEvents watch but is mount-rooted, non-journaled, non-search-feeding, with inode-trust derived from
   the filesystem:

   | Capability | `Local` (root) | `LocalExternal` (new) | `Smb` | `Mtp` |
   |---|---|---|---|---|
   | `uses_local_scanner()` (jwalk + FSEvents) | yes | **yes** | no | no |
   | `is_trait_scanned()` (`Volume` trait) | no | **no** | yes | yes |
   | `has_event_journal()` (was `is_journaled`) | yes | **no** | no | no |
   | `mount_rooted()` (index `ROOT_ID` = mount) | no | **yes** | yes | yes |
   | `feeds_search()` | yes | **no** | no | no |
   | inode trust (from `FilesystemKind`) | trusted | **fs-derived** | n/a | n/a |

## Milestones

Sequential along the M0→M4 spine; M5/M7/M8 parallelize after M4 (see Parallelization). Each ends green
(`pnpm check --fast` iterating; scoped `pnpm check` at the milestone; `--include-slow` before wrap). Real red→green TDD is
called out; lean hardest on the exclusion fix, the mount-relative live pipeline, the inode gate, and the eject-stop path.

### M0 — Safe synthetic disk-image fixture (first; guardrail + E2E foundation)

**Intent:** every later milestone tests against a *disposable disk image*, never a physical card. create+attach ≈ 1–2 s,
detach ≈ 1 s — fast enough as a per-suite fixture.

- A macOS-gated `#[cfg(test)]` Rust helper that: `hdiutil create -fs "MS-DOS FAT32" -volname … -layout MBRSPUD` (~64 MB;
  FAT32 floor ≈ 32 MB), `hdiutil attach -nobrowse`, parses `/dev/diskN` + `/Volumes/…`, and yields a guard whose `Drop`
  runs `hdiutil detach` then `hdiutil detach -force`, **each behind a hard `timeout --signal=KILL`**. Never `diskutil
  unmount` on the path; never cycle. Exposes `mount_point()` + a tree-populator (dirs, sized files, an empty file).
- An exFAT variant (`-fs "ExFAT"`) so M5's inode-trust path is exercised on both FAT and exFAT.
- **A live-FSEvents regression probe** (fold in `local-drive-indexing-probes/fsevents-probe.swift`, or a Rust equivalent over the app's own
  `cmdr-fsevent-stream`): attach → arm a watcher → mutate → assert a live event is delivered. This pins the load-bearing
  fact M3/M5 depend on (live FSEvents fire on FAT/exFAT despite no journal), so a future macOS change that broke live
  delivery would fail loudly here rather than silently making external indexes never update.
- Keep `local-drive-indexing-probes/fat32-probe.sh` + `local-drive-indexing-probes/fsevents-probe.swift` as the human-run references; the Rust helpers are the
  automated form.
- **Guardrail doc:** a "Testing external drives" note in `indexing/DETAILS.md` + a one-line C.md must-know: the incident,
  disk-image-only rule, and the timeout requirement, so a future agent can't "clean up" the timeouts or reach for a card.

Tests: smoke-test the helper with 1–2 cases first (`test-infra-smoke-first`) — attach, read the populated tree via
`std::fs`, detach cleanly, assert the mount point is gone. Checks: `pnpm check rust`, `pnpm check desktop`.

### M1 — Split the conflated capability axes; add `LocalExternal`

**Intent:** make the orthogonality explicit with **zero behavior change** for the three existing kinds (characterization
tests, not red→green).

- Add `IndexVolumeKind::LocalExternal`. Rename `is_journaled()` → `has_event_journal()`. Add `uses_local_scanner()`,
  `mount_rooted()`, `feeds_search()` per the table. Keep `is_trait_scanned()` = `Smb | Mtp`, and assert in a test that it
  is the exact complement of `uses_local_scanner()` so they can't drift.
- Route each existing predicate call site to the axis it actually means: `feeds_search` (`manager.rs:190`) →
  `kind.feeds_search()`; launch-freshness seed (`state.rs:556`) → `has_event_journal()`; `rescan_scanner_for_kind`
  (`manager.rs:130`) → `uses_local_scanner()`. (The `resume_or_scan` dispatch is rewritten in M4.)
- **No new `VolumeIndexStatus` field** (first-draft mistake, review #4): the FE checklist predicate reads
  `VolumeIndexActivity` (from live scan events), not `VolumeIndexStatus`, so a status field wouldn't reach it. M8 instead
  derives the checklist shape from the **existing volumes store `category`** (the same source `active-media-volume.ts:45`
  uses: `category === 'network'`), which three-ways correctly (root + `LocalExternal` local → local checklist; SMB/MTP →
  network). No backend field needed; nothing to land here for M8.

Tests: capability-tuple unit tests for all four kinds; the partition assertion. Checks: `pnpm check rust`.

### M2 — Scan-root-relative exclusions (all sites, incl. the live path)

**Intent:** absolute-prefix exclusions (`/Volumes/`, `/System/…`, `/private/var/`) keep the **boot-disk** scan on the boot
disk; a mount-rooted scan is already under `/Volumes/X` and must index everything beneath it, while still skipping
per-volume junk (`.Spotlight-V100`, `.fseventsd`, `.Trashes`, `.TemporaryItems`) at any root.

- Split `exclusions.rs` into **(a) boot-disk absolute-prefix exclusions** (only the `root` scan) and **(b) per-volume junk
  basenames** (any scan). Preserve the `CMDR_E2E_START_PATH` allowlist behavior.
- `should_exclude` takes an explicit `ExclusionScope { BootDisk, MountRooted }` (a typed enum, per no-string-matching),
  threaded to **every** call site: `scanner/mod.rs`, `reconciler.rs`, `verifier.rs`, `enrichment.rs`, **and
  `event_loop.rs:1069,1120,1360`** (the live path the first draft missed). Scope is derived from the volume kind, NOT
  `is_volume_root`.
- Extend enrichment's `volume_id == ROOT` special-case (`enrichment.rs:204`) so a `mount_rooted()` volume uses tier (b).

Tests (real red→green): a mount-rooted scan over an M0 tree returns 0 entries + false-Fresh today (red); after the fix it
indexes the full tree, skipping only junk basenames. A `root`-scope test asserts `/Volumes/…`, `/System/…` still excluded.
Checks: `pnpm check rust`, `pnpm check desktop`.

### M3 — Teach the local pipeline a mount-relative path space (the core)

**Intent:** the local scan/reconcile/live-event pipeline assumes absolute == index-relative (true only for `root`). Make
it mount-relative for `mount_rooted()` volumes, reusing the SMB transforms — do NOT fork the pipeline.

- Introduce one seam: for a `mount_rooted()` volume, map an absolute FS path → index-relative via
  `smb_watch::index_relative_path(mount_root, abs)` (confirmed `pub(crate)` and a pure prefix strip with no SMB-specific
  assumption — the same transform the read side's `index_read_path` funnels through, so it's the single source of truth),
  and skip `firmlinks::normalize_path` (boot-disk-only) for these volumes.
- **CRITICAL — apply the strip ONLY at the `store::resolve_path` argument, not at set insertion (review #2).** The same
  path strings live in **three spaces** in `event_loop.rs`/`reconciler.rs`: `resolve_path(conn, p)` wants **index-relative**,
  but `std::fs::read_dir(p)` / `Path::new(p).exists()` (e.g. `event_loop.rs:1332,1344`) want the **absolute** FS path, and
  `emit_dir_updated` / the FE `index-dir-updated` payload (`event_loop.rs:1128`) must be **absolute** to match pane paths.
  So `affected_paths` / `pending_paths` / `new_dir_paths` stay ABSOLUTE everywhere; wrap the strip locally around each
  `resolve_path` call only. Applying it at insertion breaks the fs reads and the FE emit; omitting it breaks resolution.
- Sites to wrap: every `normalize_path(&event.path)`+`resolve_path` in `event_loop.rs`
  (`227,700,711,918,1145,1284`) and `reconciler.rs` (`320,765,772,823,897,1006,1077,1127,1147`), plus
  `local_reconcile.rs:303-310` (the reconcile root). The rename pre-pass's `new_parent_path` (`reconciler.rs:479-525`) is
  FS-event-derived so the strip applies to its resolve too — but `pending_paths.insert(new_parent_path)` (`:525`) stays
  absolute. `resolve_scan_root` needs no change (already `ROOT_ID` for a volume root).
- **Check `publish_dirs_changed(ROOT_VOLUME_ID, …)` / the background-verification emit (`event_loop.rs:1127`) hardcodes
  `root`** — confirm that path is root-only and never runs for a `LocalExternal` volume, or it mis-attributes dir-updated
  events to the wrong volume. If it can run for `LocalExternal`, thread the real volume id.
- The write-side stamping of stored `EntryRow` paths must match the read-side expectation (mount-relative, leading `/`) —
  mirror how SMB/MTP already store. Verify `manager.rs::start_scan`'s `volume_root` plumbing is `/`-free (uses
  `self.volume_root`; expected no change).

Tests (real red→green): (1) a live create/rename/delete event under an M0 mount resolves and writes the right entry
(red: today it resolves to `None` and drops); (2) a mount-rooted reconcile resolves its root and reconciles in place
instead of `ScanError::Io("root is not in the index")`; (3) integration: scan an M0 tree, then mutate it live, assert
`dir_stats` update. Checks: `pnpm check rust`, `pnpm check desktop`.

### M4 — Wire the enable branch + local-external resume dispatch + classification

**Intent:** the missing branch and the resume decision (Decision #2).

- `enable_drive_index` (`commands/indexing.rs:192`): add a **local-external** branch before the SMB fall-through. Classify
  via the plumbing this milestone adds (Decision on `LocationInfo`): local, non-network, non-root, readable (disk images
  INCLUDED per Decision #1). Call `start_indexing_for(app, vid, mount_root, LocalExternal)` then
  `enforce_external_index_cap(app)`. The SMB branch now only handles genuine SMB shares.
- Classification plumbing (review #7): surface the needed facts (fs type for M5, mount root, network-ness, disk-image-ness)
  to the enable site — either thread `LocationInfo` through, or a timeout-guarded `detect_filesystem_for_path` + volume
  metadata probe (obey `src-tauri/CLAUDE.md` FS-timeout rules; never block the IPC thread).
- `resume_or_scan` (`manager.rs:233`): dispatch by `uses_local_scanner()`; within the local branch gate replay on
  `has_event_journal()`. `LocalExternal` (no journal) → skip replay, go straight to fresh-scan (empty DB) or
  reconcile-in-place (populated DB) — the path that starts the `DriveWatcher`. Never the `resume_or_scan_network`
  no-watcher path.

Tests (real red→green): (1) `enable_drive_index` on a registered local external volume returns `Started` (not `Refused`)
and registers a `LocalExternal` instance; (2) resume with a populated DB reconciles-in-place and starts a watcher; resume
with an empty DB fresh-scans; neither takes the journal-replay branch; (3) integration: enable → scan M0 tree → sizes via
enrichment. Checks: `pnpm check rust`, `pnpm check desktop`.

### M5 — Inode safety on FAT/exFAT

**Intent:** never let an untrustworthy inode drive a rename/move; store `inode: None` so the rename pre-pass is inert and
changes fall back to safe delete+create.

- Add `FilesystemKind::has_stable_inodes()` (`Fat32|ExFat → false`, others → true).
- Scanner (`scanner/mod.rs` `InsertVisitor`) + `metadata.rs::extract_metadata` set `inode: None` when the scanned volume's
  `FilesystemKind` lacks stable inodes (resolved once per scan, threaded from the volume). The rename pre-pass already
  treats `None` as no-match; hardlink dedup already no-ops at `nlink == 1`.

Tests (real red→green): an inode-reuse sequence (delete A@N, create B reusing N) must NOT emit `MoveEntryV2` on an
inode-untrusted volume, but still detect the move on a trusted one; a `has_stable_inodes()` table test. Depends on M3 (live
events must resolve at all before rename behavior is observable). Checks: `pnpm check rust`.

### M6 — Lifecycle: eject-stop (safety) + unmount cleanup + local abort event

**Intent:** stop the index cleanly before Cmdr's own eject (the only wedge-safe point), clean up dangling instances on
OS-initiated unmount, and surface an honest aborted state — framed per the SAFETY section (DidUnmount = cleanup only).

- **Eject (wedge-safe):** `commands/eject.rs` / `file_system/volume/eject.rs` stop the volume's index (`stop_indexing` —
  drops watcher, drains writer, closes handles) **before** `diskutil unmount`. This is the one path that reliably avoids
  an open FSEvents stream / SQLite handle wedging the unmount.
- **OS/Finder unmount (cleanup + best-effort):** on `NSWorkspaceDidUnmountNotification` (`volumes/watcher.rs`), if the
  vanished volume had a registered `LocalExternal` index, `stop_indexing` it (remove the dangling instance) and flip
  freshness Stale. Additionally subscribe to `NSWorkspaceWillUnmountNotification` and stop the index there as a
  best-effort pre-unmount (racy, but the earliest hook the OS offers). Document both honestly.
- **Abort event:** the local scan path emits `index-scan-aborted { volumeId }` when its root becomes unlistable because
  the volume vanished (walker root read-error / reconcile root-unlistable on an absent mount), ending without writing
  `scan_completed_at` (heals to rescan on reconnect) — mirroring `network_scan.rs`'s disconnect arm.
- **Nuance (review #6):** `shutdown()`'s cancel is cooperative (checked between `read_fs_children` calls) and it does NOT
  join the scan thread, so a scan worker already blocked inside a wedged FSKit `read_dir` won't block the drain (good) but
  also can't be interrupted and may hold vnode locks — precisely the incident's mechanism. The eject-stop-BEFORE-unmount
  ordering is the real protection (we release the watcher + handles while the FS is still healthy); the DidUnmount cleanup
  can't help there. Don't frame the drain as making unmount safe — the ordering does.

Tests (real red→green where observable): `stop_indexing` on a `LocalExternal` removes the instance; the eject command
stops the index before unmount (assert ordering); an integration test detaches an M0 image mid-life (through the guard's
timeout-safe detach) and asserts the instance is gone + an abort event fired. Depends on M4. Checks: `pnpm check rust`,
`pnpm check desktop`.

### M7 — Routing + MCP status correctness

**Intent:** reads and `cmdr://state` must reflect the drive's own index, not `root`'s.

- `routing::volume_id_for_local_path` (`routing.rs:37`): for a path under a registered non-root local mount, resolve to
  that volume's id (via the `VolumeManager` mount lookup / `path_to_id` it's registered under) instead of `root`. SMB and
  `mtp://` branches unchanged; keep it cheap on the enrichment/dir-stats hot path (mount-point lookup, cached like SMB's).
- This fixes `cmdr://state` `indexStatus` (`mcp/resources/volumes.rs` local branch) and dir-stats/status reads: an
  unindexed external drive now reports `off`, and `cmdr://state` stops disagreeing with `cmdr://indexing`.

Tests (real red→green): `get_volume_index_status_for_path` reports `off` for a mounted-but-unindexed external drive (red:
today `fresh`), `scanning`/`fresh` once enabled; a `volume_id_for_local_path` unit test for a registered external mount.
Parallelizable after M4. Checks: `pnpm check rust`.

### M8 — Frontend: checklist predicate, refusal copy, observable refusals

**Intent:** the local-external drive drives the LOCAL checklist; the SMB-named refusal stops being user-facing nonsense.

- Retarget the `isNetwork` predicate at **`IndexingDriveRow.svelte:180`** and **`IndexingDriveSummary.svelte:36`** (the
  two sites; `DETAILS.md:115` describes it) to key on the **volume's `category`** from the volumes store (`network` /
  `mobile_device` → network checklist; `local` → local checklist — the same source `active-media-volume.ts:45` already
  uses), NOT `volumeId !== ROOT_VOLUME_ID`. This three-ways correctly (root + `LocalExternal` are `local`; SMB is
  `network`; MTP is `mobile_device`) with no new backend field. `indexing-steps.ts`, `deriveSteps`, `LOCAL_STEPS`,
  `driveIndexState`, `driveIndexMenuActions`, `isDriveRow` unchanged.
- Generalize `SmbIndexGateReason` handling so `not_an_smb_volume` / `not_registered` become genuine **internal-error**
  cases (they can no longer happen for a healthy local drive) instead of "reconnect the drive" advice. Optionally rename
  the reason type drive-agnostic (blast radius: only `VolumeBreadcrumb.svelte::handleIndexRefusal`); keep the SMB variants
  (`credentials_needed`, `upgrade_failed`, `disconnected`) for the SMB path.
- Make the SMB gate observable: `ensure_direct_smb`'s early returns (`smb_index.rs:114,124,…`) log at `warn`/`info` so a
  future refusal isn't invisible (the reason this bug hid in the logs).
- i18n: adjust the affected `fileExplorer.navigation.driveIndex.*` copy in `en/*.json`; catalogs self-heal.

Tests: Vitest for the `category`-based predicate (`local` → local checklist; `network`/`mobile_device` → network) via the
existing `IndexingStatusBody` prop seam; a test that an internal-error refusal renders the internal-error copy.
Parallelizable immediately (no backend dependency). Checks: `pnpm check svelte`, `pnpm check desktop`.

### M9 — E2E (done: Rust integration test + live validation; NO CI E2E, by decision)

Two feasibility facts settle the shape:

1. **A full enable→scan→sizes→stop lifecycle test is `AppHandle`-bound and the repo has no mock-`AppHandle` harness.**
   `start_indexing_for` (and the whole registry/event pipeline) takes a real `AppHandle`; building a Tauri mock app solely
   for this test is a large, invasive infra investment out of proportion to M9. But the scan pipeline exposes an
   AppHandle-free seam: `scanner::scan_volume(ScanConfig, &IndexWriter)` runs the full walk + aggregate into a store, and
   `store::get_dir_stats_by_id` reads the result — the same seam `manager.rs` builds from an `IndexPathSpace`.
2. **The panic-class `hdiutil`/FSKit operation is deliberately kept out of CI.** Every disk-image test in the repo is
   `#[ignore]`d (`external_drive_fixture.rs`), so `pnpm check rust` compiles but never runs them — precisely because the
   2026-07-15 incident showed FSKit `msdos` can wedge the kernel on uncontrolled hardware. A Playwright E2E under
   `pnpm check desktop-e2e-playwright` DOES run in CI (GitHub macOS runners, uncontrolled FSKit), so attaching disk images
   there would reintroduce the exact panic-class operation the incident burned out. That's "unsafe from the harness" — the
   gate's escape hatch applies.

**Decision:** add a focused `#[ignore]` **Rust integration test** at the AppHandle-free seam — `external_drive_fixture.rs`
`fat32_mount_relative_scan_indexes_the_tree_with_sizes_and_null_inodes`: attach a real FAT32 image → populate a known tree
→ run `scan_volume` with the mount-rooted `IndexPathSpace` (MountRooted exclusion scope + FAT-derived untrusted-inode flag,
resolved from the real `detect_filesystem_for_path` exactly as `classify` does) → assert the drive's own index has the tree
under `ROOT_ID` by mount-relative name with recursive sizes aggregated (lower-bound asserts; macOS adds AppleDouble `._*`
sidecars on FAT) and FAT's derived inode is nulled. It's the only automated test exercising the exclusion + mount-relative
core on a real `msdos` filesystem; timeout-safe via the fixture's guarded detach; never runs in CI. **NO CI Playwright E2E**
(reason above). The full end-to-end lifecycle (drive appears `off` → enable → real `LocalExternal` scan, no SMB refusal, no
false-complete → sizes render → clean index stop, watcher dropped → detach, no stuck scanning row) was **validated live**
against the running dev app on a synthetic FAT32 image via MCP, 2026-07-15 (worktree `david/local-drive-indexing`); that
run is the end-to-end proof, and the component tests (M2 exclusions, M3 resolve/reconcile, M4 classify/resume, M5 inode,
M7 status) plus the fixture's attach/read/live-FSEvents tests cover each stage.

Checks: `pnpm check rust`; run the added `#[ignore]` test explicitly with
`cargo nextest run --run-ignored ignored-only -E 'test(fat32_mount_relative_scan_indexes_the_tree_with_sizes_and_null_inodes)'`.

## Docs to update (per the C.md/D.md contract)

- `indexing/CLAUDE.md`: the `LocalExternal` kind + split axes; the mount-relative-local-pipeline must-know; inode-untrusted
  on FAT; the eject-stop-before-unmount safety rule; a pointer to the testing note.
- `indexing/DETAILS.md`: the capability-axis split + rationale; the mount-rooted local scan/reconcile/live path (the M3
  seam); the freshness treatment (non-journaled, loads Stale, reconcile-to-Fresh on mount); the unmount/eject lifecycle
  and the DidUnmount-vs-WillUnmount-vs-eject honesty; the FAT-panic incident as the constraint; the synthetic-fixture
  how-to; the disk-image policy (Decision #1).
- `routing.rs` / `mcp/resources/volumes.rs` docs: external-drive status resolution.
- Frontend `src/lib/indexing/DETAILS.md:115`: the run-shape predicate replacing `isNetwork = !== root`.
- `docs/architecture.md`: one-line map update if external-drive support is described there.
- `docs/specs/index.md`: add this plan to "In progress"; wipe on ship once durable intent is in the colocated docs.

## Parallelization

The M0→M4 spine is strictly sequential (fixture → axes → exclusions → mount-relative pipeline → enable/resume): each
depends on the prior, and M3 is the load-bearing core. After M4 lands:

- M8 (frontend) has **no backend dependency** (it reads the volumes-store `category`) and can start immediately, in
  parallel with the whole spine. M5 (inode/`FilesystemKind`) and M7 (routing/MCP) are mutually independent and run
  concurrently after M4.
- M6 depends on M4 (needs the enable path + a live index to stop). M9 depends on all.

Default to sequential; we're not in a hurry, and the spine is where the risk is.

## Checks + wrap

- `pnpm check --fast` iterating; scoped `pnpm check <name>` per milestone; `--include-slow` before wrap. No raw
  `cargo`/`vitest`; never truncate checker output.
- Verify the data-safety / panic-adjacent paths yourself — re-run M3 (live-event correctness) and M6 (eject-stop) tests
  directly; don't integrate those on subagent trust.
- Before FF-merge to local `main`: rebase onto current `main`, clean the worktree dev data dir (~1 GB), fold
  `local-drive-indexing-probes/fat32-probe.sh` into the M0 helper (or remove it).
