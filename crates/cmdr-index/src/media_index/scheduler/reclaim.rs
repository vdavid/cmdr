//! Reclaim-space: the single-source stored-coverage split and the
//! user-explicit prune that frees the disk left behind when the user narrows the
//! image-index depth slider.
//!
//! Split out of [`super`] (the coordinator, bus wiring, and enrichment passes) because
//! it's a self-contained concern hung off [`MediaScheduler`]: one function partitions a
//! volume's stored rows into surviving vs doomed by the SAME precedence enrichment uses,
//! and the prune deletes the doomed set through the volume's ONE writer thread. The
//! reclaim commands (`commands.rs`) and the per-volume `keptCount` both call [`stored_coverage`],
//! so the three user-facing quantities can never disagree. Full rationale (the
//! single-source arithmetic, the partition rule, why the writer thread is the race
//! guarantee): [`scheduler/DETAILS.md`](DETAILS.md) § Reclaim space.
//!
//! [`stored_coverage`]: MediaScheduler::stored_coverage

use std::collections::HashSet;

use crate::media_index::coverage::FolderScores;
use crate::media_index::gate::IndexScope;
use crate::media_index::{coverage, network, store, vector};

use super::MediaScheduler;

/// The threshold-aware split of a volume's STORED media rows plus the drive-index
/// coverage count — all from ONE computation ([`MediaScheduler::stored_coverage`]) so the
/// reclaim preview, the prune, and the per-volume state can never disagree (the
/// single-source arithmetic).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredCoverage {
    /// Stored rows INSIDE current coverage (they stay). `surviving_stored +
    /// doomed_stored` is the total stored-row count (the partition invariant).
    pub surviving_stored: u64,
    /// Stored rows OUTSIDE current coverage — the reclaim prune's "delete N" AND the
    /// per-volume `keptCount` (the same set).
    pub doomed_stored: u64,
    /// Drive-index qualifying images in covered folders — what WOULD be indexed (the
    /// slider-preview number), a DIFFERENT thing from `surviving_stored` (a
    /// vanished-but-not-yet-GC'd file or a half-enriched folder makes them disagree).
    pub covered_qualifying: u64,
    /// The doomed rows' stored paths, handed to the writer as one serialized prune unit.
    pub doomed_paths: Vec<String>,
}

/// The counts-only stored-coverage split (no `doomed_paths` allocation): what the
/// per-volume state poll needs. Same three quantities as [`StoredCoverage`], shared
/// through the one canonical survival rule.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoredCoverageCounts {
    /// Stored rows INSIDE current coverage.
    pub surviving_stored: u64,
    /// Stored rows OUTSIDE current coverage (the `keptCount`).
    pub doomed_stored: u64,
    /// Drive-index qualifying images in covered folders (the slider-preview number), or
    /// `None` when the coverage counts aren't cached yet. `None` rather than `0` because
    /// this reader NEVER pays the cold walk (see
    /// [`stored_coverage_counts`](MediaScheduler::stored_coverage_counts)), and a
    /// confident `0` would read as "nothing to index" on a volume full of photos.
    pub covered_qualifying: Option<u64>,
}

/// What a reclaim prune did: the rows deleted and the space it gave back.
#[derive(Debug, PartialEq, Eq)]
pub struct PruneOutcome {
    /// The `media_status` rows removed (the images the user reclaimed).
    pub deleted_rows: u64,
    /// The content bytes the prune freed (OCR text + tags + embeddings; an "about"
    /// estimate — a `VACUUM` reclaims at least this on disk). `None` when the rows left but
    /// the `VACUUM` didn't run: the file still holds that space until the volume's next
    /// pass reclaims it, so no number would be honest.
    pub freed_bytes: Option<u64>,
}

impl PruneOutcome {
    /// Nothing was doomed, so nothing left and nothing is owed.
    const NOTHING: Self = Self {
        deleted_rows: 0,
        freed_bytes: Some(0),
    };
}

/// Why a reclaim prune removed none of the rows it was asked to. Every doomed row is still
/// stored either way, so a caller may neither report space as freed nor say the entries
/// were already cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneFailure {
    /// The volume's writer wouldn't start (its `media.db` wouldn't open for writing).
    WriterUnavailable,
    /// SQLite refused the delete and rolled it back (a full disk, a locked database).
    DeleteFailed,
    /// The volume's `media.db` couldn't be read, so the prune couldn't tell what to delete.
    StoreUnreadable,
}

/// A volume's `media.db` exists but couldn't be read (corrupt, or locked past the busy
/// timeout). Distinct from a volume that was never enriched, which has no file and
/// honestly stores nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreUnreadable;

/// The stored (index-relative) paths in `volume_id`'s `media.db`: empty when the volume was
/// never enriched (no file), [`StoreUnreadable`] when the file is there but won't read.
fn read_stored_paths(data_dir: &std::path::Path, volume_id: &str) -> Result<Vec<String>, StoreUnreadable> {
    let db_path = store::media_db_path(data_dir, volume_id);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    store::open_read_connection(&db_path)
        .and_then(|conn| store::read_status_paths(&conn))
        .map_err(|e| {
            log::warn!(target: "media_index", "stored coverage: media.db for '{volume_id}' didn't read: {e}");
            StoreUnreadable
        })
}

impl MediaScheduler {
    /// The folder scores the stored-row partition should run against, for `volume_id`
    /// under `scope` — the ONE place both stored-coverage entry points resolve them, so
    /// they can't disagree about when a partition is safe.
    ///
    /// The automatic scope NEEDS importance: an unscored volume returns `None` and the
    /// caller reports pending rather than proposing a destructive count off scores it
    /// doesn't have. The narrow scope doesn't consult importance at all, so it
    /// partitions safely against an EMPTY score map — a reclaim offer (and the kept-rows
    /// line) stays available on a volume importance has never touched, which is exactly
    /// the volume a user narrowing their scope is most likely to be looking at.
    fn partition_scores(&self, volume_id: &str, scope: IndexScope) -> Option<FolderScores> {
        if !scope.consults_importance() {
            return Some(FolderScores::empty());
        }
        coverage::importance_scores(&self.data_dir, volume_id, None)
    }

    /// The single-source stored-coverage split for `volume_id` at `threshold`:
    /// how many stored `media.db` rows fall INSIDE the current setting
    /// (`surviving_stored`) vs OUTSIDE it (`doomed_stored` + the `doomed_paths` a reclaim
    /// prune would delete), plus `covered_qualifying` (the drive-index qualifying images
    /// in covered folders — the slider-preview number, a DIFFERENT quantity from stored
    /// rows). BOTH the reclaim commands and the per-volume `keptCount` call this, so the three
    /// numbers can never disagree.
    ///
    /// `mount_root` maps a stored (index-relative) path into OS space for the
    /// override/exclude config lookup ("/" on a local volume, the mount root on a network
    /// one), exactly as enrichment does; importance keys on the index identity directly.
    /// Returns `Ok(None)` when the partition can't be computed safely (the automatic scope on a
    /// volume importance hasn't scored — transient), so the caller reports pending rather
    /// than proposing a destructive count. See [`partition_scores`](Self::partition_scores).
    /// [`StoreUnreadable`] when the volume's `media.db` won't read: ❌ never an empty
    /// partition, which the reclaim panel would voice as "nothing to delete".
    /// The selection reuses [`coverage::partition_stored`] (the enrichment precedence) and
    /// the [`coverage`] cache (the slider's qualifying counts), never a second derivation.
    pub fn stored_coverage(
        &self,
        volume_id: &str,
        mount_root: &str,
        threshold: f64,
        scope: IndexScope,
    ) -> Result<Option<StoredCoverage>, StoreUnreadable> {
        let Some(scores) = self.partition_scores(volume_id, scope) else {
            return Ok(None);
        };

        // The stored-row paths (empty when the volume was never enriched).
        let stored = read_stored_paths(&self.data_dir, volume_id)?;

        // Override/exclude are OS-path keyed; map each stored (index) path into OS space.
        let config = network::config::snapshot();
        let mount_root = mount_root.to_string();
        let is_override =
            |index_path: &str| config.covers(volume_id, &network::fetch::os_join(&mount_root, index_path));
        let is_excluded = |index_path: &str| config.is_excluded(&network::fetch::os_join(&mount_root, index_path));
        let partition = coverage::partition_stored(&stored, &scores, threshold, scope, &is_override, &is_excluded);

        // Covered qualifying reuses the slider-preview cache path (single-source).
        let covered_qualifying = coverage::get_or_build(volume_id)
            .map(|counts| coverage::covered_in_scope(&counts, &scores, threshold, scope, &is_override).1)
            .unwrap_or(0);

        Ok(Some(StoredCoverage {
            surviving_stored: partition.surviving,
            doomed_stored: partition.doomed.len() as u64,
            covered_qualifying,
            doomed_paths: partition.doomed,
        }))
    }

    /// The counts-only stored-coverage split for `volume_id` at `threshold`:
    /// `surviving_stored` / `doomed_stored` (= `keptCount`) / `covered_qualifying`,
    /// WITHOUT allocating the doomed-path list. The `media_index_volume_state` poll
    /// calls this at launch and every few seconds while the settings panel is open, so it
    /// avoids the 200k-path `Vec` [`stored_coverage`](Self::stored_coverage) builds for a
    /// prune. It reuses the ONE canonical survival rule ([`coverage::stored_row_survives`])
    /// and the [`coverage`] cache, so its numbers can never disagree with the reclaim
    /// preview. `None` when importance hasn't scored the volume (the partition isn't safe
    /// yet) or its `media.db` won't read: either way the number is unknown, never `0`.
    ///
    /// ❌ This is a POLL, so it reads the coverage cache and never builds it
    /// ([`coverage::cached`], not `get_or_build`) — a cold build here is a whole-index walk
    /// whose transient heap ran a launch to 50 GB. `covered_qualifying` is therefore
    /// `None` until something warms the cache; report that as unknown, never as `0`.
    pub fn stored_coverage_counts(
        &self,
        volume_id: &str,
        mount_root: &str,
        threshold: f64,
        scope: IndexScope,
    ) -> Option<StoredCoverageCounts> {
        let scores = self.partition_scores(volume_id, scope)?;
        let stored = read_stored_paths(&self.data_dir, volume_id).ok()?;

        let config = network::config::snapshot();
        let mount_root = mount_root.to_string();
        let is_override =
            |index_path: &str| config.covers(volume_id, &network::fetch::os_join(&mount_root, index_path));
        let is_excluded = |index_path: &str| config.is_excluded(&network::fetch::os_join(&mount_root, index_path));

        let mut surviving_stored = 0u64;
        let mut doomed_stored = 0u64;
        for path in &stored {
            if coverage::stored_row_survives(path, &scores, threshold, scope, &is_override, &is_excluded) {
                surviving_stored += 1;
            } else {
                doomed_stored += 1;
            }
        }

        // CACHED only: this runs from the `media_index_volume_state` poll, so it must never
        // pay the cold whole-index walk (`coverage::cached`). The counts arrive from the
        // passes, or from a user-initiated settings read; until then the number is honestly
        // unknown.
        let covered_qualifying = coverage::cached(volume_id)
            .map(|counts| coverage::covered_in_scope(&counts, &scores, threshold, scope, &is_override).1);

        Some(StoredCoverageCounts {
            surviving_stored,
            doomed_stored,
            covered_qualifying,
        })
    }

    /// The freed-byte estimate for a doomed path set: the content bytes those rows hold
    /// in `media.db` (OCR text, tags, and embeddings), the "about" figure the reclaim
    /// preview shows and the prune reports (a `VACUUM` reclaims at least this on disk),
    /// or `0` for an empty set or an unopenable DB.
    pub fn estimate_doomed_bytes(&self, volume_id: &str, doomed_paths: &[String]) -> u64 {
        if doomed_paths.is_empty() {
            return 0;
        }
        let db_path = store::media_db_path(&self.data_dir, volume_id);
        let set: HashSet<String> = doomed_paths.iter().cloned().collect();
        store::open_read_connection(&db_path)
            .ok()
            .and_then(|conn| store::sum_bytes_for_paths(&conn, &set).ok())
            .unwrap_or(0)
    }

    /// Prune the stored rows OUTSIDE the current setting for `volume_id` at `threshold` and
    /// `scope` (reclaim): compute the doomed set via [`stored_coverage`], estimate the
    /// content bytes it frees, delete it through the volume's ONE writer thread (the
    /// serialization guarantee — the prune and any concurrent pass can't interleave
    /// mid-batch, and a concurrent pass only enriches ABOVE-threshold rows, a disjoint
    /// set), `VACUUM` to reclaim the pages, and drop the vector + coverage caches. A
    /// USER-EXPLICIT deletion: it derives ONLY from settings state, so like the privacy
    /// retro-delete it needs no completed-scan edge (see `../DETAILS.md` § The GC safety argument).
    ///
    /// Answers the rows deleted and the freed-byte estimate, all zeros when the partition
    /// isn't safe or nothing is doomed. An unreadable store, a writer that won't start, or a
    /// delete SQLite refuses is a [`PruneFailure`], never zero rows. A `VACUUM` that fails after the delete landed
    /// reports `freed_bytes: None` and owes the volume a `VACUUM` its next pass runs.
    ///
    /// [`stored_coverage`]: MediaScheduler::stored_coverage
    pub fn prune_below_threshold(
        &self,
        volume_id: &str,
        mount_root: &str,
        threshold: f64,
        scope: IndexScope,
    ) -> Result<PruneOutcome, PruneFailure> {
        let Some(coverage) = self
            .stored_coverage(volume_id, mount_root, threshold, scope)
            .map_err(|StoreUnreadable| PruneFailure::StoreUnreadable)?
        else {
            return Ok(PruneOutcome::NOTHING);
        };
        if coverage.doomed_paths.is_empty() {
            return Ok(PruneOutcome::NOTHING);
        }
        let db_path = store::media_db_path(&self.data_dir, volume_id);

        // The freed-byte estimate over the doomed set, BEFORE deleting (same content-byte
        // method the reclaim preview reports, so the "free about X" and "Freed X" numbers
        // agree). `VACUUM` reclaims at least this much on disk.
        let freed_estimate = self.estimate_doomed_bytes(volume_id, &coverage.doomed_paths);

        let writer = self.writers.writer_for(&self.data_dir, volume_id).map_err(|e| {
            log::warn!(target: "media_index", "reclaim prune: writer for '{volume_id}' failed: {e}");
            PruneFailure::WriterUnavailable
        })?;
        let deleted = writer.prune_paths(coverage.doomed_paths).map_err(|e| {
            log::warn!(target: "media_index", "reclaim prune on '{volume_id}' didn't land: {e}");
            PruneFailure::DeleteFailed
        })?;
        if deleted == 0 {
            return Ok(PruneOutcome::NOTHING);
        }

        // Reclaim the pages, then drop the derived caches so a later search / slider
        // preview rebuilds honestly. The ANN flush lands the buffered key removals first,
        // so pruned images stop being ANN-reachable too (plan M6).
        let _ = writer.flush_ann_index();
        let vacuum = writer.vacuum();
        if let Err(e) = &vacuum {
            log::warn!(
                target: "media_index",
                "VACUUM after the reclaim prune on '{volume_id}' failed ({e}); retrying on its next pass"
            );
            self.owe_vacuum(volume_id);
        }
        vector::cache::invalidate(&db_path);
        coverage::invalidate(volume_id);
        log::info!(
            target: "media_index",
            "reclaim prune on '{volume_id}' at threshold {threshold}: {} removed (~{})",
            cmdr_fs::pluralize::pluralize(deleted as u64, "row"),
            cmdr_fs::pluralize::pluralize(freed_estimate, "byte")
        );
        Ok(PruneOutcome {
            deleted_rows: deleted as u64,
            freed_bytes: freed_bytes_after_vacuum(freed_estimate, &vacuum),
        })
    }
}

/// The bytes a prune may report as freed: its estimate once `VACUUM` gave the pages back,
/// and no number at all when it didn't (the file is exactly as large as before).
pub(super) fn freed_bytes_after_vacuum(estimate: u64, vacuum: &Result<(), store::MediaStoreError>) -> Option<u64> {
    vacuum.as_ref().ok().map(|()| estimate)
}
