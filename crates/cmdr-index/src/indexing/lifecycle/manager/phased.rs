//! The third way a manager puts a volume in sync: cover it in phases.
//!
//! `start_scan` walks the whole volume at once and `start_replay` skips the walk
//! entirely; this covers the volume in pieces, in the order its owner cares about,
//! and never truncates anything. It is what a volume with no completed scan gets.
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
    pub(super) fn register_a_phased_start(&mut self) {
        let truncated = self.rebuild_if_this_build_cant_trust_the_index();
        let empty = IndexStore::get_entry_count(self.store.read_conn()).is_ok_and(|count| count <= 1);

        // The epoch every directory a walk lists is stamped with. Only when there is
        // nothing to make stale: `BumpCurrentEpoch` seeds an absent key (there is no
        // seed-only message), but on a partially covered index it would ALSO mark
        // every row a previous session covered as stale, for nothing.
        if truncated || empty {
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
        self.phases_pending = true;
    }

    /// Drop an index whose coverage this build refuses to trust, so the phases fill
    /// a clean one instead of walking on top of it forever.
    ///
    /// The exclusion-policy stamp records which policy an index's rows were written
    /// under, and a mismatch means NOTHING in it counts as covered. A full scan
    /// repairs that by truncating and re-stamping; during the phased window nothing
    /// would, so the index would be stranded — every phase re-walking the whole
    /// scope and never re-stamping. Reports whether it truncated.
    ///
    /// The completion markers go with the rows they describe: they are claims about
    /// an index that no longer exists.
    fn rebuild_if_this_build_cant_trust_the_index(&mut self) -> bool {
        let stale = IndexStore::get_entry_count(self.store.read_conn()).is_ok_and(|count| count > 1)
            && index_predates_exclusion_policy(self.store.read_conn());
        if !stale {
            return false;
        }
        log::info!(
            "Phases: '{}' predates this build's exclusion policy, so nothing in it counts as covered; rebuilding it",
            self.volume_id
        );
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

    /// Whether `resume_or_scan` handed this volume to the phase machine.
    pub(in crate::indexing::lifecycle) fn awaits_its_phases(&self) -> bool {
        self.phases_pending
    }

    /// Start the machine, if `resume_or_scan` said this volume is its to cover.
    ///
    /// Called after `resume_branch_watch`, never before: that is where a resumed
    /// volume's covered ground gets its watcher back AND, when the journal gap is
    /// too wide to replay, the epoch bump that makes those rows render as stale.
    /// The machine's first walk starts a watcher of its own, and
    /// `ensure_branch_watch` declines when one is already running — so an earlier
    /// start would take the bump with it.
    pub(in crate::indexing::lifecycle) fn start_phases(&mut self) {
        if !self.phases_pending {
            return;
        }
        self.phases_pending = false;

        // The static half of the progress shape, for a late-joining window. ❌ No
        // `volume_used_bytes`: a phased run has no knowable total until its last
        // phase, and the design principles forbid a progress bar parked at 100%, so
        // the tier this feeds stays "phase, live count, elapsed" throughout.
        let calibration_set = IndexStore::read_scan_calibration_set(self.store.read_conn()).unwrap_or_else(|e| {
            log::warn!("Phases: couldn't read prior scan calibration: {e}");
            crate::indexing::store::ScanCalibrationSet::default()
        });
        let run_kind = ScanRunKind::classify(false, calibration_set.any.total_entries);
        self.scan_calibration = Some(ScanCalibration {
            prior: calibration_set.for_kind(run_kind.calibration_kind()),
            volume_used_bytes: None,
            run_kind,
        });

        self.phases = Some(phases::start(MachineContext {
            volume_id: self.volume_id.clone(),
            volume_root: self.volume_root.clone(),
            space: self.path_space(),
            writer: self.writer.clone(),
            events: Arc::clone(&self.events),
            freshness: Arc::clone(&self.freshness),
            cancel: self.volume_cancel.child_token(),
        }));
    }

    /// Whether the machine still has work: a phase queued, or one running.
    ///
    /// ⚠️ The question every scan entry asks, and ❌ never "is a walk running right
    /// now". A walk's flag goes false between frontier roots, and the stitch
    /// deliberately produces 50–150 of them per phase; a truncating rescan landing
    /// in one of those gaps blanks an index the machine is half way through
    /// building.
    pub(in crate::indexing::lifecycle) fn phases_have_work(&self) -> bool {
        self.phases.as_ref().is_some_and(|phases| phases.has_work())
    }

    /// Whether a phase walk is reading the disk right now. What suppresses the
    /// per-navigation verifier, exactly as a full scan does.
    pub(in crate::indexing::lifecycle) fn phases_are_walking(&self) -> bool {
        self.phases.as_ref().is_some_and(|phases| phases.is_walking())
    }

    /// Stop covering this volume. Whatever is covered stays covered and watched,
    /// and the next launch picks the rest up.
    pub(in crate::indexing::lifecycle) fn stop_phases(&mut self) {
        self.phases_pending = false;
        if let Some(phases) = self.phases.as_ref() {
            phases.stop();
        }
    }
}
