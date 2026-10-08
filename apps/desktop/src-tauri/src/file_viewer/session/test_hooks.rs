//! Test-only scaffolding for `session.rs`: the gate that parks background
//! workers at a known point, and the hooks tests use to observe or drive
//! session internals.
//!
//! Lives in its own file so the production orchestration in `session.rs` isn't
//! carrying test machinery, and so that file's length budget tracks real code.
//! The `mod` declaration is `#[cfg(test)]`, so none of this reaches a shipped
//! binary.

use std::sync::atomic::AtomicUsize;

use super::*;

/// Counts a rebuild thread's exit on every path (early return, error, or a
/// completed store) by running on drop.
#[cfg(test)]
pub struct RebuildExitGuard;

#[cfg(test)]
impl Drop for RebuildExitGuard {
    fn drop(&mut self) {
        test_gate::note_rebuild_exit();
    }
}

/// Test-only hook: simulates a watcher `Grew(eof)` event by writing into the session's
/// `pending_grew` queue. Drives `test_append_during_encoding_rebuild_not_dropped`
/// without standing up the (milestone-3) FS watcher.
#[cfg(test)]
pub fn test_only_push_pending_grew(session_id: &str, eof: u64) {
    // Delegates to the production path on purpose. Re-implementing the
    // coalescing here would mean the drain tests exercise a copy, and a bug in
    // the real rule would sail straight past them (confirmed by mutation).
    push_pending_grew(session_id, eof);
}

/// Test-only hook: reads the current `pending_grew` queue value.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test race-coverage tests")]
pub fn test_only_pending_grew(session_id: &str) -> Option<u64> {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .and_then(|s| *s.pending_grew.lock_ignore_poison())
}

/// Test-only hook: reads the rebuilding flag's strong-count parity (Some/None) to
/// help tests block-poll for completion without sleeping.
#[cfg(test)]
pub fn test_only_rebuilding_active(session_id: &str) -> bool {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .map(|s| s.rebuilding.lock_ignore_poison().is_some())
        .unwrap_or(false)
}

/// Returns the count of currently-active reads. Test-only helper for asserting that
/// a cancelled or completed read cleaned up its `active_reads` entry.
#[cfg(test)]
pub fn active_read_count(session_id: &str) -> usize {
    let sessions = SESSIONS.lock_ignore_poison();
    let Some(session) = sessions.get(session_id) else {
        return 0;
    };
    session.active_reads.lock_ignore_poison().len()
}

/// Test-only helper: drives the race in `apply_tail_extend` deterministically.
///
/// Simulates the timing the round-3 audit caught:
/// 1. The watcher thread takes a backend snapshot.
/// 2. Before its long `extend_to_boxed` returns, a separate concurrent
///    activity (encoding rebuild, upgrade) installs a brand-new backend.
/// 3. The watcher's eventual `store` must NOT clobber the new backend.
///
/// We script this by snapshotting the backend, calling `swap_callback` (which
/// the test uses to install a fresh backend via, e.g., `reload` or
/// `set_encoding`), then calling `extend_to_boxed` on the snapshot, then
/// running the same ptr-eq check the production code uses to decide
/// store-vs-discard. Returns `true` if the store was applied (snapshot still
/// current), `false` if the extend was discarded.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test tail-extend clobber-race test")]
pub fn test_only_run_tail_extend_with_swap(session_id: &str, new_size: u64, swap_callback: impl FnOnce()) -> bool {
    let dummy_cancel = AtomicBool::new(false);
    let backend_snapshot = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).expect("session must exist");
        session.backend.load_full()
    };
    // Trigger the racing swap (the test installs a new backend here).
    swap_callback();

    let extended = backend_snapshot
        .extend_to_boxed(new_size, &dummy_cancel)
        .expect("extend should succeed");

    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).expect("session must exist");
    let current = session.backend.load_full();
    if Arc::ptr_eq(&current, &backend_snapshot) {
        session.backend.store(Arc::new(extended));
        true
    } else {
        coalesce_pending_grew(session, new_size);
        false
    }
}

/// Test-only helper: returns the current tail-mode flag.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test::tail_mode_can_be_toggled")]
pub fn test_only_tail_mode(session_id: &str) -> bool {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .map(|s| s.tail_mode.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// A stand-in backend that serves `line_count` copies of one line and runs a hook
/// before each `get_lines` call.
///
/// Two things a fixture file can't give a test: a range far larger than anything worth
/// writing to disk, and a vantage point *inside* the read loop. The hook is that
/// vantage point. It can look at the destination temp file, flip a cancel flag, or
/// count chunks, at a point the production code reaches deterministically rather than
/// after a sleep.
#[cfg(test)]
pub struct ScriptedBackend {
    line: String,
    line_count: usize,
    /// Called with the 0-based index of each `get_lines` call, before it is served.
    on_get_lines: Box<dyn Fn(usize) + Send + Sync>,
    calls: AtomicUsize,
}

#[cfg(test)]
impl ScriptedBackend {
    pub fn new(line: &str, line_count: usize, on_get_lines: impl Fn(usize) + Send + Sync + 'static) -> Self {
        Self {
            line: line.to_string(),
            line_count,
            on_get_lines: Box::new(on_get_lines),
            calls: AtomicUsize::new(0),
        }
    }

    /// Bytes per line on "disk": the line plus its `\n`. Every line is the same length,
    /// which is what makes the byte-offset seeks the range reader uses exact here.
    fn stride(&self) -> usize {
        self.line.len() + 1
    }
}

#[cfg(test)]
impl FileViewerBackend for ScriptedBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize, _cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        (self.on_get_lines)(call);

        let stride = self.stride();
        let start = match target {
            SeekTarget::Row(n) => *n,
            SeekTarget::ByteOffset(b) => (*b as usize) / stride,
            SeekTarget::Fraction(f) => ((self.line_count as f64) * f) as usize,
        }
        .min(self.line_count);
        let end = (start + count).min(self.line_count);

        Ok(LineChunk {
            rows: (start..end)
                .map(|row| crate::file_viewer::ViewerRow {
                    text: self.line.clone(),
                    byte_offset: (row * stride) as u64,
                    continues: false,
                    line_number: Some(row),
                })
                .collect(),
            first_row_number: start,
            byte_offset: (start * stride) as u64,
            end_byte_offset: (end * stride) as u64,
            end: if end >= self.line_count {
                crate::file_viewer::ChunkEnd::EndOfFile
            } else {
                crate::file_viewer::ChunkEnd::CountReached
            },
            total_rows: TotalRows::Exact(self.line_count),
            total_bytes: self.total_bytes(),
        })
    }

    fn search(
        &self,
        _matcher: &Matcher,
        _cancel: &AtomicBool,
        _matches: &Mutex<Vec<SearchMatch>>,
        _progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        Ok(0)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_line_seek: true,
            supports_byte_seek: true,
            supports_fraction_seek: true,
            knows_total_lines: true,
        }
    }

    fn total_bytes(&self) -> u64 {
        (self.line_count * self.stride()) as u64
    }

    fn total_rows(&self) -> TotalRows {
        TotalRows::Exact(self.line_count)
    }

    fn total_lines(&self) -> Option<usize> {
        Some(self.line_count)
    }

    fn file_name(&self) -> &str {
        "scripted.txt"
    }
}

/// Test-only hook: registers a session over `backend` and returns its id.
///
/// No watcher, no upgrade thread, and no file behind it: the session exists so the read
/// paths (`read_range`, `write_range_to_file`) can run against a [`ScriptedBackend`].
/// Close it with `close_session` like any other session.
#[cfg(test)]
pub fn test_only_install_session(backend: Box<dyn FileViewerBackend>, path: PathBuf) -> String {
    let session_id = generate_session_id();
    let session = ViewerSession::new(ViewerSessionInit {
        backend,
        backend_type: BackendType::ByteSeek,
        upgrading: None,
        encoding: FileEncoding::Utf8,
        detected_encoding: FileEncoding::Utf8,
        watcher_stop: Arc::new(AtomicBool::new(false)),
        path,
        media_token: None,
        temp: None,
    });
    SESSIONS.lock_ignore_poison().insert(session_id.clone(), session);
    session_id
}

/// Test-only rendezvous that parks a background worker at a known point until
/// the test explicitly releases it.
///
/// The drain-and-swap tests need the worker to be *provably* parked at a
/// specific point while they set up. A timed sleep can't give them that: it's a
/// scheduling hint, so on a loaded machine the worker can run right past the
/// intended window, and the test then passes without exercising anything (or
/// fails, if it also raced a file write). The gate turns that into a handshake:
/// the worker signals arrival, the test waits for it, sets up, then releases.
///
/// **Where a gate sits decides what the test proves.** `PRE_DRAIN` is the one
/// the drain tests need: it parks with the scan already finished, so an append
/// made while parked is invisible to the fresh backend and can only reach it
/// through `pending_grew`. Parking `PRE_SCAN` instead lets the scan pick the
/// append up off disk by itself, and the test then passes whether the drain
/// works or not (confirmed by mutation: dropping the drained EOF left every
/// test green when these tests parked pre-scan).
#[cfg(test)]
pub mod test_gate {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    use crate::ignore_poison::IgnorePoison;

    /// How long a parked worker waits before proceeding anyway. Only reached
    /// when a test forgets to release; the worker then continues so the test
    /// fails on its own assertion instead of hanging to the nextest cap.
    const RELEASE_TIMEOUT: Duration = Duration::from_secs(30);

    struct GateState {
        /// Set by `arm`, consumed by the first worker to reach the gate.
        armed: bool,
        /// Set once a worker has parked.
        arrived: bool,
        /// Set by `release` to let the parked worker continue.
        released: bool,
    }

    pub struct TestGate {
        state: Mutex<GateState>,
        changed: Condvar,
    }

    impl TestGate {
        const fn new() -> Self {
            Self {
                state: Mutex::new(GateState {
                    armed: false,
                    arrived: false,
                    released: false,
                }),
                changed: Condvar::new(),
            }
        }

        /// Test side: arm the gate so the next worker to reach it parks.
        /// Call before the operation that spawns the worker.
        pub fn arm(&self) {
            let mut state = self.state.lock_ignore_poison();
            state.armed = true;
            state.arrived = false;
            state.released = false;
        }

        /// Worker side: park here if the gate is armed, else fall straight
        /// through. One-shot: the first arrival consumes the arm, so a second
        /// worker (a superseding rebuild, say) is never held.
        ///
        /// Holds no other lock while parked, so the test is free to take
        /// `SESSIONS` and `pending_grew` in the meantime.
        pub fn wait_if_armed(&self) {
            let mut state = self.state.lock_ignore_poison();
            if !state.armed {
                return;
            }
            state.armed = false;
            state.arrived = true;
            self.changed.notify_all();

            let deadline = Instant::now() + RELEASE_TIMEOUT;
            while !state.released {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    // Nobody released us. Proceed rather than hang, so the
                    // test reports a real assertion instead of a timeout.
                    break;
                };
                let (next, _) = self
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|e| e.into_inner());
                state = next;
            }
        }

        /// Test side: block until a worker has parked at the gate. Panics on
        /// timeout, because every caller's setup depends on the worker being
        /// held: continuing would silently test nothing.
        pub fn wait_until_reached(&self, timeout: Duration) {
            let mut state = self.state.lock_ignore_poison();
            let deadline = Instant::now() + timeout;
            while !state.arrived {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .expect("no worker reached the test gate in time");
                let (next, _) = self
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|e| e.into_inner());
                state = next;
            }
        }

        /// Test side: let the parked worker continue.
        pub fn release(&self) {
            let mut state = self.state.lock_ignore_poison();
            state.released = true;
            self.changed.notify_all();
        }
    }

    /// Parks the ByteSeek → LineIndex upgrade with its scan done, just before
    /// the drain-and-swap critical section.
    pub static UPGRADE_PRE_DRAIN: TestGate = TestGate::new();
    /// Parks the encoding rebuild before it starts scanning, so another
    /// `set_encoding` can supersede it.
    pub static REBUILD_PRE_SCAN: TestGate = TestGate::new();
    /// Parks the encoding rebuild with its scan done, just before the
    /// drain-and-swap critical section.
    pub static REBUILD_PRE_DRAIN: TestGate = TestGate::new();

    /// Counts rebuild threads that reached their store, and that exited by any
    /// path. A superseded rebuild must raise only the second: the test waits
    /// for both threads to exit, then asserts exactly one of them stored.
    /// Without the exit count the test would race the loser thread and miss a
    /// late stale store.
    static REBUILD_STORES: AtomicUsize = AtomicUsize::new(0);
    static REBUILD_EXITS: AtomicUsize = AtomicUsize::new(0);

    pub fn note_rebuild_store() {
        REBUILD_STORES.fetch_add(1, Ordering::SeqCst);
    }
    pub fn note_rebuild_exit() {
        REBUILD_EXITS.fetch_add(1, Ordering::SeqCst);
    }
    pub fn rebuild_store_count() -> usize {
        REBUILD_STORES.load(Ordering::SeqCst)
    }
    pub fn rebuild_exit_count() -> usize {
        REBUILD_EXITS.load(Ordering::SeqCst)
    }
}
