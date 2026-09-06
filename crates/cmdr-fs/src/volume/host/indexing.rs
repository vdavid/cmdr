//! Telling the file index that live watching lost continuity.
//!
//! The index keeps a per-volume database and trusts a backend's watcher to keep
//! it matching the server. Ordinary changes need no seam: they already travel
//! through [`ListingHost::directory_changed`], which the host fans out to the
//! index. What the index can't work out for itself is when the watch BROKE, so
//! that it can stop claiming the database is fresh.
//!
//! Getting this wrong is silent and long-lived. A backend that drops its watcher
//! without reporting leaves an index that looks fresh forever and quietly serves
//! sizes for files that were deleted weeks ago.
//!
//! ## Why this is a trait and not a `cmdr-index` dependency
//!
//! A backend crate could call the index handle directly — both are Tauri-free
//! crates. It must not. The point of a backend crate is that
//! `cargo check -p cmdr-ftp` compiles the protocol and nothing else; a
//! dependency on the index would put a quarter of the codebase back inside every
//! inner loop, for two method calls. So the vocabulary below is the seam's own,
//! and the host's adapter maps it to whatever the index calls things.
//!
//! [`ListingHost::directory_changed`]: super::listings::ListingHost::directory_changed

/// How a live watch lost continuity.
///
/// Each means "the index can no longer trust that it has seen every change", and
/// the index heals them the same way; the distinction is for honest diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchGap {
    /// The watcher stopped and isn't coming back on its own.
    WatcherStopped,
    /// The watcher survived, but changes were dropped that it can't recover —
    /// a server-side overflow, a queue that filled faster than it drained.
    EventsOverflowed,
    /// The connection reset, so the event stream restarted from nothing and
    /// whatever happened during the gap was never reported.
    ConnectionReset,
}

/// What a backend owes the file index.
///
/// Cmdr answers both from the index handle; a test or a tool answers neither
/// (`NoIndexNotifier`).
pub trait IndexNotifier: Send + Sync {
    /// Live watching on `volume_id` broke, and here's how.
    ///
    /// Report it at every exit a watcher has, including the ones that look like
    /// setup rather than failure: a session that never connected keeps the index
    /// just as blind as one that died mid-stream. Cheap and idempotent — it's a
    /// no-op for a volume that isn't indexed or is already known stale — so ❌
    /// don't try to work out first whether it's worth reporting.
    fn watch_gap(&self, volume_id: &str, gap: WatchGap);

    /// `volume_id` is serving again after a reconnect, so any index the user had
    /// enabled for it can pick back up.
    ///
    /// Fire-and-forget, and safe to call while holding a reconnect lock: the
    /// host spawns rather than indexing inline. A superseded instance must NOT
    /// call it — the index for that id belongs to its successor.
    ///
    /// Reconnecting does NOT make a stale index fresh again; only a rescan does.
    /// This asks the index to resume, not to forget the gap it was told about.
    fn resume_after_reconnect(&self, volume_id: &str);

    /// Live watching on every storage of `device_id` broke at once, and here's
    /// how.
    ///
    /// The device twin of [`watch_gap`](Self::watch_gap), for a backend whose
    /// one session carries several volumes: an MTP phone opens a single PTP
    /// session per device, so a reset invalidates each storage on it together.
    /// ❌ Don't loop [`watch_gap`](Self::watch_gap) over the device's volumes
    /// instead — which volumes a device carries is the host's list, and a dead
    /// session is exactly when the backend can no longer enumerate it.
    ///
    /// Same cheap, idempotent, report-blindly contract as
    /// [`watch_gap`](Self::watch_gap), and the same no-op default as the two
    /// object methods below.
    fn device_watch_gap(&self, device_id: &str, gap: WatchGap) {
        let _ = (device_id, gap);
    }

    /// One object on `device_id` appeared or changed, named by the bare protocol
    /// handle the device reported.
    ///
    /// For a device backend whose events name an opaque handle rather than a
    /// path: an MTP phone reports `ObjectAdded { handle }` and says nothing about
    /// which storage it lives in or what it's called. ❌ Don't resolve it first —
    /// that's a round trip per event, and the index may be mid-walk and about to
    /// read the object anyway, so it owns the routing and the
    /// gate-before-resolve decision.
    ///
    /// Keyed by DEVICE, not by volume, because one session carries every storage
    /// on the phone and the handle namespace spans them all.
    ///
    /// Defaults to doing nothing: a host with no device index has nowhere to put
    /// this, and a backend must be able to report blindly.
    fn device_object_changed(&self, device_id: &str, handle: u32) {
        let _ = (device_id, handle);
    }

    /// One object on `device_id` is gone, named by the handle it had.
    ///
    /// Costs the index no round trip: the object no longer exists to be asked
    /// about, so each indexed storage matches on the handle it stored. Same
    /// device-level keying and same no-op default as
    /// [`device_object_changed`](Self::device_object_changed).
    fn device_object_removed(&self, device_id: &str, handle: u32) {
        let _ = (device_id, handle);
    }
}

/// Nothing is indexed, so nothing needs telling.
pub(super) struct NoIndexNotifier;

impl IndexNotifier for NoIndexNotifier {
    fn watch_gap(&self, _volume_id: &str, _gap: WatchGap) {}
    fn resume_after_reconnect(&self, _volume_id: &str) {}
}

#[cfg(any(test, feature = "testing"))]
pub use recording::RecordingIndexNotifier;

#[cfg(any(test, feature = "testing"))]
mod recording {
    use std::sync::Mutex;

    use super::{IndexNotifier, WatchGap};
    use crate::ignore_poison::IgnorePoison;

    /// An [`IndexNotifier`] that remembers what it was
    /// told, so a watcher test can prove every exit path reports its gap.
    #[derive(Default)]
    pub struct RecordingIndexNotifier {
        gaps: Mutex<Vec<(String, WatchGap)>>,
        device_gaps: Mutex<Vec<(String, WatchGap)>>,
        resumes: Mutex<Vec<String>>,
        objects_changed: Mutex<Vec<(String, u32)>>,
        objects_removed: Mutex<Vec<(String, u32)>>,
    }

    impl RecordingIndexNotifier {
        /// A recorder with nothing reported yet.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every gap reported so far, in order.
        pub fn gaps(&self) -> Vec<(String, WatchGap)> {
            self.gaps.lock_ignore_poison().clone()
        }

        /// Every device-wide gap reported so far, in order. Kept apart from
        /// [`gaps`](Self::gaps) on purpose: a backend that reported per volume
        /// where it meant per device would leave the device's other storages
        /// claiming to be fresh.
        pub fn device_gaps(&self) -> Vec<(String, WatchGap)> {
            self.device_gaps.lock_ignore_poison().clone()
        }

        /// Every volume a resume was requested for, in order.
        pub fn resumes(&self) -> Vec<String> {
            self.resumes.lock_ignore_poison().clone()
        }

        /// Every `(device_id, handle)` reported as appeared-or-changed, in order.
        pub fn device_objects_changed(&self) -> Vec<(String, u32)> {
            self.objects_changed.lock_ignore_poison().clone()
        }

        /// Every `(device_id, handle)` reported as gone, in order.
        pub fn device_objects_removed(&self) -> Vec<(String, u32)> {
            self.objects_removed.lock_ignore_poison().clone()
        }
    }

    impl IndexNotifier for RecordingIndexNotifier {
        fn watch_gap(&self, volume_id: &str, gap: WatchGap) {
            self.gaps.lock_ignore_poison().push((volume_id.to_string(), gap));
        }

        fn device_watch_gap(&self, device_id: &str, gap: WatchGap) {
            self.device_gaps.lock_ignore_poison().push((device_id.to_string(), gap));
        }

        fn resume_after_reconnect(&self, volume_id: &str) {
            self.resumes.lock_ignore_poison().push(volume_id.to_string());
        }

        fn device_object_changed(&self, device_id: &str, handle: u32) {
            self.objects_changed
                .lock_ignore_poison()
                .push((device_id.to_string(), handle));
        }

        fn device_object_removed(&self, device_id: &str, handle: u32) {
            self.objects_removed
                .lock_ignore_poison()
                .push((device_id.to_string(), handle));
        }
    }
}
