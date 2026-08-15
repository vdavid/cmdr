# Indexing read side details

Read this before any non-trivial work in `indexing/read/`: editing, planning, reorganizing, or advising. Must-know
invariants are in `CLAUDE.md`.

This area serves recursive sizes, index status, and coverage back to the app. Five concerns: enrichment (the hot path),
the IPC query surface, write-op expected totals, the "size updating" hourglass, and the coverage frontier. All read via
the per-volume `ReadPool`; none takes the lifecycle registry lock, and nothing here imports `lifecycle::state` at all.

## Where the handles come from (`handles.rs`)

`VolumeHandles<T>` is a volume-id-keyed table of one kind of read handle; there is one for `ReadPool` and one for
`PendingSizes`. Lifecycle PUSHES a volume's handles in as it reserves the volume's registry slot, and withdraws them on
every teardown path; this side only ever looks one up. A missing entry means "not indexed", which is the read path's
skip signal.

Two properties make it safe, and both must survive any edit:

- **The table lock is a LEAF.** Every operation is a hash lookup plus an `Arc` clone, with nothing called while the
  guard is alive. Lifecycle takes it while holding `INDEX_REGISTRY`; nothing takes `INDEX_REGISTRY` while holding a
  table. One direction only. ❌ Never add a callback parameter, or a `log::` call that formats a handle, here.
- **Withdrawal is the skip point.** Teardown uninstalls before it drains or deletes anything, so once a volume's DB is
  going away no reader can still open a connection to it.

Why push rather than let this side pull from the registry: enrichment runs on every listing (~2/s per live pane), and
resolving a handle out of `INDEX_REGISTRY` put that on the far side of the same mutex a 5 s shutdown drain holds — plus
it made `read` and `lifecycle` import each other. Rationale and the teardown ordering: `../lifecycle/DETAILS.md` §
"Where a volume's read handles live". Regression-guarded by
`tests::integration_tests::enrichment_under_contention{,_non_root}` (enrich while a background thread holds the
registry; pre-fix the non-root case HANGS).

## Enrichment (`enrichment.rs`)

`ReadPool` is defined here: lock-free thread-local read connections for enrichment and verification. `with_conn`'s
signature (`fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T)`) ensures the `&Connection` can't escape the
closure, so async task migration can't break thread affinity — enforced by the type, not convention. Every volume's
pool, root included, lives in the `READ_POOLS` table (above); `get_read_pool_for(vid)` is a lookup in it, and
`get_read_pool()` is that lookup for `ROOT_VOLUME_ID`, kept for the callers that are root-scoped by construction.

`test_install_root_read_pool` / `test_uninstall_root_read_pool` / `test_read_pool_lock` are gated on
`any(test, feature = "testing")` rather than plain `cfg(test)`, and are `pub` under it, because
`crates/cmdr-index/benches/index_benchmarks.rs` compiles as an external crate: without them the enrichment bench would
measure the no-index-registered early return instead of a read. Callers still have to hold the lock — the root pool is a
process global. How the feature gets turned on for dev targets and only those: `docs/tooling/testing.md` § criterion.

**The thread-local is a small LRU, not one slot.** `THREAD_CONNS` holds up to `sqlite_util::THREAD_CONN_SLOTS` (3) open
connections per thread, keyed by db path plus the pool's invalidation generation. One slot made the lock-freedom
expensive in the ordinary two-pane case: a blocking thread alternating between the left pane's volume and the right
pane's closed and reopened on every alternation, re-running the pragmas and the collation registration and discarding
that connection's whole `prepare_cached` statement cache — recompiling the statements is the expensive part, and this is
the hot path. Three slots cover both panes plus a background reader (search, the importance scheduler) landing on the
same thread. Raising the slot count raises the process's connection count, which is affordable only because
`../store/DETAILS.md` § "SQLite page memory is one process-wide slab" decoupled memory from it. ❌ Don't put a mutex
here: the lock-freedom is the design. `invalidate()` still works — it bumps the generation, and the next `with_conn` for
that path drops the stale entry before opening the replacement, so the two never coexist. Regression tests:
`read_pool_alternating_volumes_does_not_reopen` (integration) and `sqlite_util::tests`, both asserting on a real reopen
counter rather than timing.

**Every pool starts at a generation no pool has used** (`NEXT_POOL_GENERATION`), and that is load-bearing rather than
tidy. Because the cache key is `(db_path, generation)`, a successor pool starting at a fixed `0` inherits its
predecessor's cached connections — including one opened before the database file was DELETED and recreated, which still
answers from the unlinked inode. That reads the old index as if it were the new one, for as long as the slot survives,
on whichever threads had read that volume. Two live routes into it: "Forget this drive" followed by turning indexing
back on, and a search's walk evicting an index whose coverage this build refuses (`../resources/DETAILS.md` §
"Rebuilt-from-scratch coverage is EVICTED"). ❌ Don't reset the starting generation to a constant. Pinned by
`read_pool_over_a_recreated_database_never_serves_the_old_one`.

`enrich_entries_with_index(entries)` is the root-defaulting wrapper;
`enrich_entries_with_index_on_volume(volume_id, entries)` is the volume-routed form. Called when entries land in the
listing cache (streaming, watcher update, re-sort), NOT on `get_file_range`; live freshness flows separately via
`index-dir-updated` → `refreshIndexSizes` → `getDirStatsBatch`. A live pane triggers a pass about twice a second whether
or not anything changed.

**The skip-vs-route gate.** `get_read_pool_for(volume_id)` returning `None` IS the "no index registered for this volume"
signal — enrichment early-returns before any DB work. The gate is pool-presence rather than registry-key presence so it
can never disagree with the routing call (`get_read_pool_for`): the gate and the route ask the exact same question. This
replaces the old `should_exclude`-only gate. For the `root` volume specifically, the
`scanner::should_exclude(parent_path)` check is ALSO kept: a `root`-volume listing navigated to `/Volumes/`, `/mnt/`,
`/proc/`, or a system path isn't in root's index, so it would still miss every lookup and log "Parent path not found" on
every refresh.

**Integer-keyed fast path.** Resolve the parent dir once (`listing_parent_path`, a pure helper) →
`list_child_dir_ids_and_names(parent_id)` → `get_dir_stats_batch_by_ids` → match by normalized name. Two indexed queries
instead of N `resolve_path` calls. Falls back to individual path resolution for the mixed-parent edge case.

**Read-side path mapping.** Both `enrich_via_parent_id_on` (fast path) and `enrich_via_individual_paths_on` (fallback)
map their mount-absolute paths into the volume's index path space via `routing::index_read_path` before `resolve_path` —
a pass-through for `root`, a mount-relative strip for SMB, a scheme/storage strip for MTP. Without it an indexed SMB
folder enriches to nothing. Owned by `../paths/DETAILS.md`.

**Deriving the honest-size booleans.** `apply_dir_stats` sets `recursive_size_complete = min_subtree_epoch > 0` and
`recursive_size_stale = complete && min_subtree_epoch < current_epoch`. `current_epoch` is read ONCE per
`enrich_entries_with_index_on_volume` pass, on the same `ReadPool` conn that fetches the stats, and threaded into both
enrichment forms. The frontend renders from `{recursive_size, complete, stale}` only; it never learns the epoch scheme.
The epoch model itself is owned by `../writer/DETAILS.md`.

**Log memo.** A per-pass line is ~14,000 lines an hour from two idle panes, and the varying counts and path defeat the
log writer's coalescer. So the pass keeps ONE line, `enrich: 12/14 dirs got sizes under <parent>`, gated on
`EnrichResultMemo`: it fires only when `(dir_count, enriched)` differs from the last logged pass for that
`(volume_id, parent_path)`. An idle pane is silent. The memo is bounded (256 listings, cleared wholesale when full).

## The IPC query surface (`queries.rs`)

The read-only index queries the IPC commands call (status + dir-stats), distinct from the lifecycle/registry core; none
mutate registry state.

- `get_status(vid)` / `get_debug_status(vid)` — read a volume's phase (plus the `Initializing` temp store) under the
  registry lock.
- `get_volume_index_status(path)` / `get_volume_index_status_by_id(volume_id)` — build the per-drive badge shape
  (`VolumeIndexStatus { volume_id, enabled, freshness, scan_completed_at, scan_duration_ms, coalesced_signals_since_sweep, next_sweep_due_at }`).
  The path form resolves the volume from a listing path (the always-visible active-drive badge); the id form is keyed by
  `volume.id` (the per-drive dropdown rows). Both return the same shape. `next_sweep_due_at` is computed here so the
  sweep-window length stays in the policy module (owned by `../reconcile/DETAILS.md`), not duplicated in the frontend.
  It also carries `unreadable_locations` + `unreadable_retried`, so a finished index can admit it has holes (below).

**What a COMPLETED index couldn't read** (`unreadable_ground`). A finished walk can hold no rows for directories it was
refused (`Denied`), ones Cmdr declines to read at all (`Declined`), and ones that stopped answering (`Abandoned`), so
"done" can mean "done, with holes" and the badge has to be able to say so. The status carries a COUNT of places
(`cmdr_fs::path_locations::location_count` over all three lists — the same rule search's coverage note uses, so the two
surfaces can't disagree about one drive) plus whether any of it is the retryable kind, which is the only distinction the
badge makes: what to DO about a refusal is search's note, not a tooltip's job.

⚠️ **The completion gate is what makes it affordable**, not just meaningful. The coverage descent stops at the first
fully covered subtree, so on a complete index with no holes it answers at the volume root immediately, and with holes it
walks only the ancestor chains leading to them (76 cut points on a real machine). On an INCOMPLETE index the frontier is
most of the drive and this would be a full descent — on a call the badge makes on every scan and freshness event. ❌
Don't lift the gate. A mount-rooted volume with no recorded `volume_path` reports nothing rather than falling back to
`/`, which would answer for the boot disk instead.

- `list_dir_children(path)` — the immediate children a directory's rows describe, for the agent's `list_dir` tool. ⚠️ It
  answers `None` for a directory that has a ROW but no LISTING (`listed_epoch == 0`), not just for one with no row: rows
  sit under an unlisted directory routinely (FSEvents verification upserts children without marking their parent listed,
  and the cover walk materializes a frontier path's ancestor chain at epoch 0), and they are a lower bound on what is
  there. A `Vec<EntryRow>` carries nowhere to say so, and the consumer's contract is that a lower-bound read has to say
  so, hence "not indexed" rather than a partial listing. `> 0` rather than "at the current epoch" matches the coverage
  descent rule: an old listing is stale, not absent (Decision 5 trusts it).
- `get_dir_stats(path)` / `get_dir_stats_batch(paths)` — resolve the volume via `routing::volume_id_for_local_path`,
  delegate to `*_on_volume`, and read dir aggregates off the volume's `ReadPool` (mapping the path via
  `routing::index_read_path`). `dir_stats_from` derives the same `{complete, stale}` booleans as enrichment;
  `get_dir_stats_on_volume` reads `current_epoch` inside its `with_conn`, `get_dir_stats_batch_on_volume` once per call.
  The FE copies the booleans onto the `FileEntry` (including the `..` parent row, which renders from the current dir's
  own stats, so a partially-scanned dir shows `..` as `≥`/`—`).

The IPC boundary stays path-based; the volume is resolved internally. The path-based commands map an SMB-mounted path to
its `smb_volume_id`, an `mtp://` path to its `{device}:{storage}` id, a registered local external mount to its own id,
and the boot disk (plus cloud-drive folders) to `root` — routing owned by `../paths/DETAILS.md`. The routed reads skip
cleanly (`get_read_pool_for` → `None`) when the resolved volume has no registered index, so an unindexed SMB share or a
mounted-but-unindexed external drive costs zero DB work — which is also why such a drive reports `off` rather than
inheriting `root`'s freshness. The MCP server consumes these read APIs too (`cmdr://indexing`, the `await index_status`
condition), never re-deriving freshness.

## Expected totals (`expected_totals.rs`)

`expected_totals_for_sources()` returns the index-derived `(file count, byte total)` for a set of source paths so a
write operation (copy/move/delete) can render a real scan-phase progress bar before the foolproof re-scan completes. Per
source: `resolve_path` → `get_entry_by_id` → if a dir use `dir_stats`, if a file use the entry's `logical_size`. Uses
the same `ReadPool` as enrichment for lock-free reads. Used by `scan_preview.rs` and `scan.rs` in `write_operations/`.

**It returns `None` if ANY source isn't covered by the index** — no pool, no entry, no `dir_stats`, no `logical_size`,
OR (via `per_source_contribution`) a directory whose subtree is incomplete (`min_subtree_epoch == 0`). A partial or
lower-bound total would let the progress bar overshoot 100%. Destructive ops re-stat live in
`write_operations/conflict.rs`; the index is never load-bearing there — it's consulted only for non-load-bearing size
estimates with an explicit "unknown" fallback.

## The pending-sizes hourglass (`pending_sizes.rs`)

`PendingSizes`: an in-memory `Mutex<HashSet<String>>` of directory paths with unprocessed writes in flight, so the UI
can show a per-directory "size updating" hourglass during big deletes/copies. Two signals, cleanly split: the global
`indexing` flag means every size is in flux during a full scan; per-dir `recursive_size_pending` means live writes are
in flight for that dir even when no scan runs.

- `mark(path)` inserts the normalized path plus every ancestor; `is_pending(path)` is the membership test; `clear()`
  wipes the transient set.
- Every volume's tracker lives in the `PENDING_SIZES` table (above), installed and withdrawn in lockstep with its read
  pool; `get_pending_sizes_for(vid)` is a lookup in it.
- **Marked** at the live event loop's `pending_paths` drain points (`watch/event_loop`'s `mark_pending_and_drain`,
  live-only — NOT the shared `process_fs_event`, so replay doesn't flag everything during startup).
- **Cleared wholesale** by the writer thread once `queue_depth` hits 0. This is self-healing: an empty queue means no
  unprocessed work, so the set is correct to empty, and there's no per-entry increment/decrement to leak (no "stuck
  hourglass forever" class). Chosen over counters precisely for that.
- **Read** when building `DirStats` (`queries.rs`), surfaced via `DirStats.recursive_size_pending`. It rides `DirStats`
  only, NOT the Rust `FileEntry`/`get_file_range` enrichment path — that path isn't where live size refreshes flow, and
  adding a field to `FileEntry` (no `Default`, ~30 literal sites) buys only a sub-2s hourglass on a folder navigated
  into mid-storm. This half is deliberately not "fixed".

**The held-roots tier (for coalesced rescans).** A detached `reconcile_subtree` runs for seconds while the writer queue
oscillates empty, so the wholesale queue-drain `clear()` would wipe the mark long before the reconcile finishes, and
nothing marked its scope at queue time. So `PendingSizes` has a SECOND held-roots tier (rescan root paths only):
`queue_must_scan_sub_dirs` holds the root; `is_pending(path)` is true for any transient mark OR any path related to a
held root in EITHER direction (an ancestor-or-equal, whose aggregate includes the rewriting subtree, OR a descendant,
whose own rows are being rewritten); and the writer-drain `clear()` wipes only the TRANSIENT set — holds survive.
Holding roots (not expanded ancestors) with a query-time prefix test keeps release exact under overlapping rescans
(`/a/b` and `/a/c` share `/a`; expanding would strip it while one is still in flight). On completion the sequence is
`release(root)` FIRST, then emit `index-dir-updated` for the root + ancestors via `WriteMessage::EmitDirUpdated`:
release before emit, else the triggered refetch re-reads `pending = true`. The mark/clear mechanics that feed this from
the writer side (the `dir_stats` ledger, the drain point) are owned by `../writer/DETAILS.md`.

## The coverage frontier (`coverage.rs`)

`Index::coverage(volume_id, scope_path, dimension)` answers **what a scope still needs walked before the index alone can
answer for it**: the shallowest directories nothing has listed, the ones a walk has tried and can't read, and a token
saying which state of the index the answer describes. The covered half is never returned — the two are complementary
over the same subtree, so a caller runs its own query over the scope unfiltered and gets exactly the covered rows.
That's the whole reason there's no deduplication anywhere in the search path.

One field of the answer is not a read of the database at all: `being_walked` names the frontier roots a walk is covering
RIGHT NOW, and `Index::coverage` fills it from the in-flight claims (`../lifecycle/cover/live.rs::ground_being_walked`)
after the query returns — this module stays a pure read, and the layering that keeps `read/` from importing lifecycle
state holds. It's a reading, not a reservation: it can go stale immediately, and `Claim::take` stays the authority on
what a walk actually got. What it's for: only one walk may have a patch of ground, so a caller that would otherwise
commit to a walk taking NOTHING can tell the difference between "nobody has been here" and "somebody is here already",
and wait rather than answer empty (`search/DETAILS.md` § The shape).

Read `docs/specs/unindexed-search-plan.md` § "The core mechanism" for the product intent this serves. The WALK half —
what actually fills a frontier in, and why it may never delete — is `../lifecycle/cover/DETAILS.md`.

### The descent rule

Both epoch fields plus `entries.unreadable_cause`. Descending from the scope root, each directory is exactly one of:

- `min_subtree_epoch > 0` ⇒ **covered**. Cut; serve from the index.
- `min_subtree_epoch == 0 && listed_epoch > 0` ⇒ **listed**. The directory itself is covered ground; descend into its
  child directories and classify each.
- `listed_epoch == 0 && unreadable_cause != 0` ⇒ **unreadable**. Cut; reported rather than dropped, in the list its
  CAUSE names. Nothing is coming for that subtree right now, so offering it as frontier would be a promise no walk
  keeps. The three answers stay APART all the way to the screen (`CoverageMap::permission_denied` / `::declined` /
  `::abandoned`), because each is a different sentence: `Denied` is the one a user can act on, `Declined` is a standing
  policy over a NAS snapshot tree, and `Abandoned` is ground Cmdr gave up on and retries on a backoff. "Grant Full Disk
  Access" over either of the last two is advice that does nothing. The causes themselves are `../store/DETAILS.md` §
  "What coverage needs".

  ⚠️ **An `abandoned` list is a hole in an otherwise complete answer, and only this list says so.** Those subtrees left
  the frontier, so nothing else in a coverage answer hints that they were skipped; a caller reporting how complete its
  result is has to consult it (search folds it into `SearchRunCoverage::abandoned_ground`).

- `listed_epoch == 0` ⇒ **frontier**. Cut; the subtree goes to the walk.
- No `entries` row at all (a cold volume, or a path this index has never seen) ⇒ the scope root is the whole frontier.

❌ **Never collapse this to `min_subtree_epoch` alone.** The min is 0-absorbing upward, so one uncovered directory
anywhere forces `0` on every ancestor including the scope root: "the shallowest node at zero" is always the scope root
and the frontier degenerates to "walk everything". Two review rounds of the plan caught exactly that.

**The premise the rule rests on**: `min_subtree_epoch > 0` implies `listed_epoch > 0`, so the four cases are disjoint
and exhaustive. It holds because both writers of the column seed from the directory's own `listed_epoch` and 0-absorb
from there (`store::recompute_min_subtree_epoch`'s `own == 0` early return, `aggregator::compute_bottom_up`'s seed).
`min_subtree_epoch_implies_listed` pins it against the real aggregator rather than against a hand-written fixture. If it
ever breaks, the descent starts skipping ground nobody has read, with no signal.

### What the tests hold it to

- `coverage_partitions_the_subtree` — every directory in the scope is accounted for exactly once, by exactly one
  verdict. A cut owns its whole subtree; a listed interior node owns only itself.
- `every_verdict_matches_its_directory` — the partition alone is NOT enough, and this is the load-bearing half: "the
  scope root is the whole frontier" partitions perfectly and is the degenerate answer. A frontier cut has to be a
  directory nothing listed, and a covered cut has to have every directory under it listed.

Both run over trees generated with random listed / unreadable flags whose `dir_stats` are built by the real aggregator,
so the premise holds by construction from the canonical code.

### Exclusions, and the stamp that makes coverage trustworthy

A policy-excluded directory gets no `entries` row, so it drives nothing to zero and needs no case in the descent. What
it DOES need is a guarantee the rows were written under the policy this build applies: `store::EXCLUSION_POLICY_KEY`, a
content fingerprint of the exclusion constants stamped right after a truncating full walk. **An absent or mismatched
stamp means every coverage claim in that database is unknown, and the whole scope goes to the walk.** Without it,
removing a name from the policy would leave the subtrees it used to skip row-less while their parents keep claiming
coverage — permanently invisible to search, with nothing to trigger a re-walk.

### The freshness token

`CoverageToken` is the volume's `current_epoch` paired with the highest entry id the database has handed out. Ids come
from one monotonic per-volume counter, so any walk that writes rows moves the pair, and both halves are an index seek
rather than a scan. It's opaque and equality-only on purpose: the only question worth asking is "is the snapshot I'm
serving the covered half from still the one this answer describes?". `Index::coverage_token(volume_id)` reads it without
doing a descent, which is what a caller takes when it loads that snapshot.

Rejected as the token: `Index::search_generation`, which is process-global, fed only by root's writer, stamped `0` for
every non-root volume, and ticks about 5.7 times a second on an idle boot disk. And `current_epoch` alone, which a walk
never bumps.

**It's a watermark, not a version, and the difference has one real edge.** Deleting the highest-id row lowers the mark,
so an unequal token means "something changed", never "this one is newer" — ❌ don't order two tokens or treat one as a
clock. The edge is ABA: a delete followed by inserts that climb back to the same id at the same epoch reads as
unchanged. In one process it can't happen, because the writer's id counter only ever climbs (it resyncs downward from
`MAX(id)` only on a primary-key conflict); across a restart the counter reseeds from `MAX(id)`, so freed ids can be
handed out again — and a restart also drops every arena, which is the snapshot the token exists to validate. If that
stops being true (an arena that survives a restart, say), this needs a monotonic per-volume write counter instead.

The whole read runs in one deferred transaction, so the frontier and the token describe the same database state rather
than two states either side of a committing writer.

### Cost

Measured 5.4 ms warm (release) over a real 658 188-folder root index, against a 50 ms budget, considering 7 762
directories — 1.2% of them. The cost tracks directories on the paths to the gaps, never the size of the index, because a
covered subtree is one row lookup and the descent stops. No new database index was needed; the numbers, the method, and
when to revisit that call are in `docs/notes/coverage-frontier-query-2026-08-05.md`. Re-measure with
`coverage::tests::measure_frontier_query_on_a_real_index` (`#[ignore]`d; it WRITES to the DB you point it at, so use a
copy).
