# Multi-connection SMB index scanning

Speed up the cold SMB index scan by spreading its directory listings across several TCP connections, and loosen the
single SQLite writer so it keeps up with the higher ingest. Two milestones; M1 is the value, M2 removes the writer as
the next bottleneck.

## Why (measured, not re-derived here)

A two-sided probe (client + SSH into the QNAP) proved the cold NAS scan (~39 dirs/s) is bottlenecked on **per-connection
serialization in the NAS's ksmbd**, not the raidz1 disks (`disk_wait` 2.6–4.5 ms, CPU 15–25%, real IOPS headroom).
Spreading the same 64 total in-flight listings over 4 TCP connections lifted cold throughput ~3.8× (7.4–9.7 → 37 dirs/s)
at flat disk latency; warm scaling is near-linear too. Full data:
`~/projects-git/vdavid/smb2/docs/benchmark-findings.md` §§ dated 2026-07-22 ("Directory-listing throughput probe" and
"NAS-side ground truth"). This doc does not restate the numbers; it links.

## The seam (agreed shape)

- **The scanner stays transport-agnostic and unchanged in its walk/budget logic** (`indexing/network_scanner`): BFS
  queue + `FuturesUnordered`, budget from `scan_pace.rs` (64 full / 1 yielding). The global budget already caps total
  in-flight listings across the pool, so the pacer's "drop to 1 while the user navigates" survives automatically. Pacing
  does NOT move into the pool.
- **A scan-connection pool lives inside `SmbVolume`** (`file_system/volume/backends/smb`), invisible to the scanner. It
  opens lazily when a scan starts, closes when the scan ends. `list_directory_for_scan` draws from the pool
  (round-robin); the pane's existing session keeps serving browsing.
- **smb2 needs no changes.** A pool member is a full `SmbClient` + `Tree` from the existing `build_session` (a SEPARATE
  TCP session). `Connection::clone` only multiplexes over one session, so separate connections are separate
  `SmbClient`s.

## M1 — the scan-connection pool

### New trait seam (`Volume`)

Two default-no-op async methods, so every backend but SMB is unaffected:

- `begin_scan_session(&self)`: a backend opens scan-scoped resources.
- `end_scan_session(&self)`: tear them down.

Called by the lifecycle (`indexing/lifecycle/network_scan.rs::start_volume_scan`), wrapping the spawned walk task:
`begin` before the walk, `end` right after it returns (all completion arms run after, so `end` always fires). MTP and
local keep the default (MTP's single USB pipe can't parallelize; local doesn't reach this path).

### `SmbVolume` pool (`smb/scan_pool.rs`, new)

- Field: `scan_pool: Arc<tokio::sync::RwLock<Option<Arc<ScanPool>>>>`. Hot path read-locks, clones the `Arc`, drops the
  guard.
- `ScanPool { members: Vec<Arc<PoolMember>>, slots: PoolSlots, params, volume_id, closed: AtomicBool }`.
- `PoolMember { session: tokio::sync::Mutex<Option<MemberSession>>, ... }`,
  `MemberSession { client: SmbClient, tree: Arc<Tree> }`. Mutex (not RwLock) because cloning the `Connection` needs
  `client.connection_mut()` (`&mut`); each member has its own Mutex, so different members list truly in parallel.
- `PoolSlots` (pure, unit-testable without a live server): a round-robin `next: AtomicUsize`, a per-slot `alive` flag,
  and a per-slot `reconnecting` single-flight flag. Owns `next_alive()`, `mark_dead(idx)`, `mark_alive(idx)`,
  `try_begin_reconnect(idx)`. The session `Option` is authoritative; `alive` only guides selection.
- Constant `SCAN_POOL_SIZE = 4` (the benchmarked config; tunable). `begin_scan_session` opens up to 4 members
  concurrently but STAGGERED (`POOL_LOGIN_STAGGER`) so 4 session-setups don't hit the server at once; a rejected member
  N is logged and the pool runs with fewer. Only when Direct and not unmounted.

### `list_directory_for_scan` (SMB override)

1. If a pool is active: up to `members.len()` attempts — `acquire()` a live member (try_lock; skip contended/dead,
   kicking its reconnect), drive `tree.list_directory` on the cloned `Connection`. On success, map + return. On a typed
   `ConnectionLost`/`SessionExpired`, `mark_dead` + reconnect that member and RETRY on a sibling (does not surface as a
   disconnect). On any other error (permission, not-found), return it (a real per-dir error, same on any connection).
2. If the pool is exhausted (all members momentarily dead), fall back to the main session (`list_directory_impl`). This
   keeps the scan progressing and, if the main session is also dead, yields the genuine `DeviceDisconnected` the
   scanner's terminal path expects.

### Failure matrix

- **Dead member mid-listing**: retry on a sibling; mark dead; single-flight background reconnect (`build_session`,
  bounded growing backoff mirroring `reconnect.rs::WATCHER_DEATH_RECONNECT_BACKOFF`; give up on auth like the watcher
  loop). The walk never aborts for one dead member.
- **Auth rejection opening member N** (begin, or a member's reconnect): log, run with fewer; listings fall back to the
  main session, which owns the credential-refresh / `needs_auth` dance. Pool params are a cached snapshot; a
  password-change mid-scan degrades the pool to the main session (documented, acceptable).
- **Cancel mid-scan**: the walk returns `was_cancelled`; `end_scan_session` tears the pool down (drops members → closes
  TCP, sets `closed` so background reconnects stop). Idempotent.
- **NAS reboot / whole-volume disconnect**: every member dies; retries exhaust; fallback to the main session fails →
  `DeviceDisconnected` → the scanner's terminal-disconnect path keeps the honest partial. `on_unmount` also tears the
  pool down (a live member must not keep walking an unmounted volume).

### Tests

- Pure `PoolSlots` unit tests (no server): round-robin over live members, `mark_dead` removes a slot, `next_alive`
  returns `None` when all dead, `try_begin_reconnect` is single-flight, `mark_alive` re-adds.
- `#[ignore]` Docker-SMB integration tests (mirror `smb_integration_test`): a `begin_scan_session` scan lists correctly
  across the pool; killing a member mid-scan still completes; `end_scan_session` closes members.
- The existing `network_scanner` integration tests keep passing unchanged (they call `scan_volume_via_trait` directly
  without `begin_scan_session`, so they exercise the main-session fallback — proving the pool is a transparent
  accelerator).

## M2 — loosen the writer for ~4× ingest

The June bench (`indexing/writer/DETAILS.md` / `network_scanner/DETAILS.md` § "Bounded-concurrency walk") showed the
single writer becomes the bottleneck when in-flight rises; at 4× wall-clock ingest the fresh scan's per-second insert
rate rises the same. `insert_entries_v2_batch` already wraps each batch in a savepoint, so in autocommit each
`InsertEntriesV2` message costs one fsync.

Shape (low-risk, reuses existing tested machinery):

- **Wrap the fresh network scan's insert stream in periodic explicit transactions.** `scan_volume_via_trait` sends
  `BeginTransaction` before the loop and `CommitTransaction` + re-`BeginTransaction` every ~2 s (or every K flushes), so
  fsync happens once per window instead of once per 2000-entry batch. Reuses the replay path's `BeginTransaction` /
  `CommitTransaction` and the writer's `is_autocommit`-gated deferred-checkpoint / repair-drain logic.
- **Close the transaction on EVERY exit** (clean finish, cancel, empty-root, disconnect) BEFORE marks + aggregate, so
  the connection never returns mid-transaction and `finish_partial_scan`'s marks/aggregate run in autocommit exactly as
  today.
- Optionally enlarge the network `BATCH_SIZE` (fewer, larger multi-row inserts).

**Crash-safety.** An uncommitted transaction rolls back on process death → the partial is lost → next launch heals to a
rescan (identical to today's `scan_completed_at`-absent behavior). Marks/aggregate are still sent AFTER the inserts are
committed, so a crash never leaves ancestors claiming exact sizes over an unstamped descendant (same window as today).
`MarkDirsListed` before `ComputeAllAggregates` ordering is untouched. If any invariant here can't be met cleanly, land
only the batch-size bump and report the transaction-batching design for review (per the lead's crash-safety caution).

TDD the exit-always-commits property and the "marks land after committed inserts" ordering.

## Docs to update (per `.claude/rules/docs.md`)

- `smb/CLAUDE.md` + `smb/DETAILS.md` (new): the scan pool exists, why 4, lazy open/close, member reconnect, main-session
  fallback, pacer interaction (pool respects the global budget, doesn't own pacing).
- `network_scanner/CLAUDE.md` + `DETAILS.md`: `list_directory_for_scan` may fan out across a backend pool; the writer
  cadence (periodic transactions) if M2 lands.
- `volume/CLAUDE.md` + `DETAILS.md`: the `begin_scan_session` / `end_scan_session` trait seam.
- Evidence stays in the smb2 findings doc; link, don't restate.

## Verification

- `pnpm check` (scoped while iterating, wider before wrapping).
- Live real-NAS before/after is the authoritative throughput proof but needs David's `/Volumes/naspi`; the read-only
  smb2 probe already measured the ceiling. Leave the real-NAS before/after to David unless a cheap read-only
  reproduction is available in-session.
