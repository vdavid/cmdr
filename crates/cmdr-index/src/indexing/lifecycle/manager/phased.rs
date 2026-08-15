//! The third way a manager puts a volume in sync: cover it in phases.
//!
//! `start_scan` walks the whole volume at once and `start_replay` skips the walk
//! entirely; this covers the volume in pieces, in the order its owner cares about,
//! and never truncates anything. It is what a volume with no completed scan gets.
//!
//! It also holds `cover_or_scan`, the ONE door every "walk this volume whole"
//! caller goes through, because the choice between the two answers is the same
//! choice `resume_or_scan` makes and belongs beside it.
//!
//! Two things happen here that `start_scan` does for itself, and they are the
//! reason this is a file rather than three lines in `resume_or_scan`:
//!
//! - **The database has to be prepared for a walk.** `prepare_database_for_a_walk`
//!   does it for a search-driven start, on its own write connection, before any
//!   writer exists. By here the writer is live, so a second write connection is
//!   exactly the thing the single-writer rule forbids — the same work goes through
//!   writer messages instead.
//! - **The first walk waits for the branch-watch resume.** Starting it earlier
//!   silently costs the epoch bump that makes last session's rows read as stale.

use super::*;
use crate::indexing::lifecycle::phases::{self, MachineContext};
use crate::indexing::scanner::{exclusion_policy_stamp_message, index_predates_exclusion_policy};
use crate::indexing::watch::branches;

/// Where a volume sits between "the launch route handed it to the phase machine"
/// and "the machine is running".
///
/// ⚠️ **Every state but `No` counts as WORK** (`phases_have_work`), the window
/// [`PhaseStart::run`] spends off the registry lock included. The driver thread
/// can already be walking in there while `phases` is still `None`, and a scan
/// entry that read "no work" would truncate the index underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing::lifecycle) enum PendingPhases {
    /// Nothing owed: this volume was never the machine's, or its machine is the
    /// `phases` handle.
    No,
    /// The machine covers this volume and nothing has started it yet. The start
    /// waits for `resume_branch_watch`, or last session's covered ground comes
    /// back watched-but-never-epoch-bumped, rendering as current when nothing
    /// verified it (`state/startup.rs`).
    Owed,
    /// `state::start_pending_phases` is standing the machine up right now, off
    /// the registry lock.
    BeingStarted,
}

/// Whether the index a phased start finds is one the machine can add to.
///
/// The launch route decides this (`manager/launch_route.rs`); every other entry
/// point keeps what is there, because by then the launch has already answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing::lifecycle) enum PhasedStart {
    /// Add to what is already there. Every never-completed index except the one
    /// below: a fresh install, a phased partial, a volume a search walked.
    KeepTheRows,
    /// Throw the index away first. Rows nothing can account for.
    RebuildFirst,
}

impl IndexManager {
    /// Note that this volume is the phase machine's, and give its database what a
    /// walk needs before anything walks it.
    ///
    /// ⚠️ **Without the exclusion-policy stamp nothing here ever converges, and it
    /// fails silently.** An absent stamp makes `index_predates_exclusion_policy`
    /// answer yes, and every coverage query then short-circuits to "the whole scope
    /// is frontier": the frontier never shrinks, every root after the first walk is
    /// non-virgin, and each one takes the serial repair. It looks exactly like the
    /// stitch not working. `prepare_database_for_a_walk` runs only for a
    /// `WriterOnly` start and `start_scan`'s stamp only on its non-reconcile branch,
    /// and a phased start is neither.
    pub(in crate::indexing::lifecycle) fn register_a_phased_start(&mut self, start: PhasedStart) {
        let truncated = self.rebuild_if_the_machine_cant_add_to_this_index(start);
        let empty = IndexStore::get_entry_count(self.store.read_conn()).is_ok_and(|count| count <= 1);

        // The epoch every directory a walk lists is stamped with. Sent to SEED an
        // absent key (there is no seed-only message) and after a truncate, and ❌
        // never otherwise: on a partially covered index it would mark every row a
        // previous session covered as stale for nothing, and on a volume a search
        // walk is writing to RIGHT NOW it would make that walk's fresh rows read as
        // stale the moment it lands them (the walk read the old epoch on its own
        // connection when it started).
        let epoch_unseeded = IndexStore::get_meta(self.store.read_conn(), crate::indexing::store::CURRENT_EPOCH_KEY)
            .is_ok_and(|value| value.is_none());
        if truncated || epoch_unseeded {
            let _ = self.writer.send(WriteMessage::BumpCurrentEpoch);
        }
        // What lets a reader prefix this index's mount-relative paths back to
        // absolute ones, which is what keeps a walk-built external index readable
        // once the drive is offline.
        let _ = self.writer.send(WriteMessage::UpdateMeta {
            key: "volume_path".to_string(),
            value: self.path_space().volume_root_string(),
        });
        // ⚠️ The stamp's own rule is "❌ send it ONLY right after a `TruncateData`".
        // The legal form of that is `entry_count <= 1 || we-just-truncated`: read as
        // "only after a truncate" it never stamps a fresh install and reinstates the
        // fatal bug above; read as "always" it silently blesses rows written under
        // an older policy. Both misreadings are silent.
        if truncated || empty {
            let _ = self.writer.send(exclusion_policy_stamp_message());
        }
        // Committed before the first walk reads `current_epoch` on its own
        // connection, exactly as a scan start flushes before its walker starts.
        if let Err(e) = tokio::task::block_in_place(|| self.writer.flush_blocking()) {
            log::warn!(
                "Phases: preparing '{}' for a walk may not have landed: {e}",
                self.volume_id
            );
        }
        self.pending_phases = PendingPhases::Owed;
    }

    /// Drop an index the machine can't honestly add to, so the phases fill a clean
    /// one instead of walking on top of it forever. Reports whether it truncated.
    ///
    /// Two reasons, and both are silent if left alone:
    ///
    /// - **The launch found rows nothing can account for** ([`PhasedStart::RebuildFirst`]).
    ///   The rows are real, but with no branch set nothing records which ground they
    ///   cover — so nothing watches it and nothing bumps the epoch for it, and
    ///   resuming into it would render last session's sizes as CURRENT with nothing
    ///   having verified them.
    /// - **The exclusion policy changed.** The stamp records which policy an index's
    ///   rows were written under, and a mismatch means NOTHING in it counts as
    ///   covered. A full scan repairs that by truncating and re-stamping; during the
    ///   phased window nothing else would, so the index would be stranded — every
    ///   phase re-walking the whole scope and never re-stamping.
    ///
    /// The completion markers go with the rows they describe: they are claims about
    /// an index that no longer exists.
    fn rebuild_if_the_machine_cant_add_to_this_index(&mut self, start: PhasedStart) -> bool {
        let populated = IndexStore::get_entry_count(self.store.read_conn()).is_ok_and(|count| count > 1);
        let why = if !populated {
            None
        } else if start == PhasedStart::RebuildFirst {
            Some("it has rows but no record of which ground they cover")
        } else if index_predates_exclusion_policy(self.store.read_conn()) {
            Some("it predates this build's exclusion policy, so nothing in it counts as covered")
        } else {
            None
        };
        let Some(why) = why else {
            return false;
        };
        log::info!("Phases: rebuilding '{}': {why}", self.volume_id);
        let _ = self.writer.send(WriteMessage::TruncateData);
        for key in ["scan_completed_at", phases::HOME_COVERED_AT_KEY] {
            let _ = self.writer.send(WriteMessage::DeleteMeta(key.to_string()));
        }
        // The branch set describes ground that is about to stop existing, and it is
        // what a later launch reads to tell a phased partial from a legacy one.
        branches::clear(&self.volume_id, &self.writer);
        self.branch_watched = false;
        true
    }

    /// Put this volume back in sync by walking it WHOLE: the phase machine while
    /// its first index is still being built, today's full (re)scan once one has
    /// completed.
    ///
    /// **The one door every "walk this volume" caller goes through**, and the
    /// reason a truncating scan can no longer land on a half-built index. Four
    /// callers reach it, and each was its own way to blank one: the per-drive
    /// "Turn on indexing for this drive" button and the FDA-deny start (both
    /// through `start_volume` → `awaits_its_first_scan` → `force_scan`), "Rescan
    /// now" itself, and `perform_registry_rescan` (a coalesced shallow
    /// `MustScanSubDirs`, a replay that couldn't roll forward, an ingestion
    /// backlog). ❌ Don't add a fifth caller that reaches past this into
    /// `start_scan`.
    ///
    /// A rescan during the phased window RESTARTS the machine rather than
    /// truncating: whatever is covered stays covered, and the queue is rebuilt
    /// from the host's current answers plus a coverage query per root, so it picks
    /// up folders the user has come to care about since. A machine that already
    /// has work is left alone — the walk the caller asked for is in flight, which
    /// is what [`ScanStartError::AlreadyScanning`] means everywhere else.
    ///
    /// ⚠️ It only REGISTERS the phased start. The machine is started by
    /// `state::start_pending_phases` once the manager is back in the registry as
    /// `Running`; see that function for why starting it from in here would make
    /// every one of its first walks report "did not run".
    pub(in crate::indexing) fn cover_or_scan(&mut self, scan_trigger: &str) -> Result<(), ScanStartError> {
        if !self.first_index_is_the_machines() {
            return self.start_scan(scan_trigger);
        }
        if self.phases_have_work() {
            return Err(ScanStartError::AlreadyScanning);
        }
        log::info!(
            "'{}' has no completed scan, so '{scan_trigger}' restarts its phases instead of rebuilding it",
            self.volume_id
        );
        self.register_a_phased_start(PhasedStart::KeepTheRows);
        // ⚠️ `perform_registry_rescan` stops the watcher and the live loop before it
        // calls this, expecting the full scan it used to reach to start fresh ones.
        // The machine starts a watcher too, but only from a walk's
        // `begin_branch_coverage` — so a volume whose frontier is already empty
        // would take stock, complete, and spend the rest of the session with
        // NOTHING watching ground it serves as covered. Idempotent: it declines
        // when a watcher is already up or nothing is covered yet. ❌ Not
        // `resuming`: this is the same session, so there is no gap to admit and a
        // bump here would mark rows stale that nothing happened to.
        self.ensure_branch_watch(false);
        Ok(())
    }

    /// Whether this volume's first index is still the phase machine's to build:
    /// a locally-walked volume, no completed scan on record, and the
    /// phased-first-index switch on.
    ///
    /// ❌ Not `awaits_its_first_scan`, which is a REGISTRY question with its own
    /// two documented shapes and is deliberately left alone
    /// (`state/queries.rs`) — re-keying it would make the per-drive enable button
    /// a silent no-op on the volumes it was written to serve.
    fn first_index_is_the_machines(&self) -> bool {
        phases::phased_first_index()
            && self.kind.uses_local_scanner()
            && self
                .store
                .get_index_status()
                .is_ok_and(|status| status.scan_completed_at.is_none())
    }

    /// Whether `resume_or_scan` handed this volume to the phase machine.
    pub(in crate::indexing::lifecycle) fn awaits_its_phases(&self) -> bool {
        self.pending_phases == PendingPhases::Owed
    }

    /// Take the pending phase start off this manager, if it has one: everything
    /// standing the machine up needs, cloned off the handles the manager already
    /// holds.
    ///
    /// ⚠️ **Cheap by contract.** `state::start_pending_phases` calls this with
    /// `INDEX_REGISTRY` held, so ❌ nothing here may read a database, ask the host
    /// anything, or spawn. That is [`PhaseStart::run`]'s job, on the far side of
    /// the guard.
    pub(in crate::indexing::lifecycle) fn take_the_phase_start(&mut self) -> Option<PhaseStart> {
        if self.pending_phases != PendingPhases::Owed {
            return None;
        }
        self.pending_phases = PendingPhases::BeingStarted;
        Some(PhaseStart {
            db_path: self.writer.db_path(),
            context: MachineContext {
                volume_id: self.volume_id.clone(),
                volume_root: self.volume_root.clone(),
                space: self.path_space(),
                writer: self.writer.clone(),
                events: Arc::clone(&self.events),
                freshness: Arc::clone(&self.freshness),
                cancel: self.volume_cancel.child_token(),
            },
        })
    }

    /// Take the machine [`PhaseStart::run`] built, and stop reporting the start as
    /// pending.
    ///
    /// Hands the machine BACK when this manager isn't the one that asked for it: a
    /// stop and a fresh start can both land in the window off the lock, and
    /// overwriting the new manager's machine would leave the old one walking with
    /// nothing able to stop it. The caller stops what it gets back.
    pub(in crate::indexing::lifecycle) fn hold_the_started_phases(
        &mut self,
        started: StartedPhases,
    ) -> Option<StartedPhases> {
        if self.pending_phases != PendingPhases::BeingStarted {
            return Some(started);
        }
        self.pending_phases = PendingPhases::No;
        self.scan_calibration = Some(started.calibration);
        self.phases = Some(started.handle);
        None
    }

    /// Whether the machine still has work: the start owed or in flight, a phase
    /// queued, or one running.
    ///
    /// ⚠️ The question every scan entry asks, and ❌ never "is a walk running right
    /// now". A walk's flag goes false between frontier roots, and the stitch
    /// deliberately produces 50–150 of them per phase; a truncating rescan landing
    /// in one of those gaps blanks an index the machine is half way through
    /// building.
    ///
    /// ⚠️ The pending state is half the answer, and ❌ not a nicety. Between the
    /// launch route handing this volume over and the machine's handle landing
    /// there is no handle to ask, and the second half of that window has a driver
    /// thread already walking in it ([`PendingPhases`]).
    pub(in crate::indexing::lifecycle) fn phases_have_work(&self) -> bool {
        self.pending_phases != PendingPhases::No || self.phases.as_ref().is_some_and(|phases| phases.has_work())
    }

    /// Whether a phase walk is reading the disk right now. What suppresses the
    /// per-navigation verifier, exactly as a full scan does.
    pub(in crate::indexing::lifecycle) fn phases_are_walking(&self) -> bool {
        self.phases.as_ref().is_some_and(|phases| phases.is_walking())
    }

    /// Stop covering this volume. Whatever is covered stays covered and watched,
    /// and the next launch picks the rest up.
    pub(in crate::indexing::lifecycle) fn stop_phases(&mut self) {
        self.pending_phases = PendingPhases::No;
        if let Some(phases) = self.phases.as_ref() {
            phases.stop();
        }
    }
}

/// A phase-machine start, lifted off its manager and waiting for the registry
/// lock to be released.
pub(in crate::indexing::lifecycle) struct PhaseStart {
    /// Where the calibration is read from. The manager's own read connection is
    /// behind the lock this start exists to get out from under.
    db_path: PathBuf,
    context: MachineContext,
}

impl PhaseStart {
    /// Stand the machine up, and hand back what its manager holds on to.
    ///
    /// ⚠️ **Everything blocking about a start is here**, and it runs with NO
    /// registry lock held: the calibration read, the host's `open_listings` ask
    /// inside `phases::start`, the reporter, and the driver thread. That thread's
    /// first act is to resolve a write context THROUGH the registry
    /// (`cover::context_for_walk`), so a start under the lock would make its first
    /// walk wait on us for as long as the slowest of those takes.
    pub(in crate::indexing::lifecycle) fn run(self) -> StartedPhases {
        // The static half of the progress shape, for a late-joining window. ❌ No
        // `volume_used_bytes`: a phased run has no knowable total until its last
        // phase, and the design principles forbid a progress bar parked at 100%, so
        // the tier this feeds stays "phase, live count, elapsed" throughout.
        let calibration_set = IndexStore::open_read_connection(&self.db_path)
            .and_then(|conn| IndexStore::read_scan_calibration_set(&conn))
            .unwrap_or_else(|e| {
                log::warn!("Phases: couldn't read prior scan calibration: {e}");
                crate::indexing::store::ScanCalibrationSet::default()
            });
        let run_kind = ScanRunKind::classify(false, calibration_set.any.total_entries);
        StartedPhases {
            calibration: ScanCalibration {
                prior: calibration_set.for_kind(run_kind.calibration_kind()),
                volume_used_bytes: None,
                run_kind,
            },
            handle: phases::start(self.context),
        }
    }
}

/// A running phase machine on its way back to the manager that asked for it.
pub(in crate::indexing::lifecycle) struct StartedPhases {
    handle: phases::PhaseHandle,
    calibration: ScanCalibration,
}

impl StartedPhases {
    /// Stop a machine whose manager went away while it was being stood up.
    pub(in crate::indexing::lifecycle) fn stop(&self) {
        self.handle.stop();
    }
}
