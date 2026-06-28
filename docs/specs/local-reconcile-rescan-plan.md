# Local reconcile rescan + schema-rebuild disk reclaim

Status: planned (2026-06-28). Branch `david/index-reconcile`.

## Why (the problem)

The root index DB (`index-root.db`) was observed at 2.5 GB on disk, of which ~1.6 GB (64%) was SQLite freelist — dead
pages never returned to the OS. Root cause is two independent freelist sources on the LOCAL (jwalk) indexing path, made
un-reclaimable by a third issue:

1. **Schema-bump rebuild via `DROP TABLE` on the live file.** On a schema-version mismatch, `IndexStore::try_open`
   (`store/connection.rs`) calls `reset_schema` (DROP + recreate tables) on the existing DB file. `DROP TABLE` moves
   every old page onto the freelist but never shrinks the file or VACUUMs. The just-shipped v14 forced rebuild did
   exactly this, minting ~1.6 GB of freelist in one shot.

2. **Local rescan always truncates.** `manager.rs::start_scan` sends `TruncateData` on every rescan (mass DELETE of all
   rows), then bulk-reinserts. Each rescan churns a large freelist. SMB/MTP already avoid this: they reconcile in place
   (diff each dir, write only changes) — see `network_scan.rs:162-203`.

3. **Reader-pinned incremental vacuum can't reclaim.** The 30s `IncrementalVacuum` + `wal_checkpoint(TRUNCATE)`
   maintenance (`state.rs:626`, `writer/maintenance.rs`) cannot return freelist pages to the OS while long-lived root
   readers (open panes' enrichment `ReadPool`, the live event loop's read connection) pin a snapshot older than the
   mass-delete. Measured empirically: over 5 min of live running, the 2.5 GB file shed 40 KB. At that rate the 1.6 GB
   would take ~135 days.

This plan fixes (1) and (2) — the two SOURCES of large freelist — and **deliberately defers (3)** (see Non-goals). With
both sources gone, the only residual freelist is small steady live-event churn, which the existing incremental vacuum
reclaims at whatever rate the readers allow; the acute bloat never forms.

There is also a real UX win in (2): during a reconcile rescan the last-good directory sizes stay visible (marked stale)
instead of every dir showing `<dir>` while the truncate-rebuild repopulates from empty. That's the primary reason David
wants the local path on the SMB/MTP reconcile model, beyond the disk reclaim.

## Goals

- On schema mismatch, recreate the index DB as a fresh, zero-freelist file (reclaim disk immediately, no reader-pinning
  dependency).
- On a LOCAL rescan of an already-populated index, reconcile in place (diff against the live tree, write only changes)
  instead of truncate-and-rebuild — keeping stale sizes visible throughout and never minting a large freelist.
- Preserve every existing indexing invariant (see the checklist). This area is notoriously hard to debug, so correctness
  and safety dominate; speed of the (rare) rescan walk is explicitly secondary.

## Non-goals (deferred, captured so they're not lost)

- **Reader-pinned incremental vacuum (issue 3).** Not fixed here. Once issues 1 and 2 stop creating large freelists, the
  payoff of a reader-quiesce/barrier mechanism shrinks to reclaiming small live-churn freelist, which doesn't justify
  adding reader-barrier machinery to this fragile area now. Documented as a follow-up in `docs/specs/later/` (Milestone
  4). If, after this lands, a long-running session is still observed bloating from live churn, revisit then with data.
- **Parallelizing the reconcile walk.** Milestone 2 uses a serial BFS reconcile (safe, reuses proven code). A rescan is
  rare (journal gap / overflow / stale-on-launch / forced), and the index stays visible-stale throughout, so a
  slower-but-correct rescan walk is an acceptable trade for safety. Parallelization is an explicit optional follow-up
  (Milestone 3), gated on a measured, unacceptable walk-time regression on real `/`. Don't build it speculatively.
  - **But weigh the overflow feedback risk (review S3), it's not purely cosmetic.** During a scan nobody drains the 20K
    FSEvents channel; the macOS watcher forwards with backpressure, so a full channel blocks the forwarder and the
    FSEvents KERNEL buffer then overflows → the overflow flag forces ANOTHER full rescan. A serial reconcile 3-5x slower
    than jwalk widens this window proportionally; under active FS load (a build, Spotlight, Time Machine, a big
    download) that meaningfully raises overflow probability, and an overflow-triggered rescan is itself slow → possible
    churn loop. Today's fast jwalk mitigates by speed. This is the real reason walk-time matters (beyond UX), and it's
    the strongest argument that M3 may not be truly optional. M2c MUST measure under FS-active conditions, not just an
    idle `/`, and the M3 trigger is "overflow-induced churn observed OR walk time regresses badly", not walk time alone.

## Key design decisions (and intent)

### D1 — Schema mismatch routes through the existing delete-and-recreate path

`store/connection.rs` already has `delete_and_recreate(db_path)` that removes the main file + `-wal` + `-shm` and opens
a fresh DB. It's currently used only as the corruption fallback inside `IndexStore::open`. The schema-mismatch branch in
`try_open` instead does `reset_schema` (DROP TABLE on the open connection).

**Decision:** make schema mismatch a recreate, not a DROP. The cleanest, lowest-risk shape: in `try_open`, on a version
mismatch, return a typed `IndexStoreError` (e.g. `SchemaMismatch`) rather than calling `reset_schema`;
`IndexStore::open`'s existing fallback already calls `delete_and_recreate` on any `try_open` error, so this reuses that
path verbatim and the fresh file is born with zero freelist.

**Intent / why this shape:** we must not delete the file out from under the open `conn` (handle still alive). Returning
before constructing the store, and letting `open` drop the failed attempt then call `delete_and_recreate` (which opens
its own fresh connection), sidesteps the open-handle-delete problem. Keep the existing distinct log lines so "schema
upgrade" reads differently from "corruption" in logs.

**Watch out:** `open`'s current log says "Index DB open failed … deleting and recreating" — a schema upgrade isn't a
failure. Add a branch (match the typed error) so the schema-upgrade case logs as a clean upgrade, not a scary failure.
Don't let `delete_and_recreate` swallow a genuine corruption signal — keep them distinguishable in logs.

### D2 — Local rescan: reconcile in place, reusing the source-agnostic diff

The per-dir diff `reconciler::diff_dir_against_db(dir_id, live_children, db_children, writer)` already takes
source-agnostic `LiveChild`s and is shared by the local live reconcile (`reconcile_subtree`, `std::fs::read_dir`) and
the network rescan (`reconcile_volume_via_trait`, `Volume::list_directory`). So local already has the per-dir building
block via `reconcile_subtree`.

**Decision:** add a LOCAL full-tree reconcile that BFS-walks from the volume root with `std::fs::read_dir`, calls
`diff_dir_against_db` per dir, then finishes with `MarkDirsListed` (for every successfully-listed dir) followed by a
SINGLE `ComputeAllAggregates`. The fresh-vs-reconcile predicate: `entry_count > 1` → reconcile (no truncate); DB with
only the ROOT sentinel (or empty) → keep today's truncate + jwalk bulk build.

**⚠️ Predicate must be `> 1`, NOT `> 0` (blocker found in review).** `create_tables` → `ensure_root_sentinel` always
inserts the ROOT row (`id=1`), and `TruncateData` re-inserts it, so a never-scanned DB has `entry_count == 1`, not 0
(`stress_tests_partial_aggregation` asserts "only root sentinel survives truncate"). With `> 0`, a brand-new user's
FIRST `/` scan would route to the slow serial reconcile instead of fast parallel jwalk — the exact onboarding
catastrophe this plan must avoid. Use `> 1` (populated beyond the sentinel) and add a regression test: fresh
sentinel-only DB → first scan takes the jwalk/truncate path, not reconcile.

**Proactive fix — the NETWORK path has this same latent bug.** `network_scan.rs` uses `get_entry_count(...) > 0`, and
`DETAILS.md` claims "a true first connect has an empty DB ⇒ truncate path." That's wrong: a first SMB/MTP connect also
has the sentinel, so first network scans already silently run as reconcile (diffing against the 1-row DB, adding
everything via per-entry `UpsertEntryV2` instead of bulk). It works (small network trees, functionally correct) so it's
gone unnoticed, but it's a real discrepancy. Fix the network predicate to `> 1` in the same pass and correct the
`DETAILS.md` claim — OR, if there's an intentional reason network tolerates it, document that. Flag to David before
changing network behavior.

**Intent / why:**

- First scan (empty DB, new users — the critical onboarding moment) is UNCHANGED: truncate + parallel jwalk bulk insert.
  No onboarding regression.
- Steady state is the FSEvents journal rolling forward; full rescans are the rare exception. So paying a serial
  reconcile only on rescans is fine.
- Reuse the EXACT finish shape the network path uses (`send_marks` then one `ComputeAllAggregates`) — the bench
  (`reconcile_bench.rs`) proves the single-aggregate design is ~cheaper than truncate-rebuild on the WRITE PATH (981 ms
  vs 1201 ms at 487k entries; ~14s vs ~17s extrapolated to 6M). The per-dir `PropagateMinSubtreeEpoch` arm is a ~2.4x
  REGRESSION and must NOT be used for the full rescan (it stays only for small-scope live reconciles).
  - **These ms figures are WRITE-PATH ONLY** (the bench explicitly isolates from FS walk cost). They are NOT the
    wall-clock of a real reconcile — that is dominated by serial `read_dir` + `stat` of every entry (minutes at 6M),
    which the bench does not measure. Don't read "~14s" as "the whole reconcile". The walk time, and whether it
    regresses unacceptably vs parallel jwalk, is the open question M2c measures.

**Share, don't fork:** `finish_reconcile` / `send_marks` currently live in `volume_scanner.rs`. Extract the finish
(marks + single aggregate) into a shared helper both the network and local reconcile call, rather than copy-pasting.
Same for the BFS scaffolding where it's source-agnostic. Forking risks the two paths drifting on the ordering invariant
(I7 below).

**Error-type coupling when extracting (review S5).** `finish_reconcile`/`send_marks` return
`Result<_, VolumeScanError>`, a type used nowhere outside `volume_scanner`. Extracting into a shared helper either drags
a network-named error into the local reconcile or needs a neutral error type — introduce a neutral one (the local
reconcile shouldn't surface a "VolumeScan" error). Also note there are already TWO `send_marks` (`scanner/mod.rs` chunk
10_000 → `()`; `volume_scanner.rs` chunk 10_000 → `Result`) while `reconcile_subtree` inlines its own with
`MARK_CHUNK = 900`. RESOLVED: the writer-side `IndexStore::mark_dirs_listed` (`store/meta.rs`) chunks SQL params
internally at `CHUNK = 900` (under SQLite's 999-param ceiling), wrapped in a savepoint — so the 10_000 message-chunk
callers are already safe and the 900 in `reconcile_subtree` is redundant message-level chunking, NOT load-bearing. The
shared helper can use 10_000 unconditionally.

### D3 — Keep jwalk for the first/fresh scan; serial readdir BFS only for reconcile

Don't convert the fast parallel jwalk bulk build to serial. The reconcile path is a separate, serial BFS used only when
the DB is already populated. This keeps onboarding fast and confines the new code to the rare path.

**Trade-off accepted:** a serial reconcile walk of a 6M-entry tree stats entries one-by-one and will likely be slower
wall-clock than today's parallel jwalk rescan. Acceptable because (a) rescans are rare, (b) the index stays
visible-stale throughout so the user is never blocked, (c) safety > speed here. Milestone 2c MEASURES this on real `/`;
if the regression is unacceptable, Milestone 3 (optional) parallelizes the walk (parallel jwalk feeding a single serial
diff-consumer that owns the one read connection — keeps stat work parallel, diff serial so no ID races). Build M3 only
if the measurement demands it.

## Invariants checklist (MUST NOT violate — this area is fragile)

Every item below is load-bearing; each has or gets a regression test. Implementing agents must verify each against the
actual code (line refs from an exploration pass, may have drifted) before relying on it.

- I1 — **Single aggregate, never per-dir propagate, for the full rescan.** Finish with one `ComputeAllAggregates`. The
  per-dir `PropagateMinSubtreeEpoch` loop is a ~2.4x regression (and is correct only for short-ancestor live
  reconciles). Source: `reconcile_bench.rs`, DETAILS § epoch model.
- I2 — **Never `INSERT OR REPLACE`; reconcile uses `UpsertEntryV2` + targeted deletes, never the bulk `InsertEntriesV2`
  path on a populated table.** `INSERT OR REPLACE` reassigns IDs and orphans children, and is ~12x slower under
  `platform_case` collation. Source: `manager.rs:545` comment, indexing CLAUDE.md.
- I3 — **Don't drop `UNIQUE (parent_id, name_folded)` nor `name_folded`** (multi-TB ghost-size hazard).
- I4 — **The four mid-scan partial-aggregation rules** stay intact (maps not consumed/mutated, no SQL fallback mid-scan,
  non-blocking try_send only, partial messages only within the scan progress loop). Reconcile does NOT use
  `InsertEntriesV2`, so the accumulator maps stay empty and `ComputeAllAggregates` takes its SQL path — confirm this
  interaction is correct for the reconcile finish.
- I5 — **Recursion set is decoupled from the write decision.** Every child dir present in BOTH live and DB MUST be
  recursed into, changed or not. Gating recursion on `changed` falsely "completes" a rescan over unscanned subtrees (a
  prior prod bug). Regression: `reconcile_descends_into_existing_unchanged_child_dirs`.
- I6 — **New child dirs resolved by `(parent_id, name)`, not absolute path.** (Network needs this because its root isn't
  `/`. Local root IS the volume root → `ROOT_ID`, so absolute-path resolution happens to work, but prefer the
  one-component `resolve_component(parent_id, name)` for consistency and to dodge the firmlink/ symlink edge cases.
  Verify against `firmlinks` handling.)
- I7 — **`MarkDirsListed` BEFORE `ComputeAllAggregates`, and only for successfully-listed dirs.** Aggregate before marks
  ⇒ whole tree rolls to `min_subtree_epoch = 0` (incomplete). A mark queued after the aggregate ⇒ that dir drags
  ancestors to incomplete. The single in-order writer enforces this once sequenced right.
- I8 — **No `scan_completed_at` on an empty root, and bail BEFORE diffing the root (review N4).** The data-safety core
  is to detect a zero-child root and bail _before_ the diff runs — otherwise the diff sees an empty live listing and
  DELETES every existing child, blanking the index. Then skip the completion marker and keep the prior index. NOTE: this
  is NET-NEW code for local — only the network path has an empty-root guard today; the local fresh path has none. Local
  `/` is realistically never empty, but a transient half-dead state must not blank or falsely-complete the index.
  Regression: empty-root reconcile keeps prior index, no completion marker.
- I9 — **FSEvents during the rescan are buffered and replayed after.** Local already buffers FSEvents during scan and
  reconciles them post-scan (`manager.rs` post-scan handler). The reconcile path must preserve this: the watcher arms
  before the snapshot, events buffer during the walk, and are replayed/reconciled on completion. Don't regress the
  buffer→replay handoff.
- I10 — **ID counter ownership.** Allocate new IDs only via the writer's shared `Arc<AtomicI64>`, never `MAX(id)`.
  Reconcile's new-entry inserts go through `UpsertEntryV2`, which already uses the shared counter — confirm the local
  reconcile new-dir resolution honors this.
- I11 — **Reconciler/event-loop read connection is READ-only** (a write-mode connection causes `SQLITE_BUSY` that
  silently kills live indexing). The reconcile walk's diff reads use a read connection.
- I12 — **Interruptible + crash-coherent.** On cancel: discard the partial cleanly (no marks/aggregate, no
  `scan_completed_at`). On a non-cancel early stop: the network path marks listed dirs + aggregates so the partial is
  coherent and the next launch re-reconciles (no `scan_completed_at` ⇒ heal-to-rescan). Local has no
  terminal-disconnect, but DOES have cancel and read errors — mirror the coherent-partial behavior so an interrupted
  local reconcile leaves a valid, visible, stale index that heals on next launch.
- I13 — **Apply the same root-alias skip the jwalk path uses (review S1, NEW).** The jwalk fresh scan skips the
  `/private` canonicalization-alias symlinks `/tmp`, `/var`, `/etc` via `is_canonicalization_alias`
  (`scanner/exclusions.rs`). The local reconcile building block (`read_fs_children`) today applies only
  `scanner::should_exclude`, NOT the alias skip. A `/` reconcile would otherwise see those three live, find them absent
  from the DB, and re-add them every reconcile — diverging from the fresh-scan DB. The local reconcile walker MUST apply
  `is_canonicalization_alias` too, so fresh and reconcile converge to the same DB. Regression: reconcile-of-`/` does not
  add `/tmp`,`/var`,`/etc`.

## Milestones

### M1 — Schema mismatch recreates the DB file (issue 1)

Smallest, independently landable, fixes the immediate 2.5 GB. Do first.

- Change: `store/connection.rs` `try_open` — on version mismatch, return a typed `SchemaMismatch` error instead of
  `reset_schema`; `IndexStore::open` routes it to `delete_and_recreate`. Add a clean-upgrade log branch (don't log a
  schema upgrade as a failure). NOTE (review S4): do NOT remove `reset_schema` — the `#[cfg(test)]` `clear_all` helper
  (`store/meta.rs`) still calls it, so it stays live (test-only). Just drop the `connection.rs` use.
- TDD (red→green, this is a data-path correctness fix so test-first):
  1. Write a test that opens a DB, stamps an OLD `schema_version`, writes rows to bloat + DROP `entries` / `dir_stats`
     to leave a freelist (KEEP the `meta` table intact with the OLD `schema_version` — review N1: if the DROP removes
     `meta`, `try_open` reads version `None`, treats it as a fresh DB, and never recreates, so the test passes/fails for
     the wrong reason), close, then `IndexStore::open` and assert: tables exist & empty, `schema_version` == current,
     AND `freelist_count == 0` / main-file size is small (the reclaim is the point — assert it, or the fix is invisible
     to tests). See it FAIL against the current `reset_schema` behavior (freelist non-zero / file not shrunk).
  2. Implement D1. See it pass.
  3. Add a test that a genuinely corrupt DB still recreates (corruption fallback unchanged).
- Docs: update `store/DETAILS.md` (or the indexing DETAILS) on the schema-mismatch behavior; one-line guardrail in the
  nearest CLAUDE.md only if ignoring it can silently re-bloat. Update the "disposable cache: schema mismatch → drop +
  rebuild" line (it's now delete-file + rebuild).
- Checks: `pnpm check rust` (or scoped `pnpm check clippy` + the indexing tests).

### M2 — Local reconcile rescan (issue 2)

The core change. Reuse the shared diff + finish; serial BFS.

- M2a — **Extract the shared finish.** Pull `finish_reconcile` (send marks → single `ComputeAllAggregates`) and
  `send_marks` out of `volume_scanner.rs` into a shared location both network and local reconcile call. Pure refactor,
  no behavior change; existing network tests stay green (prove it).
- M2b — **Local full-tree reconcile walker.** Add a serial BFS from the volume root using `std::fs::read_dir` (mirror
  the existing local live `reconcile_subtree` per-dir logic, applied tree-wide, WITH the alias skip I13), calling
  `diff_dir_against_db` per dir, accumulating listed dir IDs, honoring the cancel flag, then the shared finish (I7).
  Wire the `entry_count > 1 ⇒ reconcile` predicate (B1) into `manager.rs::start_scan`: reconcile skips ONLY
  `TruncateData`; keep `BumpCurrentEpoch` and the unconditional `flush_blocking` (network does exactly this
  conditional-truncate / unconditional-flush; the flush must stay so the walker reads the bumped `current_epoch` on its
  read connection — review N3). Honor the empty-root guard (I8) and crash-coherent partial (I12).
  - **Integration shape (review S2 — the riskiest seam, spell it out).** The local FSEvents buffer→replay→ live-loop
    machinery lives ONLY in the `manager.rs::start_scan` completion handler (drain the 20K tokio channel via `try_recv`
    into `EventReconciler`, `replay(...)`, write meta, `BackfillMissingDirStats`, then `run_live_event_loop`). The
    network path does NOT have this. So the reconcile walker must plug in by REPLACING the `scanner::scan_volume(...)`
    call and returning the SAME `ScanSummary` shape into the UNCHANGED completion handler; the shared finish (marks →
    `ComputeAllAggregates`) runs INSIDE the walker task (exactly as `scan_volume` does its marks+aggregate in-thread),
    so the order is walk → marks → aggregate → (join) → FSEvents replay → live loop. ❌ Do NOT fork a network-style
    completion path — that silently drops the local FSEvents replay + live-update wiring.
  - **Return type + thread shape (review round 2).** `scan_volume` returns
    `Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError>` and the handler `join`s
    the thread and matches `Ok(Ok(summary))`. The local walk is SYNCHRONOUS (`std::fs::read_dir`), so the reconcile
    walker runs in a `std::thread` (NOT a tokio task like the async network reconcile) and returns the SAME shape. Map
    the shared-finish neutral error and the empty-root signal into `ScanError` variants so the `manager.rs` match stays
    literally untouched.
  - **Honest progress (review round 2, design principle "rock solid").** The reused completion handler still spawns
    `ScanProgressReporter` reading `scan_handle.progress.{entries_scanned, dirs_found, bytes_scanned}`. The network
    reconcile increments these as it walks; the LOCAL reconcile walker MUST do the same, or a multi-minute serial
    reconcile shows a frozen progress bar on the slowest path.
  - **Guardrail: no long-lived read transaction (review round 2).** The walker's read connection must stay in autocommit
    — each `list_children_on` / `resolve_component` is its own implicit, fully-drained read txn. ❌ Do NOT "optimize" by
    wrapping the whole walk in one `BEGIN` read txn: it pins a stale snapshot and breaks every post-`flush()` new-dir id
    resolve (and re-introduces the freelist-pinning this plan fights). `reconcile_subtree` and the network reconcile
    already avoid this.
  - **Harmless quirk to not mistake for a bug:** the handler calls `set_expected_total_entries(summary.total_entries)`
    (e.g. 6M) while the reconcile sends ZERO `InsertEntriesV2`. The "saving entries" progress is emitted only from
    `handle_insert_entries`, which won't run, so no stuck "saving 0/6,000,000" overlay forms; the value sits dormant and
    resets on the next aggregate. Optionally skip the call on the reconcile branch for tidiness; not required.
- M2c — **Tests + measurement.**
  - Port/extend the reconcile regression tests to the local path: descends-into-unchanged-dirs (I5), deletion sweep
    (removed-on-disk entries deleted), empty-root-no-completion (I8), interrupted/cancelled reconcile leaves coherent
    stale index (I12), modified-file size update propagates.
  - TDD the risky ones (I5, I12, deletion) red→green.
  - Extend the `reconcile_perf_gate` bench note with the local path, and MEASURE a real rescan of `/` on David's machine
    (wall-clock + final DB size + freelist) vs today's truncate-rebuild, BOTH idle and under FS-active load (review S3:
    a build or heavy disk activity running concurrently), watching for FSEvents overflow during the slow walk. Record in
    `docs/notes/`. This is the data that decides whether M3 is truly optional.
  - E2E: a focused desktop-e2e-playwright scenario if one fits (index a fixture dir, force a rescan, assert sizes stay
    visible and the DB doesn't balloon). Keep it in the feature-specific set.
- Docs: update indexing `DETAILS.md` (local now reconciles; the shared finish; the predicate) and the indexing
  `CLAUDE.md` must-knows (the "LOCAL always truncates" line is no longer true — fix it). Keep current-state, not
  history.
- Checks: `pnpm check` (full) at the milestone; focused E2E.

### M3 — (Optional) parallelize the reconcile walk

Only if M2c's measurement shows an unacceptable rescan walk-time regression on real `/`. Design: parallel jwalk walk
feeding a single serial diff-consumer that owns the one read connection (stat work parallel, diff serial → no ID races,
new-dir resolution at level boundaries like the network path). Spec the details here if and when triggered; do not build
speculatively.

### M4 — Document the deferred reader-pinned vacuum (issue 3)

Write `docs/specs/later/index-vacuum-reader-pinning.md`: the diagnosis (readers pin the snapshot, checkpoint can't
truncate, incremental_vacuum returns ~nothing), why it's deferred (issues 1+2 remove the large freelist sources), and
the candidate fix shape (quiesce/recycle the root read snapshots around a maintenance checkpoint, or a reader barrier on
`ReadPool::with_conn`). Add the index.md entry. No code.

## Parallelization within this plan

Run milestones SEQUENTIALLY. M1 is independent and could land first on its own. M2a (refactor) must precede M2b. M4
(docs) can be written any time. Do NOT parallelize M2 sub-steps — the ordering and invariant coupling make sequential
safer, and we're not in a hurry.

## Verification (lead-owned, per execute.md)

- Re-run the data-safety-critical tests directly (M1 reclaim test, M2 deletion + interrupted-reconcile tests).
- Read the actual diffs; confirm scope matches intent, nothing skipped/stray.
- Drive the real app via MCP after M2: index a dir, force a rescan, confirm sizes stay visible (stale) and the DB file
  doesn't balloon; check logs for the reconcile path being taken (`reconcile rescan` vs `fresh scan`).
- Rebase onto current local `main` before the FF-merge.
- Strip milestone tags (M1/M2a/…) from touched code and docs before wrap (keep them in this plan file).
