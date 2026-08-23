# Local FS watch details

Read this before any non-trivial work in `watch/`: editing, planning, reorganizing, or advising. Must-know guardrails
are in `CLAUDE.md`.

This area owns the drive watcher, the live/replay event loops, the unbounded ingestion buffer + pressure model,
removal-storm coalescing, rename-detection-by-inode, and the churn-monitor spike. Points outward: the registry / phase
machine / manager wiring in `../lifecycle/DETAILS.md`; the reconciler, the per-subtree rescan throttle, and the
post-replay verification COST-BOUNDING (the two teeth) in `../reconcile/DETAILS.md`; the writer message protocol
(`MoveEntryV2` / `DeleteSubtreeById` / delta propagation) and the honest-sizes model in `../writer/DETAILS.md`;
`IndexPathSpace`, firmlink normalization, and the FAT/exFAT inode-nulling rule in `../paths/DETAILS.md`; the
`extract_metadata` primitive at `../metadata.rs` (documented in the [hub](../DETAILS.md)).

## Module structure

- **watcher.rs** — the drive-level filesystem watcher. macOS: FSEvents via `cmdr-fsevent-stream` with event IDs and
  `sinceWhen` replay. Linux: `notify` (inotify) with recursive watching and a synthetic event counter. Other platforms:
  stub. `supports_event_replay()` lets callers branch on whether journal replay is available. `start_branches` +
  `watch_branch` are the branch-watched entry points (below).
- **branches.rs (+branches/tests.rs)** — `WatchScope`, `BranchWatch`, and the admission rule a live loop reads events by
  (below).
- **event_loop.rs** — holds only what more than one loop uses: `merge_fs_events` (deduplication with flag priority),
  `open_read_conn_with_retry` (read-connection open at each loop's start), `ReplayConfig` (the manager→replay bridge
  struct, which also carries the volume's stop token down so nothing here looks one up), the cross-loop flush/gap
  constants (`LIVE_FLUSH_INTERVAL_MS`, `THROTTLE_SWEEP_INTERVAL_MS`, `JOURNAL_GAP_THRESHOLD`), and the
  ingestion-pressure model (`INGESTION_BACKLOG_WARN`, `INGESTION_HARD_CAP`, `classify_ingestion_pressure`,
  `BacklogTracker` / `report_backlog`). Re-exports `run_live_event_loop` / `process_live_batch` /
  `run_replay_event_loop` so external callers (`lifecycle/manager.rs`, `scan_completion.rs`, the stress tests) keep
  stable paths.
- **event_loop/live.rs** — `run_live_event_loop` (real-time processing after scan completes), `process_live_batch`
  (three-phase; below), and the live-path helpers `mark_pending_and_drain` / `split_parent_and_name`.
  `detect_renames_by_inode` lives here.
- **event_loop/replay.rs** — `run_replay_event_loop` (cold-start journal replay, two-phase, boot disk only; hands off to
  live mode and spawns verification), the replay-only bounded-buffer constants (`MAX_AFFECTED_PATHS`,
  `REPLAY_EVENT_COUNT_LIMIT`, `REPLAY_DEDUP_BATCH_SIZE`), and `defer_replay_rescan` / `flush_replay_batch`. Deferred
  `MustScanSubDirs` anchors are collected into a `HashSet` (dedup) and handed to the live drain after replay; NO
  subtree-count cap and no full-scan escalation on churn (the live drain dedups, ancestor-collapses, and
  per-subtree-throttles them). Full-scan fallback stays only for a genuine journal purge, >10M replayed events, or a
  watcher-channel overflow; its cause rides the `oneshot::<RescanReason>`. ⚠️ It also OWNS the volume's ground for the
  replay phase (`ReplayConfig::ground`, taken by `start_replay`) and drops it beside the `scanning.store(false)` that
  ends that phase, rather than where the task ends — this same task runs the live loop afterwards, and ground held that
  long would refuse every scan and search walk on the boot disk until quit. Why replay claims ground it never walks:
  `../lifecycle/DETAILS.md` § "The two single-flight questions a scan has to ask".
- **event_loop/verification.rs** — `run_background_verification` + `verify_affected_dirs` (below).
- **event_loop/verify_guard.rs** — the two pure cost-bounding decisions for verification (`VerifyVerdict`,
  `HUGE_DIR_CHILDREN`). Structural role below; the cost-bounding RATIONALE is canonical in `../reconcile/DETAILS.md` §
  Bounding verification cost.
- **event_loop/storm.rs** — removal-storm coalescing helpers (`REMOVAL_STORM_THRESHOLD`, `STORM_GROUP_PREFIX_DEPTH`).
- **event_loop/tests/** — `activity` / `ingestion` / `merge` / `rename` / `split_parent` clusters plus shared fixtures
  in `mod.rs`.
- **churn_monitor.rs (+churn_monitor/)** — the off-by-default per-subtree churn observability spike (below).
- **activity_monitor.rs (+activity_monitor/)** — the per-folder activity tap over the corrected stream, plus
  `BatchObservers`, the pair `process_live_batch` takes (below).

## Watching what a search walked

A volume the user turned indexing on for is walked whole and watched whole. A volume a SEARCH walked
(`../lifecycle/cover/`, `Activation::WriterOnly`) has a few covered branches and nothing else, and it starts with no
watcher at all — a `DriveWatcher` is created only by `start_scan` and `start_replay`. `branches.rs` is what gives that
shape a live loop, and it's the whole of why walk-written coverage carries no expiry (plan Decision 9): a walked branch
is kept as current as an indexed drive's rows, so nothing has to be re-walked and nothing has to age out.

**`WatchScope` is per LOOP, and both arms carry the branch set.**

- `WholeVolume(watch)` — a scanned volume. Every event is the index's business, EXCEPT one landing in ground a cover
  walk is covering right now.
- `Branches(watch)` — a search-built index. Only events inside covered branches are its business.

**The three verdicts** (`BranchWatch::admit`), by the DEEPEST branch containing the event's path:

1. no walk on it ⇒ **process**;
2. a walk covering it ⇒ **buffer**, released when that walk ends;
3. no branch at all ⇒ **discard** on a branch-watched volume, **process** on a whole-watched one.

**Why buffering, on every volume.** Letting a live loop write into a branch a walk is covering is `cover/live/mod.rs`'s
two-walks collision one level down: the parallel walker allocates fresh ids, `insert_entries_v2_batch` is
`INSERT OR IGNORE`, and whichever row loses takes its subtree with it. Discarding instead drifts the branch's aggregates
with nothing to signal it. So the events wait — the per-branch shape of the scan-completion handshake, which buffers a
whole volume's events for the same reason. Deepest-match is what lets a walk over a POCKET inside an already-live branch
buffer while the rest of that branch keeps flowing.

**Buffer overflow re-lists rather than replays.** Past `BRANCH_BUFFER_CAP` (100,000 events) the buffer stops being a
complete record, so the branch is dropped and queued as a `MustScanSubDirs` anchor when its walk ends.

**Branches absorb what they cover, and that is the ONE collapse rule** (`State::insert` /
`State::absorb_settled_under`). A branch arriving over settled ones retires them, however it arrives: a walk registering
it, a resume restoring it, an explicit `collapse_to`. A walk finishing absorbs whatever settled underneath it while it
was live. Two reasons, and the second is a correctness one:

- The set is the shortest description of the ground the volume watches branch by branch, and every entry it doesn't need
  is one more thing to persist, restore, and reason about.
- A settled descendant entry under a branch a walk is covering RIGHT NOW is the deepest match, so its events would
  PROCESS live while the walk writes the same names — the two-writer collision the buffering exists to prevent, arriving
  through the set itself.

❌ An entry with `walks > 0` is never absorbed: its buffer belongs to that walk, and dropping the entry would strand the
events it holds. Settled entries are always safe, because a branch only buffers while a walk covers it.

**The set is a `BTreeMap` keyed by path, and both of its questions are bounded by the PATH rather than by the set.**
"What holds this?" (`deepest_containing`, once per event on the live hot path) walks the path's own ancestors via
`self_and_ancestors`; "what sits under this?" (`absorb_settled_under`, and the sweep re-anchoring) is a range scan
bounded by `descendant_range_prefix`. Both primitives live in `../paths/path_prefix.rs` and are component-aware, so
`/vol/buildcache` never answers to a branch at `/vol/build`. Phased indexing registers an entry per frontier root, so a
mid-phase set holds thousands: a container that scanned itself cost seconds of HELD LOCK per churn burst and made
registering a wide frontier quadratic. Numbers before and after: `docs/notes/branch-set-cost-2026-08-15.md`.

⚠️ The volume root is its OWN descendant-range prefix, so `State::descendants` also tests that the key is longer than
the path it's under. Without it a branch at the volume root absorbs itself the moment its walk ends, and the volume
silently stops watching everything it just covered. Anchor:
`branches::tests::a_branch_at_the_volume_root_holds_everything_under_it`.

**`collapse_to(root)` mutates the set the running loop is READING**, in place, and then persists. The live loop and its
reconciler each captured an `Arc<BranchWatch>` at `ensure_branch_watch`. ❌ Never express a collapse as
`branches::clear` plus a begin/finish pair: `clear` calls `forget`, so `live_for` mints a brand-new set nobody is
reading — the persisted meta would say `["/"]` while the loop filtered against the stale entries for the rest of the
session, and `is_branch_confined` would read that same stale `Arc` and keep the shallow sweep disabled until the next
launch. It fails silently and only at runtime. (`start_scan`'s `clear` is safe only because the loop is torn down and
replaced in the same breath.) Anchor: `branches::tests::the_branch_collapse_is_visible_to_the_running_live_loop`.

**A coalesced sweep above the branches is RE-ANCHORED onto them, never dropped.** FSEvents reports "a lot changed under
here" at a shallower path than the branch; a plain prefix test would lose every change inside covered ground behind one
`MustScanSubDirs`. On a whole-watched volume the original sweep is kept too — unless a walk is covering one of the
branches under it, in which case it waits rather than walking into the walk.

**The reconciler is confined by the same scope** (`EventReconciler::within`, `WatchScope::may_walk`): it never walks
ground a cover walk holds (any volume), and on a branch-watched volume never outside the branches — an escalation anchor
in unwalked ground is left to the next search, which is where growing coverage belongs. A branch-watched volume also
never routes a shallow `MustScanSubDirs` to the visible scanner, whose shallow arm rescans the WHOLE volume.

**The journal position follows the STREAM, not what was processed.** A branch-watched loop discards most events, and
`process_live_batch` only advances `last_event_id` for what it wrote; left there, a volume with quiet branches would age
its stored position until the next launch's replay gap is too wide to be worth replaying. `safe_event_id` advances it
from every event SEEN, and returns `None` while anything is buffered (advancing past a held event would let a restart
skip it).

⚠️ Two limits left standing there, both self-healing through the frontier. `process_live_batch` sends its own
`UpdateLastEventId` for what it processed, so a processed event with a higher id than a HELD one can still carry the
position past it — a crash in that window loses that one change from the journal replay. And a branch is persisted when
its walk FINISHES, so a crash mid-walk leaves covered rows with no branch entry: they stay covered and served (Decision
5's covered-but-stale) but unwatched until something walks that ground again.

**Where the set lives, and how it comes back.** In memory per volume (`live_for`), and on the volume's own database as
`meta.walk_covered_branches` — index-relative, so a drive that returns at a different mount point still finds its
branches. It comes back when the volume's index does (`state::resume_branch_watch`, the `WriterOnly` arm of
`start_indexing_for`), which is the first moment anything can read that coverage: an unregistered volume answers neither
sizes nor coverage questions. ❌ There is no launch-time pass, and adding one would start writers for drives nobody is
asking about. `resumed_for` restores INTO whatever the session already holds rather than replacing it.

**Replay, or an honest epoch bump.** A resumed watch starts from the stored `last_event_id` so FSEvents replays what
happened while the app was off. When it can't (no replay support, no stored id, or a gap past `JOURNAL_GAP_THRESHOLD`)
the resume bumps `current_epoch`: the rows stay covered and trusted (Decision 5) and the read side renders them stale
rather than current. ❌ Never on the walk path — a bump right after a walk would mark rows stale that were written a
second ago.

**Two switches, one gate** (`master::branch_watch_allowed`): the master switch and the sticky per-drive `user_disabled`
veto. NOT the `user_enabled` opt-in — that means "the user turned this drive on for background indexing", which is
exactly what someone searching an unindexed drive did not do. A vetoed drive gets no watcher, so its walked ground stays
covered and served but stops being kept current; it is NOT re-walked (the walk marked those directories listed, so the
frontier never offers them again).

**Branch-watched or whole-watched, never both.** `ensure_branch_watch` declines when a watcher is already running, and
`start_scan` retires the branch set (`branches::clear`) because a scanned volume answers for every path. A walk on a
whole-watched volume registers its branches only to buffer for the walk's duration (`AfterWalk::Forget`).

**`AfterWalk::Forget` asks whether the LOOP already answers for the ground, ❌ never whether a branch watcher is up**
(`IndexManager::after_walk`). The two differ in exactly the cases that matter: a `DriveWatcher::start_branches` failure
is non-fatal and logged, and a vetoed drive never gets a watcher at all, yet both leave ground that IS covered.
Forgetting there dropped the persisted set and with it the only record that anything walked that ground — the one thing
that tells a partial index somebody's searches built apart from one nothing ever walked. So the set persists whenever
the volume isn't whole-watched, and covered-but-unwatched is a state the index states honestly: the epoch bump on the
next resume is what stops those rows reading as current.

**Local volumes only.** This is a local-filesystem watcher, so a share or a phone gets no branch watch: its walked
branches are exactly as stale as its scanned index, which loads Stale on every launch anyway. ⚠️ Known gap: SMB's own
change-notify translator (`../transports/smb/`) writes through the volume's writer unfiltered, so a cover walk on a
share races it the way a local walk used to race the local loop.

## Platform split: why Linux watches branches and macOS watches the volume

On macOS a branch watch is one FSEvents stream over the VOLUME ROOT plus the scope's path test. A stream costs nothing
per directory below its root, and rooting it at the volume is what makes `sinceWhen` replay the whole journal — a branch
added last session would otherwise have no history to replay. `watch_branch` is a no-op.

On Linux `notify`'s recursive mode registers ONE inotify watch per directory against `max_user_watches` (8,192 by
default on many distributions), so `start_branches` watches the covered branches themselves and `watch_branch` adds each
new one to the RUNNING watcher (which `notify` supports, so no stream restart per walk). The registration walks the
branch adding watches, slow enough on a big tree to matter, so it runs on a blocking task rather than on the search
thread holding the registry lock. The window that opens is one the branch is being WALKED across, and its events buffer
until the walk ends either way. ❌ Don't unify these by watching the volume root on Linux: it spends the machine's watch
budget on ground we don't answer for, and fails outright on a big tree.

## Data flow (live + replay)

```
Live mode:
  |-- macOS: FSEvents -> reconciler (resolve_path -> entry IDs) -> UpsertEntryV2/MoveEntryV2/
  |          DeleteEntryById/DeleteSubtreeById -> writer -> SQLite
  |-- Linux: inotify (via notify) -> same pipeline
  |-- The loop holds a READ connection for integer-keyed path resolution (never a write one)
  |-- Events deduplicated by normalized path, flushed every 1s; writer flush before emit ensures atomic dir_stats
  |-- process_live_batch is three-phase: dir creates (depth-sorted) -> rename pre-pass (inode -> MoveEntryV2)
  |          -> remaining events (removal-storm coalescing). Flushes between phases so later phases see committed state.

Cold-start replay (boot disk only, has_event_journal()):
  |-- sinceWhen replay -> two-phase drain -> hands off to live mode -> spawns run_background_verification
```

## Unbounded ingestion buffer

The watcher→loop channel (`mpsc::unbounded_channel`, created in `lifecycle/manager.rs`'s `start_scan` / `start_replay`)
is UNBOUNDED so the FSEvents forward task (`watcher.rs`) NEVER blocks. A bounded 20K channel used to backpressure:
during a long replay the loop drains slower than FSEvents produces → the channel fills → `send().await` blocks the
forward task → the upstream cmdr-fsevent-stream buffer overflows and sets its flag → `WatcherChannelOverflow` → a forced
full scan (measured firing at a 100M-event replay). So a slow drain, not real data loss, threw away a working replay.
Decoupling ingestion from processing removes that cascade. Memory is bounded by the loop instead of the channel, via
`classify_ingestion_pressure(event_rx.len())` (checked on each loop's flush tick and at the replay dedup-batch
boundary):

- **Healthy** (`<= INGESTION_BACKLOG_WARN = 20_000`): do nothing. Steady state sits well under this (each event ~300 B).
- **FallingBehind** (`> 20_000`): REPORT the backlog (rate-limited to one line per `INGESTION_WARN_INTERVAL = 5 s`),
  never drop. This is a metric, not an action — the old forced-scan point is now merely a signal. The report is decided
  by TREND, not depth: `BacklogTracker::sample` compares each sample against the previous one and returns
  `(warn, message)` — a shrinking queue is progress and goes out at `info` with the drain rate and an ETA ("working
  through a backlog of 787,194 events (down 43,866 in 5.0s, ~89s left at this rate)"), while only a flat-or-growing one
  warns ("ingestion queue not draining"). Why: depth alone can't distinguish a healthy cold start from a stuck queue, so
  an 800k-event replay that drained monotonically to completion emitted ~90 "falling behind" warnings while nothing was
  wrong — the surest way to train everyone to ignore the log. `IngestionPressure::Healthy` calls `reset()`, ending the
  episode so a later backlog is never compared against a depth from minutes ago. Both replay phases and the live loop
  share one tracker each via `report_backlog`.
- **Overflowing** (`> INGESTION_HARD_CAP = 5_000_000` ≈ 1.5 GB): DELIBERATELY fall back to a full scan (RescanReason
  `IngestionBacklog`) — OUR decision that we're hopelessly behind, at a far higher threshold than the old OS overflow,
  and comfortably below the global 16 GB memory watchdog. The live loop spawns `manager::perform_registry_rescan`; the
  replay loop uses its `fallback_tx`. The genuine upstream-drop `WatcherChannelOverflow` path is preserved
  (cmdr-fsevent-stream can still drop before our forward task reads). `classify_ingestion_pressure` is pure/unit-tested;
  the repro (a backlog past the old 20K cap absorbs without forcing a scan) lives in `event_loop/tests/ingestion.rs`.

## Rename detection by inode (FS identity, not intent tracking)

A rename used to land as `DeleteSubtreeById(old_path)` + `UpsertEntryV2(new_path)`, which wiped the renamed dir's
`dir_stats` and dropped the entire subtree from the index. Result: the dir's size column flipped to the "dir"
placeholder until the next full scan or per-navigation verification healed it. The fix uses inode as stable identity:
`process_live_batch` runs a `detect_renames_by_inode` pre-pass between the dir-create phase and the general phase. For
each `item_renamed` event whose new path stat'd OK, it looks up the inode via `find_entry_by_inode()` and, if the
existing row is at a different `(parent_id, name)`, sends a `MoveEntryV2` that rewrites the row in place (preserving
`entry_id`, preserving `dir_stats`). The OLD-path event of the same rename pair stays in the batch but resolves to None
after the post-pre-pass flush, so it silent-no-ops. This is filesystem-driven, not intent-driven: no rename buffer, no
cross-batch state, no Cmdr-vs-external rename detection. Just `stat()` + a DB lookup per `item_renamed` event. The
kernel preserves directory inodes across rename on APFS, HFS+, ext4/btrfs/XFS, and NTFS, which covers every internal
Mac/Linux disk and most external drives.

**Inode is NOT trusted on FAT/exFAT.** A derived `st_ino` there is unstable and a delete+create aliases a fresh file
onto a freed inode, so the pre-pass would FALSE-MATCH it as a move and re-home the deleted entry's `dir_stats` onto an
unrelated file. Every local write path stores `inode: None` on such a `LocalExternal` volume, making
`find_entry_by_inode` inert and every change fall back to the safe delete+create. The volume-level trust decision
(`trust_inode` / `inodes_trustworthy`) is canonical in `../paths/DETAILS.md`.

## Removal-storm coalescing (`event_loop/storm.rs`)

`rm -rf` is depth-first (unlink all files, then rmdir each emptied dir, the root LAST), and FSEvents reports that order
faithfully, so the cheap one-`DeleteSubtreeById` path used to fire only at the very END — after the reconciler chewed
through hundreds of thousands of per-file removals (2–5 minutes on a 60 GB tree). `process_live_batch` now synthesizes
the coalescing the kernel didn't: per 1 s batch it groups removal events by a component-capped prefix
(`STORM_GROUP_PREFIX_DEPTH = 8`, the GROUPING KEY only) and, when a group exceeds `REMOVAL_STORM_THRESHOLD` (200),
queues ONE `queue_must_scan_sub_dirs` anchored at the group's **deepest common ancestor** — NOT the capped prefix, which
on a deep incident path (~11 components) would re-list a whole worktree instead of just `target`. From then on, removal
events under a queued-or-active rescan prefix are dropped, with three load-bearing rules:

- the reconciler reads the active rescan path from a shared slot (`active_rescan_path`, set at spawn / cleared on
  completion — `start_next_rescan` pops the path out of `pending_rescans` before spawning);
- only STRICT descendants are dropped (the deleted root's own `rmdir` must take the normal per-file path →
  `DeleteSubtreeById`, because `reconcile_subtree` on a root gone from disk deletes nothing);
- every dropped event re-queues the anchor (set-dedup makes it idempotent) so a sub-threshold tail batch after the walk
  already listed those dirs still gets a follow-up.

A cheap complement below the threshold: each batch's removals are sorted dirs-before-files, shallower-first, so a small
dir's `rmdir` processes before its children's unlinks and turns them into cheap unknown-path skips. Net: index latency ≈
15–30 s after the `rm` finishes instead of minutes, and ~20× less CPU/IO. Routing through the rescan queue (not a
bespoke "big delete" path) inherits dedup, ancestor-collapse, 1-concurrency, the held-hourglass tier, and the completion
emit for free. The rescan queue itself and its per-subtree throttle live in `../reconcile/DETAILS.md`.

## Background verification (structure)

`run_background_verification(affected_paths, writer, events, cancel)` runs off the async pool AFTER live mode starts (so
the app is responsive immediately) and readdir-diffs each directory the replay touched — FSEvents journal replay
coalesces events, so a child deletion may only show as "parent dir modified" and a new child may get no individual
creation event, so each affected parent is re-listed and reconciled with the DB. Corrections go through the writer
channel, which serializes them with live writes. It is **root-scoped (boot disk only)**: it reads the ROOT `ReadPool`
(`get_read_pool()`), resolves against root's index, and publishes under `ROOT_VOLUME_ID` — post-replay, and replay is
gated on `has_event_journal()`, so it never runs for a mount-rooted volume.

`verify_affected_dirs` is the lock-free, two-phase DB-vs-disk reconcile it calls (it acquires NO lifecycle lock): Phase
1 (sync, SQLite) materializes each affected path's DB children off the `ReadPool`; Phase 2 (`spawn_blocking`) readdirs
disk and diffs, sending `UpsertEntryV2` / `DeleteEntryById` / `DeleteSubtreeById` / `PropagateDeltaById` corrections. It
consults `verify_guard.rs`'s two pure decisions to cap the per-directory cost. **The cost-bounding rationale (the two
teeth: the `LIMIT`-probe before the snapshot and the `read_dir` iteration cap, and why a declined dir must NOT be marked
`listed_epoch = 0`) is canonical in `../reconcile/DETAILS.md` § "Bounding verification cost (the two teeth)".** Don't
restate it here.

## Churn-monitor spike (`churn_monitor.rs`)

Read-only per-subtree churn observability for the sealed-subtrees spike, off unless `CMDR_CHURN_SPIKE` is set. It hooks
`process_live_batch`, which takes a `BatchObservers` by `&mut` so BOTH live loops (`live.rs` post-scan and `replay.rs`
Phase 3 post-replay) are covered by construction — hooking only one of them measured nothing on the whole cold-start
replay route, and `churn_monitor/tests.rs::every_live_loop_owns_a_real_churn_observer` now guards that. It rolls every
path's churn up the ancestor chain and logs one `indexing::churn` rollup per period (top-N directories by rolled-up
count, with a distinct-churny-children signal). Writes no index state and changes no behaviour. Pure and clock-injected,
so it's promotable into real churn accounting rather than throwaway. Collection and analysis handover:
`docs/notes/churn-observability-spike.md`.

## The activity tap (`activity_monitor.rs`)

The second observer on the same batch, and a different measurement. The churn monitor reads RAW deduplicated paths,
which is right for "how hard does this subtree churn" and wrong for "did something meaningful happen here": there a
rename is a create plus a delete, and an `rm -rf` is sixty thousand removals. The tap folds the batch AFTER the
corrections, and emits one `IndexEvent::FolderActivity` per batch carrying a `FolderChangeRollup` per folder. What the
rollups are FOR is the host's business; this module names no consumer. (Cmdr's is `agent/wake/`.)

**Two observers, one struct.** `BatchObservers` bundles them because `process_live_batch` sits at exactly seven
arguments and `clippy::too_many_arguments` defaults to seven, which `clippy.toml` doesn't raise. Both scanners
(`churn_monitor/tests.rs` and `activity_monitor/tests.rs`) assert every live-batch driver builds one with
`BatchObservers::from_env(`, which is the hole the `&mut` can't cover: a third live loop in a new file, or an existing
one downgrading to the test-only disabled pair.

⚠️ **Three of the four counters are UNREACHABLE from the natural reading point**, and taking "fold the corrected stream"
literally ships a tap that counts almost nothing. Each is wired explicitly in `process_live_batch`:

- **Matched renames.** `detect_renames_by_inode` `retain`s its matches out of the batch, so only the FAILED matches
  reach Pass 2. It returns the matched paths for exactly this reason; without them the tap counts the noise, drops the
  signal, and a rename-only batch reports nothing at all.
- **Storm-coalesced removals.** The storm path queues an anchor as a rescan and drops every strict-descendant removal,
  so a sixty-thousand-file delete inside a surviving folder would contribute nothing. The anchor is surfaced and counted
  as ONE removal inside the anchor itself — the only input credited to the named folder rather than its parent, because
  the storm happened IN it. (`storm::scope_to_requeue` keeps the anchor's own removal, so a deleted anchor also shows up
  once through the normal path.)
- **Directory creations.** `pending_events.drain()` splits the batch in two and Pass 1 consumes the `dir_creations`
  vector, which no later pass sees. Folded in at Pass 1.

⚠️ **The flags are not one-hot.** One coalesced `FsChangeEvent` can carry `item_created`, `item_removed`, and
`item_renamed` at once, so `kind_of` picks one: **renamed, then created, then removed, then modified**. A different
order moves what a consumer reads out of the counts materially, which is why it is one documented function with a test
rather than an incidental branch order. An event carrying none of the four (a bare `must_scan_sub_dirs` anchor) counts
as nothing, or every rescan would inflate into activity.

**A directory's own event counts in its PARENT.** A rollup describes the folder a change happened IN, and `/a/b`
appearing is a change in `/a`. The storm anchor is the documented exception above.

**Nothing survives a batch**: the map drains on report, so memory is bounded by one batch's folders, and a per-batch
folder cap (4,096, the same order as the host channel's bound) stops a pathological batch handing a host half a million
rollups to loop over on the live-loop thread. Past it the extra folders are dropped and logged.
