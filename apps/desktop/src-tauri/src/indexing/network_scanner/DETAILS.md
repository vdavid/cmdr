# Network scanner (SMB/MTP) details

Read this before any non-trivial work in `network_scanner/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in [CLAUDE.md](CLAUDE.md).

This area owns the `Volume`-trait BFS scan/reconcile walk, its round-trip disciplines, the terminal-disconnect
partial-preserving finish, the consecutive-failure backstop, scan pacing, and the NAS system-dir skips. Points outward:
the registry / phase machine / freshness / gating / manual-rescan routing in
[`../lifecycle/DETAILS.md`](../lifecycle/DETAILS.md); the honest-sizes model + `dir_stats` ledger + the shared
`Arc<AtomicI64>` id counter in [`../writer/DETAILS.md`](../writer/DETAILS.md); the reconcile mode predicate, the shared
per-dir diff (`diff_dir_against_db`), the `BulkReconcileGuard`, and the completion-handler empty-root policy in
[`../reconcile/DETAILS.md`](../reconcile/DETAILS.md); mount-relative path spaces in
[`../paths/DETAILS.md`](../paths/DETAILS.md); SMB/MTP transport enable + live watch in
[`../transports/CLAUDE.md`](../transports/CLAUDE.md). The local guarded walker is a different scanner:
[`../scanner/DETAILS.md`](../scanner/DETAILS.md).

## The `Volume`-trait scan path

`scan_volume_via_trait(volume, root, writer, progress, cancelled)` is a BFS over `Volume::list_directory`, the same API
the live pane uses. BFS (not DFS) so a directory's id is registered in the `ScanContext` before its children are listed
(their parent lookup must hit). It produces `EntryRow`s into the EXACT downstream pipeline the local scan uses — its own
`ScanContext` for ids/parent-ids (scan root → `ROOT_ID`), the shared `Arc<AtomicI64>` counter, `InsertEntriesV2`
batches to the single writer, and `ComputeAllAggregates` + `WalCheckpoint` on clean completion. (The `ScanContext`
path→id map stays here on the serial network BFS; the parallel local walker dropped it in favor of the carried-`parent_id`
model.) Sizes come from `FileEntry.size` (SMB stat); since SMB has no separate physical size or inode, physical mirrors
logical and inode is `None`. Symlinks contribute no size (matching the local scanner's `du`-style omission).

Three disciplines for network round trips (all in `list_one_directory`):

- **Cancelable at every round trip**: the cancel flag is checked before each directory listing; a set flag flushes the
  current batch and returns `was_cancelled`.
- **Timeout-wrapped, but DETACHING**: each listing runs in its OWN task and `LIST_TIMEOUT` (120 s) races that task's
  JOIN HANDLE, so a wedged mount yields `VolumeScanError::Timeout` instead of parking forever. ❌ Never wrap the listing
  future directly: dropping the handle detaches the task, dropping the future cancels it mid-round-trip, and on MTP that
  abandons a PTP transaction and wedges the phone (`mtp/connection/CLAUDE.md`). A background MTP scan crosses 120 s
  routinely, since it parks at `background_yield_point` while the user is active.
- **`autoreleasepool`-drained per listing on macOS**: the SMB listing path touches NSURL/`NSString`-adjacent ObjC, and
  unpooled autoreleases leak multi-GB over a long walk (same rule as the writer thread). We can't hold an
  `autoreleasepool` guard across an `.await` (it isn't `Send`), so we drain AFTER the await resolves
  (`drain_autorelease_pool` wraps a no-op closure), not around it.

A sub-directory that fails to list (permission, transient) is skipped and the walk continues (like the local walker
skipping errored dirs); failing to list the ROOT is fatal (nothing to index) so the caller discards.

### Terminal disconnect keeps an honest partial; cancel discards

A mid-walk **disconnect** (the typed `DeviceDisconnected`/`Disconnected`, or the consecutive-failure backstop for a
disconnect-shaped untyped error) is TERMINAL: the walk stops immediately rather than churning the still-queued dirs into
silently-empty rows (the reported prod bug). Before returning the typed error, it runs the partial-preserving write
sequence (`finish_partial_scan`: flush + `MarkDirsListed` + `ComputeAllAggregates`) so the kept partial is
self-describing — scanned subtrees roll up to `min_subtree_epoch > 0` (exact, stale once the epoch is bumped), unscanned
ones stay `0` (`—`/`≥`). The completion handler (`lifecycle/manager.rs`) then keeps the instance + DB and marks the
volume Stale.

A **user cancel** still discards: `cancelled` returns `was_cancelled` with no marks/aggregate, and the completion
handler resets the volume to gray.

This scanner NEVER writes the `scan_completed_at` meta marker (on any path); the caller's completion handler does, only
on a clean finish — the same `scan_completed_at`-absent ⇒ no-Fresh / heal-to-rescan mechanism the local scanner relies
on.

### The consecutive-failure backstop (`CONSECUTIVE_FAILURE_ABORT`)

A single global counter (`usize`, `CONSECUTIVE_FAILURE_ABORT` = 32) over the serial BFS: consecutive failed listings
with no success in between abort the WHOLE walk as a disconnect-shaped terminal error (running `finish_partial_scan`
first). A single success resets it. Under the concurrency pump below, "consecutive" spans up to `FULL_LISTING_BUDGET`
in-flight failures rather than strictly one at a time — the same loose-consecutive caveat the local walker's per-subtree
give-up budget notes; the two are mirrored (not shared) counters: this one aborts the whole serial walk, the local one
prunes one parallel subtree.

### Bounded-concurrency walk (`FULL_LISTING_BUDGET`)

Both the fresh scan and the reconcile walk keep up to `FULL_LISTING_BUDGET` (64) `list_directory` round trips in flight
at once via a `FuturesUnordered` pump, instead of one-at-a-time. Directory listing is latency-bound — each dir is an
open+query+close round trip over an otherwise-idle link — so overlapping them is a near-linear speedup (one real
first-scan went from ~28 dirs/s serial to ~137 dirs/s and ~4,700 entries/s, ≈7–8× end to end). **Only the network I/O
overlaps**: results are processed serially on the walk task, so `ScanContext` id allocation (fresh) and the DB read
connection + diff (reconcile) stay single-owner with no locking, and the "a dir's id is registered before its children
are listed" invariant still holds — a child is enqueued only after its parent's result is processed.

**Decision/Why concurrency is safe for the data-integrity guarantees:** cancel drops the in-flight set (the smb2/MTP
backends tolerate a dropped request waiter); a typed terminal disconnect stops topping up and runs the
partial-preserving finish; the consecutive-failure backstop still trips on a real disconnect (failures pile up with no
successes to reset the counter). The reconcile path's new-dir id resolution flushes at a WAVE boundary (queue AND
in-flight both drained) rather than per BFS level. Pinned by `walk_lists_directories_concurrently` (proves
max-in-flight > 1, capped at `FULL_LISTING_BUDGET`) plus the disconnect/backstop tests (bounded stop, no full-queue
churn) and the reconcile-correctness suite (identical index vs a from-scratch scan).

**Decision/Why 64, and where the new ceiling is** (measured on a raidz1-of-4-HDDs QNAP, 64 GB RAM, ZFS, 2026-06-29):
past ~64 there's little to gain because the bottleneck moves off the network and onto the single SQLite **writer**. At
128 in-flight on a fresh scan the writer's queue spiked into the thousands during big-directory bursts (it processed
~24k messages in a 5 s window at ~98% busy, then drained to ~0), backpressuring the walk. The NAS itself was never the
limit: the HDDs sat ~10–18% busy (ZFS ARC served most directory metadata from RAM, so the platters barely moved — a
genuinely *cold* scan would lean harder on raidz1's ~150 random IOPS), CPU was ~idle, and SMB credits weren't observed
saturating. So `FULL_LISTING_BUDGET` is set where the concurrency win is essentially captured without piling work onto
the writer or a busy NAS.

### Two levers past 64 in-flight: connections, and the writer

`FULL_LISTING_BUDGET` stays 64 — but a later NAS-side probe (2026-07-22) showed the *cold* single-session plateau is
per-connection serialization in the server's ksmbd, not the disks, and that spreading the SAME 64 in-flight listings
over several TCP connections lifts cold throughput ~3.8×. That's a BACKEND concern, not a scanner one: the SMB backend
opens a small pool of extra sessions per scan and `list_directory_for_scan` fans out across them, invisibly to this walk
(the global budget still caps total concurrency). Canonical: `file_system/.../backends/DETAILS.md` § "SMB
scan-connection pool"; evidence: `smb2/docs/benchmark-findings.md`.

At ~4× listing throughput the single writer's per-second insert rate rises the same, so the FRESH scan
(`scan_volume_via_trait`) now wraps its `InsertEntriesV2` stream in ONE explicit transaction committed on an interval
(`SCAN_COMMIT_INTERVAL`, 2 s) via `begin_scan_tx` / `commit_scan_tx`. `insert_entries_v2_batch` already savepoints each
batch, so in autocommit every batch was an fsync; the outer transaction amortizes fsync to once per interval.
`commit_scan_tx` (idempotent) closes the transaction before EVERY exit — clean finish, cancel, root-fatal, empty-root,
disconnect, consecutive-failure — so the connection never returns mid-transaction and `finish_partial_scan`'s marks +
`ComputeAllAggregates` run in autocommit exactly as before (marks still precede the aggregate). **Crash-safety:** an
uncommitted transaction rolls back on process death → the partial is lost → next launch heals to a rescan (identical to
today's `scan_completed_at`-absent behavior); marks/aggregate are still sent AFTER the inserts commit, so a crash never
leaves ancestors claiming exact sizes over an unstamped descendant. Reconcile is untouched — it already brackets its
bulk writes via `BulkReconcileGuard`. The remaining lever is fewer round trips per huge directory (a larger
`QueryDirectory` buffer in smb2), NOT more in-flight listings.

## Yielding to navigation (`scan_pace.rs`)

The walk's listing budget isn't a constant: at every top-up it asks `ScanPacer::listing_budget()`, which returns
`FULL_LISTING_BUDGET` (64) while the share is quiet and `YIELDING_LISTING_BUDGET` (1) while the user is browsing it.
Both the fresh scan and the reconcile walk read it. **Why it exists:** a scan and the pane's own listings share ONE SMB
session (every `SmbVolume` clone multiplexes frames over the same connection), so 64 in-flight listings bury a
navigation behind the backlog — a 40-entry folder took **10.7 s** to open mid-scan on a real QNAP (`/Volumes/naspi`,
~2M entries, 2026-07-19) and was instant the second the scan finished. That's also the first impression the app makes on
someone who connects a NAS and enables indexing because it sounds good.

**The signal** is `media_index::foreground`'s per-volume timestamp, stamped by the listing IPC
(`note_foreground_activity_on`) on every navigation. A share counts as in use for `SCAN_FOREGROUND_IDLE_THRESHOLD` (2 s)
after the last one — long enough to span the gaps in real browsing so a session of clicking around is ONE throttled
stretch, short enough to be back at full speed a couple of seconds after the user stops. There's no separate debounce:
the window IS the debounce.

**Decision/Why throttle instead of park, and why no anti-starvation floor.** The obvious gate ("only scan while idle")
converts "indexing is in the way" into "indexing never finishes", and then needs a quota, a minimum-progress floor, or a
consecutive-yield cap to climb back out — all state that can be reset wrong, leak, or wedge. A budget that bottoms out
at ONE listing instead of zero makes forward progress **structural**: browse the share non-stop for an hour and the scan
spends that hour at one listing at a time and still completes. Nothing to expire, nothing to re-arm. The cost is that a
throttled scan is roughly an order of magnitude slower, which is the correct trade for background work with no deadline.
❌ Don't "improve" this by letting the yielding budget reach 0.

**What the user feels.** In-flight listings are never cancelled (that would throw away a completed round trip), so the
yield takes effect within one drain of the current backlog: the navigation that TRIGGERS the throttle still waits out up
to 64 in-flight listings, and every one after it queues behind at most one. If that first hop ever measures badly on
real hardware, the lever is a lower `FULL_LISTING_BUDGET`, not cancelling in-flight work.

**Decision/Why the scope is per volume, not app-wide.** The contention is one share's SMB session, so browsing a LOCAL
folder is no reason to slow a NAS scan — the app-wide signal would throttle it for activity that isn't competing at all.
Media enrichment keeps reading the app-wide signal, because it's heavy on-device ML where any foreground work is reason
enough to wait; `media_index/foreground.rs` documents the two scopes side by side. A volume nobody has browsed has no
entry and reads as idle, so a first scan starts at full speed. ❌ Don't collapse a missing entry to a `0` timestamp: `0`
is a real point on that clock, so "never browsed" would read as "browsed at startup" and throttle every scan for the
app's first two seconds.

Pinned by `browsing_the_share_throttles_the_scan_to_one_listing_in_flight`,
`a_continuously_browsed_share_still_finishes_its_scan` (the anti-starvation guarantee, end to end),
`browsing_a_different_volume_does_not_throttle_the_scan` (the scope decision), and the pure-decision tests in
`pace_tests.rs` (including `the_budget_is_never_zero_for_any_input`). The transfer side of the same problem lives in
`file_system/volume/backends/smb/foreground_yield.rs`.

## NAS snapshot/system dirs aren't recursed (`system_dirs.rs`)

The BFS does NOT descend into NAS snapshot/system pseudo-directories (`@eaDir`, `@Recently-Snapshot`, `@Recycle`,
`#recycle`, `#snapshot`, `.snapshot`, `$RECYCLE.BIN`, `System Volume Information`, …; matched case-insensitively by
`system_dirs::is_recursion_excluded_dir`). Both the fresh scan and the reconcile walk apply it: the dir's own row is
still indexed (so it stays listed and navigable — a user can walk into `@Recycle` to restore a file), but its subtree is
never walked, so it rolls up as honestly-unknown (`—`/`≥`) rather than a misleading total. **Decision/Why:** these dirs
are hardlinked, huge, and re-walking them costs a full filesystem traversal *per snapshot* over serialized SMB — a real
first-scan stalled near 50% grinding `@Recently-Snapshot`, which alone reported 44 TB on a 10 TB volume. Summing them is
both ruinous and wrong (the bytes are deduped, not real consumed space). The names are reserved vendor conventions
(`@`/`#`/`$` prefixes) that don't collide with user folders, so a name match is safe. **Guardrail:** don't remove the
exclusion to "fill in" the missing sizes — that re-triggers the stall. Scope is the network scanner only (the home of
these dirs); the local walker has its own `should_exclude` ([`../scanner/DETAILS.md`](../scanner/DETAILS.md)).
`FileEntry` carries no DOS hidden/system attribute today; if one is plumbed through, "hidden + system" would generalize
this without the hardcoded list.

## Empty root

The two network walkers (`scan_volume_via_trait`, `reconcile_volume_via_trait`) return the typed
`VolumeScanError::EmptyRoot` when the ROOT listing yields ZERO children, so the completion handler takes its `Err` arm
and writes NO `scan_completed_at`. A false "complete" over a transiently-empty root permanently strands the index
(startup loads Stale and never rescans; a manual rescan re-"completes" the same empty root). The full completion-handler
policy — empty (`EmptyRoot`) vs failed (`Volume`/`Io`) root, why both reconcile paths bail BEFORE diffing the root, and
the accepted genuinely-empty-volume false-negative — is canonical in
[`../reconcile/DETAILS.md`](../reconcile/DETAILS.md) § No completion marker on an empty root.

## Reconcile

`reconcile_volume_via_trait` is the rescan-in-place BFS: it keeps every `scan_volume_via_trait` round-trip discipline
(cancelable, `LIST_TIMEOUT`-wrapped, `autoreleasepool`-drained, the typed terminal-disconnect branch, the
consecutive-failure backstop) but diffs each dir against the DB via the shared `diff_dir_against_db` instead of
inserting fresh, so the last-good index stays visible-stale throughout. The mode predicate (reconcile vs truncate), the
shared per-dir diff, the `BulkReconcileGuard` delta-propagation suppression, and the finish (`MarkDirsListed` → one
`ComputeAllAggregates`) are all canonical in [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md).
