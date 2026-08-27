# Reconcile details

Read this before any non-trivial work in `reconcile/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

This area owns the mechanisms below. Points outward: the honest-sizes data model (`listed_epoch` / `min_subtree_epoch` /
`current_epoch`), the `dir_stats` ledger, the `BulkReconcileGuard` debt-recording contract, and writer-wait attribution
are canonical in `../writer/DETAILS.md`; the guarded reader and the `should_exclude` policy in `../scanner/DETAILS.md`;
the live event loop, removal-storm coalescing, and the `verify_affected_dirs` / `verify_guard.rs` code in
`../watch/DETAILS.md`; `resolve_path_under` + mount-relative paths in `../paths/DETAILS.md`; the compute math in
`../aggregator/DETAILS.md`.

## Non-destructive rescan (reconcile, not truncate)

A FIRST scan (empty DB) truncates and bulk-builds — fastest on an empty table. A RESCAN of an already-populated index
RECONCILES in place: it walks the tree and diffs each dir against the DB, writing only the differences, so the last-good
index stays visible (stale) throughout and a mid-rescan disconnect leaves the prior data intact. Perf + correctness were
gated before building this; the evidence is in `docs/notes/m3-reconcile-rescan-gate.md`.

**The LOCAL reconcile's cost is the open question.** Measured on the boot volume: the serial reconcile walk took 1,309 s
where the parallel fresh scan of the same tree took 68.1 s, and 92.3% of that time sat inside the directory read
(`docs/notes/reconcile-latency-spike.md`). The `lstat` share of that read is now gone — `read_fs_children` batches via
`getattrlistbulk` on macOS (see "The shared local read" below), which on a boot disk was 69% of read time — but the walk
is still serial against a parallel fresh scan. Replacing the local rescan with a fast parallel build that swaps in
atomically is under evaluation, including the traps that shape it (SQLite has no `ALTER INDEX ... RENAME`, `start_scan`
clears `scan_completed_at` before the scan runs, and `MutationTracker::bump` can't tell which table changed):
`docs/notes/swap-scan-feasibility.md`.

**Before trusting that speed comparison, read `docs/notes/indexing-benchmarks-2026-07-21.md`.** Measured on an idle
machine, the fresh parallel scan takes 52.7 s and the reconcile 476.9 s — but that scan left the index ~10% short
(6,001,637 rows, versus 6,663,048 after the reconcile filled in the five subtrees it had skipped). The parallel walk
buys part of its speed by giving up. What it gives up on is a genuinely unresponsive mount, not a busy machine: the
abandonments were `LOCAL_LIST_TIMEOUT` and the 32-consecutive-failure give-up budget firing inside a phone's File
Provider mount. (❌ Not "rayon contention" — the walker has never used rayon, see `../scanner/walker/CLAUDE.md`.) A
SCOPED walk doesn't meet that shape unless the scope contains such a mount: measured over four real trees up to 1.2M
entries, the parallel walker wrote exactly the reconcile's row count and abandoned nothing
(`docs/notes/cover-walk-primitive-2026-08-05.md`).

**Mode predicate.** Both scan entry points pick reconcile vs truncate from the entry count read off the live read
connection BEFORE any truncate, but the threshold differs by path:

- **LOCAL (`start_scan`, `local_rescan_reconciles`): `entry_count > 1 && prior_scan_completed`.** `create_tables` →
  `ensure_root_sentinel` always inserts the ROOT row (id=1), and `TruncateData` re-inserts it, so a never-scanned DB has
  `entry_count == 1`, not 0. The `> 1` half routes a populated index (rows BEYOND the sentinel) to reconcile and a
  fresh/sentinel-only DB to the fast parallel bulk build — a `> 0` test would send a brand-new user's FIRST `/` scan
  down the serial reconcile (the onboarding regression). The **`prior_scan_completed` half is the completeness gate**
  (snapshotted via `get_index_status().scan_completed_at.is_some()` BEFORE the scan-start `DeleteMeta` clears it):
  reconcile ONLY a previously-COMPLETED index. A populated-but-never-completed PARTIAL takes the fast parallel rebuild —
  reconcile's serial per-dir walk over an add-everything delta is dramatically slower than a parallel bulk rebuild when
  the index is only a small fraction complete (a 4%-complete partial made the app look hung for ~15 min on a real `/`).
  NETWORK keeps reconcile-the-partial unchanged (a NAS rescan is slow, so keeping the partial visible is worth more, and
  network partials are small). Pinned by `manager::tests::local_rescan_reconciles_only_beyond_the_root_sentinel`.
- **NETWORK (`start_volume_scan`): `get_entry_count(...) > 1`.** Same sentinel reasoning and same `> 1` rule: a first
  SMB/MTP connect carries only the ROOT sentinel, so `> 1` routes it to the fast bulk build. (The two predicates are
  kept in lock-step; if one moves, move both.)

"Populated" is true for both a prior COMPLETED index and a persisted PARTIAL, so both are rescanned non-destructively,
never blanked. `clear_index` deletes the DB (so the next scan sees a sentinel-only table ⇒ truncate path).

**Why reconcile sidesteps the catastrophic INSERT OR REPLACE.** Reconcile writes ONLY changed rows: a matched-but-
unchanged row is diffed and skipped (never re-UPSERTed), so a no-op rescan over an unchanged tree issues ZERO entry-row
writes and never touches the `INSERT OR REPLACE`/`platform_case` B-tree path (the ~30-min-on-5.5M trap that forced
truncate-first). Orphans don't accumulate either: a COMPLETE reconcile's per-dir delete branch removes any DB child
absent from the live listing, including a whole subtree under a re-listed parent — so an interrupted→complete cycle
self-heals. An epoch-based orphan sweep is prototyped (and tested) in `reconcile_correctness.rs` as optional insurance
for the never-completes-a-rescan user, deferred as a follow-up.

**The diff is hardlink-dedup-aware, or it never converges.** The writer nulls `logical_size` / `physical_size` on every
occurrence of a multi-link inode past the first, so each inode's bytes count exactly once (`handle_upsert_entry_v2` →
`IndexStore::has_sized_entry_for_inode`, in `../writer/entries.rs`). The live snapshot still carries the REAL size, so a
naive `snap.logical_size != db_row.logical_size` reads that intentional NULL as a mismatch on EVERY pass: the diff emits
`UpsertEntryV2`, the writer re-nulls the row, the next reconcile re-sends it, forever. `diff_dir_against_db` therefore
skips the size half when `db_row.logical_size.is_none() && snap.nlink > 1`, comparing mtime alone; `verifier.rs` makes
the same call in its per-navigation diff. Two properties hold this together: `nlink > 1` (not the NULL alone) gates the
skip, so a file that drops back to ONE link is detected as changed and its real size comes back; and a first-occurrence
row with a real DB size keeps comparing on size, since the NULL is what marks the deduped occurrence. Measured cost of
getting it wrong (production index, 2026-07-23): 393,162 file rows at `logical_size IS NULL` index-wide (6.7% of 5.88M
files), and one WebKit cache directory holding 63,690 of them was re-walked 49 times in a day for 3,345,355 of that
day's 3,968,781 row deltas. Pinned by `reconciler::tests::reconcile_deduped_hardlink_writes_nothing_on_a_repeat_pass`
(plus the mtime, drop-to-one-link, and sized-occurrence cases beside it). The FSEvents replay verifier
(`../watch/event_loop/verification.rs`) is unaffected: it only adds missing children and deletes vanished ones, never
compares sizes.

**The single-aggregate coverage constraint (load-bearing).** After the reconcile walk, the rescan path stamps every
re-listed dir (`MarkDirsListed`) and runs ONE bottom-up `ComputeAllAggregates`. It must NOT fire
`PropagateMinSubtreeEpoch` per dir: the gate measured per-dir propagation across ~37k dirs at ~2× SLOWER than a truncate
rebuild (the ancestor-walk degenerates toward O(dirs × depth) when every dir re-stamps to the same new epoch), while a
single bottom-up aggregate is faster than truncate. `finish_reconcile` sends `ComputeAllAggregates { source: Sql }`, so
the aggregate recomputes coverage AND sizes for the whole tree from the committed rows in one O(dirs) bulk-SQL pass. A
reconcile's own writes (`UpsertEntryV2`/`Delete*`, never `InsertEntriesV2`) leave the accumulator maps empty, but the
finish does NOT rely on that: declaring `Sql` — not sniffing map-emptiness — is what keeps an interleaving verification
subtree scan's map pollution from zeroing every out-of-subtree dir (see `../writer/DETAILS.md` § "The full-aggregate
source contract"). Per-dir `PropagateMinSubtreeEpoch` stays ONLY for the small-scope LIVE reconciles
(`reconcile_subtree`: per-navigation verifier, `MustScanSubDirs`, SMB-overflow `FullRefresh`), where the chain is short.

**Decision: the full reconcile suppresses per-entry ancestor propagation (`SetDeltaPropagation`).** The single-aggregate
rule governs the FINISH; this governs the WALK. Each `UpsertEntryV2`/`DeleteEntryById`/`DeleteSubtreeById` the diff
emits would otherwise auto-walk the ancestor `dir_stats` chain — O(entries × depth) across an entire pass. On a large
delta (a 270k→6M partial-completion) that wedged the writer for hours: the channel stays full, so the walk thread parks
on `send` and the app can't drain. It's also pure waste, because the FINISH's one `ComputeAllAggregates` recomputes
every dir's `dir_stats` from the entries table anyway. So both full-reconcile walkers
(`local_reconcile::run_local_reconcile`, `volume_scanner::reconcile_volume_via_trait`) bracket their BFS with
`reconciler::BulkReconcileGuard` — it sends `SetDeltaPropagation(false)` before the walk and restores `true` on EVERY
exit (clean finish, cancel, empty-root, disconnect, error, panic) via `Drop`. The writer keeps everything else under
suppression (entry insert/update/delete, hardlink dedup, the new-directory zero-valued `dir_stats` row init) — ONLY the
ancestor PROPAGATION is skipped. **Why the LIVE path keeps propagating:** `reconcile_subtree` and the FSEvents handlers
have NO final full aggregate, so their per-entry propagation IS the mechanism that keeps `dir_stats` correct. **Don't
re-add per-entry propagation to the bulk path** (it reintroduces the hours-long wedge);
`bulk_reconcile_suppresses_per_entry_propagation_until_final_aggregate` pins this.

**Suppression is a DEBT (`MarkLedgerUnpaid` / `PayLedgerIfUnpaid`).** A walk that doesn't reach its terminal
`ComputeAllAggregates` (quit, cancel, error, process death) leaves every entry it diffed with no ancestor credit, and
the coverage half is a silent lie: a directory the walk DISCOVERED sits at `listed_epoch = 0` while every ancestor still
carries the last-completed epoch, so `recursive_size_complete` reads true over an unlisted subtree. Measured on the
production index 2026-07-21: **249 directories lying, `~/Library` among them at 2.6M files**, every one traced to the
379 directories a rescan discovered in the 5 seconds before the app was quit. So `BulkReconcileGuard::begin` sends
`MarkLedgerUnpaid` BEFORE `SetDeltaPropagation(false)` (clearing `LEDGER_HEAL_KEY` on disk + arming the writer's heal
latch), and `Drop` sends `PayLedgerIfUnpaid` after restoring propagation. The two halves cover different deaths — `Drop`
covers in-process interruption, the durable marker covers process death (no `Drop` runs). Ordering is load-bearing both
ways: the marker must commit before the first suppressed write, and the payment must be the LAST thing the window does.
The heal-latch mechanism is canonical in `../writer/DETAILS.md` § the one-shot heal. Regression tests:
`local_reconcile::tests::a_reconcile_cancelled_after_discovering_a_dir_leaves_no_exact_size_lies`, and in
`reconciler::tests` `an_interrupted_bulk_window_pays_the_coverage_debt_when_it_closes`,
`a_bulk_window_that_dies_mid_walk_leaves_the_ledger_unpaid_for_the_next_launch` (`mem::forget`s the guard to simulate
process death), `a_bulk_window_that_finishes_cleanly_leaves_the_ledger_paid`.

**Existing indexes self-heal at the next launch**, no one-time repair: the incident DB has no `scan_completed_at` (the
interrupted pass deleted it), so the launch re-reconciles and its finish aggregate rebuilds every row. For an
interruption that leaves `scan_completed_at` in place, the cleared marker forces one `ComputeAllAggregates` on the next
launch (~30 s on a 600k-directory index).

**The shared local read (`reconciler::read_fs_children`).** Both local walks — the live `reconcile_subtree` and the
full-tree `local_reconcile` — list a directory through this one function, which returns `Option<Vec<FsChild>>`: `None`
means "couldn't list" (the walk skips the dir and keeps it honestly stale), `Some(vec![])` means "listed, empty". An
`FsChild` carries a `MetadataSnapshot` rather than a `std::fs::Metadata`, which is what lets the read come from a
batched syscall — a `Metadata` can't be synthesized.

On macOS the read is the fresh scan's `getattrlistbulk` batch (`scanner::bulk_read_dir_unwatched`), which returns each
child's name, type, sizes, mtime, inode, and link count _with_ the directory entry, so the walk never stats an entry
individually. Batching matters because a per-entry `lstat` dominates this walk's cost: over 771k directories / 6.6M
entries on a boot disk, `readdir` costs 106.3 s while the per-entry `lstat` costs 238.4 s — 69% of read time at ~36
µs/entry, with the process at ~10% CPU, so the walk is syscall-latency bound, not compute bound (verified on macOS 15
with a standalone single-threaded walk mimicking `read_fs_children`, 2026-07-27). The batching buys nothing else: the
walk is serial, `GuardedReader` caps each read at `LOCAL_LIST_TIMEOUT`, and the exclusion gates (`should_exclude` then
`is_canonicalization_alias`, in `child_is_indexable`) run per child, all exactly as the `read_dir` path does. Non-macOS
targets use `read_fs_children_via_read_dir` (`read_dir` + per-entry `symlink_metadata`).

**Decision/Why the fallbacks are preserved, at two levels.** A hand-parsed packed buffer can be wrong in ways an `lstat`
can't, and this walk _writes what it reads_, so both failure modes are caught rather than trusted:

- **A child with no inline attributes still gets stated.** `parse_entry` returns `stat: None` when an attribute wasn't
  returned for an entry, or when the type carries no inline sizes at all (fifo, socket, device node), and
  `read_fs_children` pays one `symlink_metadata` for that child. Reporting the parser's zeros instead would write a
  wrong size; taking the stat costs one syscall on an entry we've never actually seen in the field.
- **A directory that lost a record is re-read whole.** A record with no recoverable name can't be stated (there's
  nothing to name), so it's counted in `BulkDirRead::unusable` and the whole directory is re-read with `read_dir`. It
  must not be diffed short: `diff_dir_against_db` DELETES every DB child the live listing lacks, so one unparsed record
  would delete a real file (and its subtree) from the index. This is the same rule as the `EmptyRoot` guard and the
  cost-budget skip — a listing we don't fully trust is never handed to the diff.

Both branches are unreachable on the filesystems we've measured (`FSOPT_PACK_INVAL_ATTRS` makes every requested
attribute present), which is exactly why they're pinned by tests rather than by field evidence: `bulk_read.rs`'s
synthetic-record tests build packed records with attributes withheld, and `reconciler/tests/directory_read.rs`'s
`the_reconcile_read_matches_a_per_entry_stat` asserts the batched read equals `read_dir` + `symlink_metadata`
field-for-field over a tree of files with known sizes, an empty dir, a symlink, a broken symlink, a hardlink pair, a
unicode name, a fifo, and an excluded basename.

**The shared per-dir diff.** `reconciler::diff_dir_against_db(dir_id, live_children, db_children, writer)` is the one
place the add/remove/modify/type-change diff lives. THREE walk sources feed it source-agnostic `LiveChild`s: the local
live small-scope reconcile (`reconcile_subtree`), the local full-tree rescan (`local_reconcile::run_local_reconcile`, a
BFS), and the network full rescan (`volume_scanner::reconcile_volume_via_trait`, `Volume::list_directory` BFS). It keeps
`next_id` from the shared `Arc<AtomicI64>` (never `MAX(id)`). The shared FINISH (stamp listed dirs → ONE
`ComputeAllAggregates`) lives once in `reconciler::finish_reconcile`/`send_marks`, called by both full-rescan walkers so
they can't drift on the marks-before-aggregate ordering.

**Recursion set is decoupled from the write decision (load-bearing).** `diff_dir_against_db` returns
`matched_child_dirs` for EVERY child dir present in both the live listing and the DB, regardless of whether that dir's
own metadata changed — and the BFS recurses into all of them. A child dir being "unchanged" at its parent's level says
NOTHING about whether its OWN subtree was ever listed, so the walk MUST descend anyway. Gating recursion on `changed`
was the exact prod bug: enabling indexing on an already-partially-indexed share (root + top dirs known, subtrees never
listed) would match the top dirs, write nothing, recurse nowhere, and "complete" in 0.0s over an unscanned share. The
write decision stays change-gated (an unchanged dir emits zero rows). Regression-locked by
`reconcile_descends_into_existing_unchanged_child_dirs`.

**New child dirs are resolved by `(parent_id, name)`, NOT by absolute path (load-bearing).** When the diff discovers a
new child dir, the BFS writes its row, flushes, then needs the freshly-assigned id to recurse. It resolves that id via
`IndexStore::resolve_component(conn, parent_id, name)` — a single-component lookup under the parent id it already holds
— NOT `store::resolve_path(conn, absolute_path)`. `resolve_path` walks from `ROOT_ID` assuming the index root is `/`,
but the index root is the VOLUME root (`/Volumes/<share>`, `mtp://…`) mapped to `ROOT_ID`, so an absolute-path walk
fails at the very first component (`Volumes`) and resolves NOTHING — which left a post-Forget enable (empty DB → every
dir is "new") stopping at the root and falsely "completing" with only the top-level entries. Regression-locked by
`reconcile_from_empty_db_with_non_root_mount_indexes_full_tree`.

**Network walk disciplines preserved.** `reconcile_volume_via_trait` keeps every `scan_volume_via_trait` round-trip
discipline: cancelable per dir (cancel leaves the prior index intact — no truncate ran), `LIST_TIMEOUT`-wrapped,
`autoreleasepool`-drained, the typed terminal-disconnect branch, and the consecutive-failure backstop. A terminal
disconnect mid-reconcile stamps the dirs it DID re-list, runs the single aggregate, and surfaces the typed error; the
completion handler then bumps the epoch and keeps the instance + DB.

**LOCAL full rescan reconciles in place (`local_reconcile.rs`).** A LOCAL rescan of an already-populated index runs the
serial full-tree reconcile walker instead of truncate + fresh parallel rebuild (it skips ONLY the `TruncateData` step):
a BFS from the volume root (each read guarded), `diff_dir_against_db` per dir, the shared `finish_reconcile`. It reuses
`reconciler::read_fs_children` (which applies BOTH `should_exclude` AND `is_canonicalization_alias`, so
`/tmp`,`/var`,`/etc` aren't re-added every pass) and a single READ connection in autocommit. It runs on a `std::thread`
and returns the SAME `(ScanHandle, JoinHandle<Result<ScanSummary, ScanError>>)` shape as `scanner::scan_volume`, so
`start_scan`'s completion handler is reused UNCHANGED. **Decision/Why serial:** full parallelization would restructure
the delete-critical per-dir diff for a perf gain the rare rescan doesn't need. Hang-tolerance, not parallelism, was the
requirement, handled without touching the diff: each `read_fs_children` goes through a `GuardedReader` that caps the
read at `LOCAL_LIST_TIMEOUT` (15 s) on a persistent 8 MB-stack helper thread; an overrun is abandoned and reported as
unlistable (`None`), mapping onto the EXISTING skip handling (root won't list → failed rescan keeping the prior index;
subdir won't list → skip and keep it stale). See `../scanner/DETAILS.md`. **Panic safety:** `start_local_reconcile`
wraps `run_local_reconcile` in `std::panic::catch_unwind` and converts a panic into a typed `ScanError::Panicked(msg)`,
so a walk panic resolves the `JoinHandle` to `Ok(Err(_))` (routed through the completion handler's failure arm), not the
opaque raw-thread-panic arm. **Gotcha (hardlinks):** `build_live_children` dedups a multi-link inode's bytes ONLY in the
summary byte totals (one global `seen_inodes` for the whole walk) and deliberately leaves the per-entry `LiveChild`
snapshot RAW, deferring per-entry dedup to the writer's `UpsertEntryV2` (`has_sized_entry_for_inode`). Don't "fix" this
by zeroing the snapshot the way `run_scan` zeroes its per-entry size: the reconcile's first-seen-keeps choice is
independent of which occurrence the DB already sized, so zeroing makes the writer null BOTH occurrences and the inode's
bytes drop to zero (under-count).

## No completion marker on an empty root

A scan whose ROOT listing yields ZERO children does NOT report a clean completion. The network walkers
(`scan_volume_via_trait`, `reconcile_volume_via_trait`) return the typed `VolumeScanError::EmptyRoot`; the local
reconcile walker (`run_local_reconcile`) returns the typed `ScanError::EmptyRoot` — in both cases the completion handler
takes its `Err` arm and writes NO `scan_completed_at`. This complements the recursion-set decision: a volume that lists
fine in a live pane but scans to nothing (a transient session glitch, a half-dead connection, a wrong scan root) must
not stamp a false "complete" marker, because that marker permanently strands the index — startup would see
`scan_completed_at.is_some()` and load Stale without ever rescanning, and a manual "Rescan now" would re-reconcile the
same empty root and re-"complete" again. The real-hardware symptom was an SMB index with one row (the ROOT sentinel),
`total_entries=0`, `scan_duration_ms=2`, and `scan_completed_at` set, that refused to re-index.

- **Empty (`EmptyRoot`) vs failed (`Volume`/`Io`) root, both refuse completion via different typed variants.** A root
  listing that ERRORS already returned a root-fatal error; the empty case is a root that SUCCEEDS but returns nothing.
  Distinguishing them keeps the classification typed, never a message substring.
- **Both reconcile paths** bail at the ROOT-LISTING point, BEFORE diffing the root — otherwise `diff_dir_against_db`
  would see every prior child as "removed" and blank the index before the guard fired. For the LOCAL path this is
  net-new code (the local FRESH guarded-walker path has no empty-root guard, so the guard lives only in the reconcile
  walker).
- **A genuinely empty volume** is the accepted false-negative: it reads "not indexed" and self-heals the instant any
  file appears. The safe rule — never auto-complete an empty root — wins over indexing a real but empty volume.
- Regression-locked by `volume_scanner::tests::empty_root_fresh_scan_does_not_complete`,
  `failed_root_listing_does_not_complete`, `reconcile_empty_root_does_not_complete`, and
  `local_reconcile::tests::reconcile_empty_root_keeps_prior_index_and_signals_empty_root`.

## The reconcile cost budget (`local_reconcile/cost_budget.rs`)

The serial rescan walk had no cost backstop: on the measured boot volume it spent 1,309 s, 92.3% of it inside the
directory read, with 1.7% of directories accounting for 71% of the read time (`docs/notes/reconcile-latency-spike.md`).
Batching the read cut its constant cost but not that distribution, so the backstop still matters. Cost, not failure, is
the signal: that walk hit exactly ONE read timeout in 21 minutes while an Android phone's `/proc` tree cost ~454 s in
reads that all SUCCEEDED. So the guarded walker's "give up after 32 consecutive FAILED reads" model would have fired
zero times. (That specific tree is now excluded by name at volume roots; the budget is the general backstop for the
trees nobody anticipated — `Library/Caches/go-build/*`, Slack's `Cache_Data`, `target/debug/incremental`, a MacDroid
`.Trash`, Xcode SDK framework dirs.)

**The metric: read LATENCY, never cumulative read time.** Every read gets an allowance of `SLOW_READ_FIXED_ALLOWANCE`
(20 ms) plus `SLOW_READ_PER_ENTRY_ALLOWANCE` (100 µs) per entry it returned. A read that costs more than its allowance
is _slow_, and ONLY slow reads' time is charged to anything. Fast reads are free however many there are, so a subtree
can grow without limit and never be refused for its size.

**The attribution: one accumulator per anchor subtree.** Every directory read is charged to ONE ancestor: the one at
`ANCHOR_DEPTH` (5) below the volume root, its _anchor_. Directories above the anchor depth carry no anchor, so the top
of the tree is always walked.

**The verdict is a FRACTION, never a total.** An anchor is refused once more than `MAX_SLOW_READ_FRACTION` (5%) of the
reads charged to it were slow — every read counts in the denominator — subject to two floors: at least `MIN_SLOW_READS`
(10) slow reads, and more than `MIN_SLOW_TIME_WASTED` (5 s) lost to them. All three, or the walk carries on.

**❌ Never score a subtree on a TOTAL (of read time, or of anything else).** Two shipped rules made this mistake and
both were measured wrong, because _the opportunity to accumulate a total scales with subtree size while the total does
not_. A 105,441-directory repo reaches any fixed total eventually however healthy it is; a 91-directory phone may never
reach it however pathological it is. Cumulative read time was the first version (2026-07-21 run 1); charging only slow
reads' time was the second, and under real working load
([run 2](../../../../../../docs/notes/indexing-benchmarks-2026-07-21.md), load 12-24) it fired FIVE times, three of them
wrong. The slow-read fraction separates the same five subtrees by two orders of magnitude:

| subtree                                   |    dirs | slow reads | fraction | verdict wanted |
| ----------------------------------------- | ------: | ---------: | -------- | -------------- |
| `.cache/github-copilot/project-context`   |      62 |         14 | 22.6%    | refuse         |
| `CloudStorage/MacDroid-googlePixel9ProXL` |      91 |         18 | 19.8%    | refuse         |
| `Library/pnpm/store`                      |   6,669 |         62 | 0.93%    | walk           |
| `projects-git/vdavid/cmdr`                | 105,441 |        101 | 0.10%    | walk           |
| `CommandLineTools/SDKs/MacOSX13.3.sdk`    |   6,828 |          4 | 0.06%    | walk           |

Every one of the five was past 10 s of slow-read time, so no threshold on a total could tell them apart; the fraction
gets all five right with ~4× of margin on each side. 5% is the geometric middle of the gap between 0.93% and 19.8%.

**Why the two floors.** `MIN_SLOW_READS` (10) is BOTH the numerator floor and the sample floor (a slow read is a read),
so a three-directory subtree can't be condemned by one bad read at 33%. It's measured: the Xcode SDK was refused over
FOUR slow reads, so three was too low, and ten sits above every measured false positive (4) and below every measured
true one (14, 18). A separate floor on TOTAL reads is the wrong instrument — to help it would have to be in the
hundreds, exempting the 91-directory phone. `MIN_SLOW_TIME_WASTED` (5 s) makes the trip pay for itself (refusing a
subtree costs every directory under it its freshness); it sits above the largest legitimate single read measured (3.9 s
for the 200,000-entry fixture) so honest work can't reach it.

**When the rule may speak.** The verdict is re-evaluated on every SLOW read, and only then; the earliest possible trip
is the 10th slow read. **The honest limitation:** the fraction is measured over a PREFIX of the subtree's reads in BFS
order, so a healthy subtree whose first ten slow reads all land in its first ~200 directories can still be refused, and
the skip is a latch. The measured populations make that improbable (101 slow reads over 105,441 directories), but it's
the residual, and the shape of any online verdict. The activation counters are the instrument: if a real machine trips a
subtree it shouldn't, the fraction moves, not the logic.

**Design rejections (each was tried or considered):** per-entry allowance not plain per-read latency (a big directory is
legitimately slow — the 200,000-file fixture at ~20 µs/entry is FASTER per unit work than a 0.56 ms ordinary read); a
fraction of THRESHOLDED reads not mean/median (a mean sits on the fast reads and averages away the pathology); ❌ not
charging up the whole ancestor chain (a fraction isn't monotone up the tree — a pathological child dilutes into a
healthy parent); per-subtree not a global walk budget (a global cap truncates in BFS arrival order, so which dirs go
stale depends on queue order, unreproducible and unexplainable). The anchor depth (5) is a granularity choice, not
measured: it puts the anchor at app/project granularity where the measured offenders sit. Every threshold is injected
(`CostBudget::production()` is a plain struct literal). One clock: `GuardedReader::read` returns the read's duration
alongside the listing; timed-out reads are charged their full 15 s against the fixed allowance alone.

**❌ Two hard rules for the skip. Both are traps this subsystem has already paid for.**

- **A skipped directory is one we never listed, NEVER one we listed and found empty.** `diff_dir_against_db` reaps DB
  children absent from the live listing, so running the diff with an empty listing would DELETE the whole subtree and
  strip its bytes out of every ancestor's `dir_stats` for good. The skip is a bare `continue` before the read.
- **❌ Never stamp `listed_epoch` on a skipped directory, least of all `0`.** In a RESCAN those rows already carry a
  positive epoch, and `absorbing_min_epoch` propagates a zero to every ancestor up to `~` and `/`, marking the whole
  home folder incomplete and making `expected_totals` return `None` for every copy of `~`. Leaving rows AND epoch alone
  keeps the subtree honestly stale: last-known sizes stay visible, the live watcher keeps maintaining it, and a later
  pass heals it.

Pinned by `cost_budget::tests` (four run the SHIPPED thresholds against the measured subtrees above:
`a_subtree_with_a_low_slow_read_fraction_is_never_refused_however_large_it_grows`,
`a_small_subtree_with_a_high_slow_read_fraction_is_refused`,
`a_handful_of_slow_reads_in_a_huge_healthy_subtree_never_trips_it`,
`a_fraction_over_too_small_a_sample_is_never_a_verdict`) plus shape/boundary tests, and the data-safety pair in
`local_reconcile/tests.rs` (`a_budget_skipped_subtree_keeps_every_row_and_its_sizes`,
`a_budget_skipped_subtree_leaves_its_epoch_and_every_ancestor_epoch_untouched`).

**Observability.** A trip logs one `warn` naming the subtree, what it lost, and how many slow reads, and bumps
`reconcileBudgetSubtrees`; each undescended directory bumps `reconcileBudgetSkippedDirs`. Both ride the debug surface
(`cmdr://indexing?volume=<id>`) next to `verifyDeclinedDirs` / `verifyTruncatedDirs`. **Not in scope:** the fresh
scanner's 32-consecutive-failure guard is untouched (it's a parallel rayon walk with no BFS ancestor chain to charge, so
it needs its own design).

## Bounding verification cost (the two teeth)

Post-replay verification (`verify_affected_dirs`, in `../watch/event_loop/verification.rs`) is a bidirectional readdir
diff, so it costs O(children) per affected directory. On 2026-07-19 a cold start replayed an 18,314-event journal gap
into 288 affected dirs and then spent **7 min 6 s** at a **1.01 GB** `phys_footprint` peak with the writer channel
pegged at its 20,000 cap the whole time. Essentially all of it came from ONE directory:
`~/Library/Containers/com.google.drivefs.fpext/Data/tmp/domain-temp-gdrive-<id>/fetch_temp`, holding 1,138,220 empty
files. `0 new dirs`: no recursive amplification, just one directory's one-level diff.

Throttling can't fix this class. Re-syncing a directory costs O(children), not O(events) — the per-child events were
dropped, so all you can do is readdir and diff. So the cost is bounded instead, by two pure decisions in
`../watch/event_loop/verify_guard.rs` (threshold-injected). Both share ONE constant, `HUGE_DIR_CHILDREN` (200,000): the
largest legitimate directory measured on the same machine held ~119k children, so the threshold sits ~1.7× above it and
~6× below the incident.

- **Tooth 1 — a DB-side probe BEFORE the snapshot.** `IndexStore::count_children_capped(parent_id, conn, threshold + 1)`
  runs ahead of `list_children_on`. Phase 1 materialises `HashMap<String, (i64, Vec<EntryRow>)>` for EVERY affected
  path, so guarding only the upsert loop would leave 1.41M owned `EntryRow`s (~130–160 MB) in place. ❌ Not a
  `COUNT(*)`: the answer must not itself cost O(children).
- **Tooth 2 — an ITERATION cap, not an upsert cap.** Phase 2's `read_dir` loop `continue`s past DB-known children before
  doing any work, so an already-indexed pathological directory produces near-zero upserts while iterating 1.41M times.
  **An upsert cap would have been a no-op on the measured incident.** This tooth also covers the inverse shape: a
  directory small in the index but huge on disk.

**❌ A declined directory must NOT be marked `listed_epoch = 0`.** This reads like honesty and is the opposite. Affected
dirs carry a POSITIVE epoch from the scan, and `absorbing_min_epoch` propagates a zero all the way up, so
`min_subtree_epoch → 0` for every ancestor to `~` and `/`, rendering the whole home folder incomplete and making
`expected_totals::per_source_contribution` return `None` for every copy of `~`. The 32-failed-reads walker precedent
does NOT apply: those dirs were never listed, so they stay at 0 and nothing is downgraded. Same word, opposite
operation. Pinned by `verification::tests::a_declined_dir_leaves_its_epoch_and_every_ancestor_epoch_untouched`.

**The honest cost. This is a trade, not a free win.** Tooth 1 skips before the snapshot, so deletions from the journal
gap are NOT reaped and the ancestor chain stays inflated until some other path corrects it. Tooth 2 leaves a partially
diffed directory. A declined directory still reports `recursive_size_complete = true` — owned as debt here rather than
papered over. Scope: this fixes the STALL; it does not reclaim the search index's RAM, and it guards only
`verify_affected_dirs` (a shallow `MustScanSubDirs` still routes to `start_scan` and re-walks; `reconcile_subtree` still
diffs on a deep anchor).

**How to measure pathological directories** (one SQL query over an existing index):

```sql
SELECT COUNT(*) FROM (SELECT parent_id FROM entries GROUP BY parent_id HAVING COUNT(*) >= 10000);
```

Measured on David's production index (7,325,641 rows, 2026-07-21): 29 such directories, topped by Google Drive's
`fetch_temp` at 955,724, then test fixtures, then WebKit 129,930 / Chrome 103,245 / Firefox 74,024 caches, then
`target/debug/deps` across five repos. **The index UNDERCOUNTS the worst directories** (a read abandoned at
`LOCAL_LIST_TIMEOUT` skips the subtree, so `fetch_temp` reads 955,724 rows against ~1.4M on disk) — treat every number
as a lower bound. The guard's own activations are NOT answerable this way, so they stay counted: `verifyDeclinedDirs`
(tooth 1) and `verifyTruncatedDirs` (tooth 2).

## Where the serial walk was chosen, and where it wasn't (the search cover walk)

`../lifecycle/cover/` had this exact choice to make for a search-driven walk over a coverage frontier, and it went both
ways:

- **NOT chosen for the frontier itself.** A frontier node is virgin ground by definition, so this is a bulk add — the
  workload the parallel walker is for. Measured on four real trees from 368 to 1,202,613 entries: 3.2–5.8x faster,
  identical row counts, nothing abandoned (`docs/notes/cover-walk-primitive-2026-08-05.md`). The comment on
  `reconcile_subtree` calling itself "the LIVE small-scope fill path" is exactly right, and a frontier is not that.
- **Chosen for the repair case, which the parallel walker cannot take.** A frontier node that ALREADY holds rows (an
  FSEvents verification pass writes children under a directory without marking that directory listed) is unsafe for the
  parallel walker in both directions: deleting first would drop rows the walk did not write, and walking add-only over
  them would collide fresh ids with existing siblings, get silently skipped by `INSERT OR IGNORE`, and orphan the
  subtree. Comparing by name and writing only differences is precisely what this walk does, so `ScanError::NotVirgin`
  routes here. See `../scanner/DETAILS.md` § "Three scan roots".

`ReconcileSummary.cancelled` exists for that caller: the walk is safe to interrupt (every directory it listed is still
marked), but "the scope is covered now" and "somebody stopped us" are different answers and a summary that reported them
identically was quietly lying.

## The per-navigation verifier (`verifier.rs`)

On each directory navigation, `trigger_verification()` (called from `streaming.rs` and `operations.rs` after enrichment)
is fully fire-and-forget: it spawns a task that acquires the `INDEX_REGISTRY` lock (never blocking the navigation
thread), looks up the volume's running instance, checks dedup/debounce via static `VerifierState` (in-flight set +
recent timestamps), then spawns a second async task that: (1) reads DB children via that VOLUME's `ReadPool`, (2) reads
disk via `read_dir` + per-entry `symlink_metadata`, wrapped in `spawn_blocking` so a wedged path (stale FUSE / frozen
iCloud dir / network-as-local) can't park a tokio worker — keep this offload; don't move the disk loop back inline on
the async path (filtering through `scanner::should_exclude`), (3) diffs by normalized name, sending
`UpsertEntryV2`/`DeleteEntryById`/`DeleteSubtreeById`/`PropagateDeltaById` corrections. New directories are flushed then
scanned via `scan_subtree` with delta propagation. Debounce: 30 s per path, max 2 concurrent verifications. Only runs
after the initial scan completes (checks `ground_in_flux`). `invalidate()` clears state on shutdown/clear. The
`in_flight` slot is freed (and the path recorded in `recent`) via an `InFlightGuard` RAII `Drop`, not a post-`await`
line, so a panic in `verify_and_correct`/`emit_dir_updated` can't permanently leak a slot against
`MAX_CONCURRENT_VERIFICATIONS` (pinned by `verifier.rs::tests::in_flight_slot_is_freed_on_panic_unwind`).

**❌ The read volume, the write volume, and the path space must be ONE volume.** `verify_and_correct` takes
`volume_id` + `IndexPathSpace` + `IndexWriter`, and `trigger_verification` takes all three off the same running instance
under the registry lock (`mgr.path_space()`, `mgr.writer`). Reading root's pool while writing the caller's writer made
this a silent no-op on every SMB, MTP, and external volume: `resolve_path` was handed a mount-absolute `/Volumes/…` path
against root's `/`-rooted index, found nothing, and returned before any correction. A no-op is the BENIGN failure of a
disagreement here; the malignant one is corrections computed from one volume's rows landing in another's index, which is
why the three travel together rather than being looked up separately.

Every path in the verifier stays ABSOLUTE — the `read_dir`, the exclusion checks, `new_dir_paths`, the returned
`affected_paths`, and the FE emit (which must match pane paths) — and crosses into index-relative space at exactly one
point, `space.resolve_abs`. That is the same discipline `../paths/DETAILS.md` states for the pipeline at large. Three
more things route by the same space: `should_exclude` uses `space.exclusion_scope()` (a `BootDisk` scope would exclude
every `/Volumes/X/…` child on a mount-rooted volume), `scan_subtree` takes the space so its absolute root resolves
mount-relative, and every snapshot's inode goes through `space.trust_inode` so a FAT/exFAT drive stores `inode: None`.
Corrections publish on the lifecycle bus under the volume they were read from. Pinned by
`verify_corrects_a_mount_rooted_volumes_own_index` and `verify_scans_a_new_directory_into_a_mount_rooted_volumes_index`.

**What the verifier does and does NOT cover** (the safety argument for skipping sweeps rests on it). On each navigation
it does a full `read_dir` of the navigated directory and diffs it against the DB, correcting additions, deletions,
dir↔file type changes, and size/mtime drift, and it fully `scan_subtree`s directories new to the index — so it genuinely
keeps the directory the user is looking at correct, on whichever volume they're looking at. The verifier's own
`should_exclude` gate covers the directory it hands over; the structural policy INSIDE that subtree is `scan_subtree`'s
(every walk applies it, `../scanner/DETAILS.md` § `WalkPolicy`), which is what stops a newly discovered `/Library` from
bringing `/Library/Caches` into an index no boot scan would have put it in. But it lists **ONE level**: an existing
subdirectory is compared by name/size/mtime only, so a change deep inside a subtree the user never opens is invisible to
it, and the stale bytes stay in every ancestor until a sweep. It also only ever covers directories someone NAVIGATES to.
Those gaps are what the sweep scope and the coalesce count answer. An MTP volume gets nothing from it either way:
`mtp://` paths have no POSIX `read_dir`, so the disk half bails and the pass is inert.

**Every detached walk here runs on a token handed IN, never one looked up.** `maybe_verify` takes the volume's child
token from `state::trigger_verification` (which already holds the instance), and the subtree-rescan drain takes it from
the `RescanDrain` the `EventReconciler` was built with. ❌ Don't reach into `lifecycle::state` for a token by volume id:
besides the import cycle, a walk that starts after its volume was torn down would find nothing, default to a token that
never fires, and keep writing into a draining writer. Topology: `../host/DETAILS.md` § Cancellation.

**Progressive `index-dir-updated` emit during background verification.** `run_background_verification` emits one
`index-dir-updated` per successfully-scanned new subtree, immediately after the post-scan writer flush. Don't buffer
new-dir paths and fire a single end-of-verification emit: that window runs up to 5 minutes for a typical home folder,
and any listing opened in it stays on `<dir>` placeholders (the single emit often misses the right paths, carrying
replay `affected_paths` rather than the verification-discovered paths). The FE handler is throttled at 2 s per pane.

The rescan SCHEDULER — which anchor walks, when, how often, and what the user sees while it does — is
`reconciler/rescan/DETAILS.md`. This file is the diff engine it calls.
