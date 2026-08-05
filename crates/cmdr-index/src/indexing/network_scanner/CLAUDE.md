# Network scanner (SMB/MTP)

The `Volume`-trait BFS scanner for SMB and MTP shares, over the SAME `Volume::list_directory` API the live pane uses.
Everything downstream of `EntryRow` (id counter, writer, aggregator, `dir_stats`) is reused unchanged; only discovery
and stat'ing differ from the local guarded walker. No walk here names a backend.

## Module map

- **mod.rs** — `VolumeScanError` + the round-trip disciplines all three walks share (`list_one_directory`,
  `stat_one_directory`, typed-disconnect test, the batch/transaction helpers, progress log, summary). **full_scan.rs** —
  `scan_volume_via_trait`, the fresh BFS. **reconcile_scan.rs** — `reconcile_volume_via_trait`, the same walk diffing
  each dir against the DB. **cover_scan.rs** — `cover_volume_subtree`, the SCOPED search-driven walk.
- **scan_pace.rs** — `ScanPacer`: the per-volume paced listing budget (`FULL_LISTING_BUDGET` 64 ↔
  `YIELDING_LISTING_BUDGET` 1) that yields to navigation; `pace_tests.rs` tests it. **system_dirs.rs** —
  `is_recursion_excluded_dir` (NAS pseudo-dirs whose subtree isn't recursed) + the exclusion-list stamp driving the
  rebuild. **tests/** — scanner tests, by theme.

## Must-knows

- **BFS, not DFS.** A directory's id must be known before its children are listed (the fresh walk registers it in
  `ScanContext`; the other two carry it). The concurrency pump processes results serially to keep that true.
- **Never wrap a round trip's future directly in the timeout — race its JOIN HANDLE** (`LIST_TIMEOUT`, 120 s; both
  `list_one_directory` and `stat_one_directory`). Dropping the handle detaches the task; dropping the future cancels it
  mid-round-trip, and on MTP that abandons a PTP transaction and wedges the phone. Each round trip is also
  cancel-checked and `autoreleasepool`-drained (macOS).
- **Terminal disconnect keeps an honest partial; user cancel discards.** A typed `DeviceDisconnected`/`Disconnected` (or
  the consecutive-failure backstop, `CONSECUTIVE_FAILURE_ABORT` = 32) stops the walk and runs `finish_partial_scan`
  (flush + marks + aggregate), so scanned subtrees roll up exact-stale and unscanned ones stay `0` (`—`/`≥`); the DB is
  kept. A user cancel writes no marks/aggregate.
- **The COVER walk inverts that cancel rule, is SCOPED, and is ADD-ONLY**: it stamps what it read on EVERY exit (a
  search has to converge), roots at a frontier node's own id, and keeps whatever name the index already holds — which is
  what makes MTP same-name siblings "keep the first" instead of an `INSERT OR IGNORE` that orphans a subtree. ❌ It
  needs neither the virgin-root nor the empty-root refusal. `DETAILS.md` § "The scoped cover walk".
- **This scanner NEVER writes `scan_completed_at`**; the completion handler does, on a clean finish only, and never on
  an empty root (`VolumeScanError::EmptyRoot`): a false "complete" permanently strands the index.
- **The listing budget is PACED per volume, not constant** (`scan_pace.rs`, read by all three walks): browsing the share
  or a transfer on it drops it 64 → 1, so higher-priority work isn't queued behind the walk. ❌ Never let it reach 0 —
  one-at-a-time is what makes forward progress structural. Signals arrive via `../host/policy.rs`, once per top-up; ❌
  never per entry.
- **NAS system/snapshot dirs aren't recursed** (`system_dirs.rs`, all three walks): the dir's own row IS indexed, its
  subtree never walked (rolls up honestly-unknown). ❌ Don't remove it to "fill in" sizes — it re-triggers the stall.
- **Adding a name REBUILDS every network index** (each stamps the list it was built against; `lifecycle/network_scan.rs`
  truncate-rescans on a mismatch), and a false positive costs a user their indexed folder. ❌ No name without a vendor
  citation; ❌ stamp only right after a `TruncateData`; ❌ never migrate.
- **The fresh and cover walks batch inserts into periodic explicit transactions** (`SCAN_COMMIT_INTERVAL`, 2 s), so the
  writer fsyncs per interval rather than per batch — the lever that keeps it off the critical path under the SMB pool's
  ~4× throughput. `commit_scan_tx` closes it before EVERY exit. Reconcile brackets via `BulkReconcileGuard`.
- **A backend may fan `list_directory_for_scan` across an internal connection pool** (SMB's extra TCP sessions, opened
  by the `begin`/`end_scan_session` bracket its caller holds; `backends/DETAILS.md`). The in-flight budget still caps
  concurrency, so pacing survives.

Architecture, the concurrency pump, the scoped cover walk, the pacing decision, the NAS-dir rationale, and empty-root
handling: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
