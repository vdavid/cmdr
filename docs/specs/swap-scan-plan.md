# Swap-scan: replace the in-place reconcile rescan with a build-and-swap

Status: plan, 2026-07-22. Local-disk only. Author-facing (AI agents). Read the two foundation notes first:
[`swap-scan-feasibility.md`](../notes/swap-scan-feasibility.md) (the read-only study and the traps) and
[`indexing-benchmarks-2026-07-21.md`](../notes/indexing-benchmarks-2026-07-21.md) § "Swap-scan re-measurement,
2026-07-22" (the justification). This plan names current post-reorg paths under
[`apps/desktop/src-tauri/src/indexing/`](../../apps/desktop/src-tauri/src/indexing).

## 1. Intention

Rescanning a completed LOCAL index today runs a serial BFS reconcile in place ([`reconcile/local_reconcile.rs`](../../apps/desktop/src-tauri/src/indexing/reconcile/local_reconcile.rs)): it diffs each directory against disk and writes only the changes, so the last-good directory sizes stay visible and no large freelist is minted. It is correct and safe, but slow.

Swap-scan replaces that rescan mechanism with the fast parallel guarded walker ([`scanner/`](../../apps/desktop/src-tauri/src/indexing/scanner)): build a complete fresh index off to the side, then swap it in atomically. Measured back to back on the same machine on 2026-07-22, the fresh parallel scan is **8.4x faster** than the reconcile (107.2 s vs 897.4 s), and the completeness objection that used to make the comparison unfair is closed: the progress-based walker watchdog now reads every big directory to completion, so the fresh scan is honest AND complete (the reconcile right after it found only +1,848 entries, 0.03%, not the old +656,352 / ~10%). Even the load-contaminated fresh scan (138 s) beats the reconcile 6.5x, so the win is robust to contention.

Non-obvious whys worth stating up front:

- **The reconcile stays.** Swap-scan does not delete the reconcile path. The reconcile remains the safety-net rescan for the cases swap-scan declines (not enough free disk, feature flag off, or a build failure). See § 4 routing and § 6 rollout.
- **The win is speed AND resource honesty, not just speed.** The reconcile holds a single serial read connection for ~15 minutes on root; the swap build is a bounded parallel walk that finishes in ~2 minutes and then releases. The tradeoff swap-scan accepts is a transient second copy of the DB on disk (§ 3, § 5).
- **This is a local-disk mechanism.** The 8x figure is measured on root (the boot volume). SMB and MTP rescans use the trait-BFS scanner and a different predicate ([`lifecycle/network_scan.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/network_scan.rs)); they are explicitly out of scope (§ 4).

## 2. The data-safety spine (Cmdr principle 4)

This is the heart of the plan. Everything else serves it.

**Invariant (must hold at every instant and across any crash, cancel, or ENOSPC):** exactly one authoritative, complete index exists on disk for a volume, and the app can always find it. A failure at any point leaves EITHER the old complete index OR the new complete one, never a torn or half-built state visible to a reader or to the next launch.

### 2.1 Recommended variant: build a separate DB file, then swap

The feasibility note weighs two variants: shadow tables in the same DB file (`entries_new` / `dir_stats_new`, then `DROP` + `ALTER TABLE ... RENAME` in one transaction) versus a separate DB file (`index-{vid}.building.db`) promoted by a durable pointer. **This plan recommends the separate-file variant**, and the deciding reason is stronger than the four the note already lists (no 82-site table-name threading, no index-name collision, no giant `DROP` + 274k-page freelist + 7-minute vacuum tail, cleanup is `remove_file`):

**The separate-file variant preserves the single-writer-per-DB invariant by construction.** The whole subsystem rests on "every write to a volume's DB goes through one dedicated writer thread" ([`writer/CLAUDE.md`](../../apps/desktop/src-tauri/src/indexing/writer/CLAUDE.md)). A separate file keeps exactly one writer per file: the OLD writer keeps serving the live `index-{vid}.db`, a NEW temporary writer builds `index-{vid}.building.db`, and neither shares the other's write connection, id counter, or `dir_stats` ledger. The shadow-table variant would force two id spaces onto one shared `Arc<AtomicI64>` counter and one write connection, or duplicate the whole write path. The file variant maps onto the architecture that already exists; the in-file variant fights it.

Both variants pay the same ~2.3 GB disk peak (two full copies coexist during the build). Everything else favors the file variant, so the equal disk cost is not a tie-breaker against it.

### 2.2 The build choreography (no live writes during a scan, so this is safe)

The key de-risker the feasibility note identified (§ 2): during a local scan, `start_scan` starts the `DriveWatcher` and BUFFERS FSEvents; the live event loop only starts in the completion handler AFTER the walk finishes ([`lifecycle/scan_completion.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/scan_completion.rs)), and the per-navigation verifier is gated off while a scan runs. So during the build the OLD index takes no live writes; it is effectively read-only and fully queryable throughout. The plan makes that an explicit asserted precondition of a swap, not an accident.

Build phase (old index stays live and complete the entire time):

1. Create `index-{vid}.building.db` fresh (empty), and spawn a temporary build writer + build store bound to it. The manager holds BOTH triples during the build: `(old_store, old_writer)` serving reads, `(build_store, build_writer)` receiving the walk.
2. Run the SAME `scanner::scan_volume` guarded parallel walker, feeding `build_writer` instead of `self.writer`. The walker's progress watchdog, the 32-consecutive-failure backstop, and exclusions behave identically (§ 4).
3. **The build writer is spawned NOT search-feeding, so it never bumps `WRITER_GENERATION`.** This is the free fix for feasibility § 1's search-reload thrash: today only the search-feeding (root) writer bumps the generation, so the millions of `InsertEntriesV2` batches into the build file never invalidate the search arena. The search index keeps reading the OLD file, no reloads, for the whole ~2-minute build. (Whether we spawn non-feeding or spawn feeding with bulk-suppression is an M2 seam; see § 5.)

### 2.3 The atomic commit (the one delicate window)

When the walk completes cleanly and the build file is fully written:

1. **Write the new file's completion meta through the build writer (before quiescing it).** Carry `meta` across into the build file: `schema_version`, `scan_completed_at`, the calibration keys (`total_entries`, `total_physical_bytes`, `scan_duration_ms`), `volume_path`, the `last_event_id` baseline, and `current_epoch` (§ 3 epoch continuity). `scan_completed_at` is written HERE, so the new file only ever becomes authoritative with its completion marker already present. These are the same meta writes the completion handler does today, routed to the build writer instead of `self.writer`.
2. **Quiesce the build file.** Flush the build writer, run a TRUNCATE checkpoint (folds the WAL into the main file and zeroes it), then SHUT DOWN the build writer and DROP the build store so no fd holds `index-{vid}.building.db`. Delete its `-wal` / `-shm` sidecars. The build file is now a single self-contained, quiescent file. This deliberately avoids renaming a DB file out from under an open SQLite connection (which would leave the live writer writing to `.building.db-wal` while new readers look for `.db-wal`: split-brain).
3. **Durable commit point.** Write a tiny `index-{vid}.swap` marker file (temp + fsync + rename, then fsync the directory) recording "promote `.building.db` to `.db`". This single marker write is the atomic commit point: everything before it is reversible by deleting the build file (old index authoritative); everything after it is reversible only by finishing the promotion (new index authoritative).
4. **Promote.** Delete the old `index-{vid}.db` + `-wal` + `-shm` (unlink; in-flight readers keep the inode alive by POSIX semantics, see § 2.5), then rename the single self-contained `index-{vid}.building.db` → `index-{vid}.db` (a single-file rename, atomic on the filesystem). Remove the `.swap` marker. fsync the directory.
5. **Re-point the live handles.** Open a FRESH live store (`IndexStore::open`) and spawn a FRESH live writer (`IndexWriter::spawn_for`, search-feeding for root) on `index-{vid}.db`, exactly as `new_for_kind` does at startup. Call `ReadPool::invalidate()` so read thread-locals re-open the new file. Bump `WRITER_GENERATION` EXACTLY ONCE so the search arena reloads off the new file a single time. The completion handler's existing replay-buffered-events + go-live path then runs unchanged against the new writer/store.

### 2.4 Idempotent recovery (runs on every open / manager start)

Recovery makes the protocol crash-safe by making the on-disk state self-describing:

- **`.swap` marker present:** a crash happened during the commit window (§ 2.3 steps 3–4). The marker's intent is unambiguous ("promote `.building` to `.db`"), so recovery re-runs the promotion idempotently and converges to "the new complete index is `.db`": if `.db` still exists, delete it + sidecars; if `.building.db` exists, finish the rename; remove the marker. Never a torn state.
- **No `.swap` marker but a stray `.building.db` exists:** the build was interrupted BEFORE the commit (crash, cancel, ENOSPC, fatal storage error, or memory watchdog during the walk). The old `.db` is authoritative and complete. Delete the stray `.building.db` + sidecars.
- **Neither present:** normal steady state, nothing to do.

This backstop means even if an in-process cleanup hook is missed on an abort path (§ 3), the next `IndexStore::open` reconciles the files deterministically. It replaces the feasibility note's `DROP TABLE IF EXISTS entries_new` (in-file variant) with a `path.exists()` + `remove_file`.

### 2.5 Why the old file's readers survive the unlink

At the swap we re-point NEW work to the new file (fresh store, fresh writer, `ReadPool::invalidate`, one generation bump) and only then unlink the old file's three files. An in-flight reader mid-query on the old inode continues against the now-unlinked inode and finishes correctly (POSIX keeps the inode alive until its last fd closes); new readers opened after `invalidate` open the new `.db`. This is the same "index stays readable throughout" property the feasibility note calls out, and `ReadPool::invalidate()` already exists and is already called on three lifecycle paths ([`state.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/state.rs)). M0 spike (b) verifies this on APFS.

### 2.6 The biggest data-safety risk, and how the plan defends it

The single biggest risk is the **commit window** (§ 2.3 steps 3–4): the interval where the old index is torn down and the new one promoted. Defenses, in order:

1. A single durable atomic commit point (the `.swap` marker write), with deterministic idempotent recovery (§ 2.4) that always converges to exactly one complete index.
2. The new file gains `scan_completed_at` BEFORE the marker (§ 2.3 step 2); the old file keeps its own completion marker untouched for the entire build. So "interrupted swap leaves the old complete index" is true by construction, which directly neutralizes the feasibility note's headline-voiding `scan_completed_at`-clear bug (§ 3).
3. Promotion is a single-file rename after the build file is quiescent and self-contained (§ 2.3 step 1), so no multi-file atomicity is required and no rename happens under an open connection.
4. A pre-flight free-space gate (§ 5) refuses swap-scan when a second copy would not fit, so ENOSPC during the build is rare; and if it happens anyway, it aborts before the marker (old index intact).

## 3. Each feasibility trap as an explicit decision

The feasibility note § 6 lists the traps. Here is how the separate-file design handles each. Several are neutralized outright by the variant choice.

- **`scan_completed_at` cleared at scan start (the headline-voiding bug).** Today `start_scan` sends `DeleteMeta("scan_completed_at")` to `self.writer` before the walk ([`lifecycle/manager.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/manager.rs) Step 0a). **Decision:** on the swap-scan path, DO NOT clear `scan_completed_at` on the old writer. The old file's completion marker stays intact throughout; the new file gets its own `scan_completed_at` written just before the commit marker (§ 2.3 step 2). Interrupt-safety follows by construction.
- **Index-name collision (`idx_parent_name_folded_new`).** Neutralized: a separate DB file has its own schema namespace, so index names never collide and `create_tables` never silently rebuilds a duplicate index. No change needed.
- **Id space breaks at the swap.** Neutralized: the build writer owns its OWN `Arc<AtomicI64>` counter (fresh, starting at 2) because it is a separate writer on a separate DB. There is no shared counter to double-assign.
- **Stale ids escaping the swap (deferred repairs, in-flight verifier corrections).** Neutralized: the old writer (and its `DeferredRepairs` queue) is DROPPED at the swap; a FRESH writer with an empty queue serves the new file. Old-file ids can never reach the new file because no old-file writer survives. The verifier is gated off during a scan, so no in-flight correction is outstanding at swap time.
- **Disk peak ~2.3 GB.** Accepted, gated by a pre-flight free-space check (§ 5). Reclaim is instant on old-file delete (no freelist, no incremental-vacuum drain, no 7-minute tail).
- **Orphan `.building.db` survives forever.** Handled by idempotent open-time recovery (§ 2.4) plus explicit cleanup in the abort paths and in `clear_index` / `forget` / retention eviction ([`resources/retention.rs`](../../apps/desktop/src-tauri/src/indexing/resources/retention.rs) `delete_index_db_files`, which must learn to also remove `.building.db` and `.swap`).
- **Freshness / coverage-epoch continuity.** During the build, freshness is Scanning (blue badge), same as today; an interrupt reverts to the old index's freshness (Fresh, since it was complete), because nothing permanent flipped. **Epoch continuity is SIMPLER for swap than for reconcile:** a complete fresh walk stamps every listed directory at one `current_epoch`, so the new file is uniformly exact by construction (a complete scan means everything is exact). The only requirement is monotonicity so post-swap live events do not compare against a lower epoch: carry `current_epoch` forward (new file's `current_epoch` = old + 1), stamped uniformly by the walk. Contrast the reconcile, which must thread `min_subtree_epoch` through unchanged on size-only deltas; the swap sidesteps that entirely.
- **The `dir_stats` ledger + honest-sizes invariants (writer-owned, canonical).** The build is a fresh full scan, so it uses the normal fresh-scan aggregation path: `ComputeAllAggregates { source: Maps }` from the walker's in-memory accumulator (the same path a first scan uses today), NOT the reconcile's `source: Sql`. The ledger's four hard rules ([`writer/CLAUDE.md`](../../apps/desktop/src-tauri/src/indexing/writer/CLAUDE.md)) hold within the build writer exactly as they do for a first scan. The `UNIQUE (parent_id, name_folded)` net is present the whole time (the build file is created with the full schema and indexes from `create_tables`, unlike the in-file variant's option (a) which had to drop the net during the bulk insert). Nothing carries across the swap except `meta`; the new ledger is computed fresh and complete.
- **Lost swap under `synchronous = NORMAL`.** The commit marker write and the directory fsync make the commit point durable; recovery is idempotent (§ 2.4), so a power loss right after the marker replays the promotion and a power loss right before it discards the build file. Neither corrupts.

## 4. Interactions with the rest of indexing

- **Reconcile-vs-truncate routing (`local_rescan_reconciles`).** Today [`lifecycle/manager.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/manager.rs) `start_scan` decides fresh-truncate vs in-place-reconcile via `local_rescan_reconciles(entry_count, prior_scan_completed)`. Swap-scan is a THIRD path INSIDE the "rescan of a complete index" branch. New decision, in order: (1) empty or never-completed index → fresh truncate scan (unchanged); (2) completed + populated + LOCAL + swap-scan enabled + enough free disk → SWAP-SCAN; (3) completed + populated + LOCAL + (flag off OR not enough disk) → in-place reconcile (today's path, kept as the safety net). The `local_rescan_reconciles` predicate stays as the "is this a rescan of a complete index" gate; a new helper chooses swap vs reconcile within that branch. Keep it a pure, unit-tested function like the existing one.
- **Walker progress watchdog + the 32-failure backstop.** Unchanged: swap-scan runs `scanner::scan_volume` verbatim, only the writer target differs. The stall watchdog and backstop bound the build exactly as they bound a fresh scan.
- **The reconcile cost budget.** Not involved: swap-scan is a fresh walk, not a reconcile, so the fraction-based cost budget ([`reconcile/local_reconcile/cost_budget.rs`](../../apps/desktop/src-tauri/src/indexing/reconcile/local_reconcile/cost_budget.rs)) does not apply to the swap path. It stays exactly as-is for the fallback reconcile.
- **FSEvents buffering + completion handoff.** Unchanged shape: the `DriveWatcher` buffers during the build; the completion handler drains the buffer, replays into the (now new) live writer, and starts the live loop. The buffered events were captured relative to disk, not to index ids, so they replay correctly against the fresh index. The `scan_start_event_id` baseline is written to the new file.
- **Per-volume registry + lock discipline.** No new registry key: one `VolumeId` still maps to one `IndexInstance`. The manager grows fields for the build triple and holds them only for the build's duration. The swap re-point mutates manager fields OFF the `INDEX_REGISTRY` lock, inside the completion task (which already runs unlocked today). Never hold the registry lock across the blocking build or the swap ([`lifecycle/CLAUDE.md`](../../apps/desktop/src-tauri/src/indexing/lifecycle/CLAUDE.md) lock discipline).
- **Fatal storage error / memory watchdog mid-build.** `fail_index` and `stop_all_indexing` ([`resources/`](../../apps/desktop/src-tauri/src/indexing/resources)) stop indexing mid-scan. On the swap path they act on the BUILD writer/instance; cleanup drops the build triple and the open-time recovery (§ 2.4) removes the stray `.building.db`. The old index is untouched and authoritative.
- **Scope: LOCAL only.** Root (boot volume) is the primary target and where the 8x is measured. `LocalExternal` (USB/SD, mount-rooted, same local scanner + FSEvents path) is a natural extension guarded by the same free-space check; it is called out as a fast follow-on, not first-cut, to keep M1–M3 focused on root. **SMB and MTP are out of scope**: they use the trait-BFS `network_scanner` and the separate `network_scan.rs` predicate, which this plan does not touch.

## 5. Milestones

Each milestone lists the docs to update, the tests that prove it (marking real TDD red→green), and the checks to run. Run checks with `pnpm check` (never raw cargo/vitest); `--fast` while iterating, plain per milestone, `--include-slow` before wrapping; never pipe or tail the checker.

### M0. Spikes (gate the design; independent, parallelizable)

The feasibility note's five open experiments were mostly about the IN-FILE variant (mid-iteration reader survival across a DDL swap, `prepare_cached` re-prepare, `DROP TABLE` cost, vacuum tail). The file variant sidesteps those. The spikes that remain relevant, plus one new to the file variant:

- **(a) Build-into-second-file throughput and peak disk.** Does the parallel walker keep its ~107 s throughput when building `index-root.building.db` on a disk that ALSO holds the 1.12 GB live `index-root.db` (page-cache pressure, concurrent read load from the still-live search arena)? Measure wall time and peak disk. The 107 s figure was measured building into an otherwise-quiet data dir.
- **(b) APFS unlink-under-reader + rename correctness.** A micro-test: start a read query on `index-root.db`, on another connection delete that file and rename a second self-contained DB onto the same name, assert the in-flight reader completes with the pre-swap contents and a fresh reader opens the new contents. Confirms § 2.5.
- **(c) Crash-recovery protocol harness.** Inject a failure at each step of § 2.3 (before marker, after marker before delete, after delete before rename, after rename before marker-removal) and assert § 2.4 recovery converges to exactly one complete index. This harness graduates into the M1/M3 test suite.

Docs: capture spike results in a new `docs/notes/swap-scan-spikes-<date>.md`, linked from [`../notes/swap-scan-feasibility.md`](../notes/swap-scan-feasibility.md). Checks: none beyond the spike code compiling.

### M1. The swap file protocol and idempotent recovery (store layer)

Build the `.building.db` lifecycle, the `.swap` marker, the durable commit sequence (§ 2.3 steps 3–4), and the open-time recovery (§ 2.4) as pure store-layer code with NO scan wired in yet. **Real TDD:** write crash-injection tests first (from M0 harness (c)) and see them fail, then implement recovery until green; a "torn state resolves to exactly one complete index" test for each injection point. Also test: pre-flight free-space check returns the right decision at boundary sizes.

Docs: new "Index file swap protocol" section in [`store/DETAILS.md`](../../apps/desktop/src-tauri/src/indexing/store/DETAILS.md); a one-line guardrail in [`store/CLAUDE.md`](../../apps/desktop/src-tauri/src/indexing/store/CLAUDE.md) ("`.building.db` / `.swap` are swap-scan scaffolding; open-time recovery owns their cleanup, never leave one stranded"). Checks: `pnpm check rust`, `docs-reachable`, `docs-dead-links`.

### M2. Build-writer choreography (writer + lifecycle)

Spawn the build writer + store, feed `scanner::scan_volume` into it, suppress the generation bump during the build (§ 2.2 step 3; settle the seam: prefer spawning the build writer NON-search-feeding so it structurally cannot bump, reusing the existing "only the search-feeding writer bumps" rule, over a new bulk-suppression flag), carry `meta` across, and set the new file's `current_epoch` for continuity (§ 3). **TDD:** a "swap-scan build produces a byte-equivalent index to a fresh scan" test on a fixed fixture tree (same entries, same dir_stats, same aggregates); assert the search generation did NOT bump during the build (use the writer's per-writer probe `global_generation_bumps`, never a process-global read, per the writer test-isolation rule).

Docs: [`writer/DETAILS.md`](../../apps/desktop/src-tauri/src/indexing/writer/DETAILS.md) (build-writer role, generation-bump suppression) and [`lifecycle/DETAILS.md`](../../apps/desktop/src-tauri/src/indexing/lifecycle/DETAILS.md) (the build triple the manager holds). Checks: `pnpm check rust`, doc-graph.

M1 and M2 touch different areas (store vs writer/lifecycle) and can proceed in parallel after M0, converging at M3.

### M3. The atomic swap + re-point + search reload (the data-safety spine)

Wire the commit (§ 2.3) into [`lifecycle/scan_completion.rs`](../../apps/desktop/src-tauri/src/indexing/lifecycle/scan_completion.rs): quiesce the build file, write its meta, durable marker, promote, re-point `self.store` / `self.writer` / `ReadPool`, one generation bump, then run the existing replay + go-live path against the new writer. **Real TDD (lean hard here):** crash/cancel/ENOSPC injected at each swap sub-step leaves exactly one complete index (extend M0 harness (c) to the full wired path). Integration tests: (1) search returns correct results across a swap (old results before, new after, exactly one reload); (2) an in-flight reader started before the swap completes correctly; (3) a cancelled swap-scan leaves the old index complete and queryable with its `scan_completed_at` intact.

Docs: [`lifecycle/DETAILS.md`](../../apps/desktop/src-tauri/src/indexing/lifecycle/DETAILS.md) (the swap sequence and the completion-handler wiring); a `Decision/Why` in [`../notes/swap-scan-feasibility.md`](../notes/swap-scan-feasibility.md) is not needed (that's a research note), but add the decision record to `indexing/DETAILS.md` (§ new "Swap-scan"). Checks: `pnpm check rust`, `--include-slow` for the integration tests, doc-graph.

### M4. Routing, free-space gate, feature flag, fallback

Implement the three-way route (§ 4) as a pure function beside `local_rescan_reconciles`, the pre-flight free-space gate, the feature flag ([`config`], env `CMDR_SWAP_SCAN` plus a settings key so it is testable and dogfoodable), and the fall-back-to-reconcile when the gate refuses. **TDD:** routing-table unit tests (every combination of empty/partial/complete × flag on/off × disk sufficient/insufficient → the expected path); an ENOSPC-at-gate test that routes to reconcile; a "flag off → reconcile" test.

Docs: [`reconcile/CLAUDE.md`](../../apps/desktop/src-tauri/src/indexing/reconcile/CLAUDE.md) (swap-scan is preferred for a complete local rescan; reconcile is the free-space / flag-off fallback) and the reconcile `DETAILS.md`. Checks: full `pnpm check`.

### M5. Rollout guardrails, cleanup hooks, and field measurement

Wire the abort-path cleanup (fail_index, memory watchdog, `stop_scan`, `clear_index`, `forget`, retention eviction all remove any `.building.db` + `.swap`), the feature-flag default and kill-switch, and the field-measurement logging (swap wall time vs the stored reconcile baseline; a crash-recovery counter to confirm zero torn-index incidents). **Tests:** each abort path leaves no orphan file (assert via the recovery no-op); the kill-switch flips a live rescan back to reconcile.

Docs: append a "Swap-scan, shipped" section to [`../notes/indexing-benchmarks-2026-07-21.md`](../notes/indexing-benchmarks-2026-07-21.md) with the field numbers once measured; confirm the `indexing/DETAILS.md` decision record is complete; update this spec's status. Checks: full `pnpm check --include-slow`.

## 6. Rollout and guardrails

- **Feature flag, staged default.** Ship behind `CMDR_SWAP_SCAN` (env + a settings key), default OFF. Sequence: dogfood on David's machine → enable for the beta cohort → default ON once the field measurement (below) confirms the win and zero torn-index incidents. The flag is a kill-switch: flipping it OFF returns the next rescan to the in-place reconcile with no data migration (the two paths share the same on-disk index shape).
- **Fallback is the reconcile, decided BEFORE the scan.** The free-space gate and the flag choose the path up front (§ 4). A build FAILURE mid-swap (ENOSPC despite the gate, a walker error) aborts and leaves the old index authoritative; it does NOT auto-cascade into a reconcile in the same run (that would add a second full pass and more complexity). It logs, keeps the old index, and lets the next rescan trigger re-decide. The reconcile path is never removed during rollout.
- **What to measure in the field.** Log the swap-scan wall time per rescan and compare against the stored reconcile baseline (the calibration keys already persist prior scan duration). Emit an anonymous timing through the existing analytics so the 6.5–8.4x range is confirmed across machines, not just David's M3. Track a crash-recovery counter (how often open-time recovery had to resolve a `.swap` marker or a stray `.building.db`, and to which outcome) to prove the data-safety spine holds in the wild. Confirm the disk peak stays within the free-space gate's margin.
- **Guardrail invariants to encode as tests, not just prose:** exactly-one-complete-index across every injected failure (M1, M3); no generation thrash during the build (M2); reconcile still selected when disk is tight or the flag is off (M4); no orphan file after any abort path (M5).

## 7. Where this plan is least sure

- **The exact durable-atomicity primitive on APFS.** The marker-file + single-file-rename + directory-fsync protocol (§ 2.3) is designed to be airtight, but whether it needs an additional fsync dance (or whether APFS gives stronger rename ordering than assumed) is what M0 spike (c) must confirm before M1 hardens it.
- **The generation-bump-suppression seam (§ 2.2 step 3, M2).** Spawning the build writer non-search-feeding is the cleanest option and is preferred, but it interacts with a delicate invariant ("only the search-feeding writer bumps `WRITER_GENERATION`") and with promoting a fresh search-feeding writer at the swap. If non-feeding turns out to complicate the post-swap live writer, the fallback is a bulk-suppression flag mirroring `BulkReconcileGuard`. Settle it with the M2 tests in hand.
- **LocalExternal in the first cut.** Left as a fast follow-on to keep M1–M3 on root, but it shares the local scanner path, so pulling it into M4 is cheap if desired. Flagged rather than decided.
