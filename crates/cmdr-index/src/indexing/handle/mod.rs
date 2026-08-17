//! The index's public API: one [`Index`] handle, and everything you can ask it.
//!
//! Reading this file should tell you what the index does without opening an
//! internal. Every method is named for what the caller wants, not for the
//! machinery behind it, and the machinery is not reachable from outside.
//!
//! ## The four things it does
//!
//! - **Indexes volumes.** [`Index::start_volume`] and friends turn a drive's
//!   index on, off, or over again; the index decides how (a local walk, a
//!   `Volume`-trait walk over a share, a PTP walk over a phone) from the volume's
//!   own facts.
//! - **Answers what's on them.** [`Index::enrich`] fills recursive sizes into a
//!   listing (the hot path), and [`Index::dir_stats`] / [`Index::list_children`]
//!   answer for one path.
//! - **Says what it can't answer for yet, and fills it in.** [`Index::coverage`]
//!   hands back the frontier — the shallowest folders under a scope that nothing
//!   has listed — so a caller can walk exactly the gap and serve the rest from
//!   the index; [`Index::cover`] is the walk that closes it, streaming what it
//!   finds while it runs.
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
use tokio_util::sync::CancellationToken;

mod builder;
mod error;
mod ingest;

pub use builder::{IndexBuildError, IndexBuilder};
pub use error::IndexError;
pub use ingest::{
    IngestError, ListingAgreement, ListingObservation, ObservedEntry, SizeError, SizeFreshness, SizeProgress,
    SizeRequest, SizeStream, SizeVerdict,
};

use crate::indexing::events::{Diagnostic, EventSink};
use crate::indexing::host::policy::HostPolicy;
use crate::indexing::host::volumes::VolumeProvider;
use crate::indexing::lifecycle::cover::{self, CoverWalk};
use crate::indexing::lifecycle::state;
use crate::indexing::read::coverage::{CoverageDimension, CoverageMap, CoverageToken};
use crate::indexing::read::enrichment::ReadPool;
use crate::indexing::read::expected_totals::ExpectedTotals;
use crate::indexing::store::{DirStats, EntryRow};
use crate::indexing::volume::IndexVolumeKind;
use crate::{IndexDebugStatusResponse, IndexStatusResponse, VolumeIndexStatus};

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
    /// A search is walking this volume right now, so the full walk can't run yet:
    /// it would blank the rows that walk is still writing. The index REMEMBERS the
    /// request and runs it when the walk ends, so this is a "soon", not a "no" —
    /// a host that says nothing here leaves a button that looks broken.
    DeferredUntilSearchEnds,
    /// The same promise, with the volume held by a full walk of its own instead: a
    /// scan, or the journal replay a launch does. The index runs the requested walk
    /// when that one ends, so the person who asked for fresh data gets it.
    DeferredUntilScanEnds,
    /// The master drive-indexing switch is off, so no volume may index. Nothing
    /// is wrong with this one.
    IndexingDisabled,
    /// A share couldn't be indexed yet, for a typed reason the host can act on
    /// (sign in, reconnect, or just show the state honestly).
    Refused(crate::SmbIndexGateReason),
}

/// The lifecycle's answer to a manual rescan, in the vocabulary a host speaks.
///
/// The two enums stay separate because they answer different questions: the
/// lifecycle's is "did the walk start", the handle's is "what do I tell the person
/// who asked". This is the one place they meet.
fn started_or_deferred(outcome: crate::indexing::lifecycle::rescan_request::RescanOutcome) -> StartOutcome {
    use crate::indexing::lifecycle::rescan_request::RescanOutcome;
    match outcome {
        RescanOutcome::Started => StartOutcome::Started,
        RescanOutcome::DeferredUntilSearchEnds => StartOutcome::DeferredUntilSearchEnds,
        RescanOutcome::DeferredUntilScanEnds => StartOutcome::DeferredUntilScanEnds,
    }
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

    /// Do the index's launch work: reclaim what's stranded in the data dir, then
    /// start the boot disk's index if the launch gate allows it.
    ///
    /// `fda_pending` is the host's answer to "are we still waiting for the user
    /// to decide about full disk access?" — walking protected folders before
    /// they've chosen would stack one permission prompt per folder on top of
    /// onboarding. Reports whether indexing actually started.
    ///
    /// The reclaim half runs either way (a host that indexes nothing still
    /// shouldn't carry dead databases) and off-thread, since it reads a directory
    /// and unlinks files. It lives here rather than in
    /// [`build`](IndexBuilder::build) on purpose: `build` also backs the lazy
    /// no-host fallback a test binary gets, and a sweep firing there would delete
    /// a test fixture's database out from under it. This is the ONE call a host
    /// makes exactly once, at a real launch.
    pub fn start_root_at_launch(&self, fda_pending: bool) -> Result<bool, IndexError> {
        crate::indexing::host::runtime::spawn_blocking(crate::indexing::resources::retention::sweep_legacy_scheme_dbs);
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
    ///
    /// This is also where the drive's own database learns that the user turned it
    /// on. Every per-drive enable in the app arrives here, whatever the transport,
    /// and a search-driven walk never does — so the marker written here means "the
    /// user asked for this drive" and nothing else. `state::record_drive_index_enabled`
    /// says why it's written before the start rather than after it.
    pub async fn start_volume(&self, volume_id: &str) -> Result<StartOutcome, IndexError> {
        if state::is_active(volume_id) {
            // Active isn't the same as indexed: a search-driven walk leaves a
            // writer with no scan and no watcher behind it, and so does a first
            // scan somebody stopped. Returning `Started` there would swallow the
            // request for the very walk this call is asking for, so route it to
            // the scan instead — which is what a first full walk is.
            if state::awaits_its_first_scan(volume_id) && crate::indexing::lifecycle::master::master_enabled() {
                state::record_drive_index_enabled(volume_id);
                return Ok(started_or_deferred(state::force_scan(volume_id)?));
            }
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
        // Ordered before the record below, because the rebuild deletes the database
        // the marker lives in.
        if state::is_failed(volume_id) {
            self.forget_volume(volume_id)?;
        }
        // ⚠️ **Before the transport dispatch below, deliberately.** A share that's
        // asleep, off the network, or wanting credentials refuses at its own gate,
        // and the user's "yes" has to survive that: the persisted marker is what
        // `resume_smb_index_if_enabled` reads when the share comes back, so an
        // after-success write would mean the choice was never recorded and the
        // share stays dark until somebody notices and asks again. ❌ Don't tidy
        // this into the success arms; `cover::cold_drive_tests::intent::
        // turning_indexing_on_for_an_offline_share_records_the_choice_anyway`
        // guards it, and `record_drive_index_enabled` carries the other two reasons.
        state::record_drive_index_enabled(volume_id);

        if volume_id == crate::ROOT_VOLUME_ID {
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
            return Ok(started_or_deferred(state::force_scan(volume_id)?));
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

    /// Forget every volume's index: stop whatever is running and delete every
    /// index database on disk, live or not.
    ///
    /// The whole-index sibling of [`forget_volume`](Self::forget_volume), for the
    /// user who asks to reclaim the disk rather than to be rid of one drive. It
    /// reaches databases no volume has an instance for, which is the only way to
    /// clear what a search's walk left behind on a machine that indexes nothing.
    ///
    /// **Blocking**: draining a running volume's writer can take seconds, and
    /// this drains them one after another. Never call it on a thread the
    /// interface is waiting on.
    pub fn forget_all_volumes(&self) -> Result<(), IndexError> {
        state::clear_every_index().map_err(Into::into)
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

    /// How many bytes every index database occupies on disk right now, across all
    /// volumes and including WAL sidecars.
    ///
    /// Reads the files, not the pool, because that's the only honest answer: a
    /// database a search's walk built is on disk with nothing registered for it
    /// after a restart, and so is the index of a drive whose indexing the user
    /// turned off. Both are disk a person is entitled to see and reclaim, and
    /// [`status`](Self::status) reports neither. `0` means the index holds
    /// nothing at all.
    pub fn disk_footprint(&self) -> u64 {
        crate::indexing::resources::retention::total_index_db_bytes()
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

    /// What the index has under one directory, without touching the disk.
    ///
    /// `None` when the path isn't indexed, which includes a directory that HAS a
    /// row but that nothing has listed: rows under it are a lower bound (FSEvents
    /// verification and the cover walk's chain materialization both create that
    /// shape), and there is nothing in a `Vec<EntryRow>` for a caller to read that
    /// caveat off.
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

    // ── Coverage: what the index can't answer for yet ────────────────
    //
    // Two calls, one question. A caller that wants a complete answer over a scope
    // asks what the index doesn't cover, serves the rest from the index, and
    // walks the difference. The covered half is never enumerated: the two are
    // complementary over the same subtree, so running a query over the scope
    // unfiltered already yields exactly the covered rows.

    /// What a scope still needs walked before the index alone can answer for it.
    ///
    /// The shallowest directories nothing has listed, plus the ones a walk has
    /// already tried and can't read, plus the token saying which state of the
    /// index the answer describes. A volume with no index reports the scope
    /// itself, which is what a cold drive needs.
    ///
    /// Cheap by design: the descent stops at every covered subtree instead of
    /// walking into it, so a fully indexed drive answers in one row lookup.
    ///
    /// The answer also says which of those directories a walk is covering right
    /// now ([`CoverageMap::being_walked`]), which the database can't know: only
    /// one walk may have a patch of ground, so a caller whose whole frontier is
    /// already somebody's has nothing to walk and everything to gain by waiting.
    /// A running SCAN is deliberately not that: it holds the volume without
    /// covering any particular root, and its ground comes back on its own terms
    /// rather than a walk's (`cover::ground_being_walked`).
    pub fn coverage(
        &self,
        volume_id: &str,
        scope_path: &str,
        dimension: CoverageDimension,
    ) -> Result<CoverageMap, IndexError> {
        let mut map = crate::indexing::read::coverage::coverage_on_volume(volume_id, scope_path, dimension)?;
        map.being_walked = cover::ground_being_walked(volume_id, &map.frontier);
        Ok(map)
    }

    /// Which state of a volume's index is current right now.
    ///
    /// Take one when you load a snapshot of the index, and compare it against the
    /// token a [`coverage`](Self::coverage) answer carries: while they match, the
    /// snapshot holds every row that answer called covered. When they stop
    /// matching, something wrote rows, and the two halves have to be re-asked
    /// together or the second query silently returns fewer results than the first.
    pub fn coverage_token(&self, volume_id: &str) -> CoverageToken {
        crate::indexing::read::coverage::coverage_token_on_volume(volume_id)
    }

    /// Walk the frontier a [`coverage`](Self::coverage) answer named, filling it
    /// into the index and handing back what it finds as it goes.
    ///
    /// The other half of the same question: `coverage` says what the index can't
    /// answer for, this makes it able to. Every row goes into the volume's real
    /// index through its one writer, so the work is durable — a walk that gets
    /// cancelled still leaves every directory it read covered, and the next walk
    /// over the same scope has that much less to do.
    ///
    /// Cancel it through the `cancel` token you pass in, from wherever you like:
    /// the walk handle itself can't leave the thread that reads its batches, and
    /// the thing that decides a walk should stop (a closing dialog, a quitting
    /// app) is rarely that thread. Dropping the handle does NOT stop it: a
    /// superseded query keeps its walk, because walking is coverage work and
    /// matching is query work. Ground another walk on the same volume is already
    /// covering is left to that walk and reported as
    /// [`covered_by_another_walk`](CoverWalk::covered_by_another_walk); its rows
    /// reach the same index either way.
    ///
    /// A volume with no index gets one, built for exactly this and nothing more:
    /// no full scan of the drive, no watcher, just somewhere for the walk to
    /// write. Every kind is walkable — a local disk through the guarded walker, a
    /// share or a phone (or whatever backend comes next) through its `Volume` —
    /// so [`NotIndexed`](IndexError::NotIndexed) means only that nothing is
    /// mounted under that id to build one for.
    pub fn cover(
        &self,
        volume_id: &str,
        frontier: Vec<String>,
        dimension: CoverageDimension,
        cancel: CancellationToken,
    ) -> Result<CoverWalk, IndexError> {
        let context = cover::context_for_walk(volume_id).map_err(|e| match e {
            // Nothing to walk into and nothing built: from out here that reads
            // exactly like a drive that was never indexed, which is what it is.
            cover::NoCoverContext::NotMounted => IndexError::NotIndexed {
                volume_id: volume_id.to_string(),
            },
            // The volume IS being indexed, so `NotIndexed` would be a lie. No
            // caller acts on the difference yet; the walk simply doesn't run,
            // because the scan already covers what it would have walked.
            other => IndexError::Internal(Diagnostic(format!("can't walk '{volume_id}': {other}"))),
        })?;
        Ok(cover::start(context, frontier, dimension, cancel))
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
