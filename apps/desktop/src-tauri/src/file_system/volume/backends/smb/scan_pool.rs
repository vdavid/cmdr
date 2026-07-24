//! The per-scan connection pool: a small set of EXTRA smb2 sessions (separate
//! TCP connections) that a background index scan spreads its directory listings
//! across, so the cold walk isn't serialized on the one session the pane browses
//! through.
//!
//! # Why a pool at all
//!
//! A cold NAS index scan is metadata-read-bound but the ceiling is
//! **per-connection serialization in the server's ksmbd**, not the disks: a
//! single SMB connection can't drive the server's read queue deep enough,
//! regardless of the SMB in-flight window. Spreading the SAME total in-flight
//! listings over several TCP connections lifts cold throughput ~3.8x at flat
//! disk latency (measured NAS-side; see `smb2/docs/benchmark-findings.md`
//! 2026-07-22). So a scan opens [`SCAN_POOL_SIZE`] extra sessions for its
//! duration; the pane's own session keeps serving browsing.
//!
//! # Shape
//!
//! - Opened LAZILY by [`SmbVolume::open_scan_pool`] when a scan starts
//!   ([`Volume::begin_scan_session`](crate::file_system::volume::Volume::begin_scan_session)),
//!   closed by [`SmbVolume::close_scan_pool`] when it ends. Steady-state
//!   footprint between scans is unchanged.
//! - The scanner is unchanged and transport-agnostic: it keeps calling
//!   `list_directory_for_scan`, which draws from the pool (round-robin) when one
//!   is active and falls back to the main session otherwise. Pacing stays in the
//!   scanner (`network_scanner/scan_pace.rs`): the global in-flight budget caps
//!   the pool's total concurrency, so "drop to 1 while the user browses" survives
//!   for free. The pool never owns pacing.
//! - A pool member is a full smb2 `SmbClient` + `Tree` from the same
//!   [`build_session`] the main path uses; `Connection::clone` only multiplexes
//!   over ONE session, so separate connections mean separate `SmbClient`s.
//!
//! # Failure handling (the real work)
//!
//! A member dying mid-scan must NOT abort the walk. A listing that fails with a
//! typed `ConnectionLost`/`SessionExpired` is retried on a sibling member, the
//! dead member is dropped, and a single-flight background task reconnects it
//! (bounded backoff, gives up on auth — the main session owns the credential
//! dance). If every member is momentarily dead the listing falls back to the
//! main session, which keeps the scan progressing and, if it too is dead, yields
//! the `DeviceDisconnected` the scanner's terminal path expects. A per-directory
//! error (permission, not-found) is the same on any connection, so it's surfaced
//! immediately, never retried.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::*;

/// How many EXTRA sessions a scan opens. Four is the benchmarked sweet spot: the
/// NAS-side probe (2026-07-22) held total in-flight depth constant and varied
/// only the connection count, and 4 connections extracted the pool's real read
/// IOPS at flat latency (~3.8x cold throughput). Tunable; more connections buy
/// little past this and are less gentle on a NAS also serving other load.
pub(super) const SCAN_POOL_SIZE: usize = 4;

/// Gap between the staggered member logins at pool open, so we don't hit the
/// server with `SCAN_POOL_SIZE` simultaneous session setups.
const POOL_LOGIN_STAGGER: Duration = Duration::from_millis(75);

/// Bounded, growing backoff for reconnecting a dead pool member. Shorter than the
/// main session's watcher-death schedule: a pool member is a best-effort
/// accelerator, the scan is finite, and a member that stays down just means the
/// pool runs with fewer while listings route to the survivors and the main
/// session. Gives up after the last entry.
const POOL_MEMBER_RECONNECT_BACKOFF: [Duration; 3] =
    [Duration::from_secs(1), Duration::from_secs(3), Duration::from_secs(10)];

/// One extra smb2 session dedicated to scan listings.
struct MemberSession {
    /// Owns the `Connection`. Cloning the `Connection` needs `&mut` (hence the
    /// per-member `Mutex` around this), then all clones multiplex over this
    /// member's TCP session.
    client: SmbClient,
    tree: Arc<Tree>,
}

/// A pool slot: its session (or `None` while dead/reconnecting) plus the
/// single-flight reconnect guard. Its OWN `Mutex` so cloning the `Connection` on
/// one member never blocks a listing on another; the lock is only ever held
/// briefly (clone or install), never across a `build_session`.
struct PoolMember {
    session: tokio::sync::Mutex<Option<MemberSession>>,
}

/// The pure member-selection bookkeeping, decoupled from the real sessions so the
/// round-robin / mark-dead / single-flight-reconnect logic is unit-testable
/// without a live server. `alive[i]` is a hint that slot `i` has an installed
/// session (the session `Option` is the authority; this only guides selection).
pub(super) struct PoolSlots {
    alive: Vec<AtomicBool>,
    reconnecting: Vec<AtomicBool>,
    /// Round-robin cursor, advanced on every selection.
    next: AtomicUsize,
}

impl PoolSlots {
    fn new(n: usize) -> Self {
        Self {
            alive: (0..n).map(|_| AtomicBool::new(false)).collect(),
            reconnecting: (0..n).map(|_| AtomicBool::new(false)).collect(),
            next: AtomicUsize::new(0),
        }
    }

    fn len(&self) -> usize {
        self.alive.len()
    }

    /// The next live slot in round-robin order, or `None` if every slot is dead.
    /// Advances the cursor once per probe so concurrent callers spread across
    /// members rather than all hammering one.
    pub(super) fn next_alive(&self) -> Option<usize> {
        let n = self.alive.len();
        if n == 0 {
            return None;
        }
        for _ in 0..n {
            let idx = self.next.fetch_add(1, Ordering::Relaxed) % n;
            if self.alive[idx].load(Ordering::Relaxed) {
                return Some(idx);
            }
        }
        None
    }

    pub(super) fn mark_alive(&self, idx: usize) {
        self.alive[idx].store(true, Ordering::Relaxed);
    }

    pub(super) fn mark_dead(&self, idx: usize) {
        self.alive[idx].store(false, Ordering::Relaxed);
    }

    pub(super) fn any_alive(&self) -> bool {
        self.alive.iter().any(|a| a.load(Ordering::Relaxed))
    }

    /// Claim the (single) reconnect for slot `idx`. Returns `true` if the caller
    /// now owns it, `false` if a reconnect is already in flight for this slot.
    pub(super) fn try_begin_reconnect(&self, idx: usize) -> bool {
        self.reconnecting[idx]
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(super) fn end_reconnect(&self, idx: usize) {
        self.reconnecting[idx].store(false, Ordering::Release);
    }
}

/// A pool of extra scan sessions for one `SmbVolume`, live only for the duration
/// of a scan.
pub(super) struct ScanPool {
    members: Vec<Arc<PoolMember>>,
    slots: PoolSlots,
    /// Connection params snapshot used to (re)build members. A snapshot: if the
    /// main session refreshes credentials mid-scan (password change), members
    /// failing auth just give up and listings fall back to the main session.
    params: SmbConnectionParams,
    volume_id: String,
    /// Set once at teardown; stops in-flight member-reconnect loops from
    /// installing sessions into a pool that's going away.
    closed: AtomicBool,
}

impl ScanPool {
    /// Build the pool from per-slot sessions (`Some` = opened, `None` = failed to
    /// open). Kicks a background reconnect for any slot that started dead so a
    /// transient open failure self-heals.
    fn from_slots(
        slots_sessions: Vec<Option<MemberSession>>,
        params: SmbConnectionParams,
        volume_id: String,
    ) -> Arc<Self> {
        let n = slots_sessions.len();
        let slots = PoolSlots::new(n);
        let mut members = Vec::with_capacity(n);
        let mut dead = Vec::new();
        for (idx, sess) in slots_sessions.into_iter().enumerate() {
            if sess.is_some() {
                slots.mark_alive(idx);
            } else {
                dead.push(idx);
            }
            members.push(Arc::new(PoolMember {
                session: tokio::sync::Mutex::new(sess),
            }));
        }
        let pool = Arc::new(Self {
            members,
            slots,
            params,
            volume_id,
            closed: AtomicBool::new(false),
        });
        for idx in dead {
            pool.spawn_reconnect(idx);
        }
        pool
    }

    fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Pick a live member and clone out `(index, tree, connection)` to drive a
    /// listing on, without holding any lock across the listing. `None` when no
    /// member is currently alive (the caller falls back to the main session).
    async fn acquire(self: &Arc<Self>) -> Option<(usize, Arc<Tree>, smb2::client::Connection)> {
        for _ in 0..self.slots.len() {
            let idx = self.slots.next_alive()?;
            // Held only long enough to clone the Connection (microseconds); a
            // reconnect builds its session OUTSIDE this lock and takes it only to
            // install, so this never waits on a `build_session`.
            let mut guard = self.members[idx].session.lock().await;
            match guard.as_mut() {
                Some(sess) => {
                    let conn = sess.client.connection_mut().clone();
                    let tree = Arc::clone(&sess.tree);
                    return Some((idx, tree, conn));
                }
                None => {
                    // The alive hint was stale (a concurrent death). Correct it
                    // and make sure a reconnect is running, then try another.
                    drop(guard);
                    self.slots.mark_dead(idx);
                    self.spawn_reconnect(idx);
                }
            }
        }
        None
    }

    /// A listing on member `idx` failed with a connection-lost/expired error: drop
    /// the session and kick a single-flight reconnect. Does NOT touch the main
    /// volume's connection state — a dead pool member says nothing about the pane's
    /// session.
    fn mark_member_dead(self: &Arc<Self>, idx: usize) {
        self.slots.mark_dead(idx);
        {
            // Clear the slot so acquire() skips it; a best-effort try_lock keeps
            // this off the await path (a concurrent clone releases in microseconds,
            // and the reconnect below reinstalls regardless).
            if let Ok(mut guard) = self.members[idx].session.try_lock() {
                *guard = None;
            }
        }
        self.spawn_reconnect(idx);
    }

    /// Spawn a single-flight background reconnect for member `idx`.
    fn spawn_reconnect(self: &Arc<Self>, idx: usize) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if !self.slots.try_begin_reconnect(idx) {
            return; // one already running
        }
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            pool.reconnect_member(idx).await;
            pool.slots.end_reconnect(idx);
        });
    }

    async fn reconnect_member(&self, idx: usize) {
        for delay in POOL_MEMBER_RECONNECT_BACKOFF {
            if self.closed.load(Ordering::Relaxed) {
                return;
            }
            match build_session(&self.params).await {
                Ok((client, tree)) => {
                    let mut guard = self.members[idx].session.lock().await;
                    if self.closed.load(Ordering::Relaxed) {
                        return; // pool torn down mid-build: drop the fresh session
                    }
                    *guard = Some(MemberSession {
                        client,
                        tree: Arc::new(tree),
                    });
                    drop(guard);
                    self.slots.mark_alive(idx);
                    log::debug!("smb scan pool: member {idx} for '{}' reconnected", self.volume_id);
                    return;
                }
                Err(e) if crate::network::smb_util::is_auth_error(&e) => {
                    // The main session owns the credential-refresh / needs_auth
                    // flow. A pool member is an accelerator: give up quietly and let
                    // listings fall back to the main session.
                    log::debug!(
                        "smb scan pool: member {idx} for '{}' rejected auth ({e}); leaving it out of the pool",
                        self.volume_id
                    );
                    return;
                }
                Err(e) => {
                    log::debug!(
                        "smb scan pool: member {idx} for '{}' reconnect failed ({e}); backing off",
                        self.volume_id
                    );
                }
            }
            tokio::time::sleep(delay).await;
        }
        log::debug!(
            "smb scan pool: member {idx} for '{}' still down after backoff; pool runs with fewer",
            self.volume_id
        );
    }

    /// Stop reconnect loops (sync): flip `closed` so any in-flight or future
    /// reconnect bails. Sessions close when the last `Arc<ScanPool>` (this one plus
    /// any reconnect task holding a clone) drops.
    fn mark_closed(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }

    /// Tear the pool down: stop reconnects and drop every member session (each
    /// `SmbClient` drop closes its TCP connection).
    async fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        for member in &self.members {
            let mut guard = member.session.lock().await;
            *guard = None;
        }
    }
}

/// Open up to `n` extra sessions, staggered, returning one slot per index
/// (`Some` opened, `None` failed). Runs the logins concurrently but delayed so
/// the server doesn't see `n` simultaneous session setups.
async fn open_slots(params: &SmbConnectionParams, n: usize, volume_id: &str) -> Vec<Option<MemberSession>> {
    let mut futs = FuturesUnordered::new();
    for i in 0..n {
        let params = params.clone();
        futs.push(async move {
            tokio::time::sleep(POOL_LOGIN_STAGGER * i as u32).await;
            (i, build_session(&params).await)
        });
    }
    let mut slots: Vec<Option<MemberSession>> = (0..n).map(|_| None).collect();
    while let Some((i, result)) = futs.next().await {
        match result {
            Ok((client, tree)) => {
                slots[i] = Some(MemberSession {
                    client,
                    tree: Arc::new(tree),
                });
            }
            Err(e) => {
                // A rejected Nth session (server session cap) is not fatal: run with
                // fewer. The dead slot's reconnect (kicked in from_slots) retries.
                log::debug!("smb scan pool: extra session {i} for '{volume_id}' failed to open ({e})");
            }
        }
    }
    slots
}

/// Whether an smb2 listing error means THIS pool member's session is gone, so the
/// listing should retry on a sibling and the member should reconnect. Classified
/// by the TYPED kind, never a message substring (`.claude/rules/no-string-matching.md`).
fn is_pool_member_dead(err: &smb2::Error) -> bool {
    matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired
    )
}

impl SmbVolume {
    /// Open the scan-connection pool for the duration of a scan (idempotent).
    /// No-op if disconnected, unmounted, or already open. Installs the pool even
    /// when only some members opened; installs nothing when none did, so listings
    /// fall straight through to the main session.
    pub(super) async fn open_scan_pool(&self) {
        if self.unmounted.load(Ordering::Relaxed) || self.connection_state() != ConnectionState::Direct {
            return;
        }
        if self.scan_pool.read().await.is_some() {
            return;
        }
        let params = self.params.read().await.clone();
        let volume_id = self.volume_id.clone();
        let slots = open_slots(&params, SCAN_POOL_SIZE, &volume_id).await;
        let live = slots.iter().filter(|s| s.is_some()).count();
        if live == 0 {
            log::warn!(
                "smb scan pool: no extra connections opened for '{volume_id}'; scan runs on the main session only"
            );
            return;
        }
        // Re-check unmount: a slow set of logins gives on_unmount a window to run.
        if self.unmounted.load(Ordering::Relaxed) {
            return; // drop `slots` → closes the freshly-opened sessions
        }
        let pool = ScanPool::from_slots(slots, params, volume_id.clone());
        *self.scan_pool.write().await = Some(pool);
        log::info!("smb scan pool: opened {live}/{SCAN_POOL_SIZE} extra connections for '{volume_id}'");
    }

    /// Close the scan-connection pool (idempotent). Called when a scan ends and
    /// from `on_unmount`.
    pub(super) async fn close_scan_pool(&self) {
        let pool = { self.scan_pool.write().await.take() };
        if let Some(pool) = pool {
            pool.close().await;
            log::debug!("smb scan pool: closed for '{}'", self.volume_id);
        }
    }

    /// Sync teardown for `on_unmount` (no async runtime): flip the pool's `closed`
    /// flag so reconnect loops bail, and drop this reference. Sessions close when
    /// the last `Arc` (this plus any sleeping reconnect task) drops, within one
    /// backoff step.
    pub(super) fn close_scan_pool_sync(&self) {
        if let Some(pool) = self.scan_pool.blocking_write().take() {
            pool.mark_closed();
        }
    }

    /// The `list_directory_for_scan` body: draw from the scan pool when one is
    /// active, else the main session. Retries a connection-lost listing on a
    /// sibling member; surfaces a genuine per-directory error unchanged.
    pub(super) async fn list_directory_for_scan_impl(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let pool = { self.scan_pool.read().await.clone() };
        if let Some(pool) = pool {
            let smb_path = self.to_smb_path(path);
            let display_path = self.to_display_path(&smb_path);
            // At most one attempt per member: a sibling retry after each dead
            // member, bailing to the main-session fallback once all are exhausted.
            for _ in 0..pool.member_count() {
                let Some((idx, tree, mut conn)) = pool.acquire().await else {
                    break;
                };
                match tree.list_directory(&mut conn, &smb_path).await {
                    Ok(raw) => {
                        let entries = raw
                            .iter()
                            .filter(|e| e.name != "." && e.name != "..")
                            .map(|e| directory_entry_to_file_entry(e, &display_path))
                            .collect();
                        return Ok(entries);
                    }
                    Err(e) if is_pool_member_dead(&e) => {
                        log::debug!(
                            "smb scan pool: member {idx} died listing {smb_path:?} ({e}); retrying on a sibling"
                        );
                        pool.mark_member_dead(idx);
                        continue;
                    }
                    // A real per-directory error (permission, not-found, …): the
                    // same on any connection, so surface it exactly like the main
                    // path. Don't transition the main session's state — this wasn't
                    // its connection.
                    Err(e) => return Err(map_smb_error(e)),
                }
            }
            // Every member is momentarily dead: fall through to the main session,
            // which keeps the scan progressing and, if it too is dead, yields the
            // DeviceDisconnected the scanner's terminal-disconnect path expects.
        }
        self.list_directory_impl(path).await
    }

    /// The `open_read_stream_for_scan` body: serve a SMALL hinted file from a scan-
    /// pool member via the 1-RTT compound read; everything else (no pool, no/large
    /// hint, size drift) falls through to the main-session path. Media enrichment's
    /// parallel prefetch calls this with several reads in flight, so spreading the
    /// compound reads over the pool's separate TCP connections is what lets them
    /// actually overlap (ksmbd serializes per connection; see the module docs).
    ///
    /// Pool members serve ONLY the compound (single-round-trip) shape on purpose:
    /// an error surfaces at the request boundary, where a dead member is retried on
    /// a sibling exactly like a listing. A STREAMING read on a member could die
    /// mid-stream, which would surface as a transport error to a consumer the pool
    /// can't transparently retry for — the main session (with its reconnect
    /// machinery and connection-state signaling) owns streaming.
    pub(super) async fn open_read_stream_for_scan_impl(
        &self,
        path: &Path,
        size_hint: Option<u64>,
    ) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        if let Some(size) = size_hint
            && size > 0
        {
            let pool = { self.scan_pool.read().await.clone() };
            if let Some(pool) = pool {
                let smb_path = self.to_smb_path(path);
                for _ in 0..pool.member_count() {
                    let Some((idx, tree, mut conn)) = pool.acquire().await else {
                        break; // every member momentarily dead ⇒ main session
                    };
                    let max_read = conn.params().map(|p| p.max_read_size).unwrap_or(65536) as u64;
                    if size > max_read {
                        break; // too big for one compound READ ⇒ main-session streaming
                    }
                    match tree.read_file_compound(&mut conn, &smb_path).await {
                        Ok(data) if data.len() as u64 == size => {
                            return Ok(Box::new(InlineReadStream::new(data)) as Box<dyn VolumeReadStream>);
                        }
                        Ok(_) => break, // size drifted since the scan ⇒ streaming self-corrects
                        Err(e) if is_pool_member_dead(&e) => {
                            log::debug!(
                                "smb scan pool: member {idx} died reading {smb_path:?} ({e}); retrying on a sibling"
                            );
                            pool.mark_member_dead(idx);
                            continue;
                        }
                        Err(e) if matches!(e.kind(), smb2::ErrorKind::TooLarge) => break, // grew past max_read
                        // A real per-file error (permission, not-found, …): the same
                        // on any connection; surface it typed, don't touch the main
                        // session's state (this wasn't its connection).
                        Err(e) => return Err(map_smb_error(e)),
                    }
                }
            }
        }
        Volume::open_read_stream_with_hint(self, path, size_hint).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-robin hands out each live slot in turn and cycles.
    #[test]
    fn round_robin_cycles_over_live_slots() {
        let slots = PoolSlots::new(4);
        for i in 0..4 {
            slots.mark_alive(i);
        }
        let picks: Vec<usize> = (0..8).map(|_| slots.next_alive().expect("all live")).collect();
        assert_eq!(picks, vec![0, 1, 2, 3, 0, 1, 2, 3]);
    }

    /// A dead slot is skipped; the cursor still advances so the survivors share
    /// the load evenly rather than all landing on the lowest index.
    #[test]
    fn dead_slots_are_skipped() {
        let slots = PoolSlots::new(4);
        for i in 0..4 {
            slots.mark_alive(i);
        }
        slots.mark_dead(1);
        slots.mark_dead(3);
        let picks: Vec<usize> = (0..6).map(|_| slots.next_alive().expect("two live")).collect();
        // Only 0 and 2 are handed out, alternating.
        assert!(picks.iter().all(|&i| i == 0 || i == 2), "picks were {picks:?}");
        assert!(
            picks.contains(&0) && picks.contains(&2),
            "both survivors used: {picks:?}"
        );
    }

    /// Every slot dead ⇒ no selection (caller falls back to the main session).
    #[test]
    fn all_dead_yields_none() {
        let slots = PoolSlots::new(3);
        assert!(slots.next_alive().is_none(), "fresh slots start dead");
        assert!(!slots.any_alive());
        slots.mark_alive(2);
        assert!(slots.any_alive());
        assert_eq!(slots.next_alive(), Some(2));
        slots.mark_dead(2);
        assert!(slots.next_alive().is_none());
    }

    /// An empty pool never selects (guards the `% 0` divide).
    #[test]
    fn empty_pool_never_selects() {
        let slots = PoolSlots::new(0);
        assert!(slots.next_alive().is_none());
        assert!(!slots.any_alive());
    }

    /// The reconnect guard is single-flight: the first claimant wins, later ones
    /// are refused until it ends, then a fresh claim can win again.
    #[test]
    fn reconnect_is_single_flight() {
        let slots = PoolSlots::new(2);
        assert!(slots.try_begin_reconnect(0), "first claim wins");
        assert!(!slots.try_begin_reconnect(0), "second claim refused while running");
        assert!(slots.try_begin_reconnect(1), "a different slot is independent");
        slots.end_reconnect(0);
        assert!(slots.try_begin_reconnect(0), "a fresh claim wins after the first ends");
    }
}
