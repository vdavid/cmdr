# Network scanner (SMB/MTP)

The `Volume`-trait BFS scanner for SMB and MTP shares, over the SAME `Volume::list_directory` API the live pane uses.
Everything downstream of `EntryRow` (id counter, writer, aggregator, `dir_stats`) is reused unchanged; only how entries
are discovered and stat'd differs from the local guarded walker.

## Module map

- **mod.rs** — `scan_volume_via_trait` (fresh BFS) and `reconcile_volume_via_trait` (rescan-in-place BFS); the
  round-trip disciplines, the terminal-disconnect partial-preserving finish, and the consecutive-failure backstop.
- **scan_pace.rs** — `ScanPacer`: the per-volume paced listing budget (`FULL_LISTING_BUDGET` 64 ↔ `YIELDING_LISTING_BUDGET`
  1) that yields to navigation. `pace_tests.rs` is its test module.
- **system_dirs.rs** — `is_recursion_excluded_dir`: NAS snapshot/system pseudo-dirs whose subtree isn't recursed.
- **tests.rs** — the scanner test module.

## Must-knows

- **BFS, not DFS.** A directory's id must be registered in the `ScanContext` before its children are listed (their
  parent lookup must hit). BFS guarantees that; the concurrency pump processes results serially to keep it true.
- **Never wrap the listing future directly in the timeout — race its JOIN HANDLE** (`LIST_TIMEOUT`, 120 s). Dropping the
  handle detaches the task; dropping the future cancels it mid-round-trip, and on MTP that abandons a PTP transaction and
  wedges the phone. Each round trip is also cancel-checked and `autoreleasepool`-drained (macOS).
- **Terminal disconnect keeps an honest partial; user cancel discards.** A typed `DeviceDisconnected`/`Disconnected` (or
  the consecutive-failure backstop, `CONSECUTIVE_FAILURE_ABORT` = 32) stops the walk and runs `finish_partial_scan`
  (flush + `MarkDirsListed` + `ComputeAllAggregates`) so scanned subtrees roll up exact-stale and unscanned ones stay
  `0` (`—`/`≥`); the DB is kept. A user cancel writes no marks/aggregate.
- **This scanner NEVER writes `scan_completed_at`** (on any path); the completion handler does, only on a clean finish.
  And **never on an empty root** (`VolumeScanError::EmptyRoot`): a false "complete" permanently strands the index.
- **The listing budget is PACED per volume, not constant** (`scan_pace.rs`): browsing the share drops it 64 → 1 so nav
  isn't queued behind the scan. ❌ Never let it reach 0 — one-at-a-time is what makes forward progress structural.
- **NAS system/snapshot dirs aren't recursed** (`system_dirs.rs`): the dir's own row IS indexed (navigable), but its
  subtree is never walked (rolls up honestly-unknown). Don't remove it to "fill in" sizes — it re-triggers the stall.
- **The FRESH scan wraps its inserts in periodic explicit transactions** (`SCAN_COMMIT_INTERVAL`, 2 s): the single
  writer fsyncs per interval, not per 2000-entry batch — the writer-side lever that keeps it from becoming the bottleneck
  once the SMB connection pool lifts listing throughput ~4×. `commit_scan_tx` closes the transaction before EVERY exit,
  so marks + the final aggregate run in autocommit exactly as before and a crash just loses the last interval (heals to a
  rescan). Reconcile is untouched (it already brackets via `BulkReconcileGuard`).
- **A backend may fan `list_directory_for_scan` out across an internal connection pool** (SMB opens extra TCP sessions;
  `backends/DETAILS.md` § "SMB scan-connection pool"). The walk is unchanged and transport-agnostic; the global in-flight
  budget still caps total concurrency, so pacing survives for free.

Architecture, the concurrency pump, the pacing decision, the NAS-dir rationale, and empty-root handling:
[DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
