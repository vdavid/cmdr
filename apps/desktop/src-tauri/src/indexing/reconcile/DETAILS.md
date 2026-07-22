# Reconcile details

Read this before any non-trivial work in `reconcile/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

This area owns the mechanisms below. Points outward: the honest-sizes data model (`listed_epoch` /
`min_subtree_epoch` / `current_epoch`), the `dir_stats` ledger, the `BulkReconcileGuard` debt-recording contract, and
writer-wait attribution are canonical in `../writer/DETAILS.md`; the guarded reader and the `should_exclude` policy in
`../scanner/DETAILS.md`; the live event loop, removal-storm coalescing, and the `verify_affected_dirs` /
`verify_guard.rs` code in `../watch/DETAILS.md`; `resolve_path_under` + mount-relative paths in
`../paths/DETAILS.md`; the compute math in `../aggregator/DETAILS.md`.

## Non-destructive rescan (reconcile, not truncate)

A FIRST scan (empty DB) truncates and bulk-builds — fastest on an empty table. A RESCAN of an already-populated index
RECONCILES in place: it walks the tree and diffs each dir against the DB, writing only the differences, so the last-good
index stays visible (stale) throughout and a mid-rescan disconnect leaves the prior data intact. Perf + correctness were
gated before building this; the evidence is in `docs/notes/m3-reconcile-rescan-gate.md`.

**The LOCAL reconcile's cost is the open question.** Measured on the boot volume: the serial reconcile walk took
1,309 s where the parallel fresh scan of the same tree took 68.1 s, and 92.3% of that time sat inside `read_dir` +
`lstat` (`docs/notes/reconcile-latency-spike.md`). Replacing the local rescan with a fast parallel build that swaps in
atomically is under evaluation, including the traps that shape it (SQLite has no `ALTER INDEX ... RENAME`, `start_scan`
clears `scan_completed_at` before the scan runs, and `MutationTracker::bump` can't tell which table changed):
`docs/notes/swap-scan-feasibility.md`.

**Before trusting that speed comparison, read `docs/notes/indexing-benchmarks-2026-07-21.md`.**
Measured on an idle machine, the fresh parallel scan takes 52.7 s and the reconcile 476.9 s — but the parallel scan
ABANDONS directories at `LOCAL_LIST_TIMEOUT` under rayon contention and left the index ~10% short (6,001,637 rows,
versus 6,663,048 after the reconcile filled in the five subtrees it had skipped). The parallel walk buys part of its
speed by giving up, and the directories it gives up on are the large ones whose sizes users most want.

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

**The single-aggregate coverage constraint (load-bearing).** After the reconcile walk, the rescan path stamps every
re-listed dir (`MarkDirsListed`) and runs ONE bottom-up `ComputeAllAggregates`. It must NOT fire
`PropagateMinSubtreeEpoch` per dir: the gate measured per-dir propagation across ~37k dirs at ~2× SLOWER than a truncate
rebuild (the ancestor-walk degenerates toward O(dirs × depth) when every dir re-stamps to the same new epoch), while a
single bottom-up aggregate is faster than truncate. `finish_reconcile` sends `ComputeAllAggregates { source: Sql }`, so
the aggregate recomputes coverage AND sizes for the whole tree from the committed rows in one O(dirs) bulk-SQL pass. A
reconcile's own writes (`UpsertEntryV2`/`Delete*`, never `InsertEntriesV2`) leave the accumulator maps empty, but the
finish does NOT rely on that: declaring `Sql` — not sniffing map-emptiness — is what keeps an interleaving verification
subtree scan's map pollution from zeroing every out-of-subtree dir (see `../writer/DETAILS.md` §
the source contract). Per-dir `PropagateMinSubtreeEpoch` stays ONLY for the small-scope LIVE reconciles
(`reconcile_subtree`: per-navigation verifier, `MustScanSubDirs`, SMB-overflow `FullRefresh`), where the chain is short.

**Decision: the full reconcile suppresses per-entry ancestor propagation (`SetDeltaPropagation`).** The single-aggregate
rule governs the FINISH; this governs the WALK. Each `UpsertEntryV2`/`DeleteEntryById`/`DeleteSubtreeById` the diff
emits would otherwise auto-walk the ancestor `dir_stats` chain — O(entries × depth) across an entire pass. On a large delta (a 270k→6M
partial-completion) that wedged the writer for hours: the channel stays full, so the walk thread parks on `send` and the
app can't drain. It's also pure waste, because the FINISH's one `ComputeAllAggregates` recomputes every dir's `dir_stats`
from the entries table anyway. So both full-reconcile walkers (`local_reconcile::run_local_reconcile`,
`volume_scanner::reconcile_volume_via_trait`) bracket their BFS with `reconciler::BulkReconcileGuard` — it sends
`SetDeltaPropagation(false)` before the walk and restores `true` on EVERY exit (clean finish, cancel, empty-root,
disconnect, error, panic) via `Drop`. The writer keeps everything else under suppression (entry insert/update/delete,
hardlink dedup, the new-directory zero-valued `dir_stats` row init) — ONLY the ancestor PROPAGATION is skipped. **Why
the LIVE path keeps propagating:** `reconcile_subtree` and the FSEvents handlers have NO final full aggregate, so their
per-entry propagation IS the mechanism that keeps `dir_stats` correct. **Don't re-add per-entry propagation to the bulk
path** (it reintroduces the hours-long wedge); `bulk_reconcile_suppresses_per_entry_propagation_until_final_aggregate`
pins this.

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
The heal-latch mechanism is canonical in `../writer/DETAILS.md` § the one-shot heal. Regression
tests: `local_reconcile::tests::a_reconcile_cancelled_after_discovering_a_dir_leaves_no_exact_size_lies`, and in
`reconciler::tests` `an_interrupted_bulk_window_pays_the_coverage_debt_when_it_closes`,
`a_bulk_window_that_dies_mid_walk_leaves_the_ledger_unpaid_for_the_next_launch` (`mem::forget`s the guard to simulate
process death), `a_bulk_window_that_finishes_cleanly_leaves_the_ledger_paid`.

**Existing indexes self-heal at the next launch**, no one-time repair: the incident DB has no `scan_completed_at` (the
interrupted pass deleted it), so the launch re-reconciles and its finish aggregate rebuilds every row. For an
interruption that leaves `scan_completed_at` in place, the cleared marker forces one `ComputeAllAggregates` on the next
launch (~30 s on a 600k-directory index).

**The shared per-dir diff.** `reconciler::diff_dir_against_db(dir_id, live_children, db_children, writer)` is the one
place the add/remove/modify/type-change diff lives. THREE walk sources feed it source-agnostic `LiveChild`s: the local
live small-scope reconcile (`reconcile_subtree`, `std::fs::read_dir`), the local full-tree rescan
(`local_reconcile::run_local_reconcile`, `std::fs::read_dir` BFS), and the network full rescan
(`volume_scanner::reconcile_volume_via_trait`, `Volume::list_directory` BFS). It keeps `next_id` from the shared
`Arc<AtomicI64>` (never `MAX(id)`). The shared FINISH (stamp listed dirs → ONE `ComputeAllAggregates`) lives once in
`reconciler::finish_reconcile`/`send_marks`, called by both full-rescan walkers so they can't drift on the
marks-before-aggregate ordering.

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
a BFS from the volume root over `std::fs::read_dir` (each read guarded), `diff_dir_against_db` per dir, the shared
`finish_reconcile`. It reuses `reconciler::read_fs_children` (which applies BOTH `should_exclude` AND
`is_canonicalization_alias`, so `/tmp`,`/var`,`/etc` aren't re-added every pass) and a single READ connection in
autocommit. It runs on a `std::thread` and returns the SAME `(ScanHandle, JoinHandle<Result<ScanSummary, ScanError>>)`
shape as `scanner::scan_volume`, so `start_scan`'s completion handler is reused UNCHANGED. **Decision/Why serial:** full
parallelization would restructure the delete-critical per-dir diff for a perf gain the rare rescan doesn't need.
Hang-tolerance, not parallelism, was the requirement, handled without touching the diff: each `read_fs_children` goes
through a `GuardedReader` that caps the read at `LOCAL_LIST_TIMEOUT` (15 s) on a persistent 8 MB-stack helper thread; an
overrun is abandoned and reported as unlistable (`None`), mapping onto the EXISTING skip handling (root won't list →
failed rescan keeping the prior index; subdir won't list → skip and keep it stale). See `../scanner/DETAILS.md`.
**Panic safety:** `start_local_reconcile` wraps `run_local_reconcile`
in `std::panic::catch_unwind` and converts a panic into a typed `ScanError::Panicked(msg)`, so a walk panic resolves the
`JoinHandle` to `Ok(Err(_))` (routed through the completion handler's failure arm), not the opaque raw-thread-panic arm.
**Gotcha (hardlinks):** `build_live_children` dedups a multi-link inode's bytes ONLY in the summary byte totals (one
global `seen_inodes` for the whole walk) and deliberately leaves the per-entry `LiveChild` snapshot RAW, deferring
per-entry dedup to the writer's `UpsertEntryV2` (`has_sized_entry_for_inode`). Don't "fix" this by zeroing the snapshot
the way `run_scan` zeroes its per-entry size: the reconcile's first-seen-keeps choice is independent of which occurrence
the DB already sized, so zeroing makes the writer null BOTH occurrences and the inode's bytes drop to zero
(under-count).

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

The serial rescan walk had no cost backstop: on the measured boot volume it spent 1,309 s, 92.3% of it inside `read_dir`
+ `lstat`, with 1.7% of directories accounting for 71% of the read time (`docs/notes/reconcile-latency-spike.md`). Cost,
not failure, is the signal: that walk hit exactly ONE read timeout in 21 minutes while an Android phone's `/proc` tree
cost ~454 s in reads that all SUCCEEDED. So the guarded walker's "give up after 32 consecutive FAILED reads" model would
have fired zero times. (That specific tree is now excluded by name at volume roots; the budget is the general backstop
for the trees nobody anticipated — `Library/Caches/go-build/*`, Slack's `Cache_Data`, `target/debug/incremental`, a
MacDroid `.Trash`, Xcode SDK framework dirs.)

**The metric: read LATENCY, never cumulative read time.** Every read gets an allowance of `SLOW_READ_FIXED_ALLOWANCE`
(20 ms) plus `SLOW_READ_PER_ENTRY_ALLOWANCE` (100 µs) per entry it returned. A read that costs more than its allowance
is *slow*, and ONLY slow reads' time is charged to anything. Fast reads are free however many there are, so a subtree
can grow without limit and never be refused for its size.

**The attribution: one accumulator per anchor subtree.** Every directory read is charged to ONE ancestor: the one at
`ANCHOR_DEPTH` (5) below the volume root, its *anchor*. Directories above the anchor depth carry no anchor, so the top
of the tree is always walked.

**The verdict is a FRACTION, never a total.** An anchor is refused once more than `MAX_SLOW_READ_FRACTION` (5%) of the
reads charged to it were slow — every read counts in the denominator — subject to two floors: at least `MIN_SLOW_READS`
(10) slow reads, and more than `MIN_SLOW_TIME_WASTED` (5 s) lost to them. All three, or the walk carries on.

**❌ Never score a subtree on a TOTAL (of read time, or of anything else).** Two shipped rules made this mistake and both
were measured wrong, because *the opportunity to accumulate a total scales with subtree size while the total does not*.
A 105,441-directory repo reaches any fixed total eventually however healthy it is; a 91-directory phone may never reach
it however pathological it is. Cumulative read time was the first version (2026-07-21 run 1); charging only slow reads'
time was the second, and under real working load ([run 2](../../../../../../docs/notes/indexing-benchmarks-2026-07-21.md),
load 12-24) it fired FIVE times, three of them wrong. The slow-read fraction separates the same five subtrees by two
orders of magnitude:

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
  path, so guarding only the upsert loop would leave 1.41M owned `EntryRow`s (~130–160 MB) in place. ❌ Not a `COUNT(*)`:
  the answer must not itself cost O(children).
- **Tooth 2 — an ITERATION cap, not an upsert cap.** Phase 2's `read_dir` loop `continue`s past DB-known children
  before doing any work, so an already-indexed pathological directory produces near-zero upserts while iterating 1.41M
  times. **An upsert cap would have been a no-op on the measured incident.** This tooth also covers the inverse shape: a
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

## The per-navigation verifier (`verifier.rs`)

On each directory navigation, `trigger_verification()` (called from `streaming.rs` and `operations.rs` after enrichment)
is fully fire-and-forget: it spawns a task that acquires the `INDEX_REGISTRY` lock (never blocking the navigation
thread), looks up the volume's running instance, checks dedup/debounce via static `VerifierState` (in-flight set +
recent timestamps), then spawns a second async task that: (1) reads DB children via `ReadPool`, (2) reads disk via
`read_dir` + per-entry `symlink_metadata`, wrapped in `spawn_blocking` so a wedged path (stale FUSE / frozen iCloud dir
/ network-as-local) can't park a tokio worker — keep this offload; don't move the disk loop back inline on the async
path (filtering through `scanner::should_exclude`), (3) diffs by normalized name, sending
`UpsertEntryV2`/`DeleteEntryById`/`DeleteSubtreeById`/`PropagateDeltaById` corrections. New directories are flushed then
scanned via `scan_subtree` with delta propagation. Debounce: 30 s per path, max 2 concurrent verifications. Only runs
after the initial scan completes (checks `scanning`). `invalidate()` clears state on shutdown/clear. The `in_flight`
slot is freed (and the path recorded in `recent`) via an `InFlightGuard` RAII `Drop`, not a post-`await` line, so a
panic in `verify_and_correct`/`emit_dir_updated` can't permanently leak a slot against `MAX_CONCURRENT_VERIFICATIONS`
(pinned by `verifier.rs::tests::in_flight_slot_is_freed_on_panic_unwind`).

**What the verifier does and does NOT cover** (the safety argument for skipping sweeps rests on it, and it only half
holds). On each navigation it does a full `read_dir` of the navigated directory and diffs it against the DB, correcting
additions, deletions, dir↔file type changes, and size/mtime drift, and it fully `scan_subtree`s directories new to the
index — so it genuinely keeps the directory the user is looking at correct. But it lists **ONE level**: an existing
subdirectory is compared by name/size/mtime only, so a change deep inside a subtree the user never opens is invisible to
it, and the stale bytes stay in every ancestor until a sweep. It is also **root-scoped** (it reads the root `ReadPool`
and bails inert on a mount-rooted volume). Those two gaps are exactly what the boot-disk-only sweep scope and the
coalesce count answer.

**Progressive `index-dir-updated` emit during background verification.** `run_background_verification` emits one
`index-dir-updated` per successfully-scanned new subtree, immediately after the post-scan writer flush. Don't buffer
new-dir paths and fire a single end-of-verification emit: that window runs up to 5 minutes for a typical home folder,
and any listing opened in it stays on `<dir>` placeholders (the single emit often misses the right paths, carrying
replay `affected_paths` rather than the verification-discovered paths). The FE handler is throttled at 2 s per pane.

## Per-subtree rescan throttle (`reconciler/rescan_throttle.rs`, `reconciler/rescan.rs`)

A `MustScanSubDirs` signal means "re-walk this subtree", and a hard-churning subtree (build output, caches, Cmdr's own
data dir) raises it continuously. The drain caps each anchor to ≤1 reconcile per `RESCAN_THROTTLE_WINDOW` (60 s), so a
folder's size stays bounded-fresh (≤1 window stale) without re-walking continuously. Leading + trailing, not debounce
(mirrors the per-file `throttle.rs`): a never-walked anchor reconciles immediately; a sustained one re-walks once per
window forever (the ~1 s `throttle_sweep_interval` tick re-kicks via `EventReconciler::sweep_rescan_throttle`).
`pick_and_collapse_rescan` picks the shallowest ELIGIBLE anchor; throttled anchors stay queued in `pending_rescans`
until their window elapses. The drain runs on a dedicated `Utility`-QoS thread (not the tokio blocking pool, which
`thread_qos` forbids lowering), so background subtree walks never outrank the webview for CPU. A single growing file is
handled by the per-file live path (incremental `dir_stats` deltas), never a subtree re-walk, so the throttle needs no
significant-change bypass. Tests inject a zero window via `set_rescan_throttle_window_for_test`.

**A reconcile's log line attributes its writer wait** (`reconciler/rescan.rs` `reconcile_report`,
`../writer/wait_probe.rs`). The bounded writer channel means a producer parks once it's full, so `reconcile_subtree`'s
own duration silently included the wait ("reconcile slow for … (21s)" meant "the writer was saturated for 19 of those
seconds"). `reconcile_subtree` arms the thread-local writer-wait probe at its start and reports the span as
`ReconcileSummary.writer_wait`. `reconcile_report` is pure and returns `(log::Level, String)`: `info` under 10 s; past
that the wait is named, and when it DOMINATES (over half the duration) the line drops to `debug` and says "reconcile
waited" (writer saturation has its own signal in the writer heartbeat). The probe mechanism is in `../writer/DETAILS.md`.

## Depth-split `MustScanSubDirs` routing (`reconciler/rescan_route.rs`)

The per-subtree throttle is the right tool for a DEEP/narrow anchor (a single `target/`), but NOT for a shallow/root-
scale one. Under a high-churn boot disk, macOS drops fine-grained FSEvents and raises `MustScanSubDirs` on ever-higher
paths, up to `/`. Reconciling `/` is a ~20-min walk that holds the per-dir hourglass the whole time, and under
continuous root churn the anchor never leaves `pending_rescans`, so `release_rescan_hold` keeps skipping and the local
hourglasses never clear — a 60 s throttle after a 20-min walk is noise. A channel overflow (the SAME "we lost events"
meaning) already takes the VISIBLE scanner path; this makes the two equivalent signals converge. `route_must_scan_sub_dirs`
(the single entry point for the two feeders the fix targets — the live path `process_live_event` and the post-replay
handoff `event_loop::replay`) classifies by anchor depth via `rescan_route::classify`:

- **Shallow** (`depth <= SHALLOW_RESCAN_MAX_DEPTH = 2`, i.e. `/`, `/Users`, `/Users/<me>`): `route_shallow_to_scanner`
  requests a fresh `start_scan` via `ScanTrigger` and takes NO hourglass hold and NEVER enters `pending_rescans`
  (holding it is the stuck-hourglass bug). In production `ScanTrigger::Registry` spawns
  `manager::perform_registry_rescan` (extract manager → stop watcher + live loop → `start_scan` off the lock → reinsert
  `Running`; shared with the replay full-scan fallback, single-flight). Tests inject `Disabled`/`Recording`.
- **Deep** (`depth >= 3`): unchanged — `queue_must_scan_sub_dirs` keeps the throttled reconcile drain. The removal-storm
  and Leak-B escalation feeders also call `queue_must_scan_sub_dirs` directly, so their behavior is unchanged (only the
  two named feeders route by depth).

Depth is a proxy for "re-listing this is walk-the-world expensive"; 2–3 levels is where a reconcile stops being cheap
and starts holding the hourglass for the better part of a full scan.

## The once-a-day sweep window for shallow anchors

**The measurement** (David's machine, 2026-07-18..20): **14 of 28 scans were triggered by a shallow `MustScanSubDirs`
anchor**, roughly one every 2.5 hours INCLUDING OVERNIGHT while idle (01:17, 03:44, 06:39, 08:46, 11:16). **Thirteen of
those 14 anchors were `/` itself; the fourteenth was `/System`, a sealed read-only volume where nothing writes.** So the
anchor path carries no diagnostic information: macOS isn't reporting where churn happened, it's reporting that it gave up
and coalesced to the watch root. Each trigger runs the SERIAL reconcile walk, measured at 1,309 s on this volume. That's
roughly ten multi-minute-to-multi-hour full walks a day for a signal that says nothing about what changed.

**The policy** (`SHALLOW_RESCAN_MIN_INTERVAL = 24 h`, `decide_shallow_anchor`): a shallow anchor means "this index is
now SUSPECT", not "rescan right now". At most one real sweep per volume per day.

- **Boot disk ONLY.** A mount-rooted volume keeps `EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL` (45 s), selected by
  `min_interval_for(space.is_boot_disk())`. Two load-bearing reasons not to unify: we measured the storm on `/` and have
  no evidence of one on external volumes, so a longer window there buys nothing; and the per-navigation verifier is
  root-scoped, so an external drive is the one volume kind with ZERO cover between sweeps — a 24-hour blind window there
  would be a pure correctness regression. Pinned by `an_external_volume_keeps_the_short_cooldown`.
- **Coalesced anchors are COUNTED, not silently dropped** (`SweepRecord.coalesced_since_sweep`). The count is **since
  the last COMPLETED sweep**, never a lifetime total (a lifetime counter would only measure how long the app has been
  installed). It rides `VolumeIndexStatus.coalesced_signals_since_sweep` alongside `next_sweep_due_at` (computed in
  `queries.rs`), feeding the volume tooltip's "macOS lost track of file system changes N times … next full check in N
  hours" line.
- **The badge deliberately stays GREEN.** Once-a-day sweeping is the DESIGNED operating state, not a fault, so it must
  not raise a fault signal; at the measured rate a Stale badge would sit yellow essentially all day. Yellow stays
  reserved for a sweep that fails to happen when it was due. `StaleDriveDialog.svelte` also excludes `root`.
- **The window is WALL-CLOCK (unix seconds), not `Instant`.** macOS `Instant` is `mach_absolute_time`, which doesn't
  tick while the machine sleeps (an `Instant`-based "day" on a laptop that sleeps 8 hours a night is really 32 hours of
  wall time), and `Instant` can't be restored from disk.
- **It survives relaunch, and an INTERRUPTED sweep can't reopen it.** The ledger is a process-global keyed by volume id
  (NOT a per-reconciler field, so it survives the reconciler recreation on every scan cycle). `resume_or_scan` reseeds
  it from `max(meta.shallow_sweep_at, meta.scan_completed_at)` plus `meta.shallow_coalesced_since_sweep`. Reading BOTH
  timestamps is the fix for a real hazard: `start_scan` DELETES `scan_completed_at` before walking, so keying the window
  off completion alone would make a never-finished sweep look permanently expired and put us back to sweeping every
  launch. A TRIGGERED sweep therefore stamps `shallow_sweep_at` immediately. Pinned by
  `an_interrupted_sweep_does_not_reopen_the_window_on_relaunch`.
- **Every completed full walk restarts the window and clears the count**, not only a shallow-triggered one
  (`scan_completion`): the window means "a full walk happened recently", so the user's own "Rescan now" counts too.
  Seeding takes the `max`, so a stale on-disk timestamp can't undo a sweep this process ran, and a `last` in the future
  (backwards clock jump) counts as elapsed so a bogus record can't wedge sweeps shut for years.

`classify`, `window_elapsed`, `min_interval_for`, and `decide_shallow_anchor_in` are pure/clock-injected and unit-tested
in `rescan_route.rs`; the decision and seeding take an EXPLICIT ledger so the tests use a local `HashMap` (clearing a
shared global from parallel tests flaked). `reconciler/tests.rs` holds the live-path repros.
