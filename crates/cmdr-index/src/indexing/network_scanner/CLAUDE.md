# Network scanner (SMB/MTP)

The `Volume`-trait BFS scanner for SMB and MTP shares, over the SAME `Volume::list_directory` the live pane uses.
Everything downstream of `EntryRow` is reused unchanged; only discovery and stat'ing differ from the local guarded
walker, and no walk here names a backend.

`mod.rs` holds `VolumeScanError` plus the round-trip disciplines all three walks share; `full_scan.rs` the fresh BFS,
`reconcile_scan.rs` the same walk diffing against the DB, `cover_scan.rs` the SCOPED search-driven walk; `scan_pace.rs`
the per-volume paced listing budget, `system_dirs.rs` the non-recursed NAS dirs and the exclusion-list stamp.

## Must-knows

- **BFS, not DFS.** A directory's id must be known before its children are listed, so the concurrency pump processes
  results serially.
- **Never wrap a round trip's future in the timeout — race its JOIN HANDLE** (`LIST_TIMEOUT`). Dropping the future
  cancels it mid-round-trip, and on MTP that abandons a PTP transaction and wedges the phone.
- **Terminal disconnect keeps an honest partial; user cancel discards.** A typed disconnect (or the
  `CONSECUTIVE_FAILURE_ABORT` backstop) runs `finish_partial_scan`, so scanned subtrees roll up exact-stale, unscanned
  ones stay `0`, and the DB is kept. A user cancel writes no marks or aggregate.
- **The COVER walk inverts that cancel rule, is SCOPED, and is ADD-ONLY**: it stamps what it read on EVERY exit (a
  search has to converge), roots at a frontier node's own id, and keeps whatever name the index already holds. ❌ It
  needs neither the virgin-root nor the empty-root refusal. `DETAILS.md` § "The scoped cover walk".
- **This scanner NEVER writes `scan_completed_at`**; the completion handler does, on a clean finish only and never on an
  empty root: a false "complete" permanently strands the index.
- ⚠️ **KNOWN BUG: a failed listing gets no `unreadable_cause`**, so every later search re-pays it (the local walker's
  fixed twin). ❌ Don't port `Abandoned` mechanically: a whole-share disconnect reaches the same arm and would condemn
  thousands of dirs. `DETAILS.md` § "A failed listing leaves no cause".
- **The listing budget is PACED per volume, not constant** (`scan_pace.rs`, all three walks): browsing the share or a
  transfer on it drops it 64 → 1, so higher-priority work isn't queued behind the walk. ❌ Never let it reach 0 —
  one-at-a-time is what makes forward progress structural. Signals arrive once per top-up, ❌ never per entry.
- **NAS system/snapshot dirs aren't recursed** (`system_dirs.rs`, all three walks): the dir's own row IS indexed, its
  subtree never walked. ❌ Don't remove it to "fill in" sizes — it re-triggers the stall. The cover walk stamps them
  `unreadable_cause = Declined`, ❌ never `Denied`, or the frontier hands that tree to every search.
- **Adding a name REBUILDS every network index**, and a false positive costs a user their indexed folder. ❌ No name
  without a vendor citation, ❌ stamp only right after a `TruncateData`, ❌ never migrate.
- **The fresh and cover walks batch inserts into periodic explicit transactions** (`SCAN_COMMIT_INTERVAL`), which is
  what keeps the writer off the critical path under the SMB pool's ~4× throughput. `commit_scan_tx` closes it before
  EVERY exit; reconcile brackets via `BulkReconcileGuard`.
- **A backend may fan `list_directory_for_scan` across an internal connection pool** (SMB's extra TCP sessions, opened
  by the `begin`/`end_scan_session` bracket). The in-flight budget still caps concurrency, so pacing survives.

Architecture, the concurrency pump, the scoped cover walk, the pacing decision, the NAS-dir rationale, and empty-root
handling: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
