//! The index's public API: one [`Index`] handle, and everything you can ask it.
//!
//! Reading this file should tell you what the index does without opening an
//! internal. Every method is named for what the caller wants, not for the
//! machinery behind it, and the machinery is not reachable from outside.
//!
//! ## The three things it does
//!
//! - **Indexes volumes.** [`Index::start_volume`] and friends turn a drive's
//!   index on, off, or over again; the index decides how (a local walk, a
//!   `Volume`-trait walk over a share, a PTP walk over a phone) from the volume's
//!   own facts.
//! - **Answers what's on them.** [`Index::enrich`] fills recursive sizes into a
//!   listing (the hot path), and [`Index::dir_stats`] / [`Index::list_children`]
//!   answer for one path.
//! - **Takes corrections from the host.** The app sees changes the index can't
//!   (a share's change notification, a phone's PTP event, a watcher that died),
//!   and hands them back through [`Index::apply_directory_change`],
//!   [`Index::on_device_object_changed`], and [`Index::on_watch_gap`].
//!
//! ## Where the state actually lives
//!
//! Today the handle is a thin token: the subsystems below it still carry
//! process-wide state, and most methods resolve that state rather than reading a
//! field. That's why building twice is [`IndexBuildError::AlreadyBuilt`] rather
//! than two independent indexes. The point of the handle landing first is that
//! every call site is written against the shape that survives de-globalization,
//! so moving the state inside costs no call-site churn.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::DirectoryChange;

mod builder;
mod error;
mod ingest;

pub use builder::{IndexBuildError, IndexBuilder};
pub use error::IndexError;
pub use ingest::{
    IngestError, ListingAgreement, ListingObservation, ObservedEntry, SizeError, SizeFreshness, SizeProgress,
    SizeRequest, SizeStream, SizeVerdict,
};

use crate::indexing::events::EventSink;
use crate::indexing::host::policy::HostPolicy;
use crate::indexing::host::volumes::VolumeProvider;
use crate::indexing::lifecycle::state::{self, IndexVolumeKind};
use crate::indexing::read::enrichment::ReadPool;
use crate::indexing::read::expected_totals::ExpectedTotals;
use crate::indexing::store::{DirStats, EntryRow};
use crate::indexing::{IndexDebugStatusResponse, IndexStatusResponse, VolumeIndexStatus};

#[cfg(any(test, feature = "testing"))]
pub use builder::TestInstallGuard;

#[cfg(test)]
mod tests;

/// What starting a volume's index did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    /// The volume is indexing: a scan is running, resuming, or was already
    /// active.
    Started,
    /// The master drive-indexing switch is off, so no volume may index. Nothing
    /// is wrong with this one.
    IndexingDisabled,
    /// A share couldn't be indexed yet, for a typed reason the host can act on
    /// (sign in, reconnect, or just show the state honestly).
    Refused(crate::indexing::SmbIndexGateReason),
}

/// Whose live watching lost continuity.
#[derive(Debug, Clone, Copy)]
pub enum WatchScope<'a> {
    /// One volume's watcher. Only that volume's index goes stale.
    Volume(&'a str),
    /// A whole device's event stream (a phone's PTP session). Every volume the
    /// device carries goes stale together.
    Device(&'a str),
}

/// How live watching lost continuity. Each means "the index can no longer trust
/// that it has seen every change", and the index heals the same way; the
/// distinction is for honest diagnostics.
#[derive(Debug, Clone, Copy)]
pub enum WatchGap {
    /// The watcher stopped and isn't coming back on its own.
    WatcherStopped,
    /// The watcher survived but the OS dropped events it couldn't buffer.
    EventsOverflowed,
    /// The device's session reset, so its event stream restarted from nothing.
    ConnectionReset,
}

/// This process's index.
///
/// Build one with [`Index::builder`]. Cheap to clone, and every clone is the
/// same index.
#[derive(Clone)]
pub struct Index {
    /// Where the index reports what it's doing.
    #[allow(dead_code, reason = "read once the event seam is threaded off the slot")]
    events: Arc<dyn EventSink>,
    /// What the index asks about mounted storage.
    #[allow(dead_code, reason = "read once the volume seam is threaded off the slot")]
    volumes: Arc<dyn VolumeProvider>,
    /// Whether background work may run right now.
    #[allow(dead_code, reason = "read once the policy seam is threaded off the slot")]
    policy: Arc<dyn HostPolicy>,
    /// Where the index databases live.
    data_dir: PathBuf,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Index").field("data_dir", &self.data_dir).finish()
    }
}

impl Index {
    /// Start describing this process's index; [`IndexBuilder::build`] hands back
    /// the handle.
    #[must_use]
    pub fn builder() -> IndexBuilder {
        IndexBuilder::default()
    }

    // ── Turning volumes on and off ───────────────────────────────────

    /// Start the boot disk's index at launch, if the launch gate allows it.
    ///
    /// `fda_pending` is the host's answer to "are we still waiting for the user
    /// to decide about full disk access?" — walking protected folders before
    /// they've chosen would stack one permission prompt per folder on top of
    /// onboarding. Reports whether indexing actually started.
    pub fn start_root_at_launch(&self, fda_pending: bool) -> Result<bool, IndexError> {
        if !state::should_auto_start_indexing(Some(crate::indexing::lifecycle::master::master_enabled()), fda_pending) {
            return Ok(false);
        }
        state::start_indexing()?;
        Ok(true)
    }

    /// Turn on indexing for one volume, whatever kind of storage it is.
    ///
    /// The index routes by the volume's own facts: the boot disk and local
    /// external drives run the guarded local walk, a phone walks over PTP, and a
    /// share walks over its `Volume` trait behind a direct-session gate that can
    /// refuse with a reason the host can act on. A volume that's already indexing
    /// reports [`StartOutcome::Started`], so calling this twice is safe; one whose
    /// index died is rebuilt from scratch, because the index is a disposable
    /// cache.
    pub async fn start_volume(&self, volume_id: &str) -> Result<StartOutcome, IndexError> {
        if state::is_active(volume_id) {
            return Ok(StartOutcome::Started);
        }
        // The master switch outranks every per-volume choice, so it's answered
        // here, once, rather than differently by each transport's own gate.
        if !crate::indexing::lifecycle::master::master_enabled() {
            log::info!(target: "indexing", "start_volume: refusing '{volume_id}', drive indexing is off in settings");
            return Ok(StartOutcome::IndexingDisabled);
        }
        // A failed index can't resume in place (its writer and manager are torn
        // down while the instance stays registered), so the retry is a rebuild.
        if state::is_failed(volume_id) {
            self.forget_volume(volume_id)?;
        }

        if volume_id == crate::indexing::ROOT_VOLUME_ID {
            state::start_indexing()?;
            return Ok(StartOutcome::Started);
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            use crate::indexing::transports::local_external::index::{
                LocalExternalEnable, start_indexing_for_local_external,
            };

            if cmdr_fs::volume::mtp_ids::is_mtp_volume_id(volume_id) {
                crate::indexing::transports::mtp::index::start_indexing_for_mtp(volume_id.to_string())?;
                return Ok(StartOutcome::Started);
            }
            match start_indexing_for_local_external(volume_id.to_string()).await? {
                LocalExternalEnable::Started => return Ok(StartOutcome::Started),
                LocalExternalEnable::NotLocalExternal => {}
            }
        }

        match crate::indexing::transports::smb::index::start_indexing_for_smb(volume_id.to_string()).await {
            Ok(()) => Ok(StartOutcome::Started),
            Err(reason) => Ok(StartOutcome::Refused(reason)),
        }
    }

    /// Index one volume again from scratch.
    ///
    /// An already-indexing volume gets a fresh full walk; one that isn't indexing
    /// yet is started, since its first walk is the rescan.
    pub async fn rescan_volume(&self, volume_id: &str) -> Result<StartOutcome, IndexError> {
        if state::is_active(volume_id) {
            state::force_scan(volume_id)?;
            return Ok(StartOutcome::Started);
        }
        self.start_volume(volume_id).await
    }

    /// Stop the walk that's running on a volume, keeping its index and its
    /// watcher alive.
    pub fn stop_scan(&self, volume_id: &str) -> Result<(), IndexError> {
        state::stop_scan(volume_id).map_err(Into::into)
    }

    /// Turn indexing off for a volume at the user's explicit request.
    ///
    /// Stops the walk and the watcher and keeps the database, so turning it back
    /// on resumes rather than rescans, and records the choice so a later
    /// reconnect doesn't quietly turn back on what the user turned off.
    pub fn disable_volume(&self, volume_id: &str) -> Result<(), IndexError> {
        state::disable_drive_index_persist_intent(volume_id).map_err(Into::into)
    }

    /// Forget a volume's index entirely: stop it and delete its database, so the
    /// disk comes back and a future start does a clean first walk.
    pub fn forget_volume(&self, volume_id: &str) -> Result<(), IndexError> {
        state::clear_index(volume_id).map_err(Into::into)
    }

    /// A removable drive is going away; stop indexing it if it's the kind that
    /// has to stop. Reports whether it did.
    ///
    /// Only a locally-attached external drive is stopped here: it's the one
    /// holding a filesystem watcher and open database handles that can wedge an
    /// unmount. Shares and phones tear down through their own disconnect paths
    /// and stay browsable offline, so stopping them here would fight those.
    ///
    /// **Blocking**: draining the writer can take seconds. Never call it on a
    /// thread the interface is waiting on.
    pub fn stop_removable_volume(&self, volume_id: &str) -> bool {
        if state::volume_kind(volume_id) != Some(IndexVolumeKind::LocalExternal) {
            return false;
        }
        if let Err(e) = state::stop_indexing(volume_id) {
            log::warn!(target: "indexing", "stopping the removable volume index '{volume_id}' failed: {e}");
        }
        true
    }

    /// Apply the master drive-indexing switch. Off stops every volume that's
    /// indexing; on only moves the gate, and
    /// [`drives_to_resume`](Self::drives_to_resume) says which volumes the host
    /// should start.
    ///
    /// Neither direction touches per-volume choices, so they survive any number
    /// of toggles.
    pub fn set_indexing_enabled(&self, enabled: bool) {
        // The gate moves first in both directions: on, so the starts that follow
        // pass it; off, so a concurrent reconnect can't slip in behind the stop.
        crate::indexing::lifecycle::master::set_master_enabled(enabled);
        if !enabled {
            state::stop_all_indexing();
        }
    }

    /// Every volume that should be indexing now that the master switch is back
    /// on, and isn't already.
    ///
    /// A volume the user never turned on, or explicitly turned off, isn't in the
    /// list: this is what makes the master switch restore choices rather than
    /// turn on everything.
    pub fn drives_to_resume(&self) -> Vec<String> {
        crate::indexing::lifecycle::master::drives_to_resume()
    }

    /// A share came back after a disconnect; resume its index if its own settings
    /// say it should be indexing. A no-op otherwise.
    pub fn resume_after_reconnect(&self, volume_id: impl Into<String>) {
        crate::indexing::transports::smb::index::resume_smb_index_if_enabled(volume_id.into());
    }

    // ── What the index knows about a volume ──────────────────────────

    /// One volume's scan counters, database size, and last-scan facts.
    pub fn status(&self, volume_id: &str) -> Result<IndexStatusResponse, IndexError> {
        crate::indexing::read::queries::get_status(volume_id).map_err(Into::into)
    }

    /// Everything [`status`](Self::status) reports plus the internals a developer
    /// needs: watcher counters, phase history, verification and reconcile
    /// budgets, and raw database page counts.
    pub fn debug_status(&self, volume_id: &str) -> Result<IndexDebugStatusResponse, IndexError> {
        crate::indexing::read::queries::get_debug_status(volume_id).map_err(Into::into)
    }

    /// A volume's freshness plus its last completed walk's facts: what a per-drive
    /// status badge shows. A volume with no index reports as not indexed rather
    /// than as an error.
    pub fn volume_status(&self, volume_id: &str) -> VolumeIndexStatus {
        crate::indexing::read::queries::get_volume_index_status(volume_id)
    }

    /// [`volume_status`](Self::volume_status) for whichever volume owns `path`,
    /// so a caller holding a listing path doesn't have to resolve the volume
    /// itself.
    pub fn volume_status_for_path(&self, path: &str) -> VolumeIndexStatus {
        crate::indexing::read::queries::get_volume_index_status_for_path(path)
    }

    /// Every volume the index has an instance for, in no particular order.
    pub fn volume_ids(&self) -> Vec<String> {
        state::all_registered_volume_ids()
    }

    /// What kind of storage a volume is, as the index classified it. `None` when
    /// the volume isn't indexed.
    pub fn volume_kind(&self, volume_id: &str) -> Option<IndexVolumeKind> {
        state::volume_kind(volume_id)
    }

    /// Every volume whose index is complete and current, with its kind. What a
    /// consumer that scores or enriches per volume sweeps at startup.
    pub fn ready_volumes(&self) -> Vec<(String, IndexVolumeKind)> {
        state::ready_volumes_with_kind()
    }

    /// Whether a volume's index is live AND current, so its rows can be trusted
    /// as a complete answer rather than a partial one.
    pub fn is_fresh(&self, volume_id: &str) -> bool {
        use crate::indexing::lifecycle::freshness::Freshness;
        state::is_active(volume_id) && state::get_freshness(volume_id) == Some(Freshness::Fresh)
    }

    // ── Serving what it indexed ──────────────────────────────────────

    /// Fill each directory entry's recursive size in from the volume's index.
    ///
    /// The hot path: this runs on every listing, so it does two indexed queries
    /// for the whole batch and returns untouched when the volume has no index.
    pub fn enrich(&self, volume_id: &str, entries: &mut [FileEntry]) {
        crate::indexing::read::enrichment::enrich_entries_with_index_on_volume(volume_id, entries);
    }

    /// The recursive size, file count, and directory count under one path, or
    /// `None` when the index doesn't cover it.
    pub fn dir_stats(&self, path: &str) -> Result<Option<DirStats>, IndexError> {
        crate::indexing::read::queries::get_dir_stats(path).map_err(Into::into)
    }

    /// [`dir_stats`](Self::dir_stats) for many paths in one pass, answering in
    /// the order asked.
    pub fn dir_stats_batch(&self, paths: &[String]) -> Result<Vec<Option<DirStats>>, IndexError> {
        crate::indexing::read::queries::get_dir_stats_batch(paths).map_err(Into::into)
    }

    /// What the index has under one directory, without touching the disk. `None`
    /// when the path isn't indexed.
    pub fn list_children(&self, path: &str) -> Result<Option<Vec<EntryRow>>, IndexError> {
        crate::indexing::read::queries::list_dir_children(path).map_err(Into::into)
    }

    /// What a copy or move of `sources` is about to cost, from what the index
    /// already knows: the file count and total bytes, without walking the disk.
    ///
    /// `None` when the index can't cover the sources, which is the caller's cue to
    /// show an honest "counting…" rather than a number it would have to correct.
    pub fn expected_totals(&self, sources: &[PathBuf]) -> Option<ExpectedTotals> {
        crate::indexing::read::expected_totals::expected_totals_for_sources(sources)
    }

    /// The user is looking at this directory; check that the index still matches
    /// it and repair it if not.
    ///
    /// Returns immediately: the check is scheduled, never done on the caller's
    /// thread.
    pub fn verify_directory(&self, volume_id: &str, dir_path: &str) {
        state::trigger_verification(volume_id, dir_path);
    }

    // ── Reading the database directly ────────────────────────────────
    //
    // For the query layers that run their own SQL over an index: the search
    // engine and the operation log's coverage check. They're co-designed with
    // the index and share its schema, which is why they get the connection pool
    // rather than a query API that would have to grow a case per question.

    /// A volume's pooled read connections, or `None` when it has no index.
    pub fn read_pool(&self, volume_id: &str) -> Option<Arc<ReadPool>> {
        crate::indexing::read::enrichment::get_read_pool_for(volume_id)
    }

    /// Translate an absolute path into the form a volume's index stores it under,
    /// or `None` when the path isn't on that volume.
    pub fn read_path(&self, volume_id: &str, abs_path: &str) -> Option<String> {
        crate::indexing::paths::routing::index_read_path(volume_id, abs_path)
    }

    /// Which volume's index owns an absolute local path.
    pub fn volume_id_for_path(&self, path: &str) -> String {
        crate::indexing::paths::routing::volume_id_for_local_path(path)
    }

    /// A counter that changes whenever the searchable index does, so a cached
    /// query result can tell whether it's still current.
    pub fn search_generation(&self) -> u64 {
        crate::indexing::writer::WRITER_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
    }

    // ── Corrections from the host ────────────────────────────────────

    /// The host observed a change in one directory of a volume it watches; fold
    /// it into the index.
    ///
    /// Cheap and non-blocking. During a walk of that volume the change is
    /// buffered and replayed afterwards, so a live stream can't race the walk.
    pub fn apply_directory_change(&self, volume_id: &str, parent_path: &Path, change: &DirectoryChange) {
        crate::indexing::transports::smb::watch::apply_smb_change(volume_id, parent_path, change);
    }

    /// A device reported that one object appeared or changed. The index resolves
    /// what it is and folds it in, or buffers the bare handle when the device is
    /// busy with a walk.
    pub fn on_device_object_changed(&self, device_id: &str, handle: u32) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::indexing::transports::mtp::watch::on_device_object_changed(device_id, handle);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = (device_id, handle);
        }
    }

    /// A device reported that one object is gone. Costs no device round trip:
    /// each indexed storage resolves the removal by the handle it stored.
    pub fn on_device_object_removed(&self, device_id: &str, handle: u32) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::indexing::transports::mtp::watch::on_device_object_removed(device_id, handle);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = (device_id, handle);
        }
    }

    /// Live watching lost continuity, so the index can no longer claim it has
    /// seen every change. It goes stale and schedules a heal.
    pub fn on_watch_gap(&self, scope: WatchScope<'_>, reason: WatchGap) {
        match scope {
            WatchScope::Volume(volume_id) => match reason {
                WatchGap::EventsOverflowed => crate::indexing::transports::smb::index::on_smb_overflow(volume_id),
                WatchGap::WatcherStopped | WatchGap::ConnectionReset => {
                    crate::indexing::transports::smb::index::on_smb_watcher_died(volume_id);
                }
            },
            WatchScope::Device(device_id) => {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                crate::indexing::transports::mtp::index::on_mtp_watch_continuity_lost(device_id);
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    let _ = device_id;
                }
            }
        }
    }
}

/// Serializes tests that build a handle. The seams behind it are process-wide, so
/// two tests installing at once would see each other's volumes and events.
#[cfg(any(test, feature = "testing"))]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
impl Index {
    /// Release the process's index claim for one test, restoring it on drop.
    pub(crate) fn release_build_claim_for_test() -> builder::BuildClaimGuard {
        IndexBuilder::release_build_claim_for_test()
    }
}
