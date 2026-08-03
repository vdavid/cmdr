# Network scanner (SMB/MTP)

The `Volume`-trait BFS scanner for SMB and MTP shares, over the SAME `Volume::list_directory` API the live pane uses.
Everything downstream of `EntryRow` (id counter, writer, aggregator, `dir_stats`) is reused unchanged; only discovery
and stat'ing differ from the local guarded walker.

## Module map

- **mod.rs** — `VolumeScanError` + the round-trip disciplines both walks share (`list_one_directory`, typed-disconnect
  test, progress log, summary). **full_scan.rs** — `scan_volume_via_trait`, the fresh BFS. **reconcile_scan.rs** —
  `reconcile_volume_via_trait`, the same walk diffing each dir against the DB.
- **scan_pace.rs** — `ScanPacer`: the per-volume paced listing budget (`FULL_LISTING_BUDGET` 64 ↔
  `YIELDING_LISTING_BUDGET` 1) that yields to navigation. `pace_tests.rs` is its test module.
- **system_dirs.rs** — `is_recursion_excluded_dir` (NAS pseudo-dirs whose subtree isn't recursed) + the exclusion-list
  stamp driving the rebuild.
- **tests/** — scanner tests, by theme.

## Must-knows

- **BFS, not DFS.** A directory's id must be registered in the `ScanContext` before its children are listed (their
  parent lookup must hit). BFS guarantees that; the concurrency pump processes results serially to keep it true.
- **Never wrap the listing future directly in the timeout — race its JOIN HANDLE** (`LIST_TIMEOUT`, 120 s). Dropping the
  handle detaches the task; dropping the future cancels it mid-round-trip, and on MTP that abandons a PTP transaction
  and wedges the phone. Each round trip is also cancel-checked and `autoreleasepool`-drained (macOS).
- **Terminal disconnect keeps an honest partial; user cancel discards.** A typed `DeviceDisconnected`/`Disconnected` (or
  the consecutive-failure backstop, `CONSECUTIVE_FAILURE_ABORT` = 32) stops the walk and runs `finish_partial_scan`
  (flush + `MarkDirsListed` + `ComputeAllAggregates`) so scanned subtrees roll up exact-stale and unscanned ones stay
  `0` (`—`/`≥`); the DB is kept. A user cancel writes no marks/aggregate.
- **This scanner NEVER writes `scan_completed_at`**; the completion handler does, only on a clean finish. And **never on
  an empty root** (`VolumeScanError::EmptyRoot`): a false "complete" permanently strands the index.
- **The listing budget is PACED per volume, not constant** (`scan_pace.rs`): browsing the share OR a running transfer on
  it (transfers trump indexing) drops it 64 → 1 so higher-priority work isn't queued behind the scan. ❌ Never let it
  reach 0 — one-at-a-time is what makes forward progress structural. The signals arrive through `../host/policy.rs`,
  asked once per top-up; ❌ never per entry.
- **NAS system/snapshot dirs aren't recursed** (`system_dirs.rs`): the dir's own row IS indexed, but its subtree is
  never walked (rolls up honestly-unknown). Don't remove it to "fill in" sizes — it re-triggers the stall.
- **Adding a name REBUILDS every network index**, so a false positive costs a user their indexed folder: add one only
  with a vendor citation, and only if it's SMB-visible (why ONTAP's `~snapshot` is absent). An index stamps the list it
  was built against and `lifecycle/network_scan.rs` truncate-rescans on a mismatch. ❌ Stamp only right after a
  `TruncateData`; ❌ never migrate (`../CLAUDE.md` § "Rebuild, don't migrate").
- **The FRESH scan wraps its inserts in periodic explicit transactions** (`SCAN_COMMIT_INTERVAL`, 2 s): the single
  writer fsyncs per interval, not per 2000-entry batch — the lever that keeps it from becoming the bottleneck once the
  SMB connection pool lifts listing throughput ~4×. `commit_scan_tx` closes the transaction before EVERY exit, so marks
  and the final aggregate run in autocommit and a crash loses only the last interval (heals to a rescan). Reconcile
  already brackets via `BulkReconcileGuard`.
- **A backend may fan `list_directory_for_scan` out across an internal connection pool** (SMB opens extra TCP sessions;
  `backends/DETAILS.md` § "SMB scan-connection pool"). The walk is transport-agnostic and the in-flight budget still
  caps concurrency, so pacing survives.

Architecture, the concurrency pump, the pacing decision, the NAS-dir rationale, and empty-root handling: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
