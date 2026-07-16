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
//! guarantee): [`media_index/DETAILS.md`](../DETAILS.md) § Reclaim space.
//!
//! [`stored_coverage`]: MediaScheduler::stored_coverage

use std::collections::HashSet;

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
    /// Drive-index qualifying images in covered folders (the slider-preview number).
    pub covered_qualifying: u64,
}

/// What a reclaim prune did: the rows deleted and the freed-byte estimate.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PruneOutcome {
    /// The `media_status` rows removed (the images the user reclaimed).
    pub deleted_rows: u64,
    /// The content bytes the prune freed (OCR text + tags + embeddings; an "about"
    /// estimate — a `VACUUM` reclaims at least this on disk).
    pub freed_bytes: u64,
}

impl MediaScheduler {
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
    /// Returns `None` when importance hasn't scored the volume (importance's scoring makes that transient) —
    /// the partition can't be computed safely, so the caller reports pending rather than
    /// proposing a destructive count. The selection reuses [`coverage::partition_stored`]
    /// (the enrichment precedence) and the [`coverage`] cache (the slider's qualifying
    /// counts), never a second derivation.
    pub fn stored_coverage(&self, volume_id: &str, mount_root: &str, threshold: f64) -> Option<StoredCoverage> {
        // Importance must be scored to partition safely.
        let scores = coverage::importance_scores(&self.data_dir, volume_id)?;

        // The stored-row paths (empty when the volume was never enriched).
        let db_path = store::media_db_path(&self.data_dir, volume_id);
        let stored: Vec<String> = store::open_read_connection(&db_path)
            .ok()
            .and_then(|conn| store::read_status_paths(&conn).ok())
            .unwrap_or_default();

        // Override/exclude are OS-path keyed; map each stored (index) path into OS space.
        let config = network::config::snapshot();
        let mount_root = mount_root.to_string();
        let is_override =
            |index_path: &str| config.covers(volume_id, &network::fetch::os_join(&mount_root, index_path));
        let is_excluded = |index_path: &str| config.is_excluded(&network::fetch::os_join(&mount_root, index_path));
        let partition = coverage::partition_stored(&stored, &scores, threshold, &is_override, &is_excluded);

        // Covered qualifying reuses the slider-preview cache path (single-source).
        let covered_qualifying = coverage::get_or_build(volume_id)
            .map(|counts| coverage::covered_for_volume(&counts, &scores, threshold).1)
            .unwrap_or(0);

        Some(StoredCoverage {
            surviving_stored: partition.surviving,
            doomed_stored: partition.doomed.len() as u64,
            covered_qualifying,
            doomed_paths: partition.doomed,
        })
    }

    /// The counts-only stored-coverage split for `volume_id` at `threshold`:
    /// `surviving_stored` / `doomed_stored` (= `keptCount`) / `covered_qualifying`,
    /// WITHOUT allocating the doomed-path list. The `media_index_volume_state` poll
    /// calls this every few seconds while the settings panel is open, so it avoids the
    /// 200k-path `Vec` [`stored_coverage`](Self::stored_coverage) builds for a prune. It
    /// reuses the ONE canonical survival rule ([`coverage::stored_row_survives`]) and the
    /// [`coverage`] cache, so its numbers can never disagree with the reclaim preview.
    /// `None` when importance hasn't scored the volume (the partition isn't safe yet).
    pub fn stored_coverage_counts(
        &self,
        volume_id: &str,
        mount_root: &str,
        threshold: f64,
    ) -> Option<StoredCoverageCounts> {
        let scores = coverage::importance_scores(&self.data_dir, volume_id)?;

        let db_path = store::media_db_path(&self.data_dir, volume_id);
        let stored: Vec<String> = store::open_read_connection(&db_path)
            .ok()
            .and_then(|conn| store::read_status_paths(&conn).ok())
            .unwrap_or_default();

        let config = network::config::snapshot();
        let mount_root = mount_root.to_string();
        let is_override =
            |index_path: &str| config.covers(volume_id, &network::fetch::os_join(&mount_root, index_path));
        let is_excluded = |index_path: &str| config.is_excluded(&network::fetch::os_join(&mount_root, index_path));

        let mut surviving_stored = 0u64;
        let mut doomed_stored = 0u64;
        for path in &stored {
            if coverage::stored_row_survives(path, &scores, threshold, &is_override, &is_excluded) {
                surviving_stored += 1;
            } else {
                doomed_stored += 1;
            }
        }

        let covered_qualifying = coverage::get_or_build(volume_id)
            .map(|counts| coverage::covered_for_volume(&counts, &scores, threshold).1)
            .unwrap_or(0);

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

    /// Prune the stored rows OUTSIDE the current setting for `volume_id` at `threshold`
    /// (reclaim): compute the doomed set via [`stored_coverage`], estimate the
    /// content bytes it frees, delete it through the volume's ONE writer thread (the
    /// serialization guarantee — the prune and any concurrent pass can't interleave
    /// mid-batch, and a concurrent pass only enriches ABOVE-threshold rows, a disjoint
    /// set), `VACUUM` to reclaim the pages, and drop the vector + coverage caches. A
    /// USER-EXPLICIT deletion: it derives ONLY from settings state, so like the privacy
    /// retro-delete it needs no completed-scan edge (see `DETAILS.md` § GC safety).
    /// Returns the rows deleted and the freed-byte estimate; a no-op (all zeros) when
    /// importance is unscored or nothing is doomed.
    ///
    /// [`stored_coverage`]: MediaScheduler::stored_coverage
    pub fn prune_below_threshold(&self, volume_id: &str, mount_root: &str, threshold: f64) -> PruneOutcome {
        let Some(coverage) = self.stored_coverage(volume_id, mount_root, threshold) else {
            return PruneOutcome::default();
        };
        if coverage.doomed_paths.is_empty() {
            return PruneOutcome::default();
        }
        let db_path = store::media_db_path(&self.data_dir, volume_id);

        // The freed-byte estimate over the doomed set, BEFORE deleting (same content-byte
        // method the reclaim preview reports, so the "free about X" and "Freed X" numbers
        // agree). `VACUUM` reclaims at least this much on disk.
        let freed_bytes = self.estimate_doomed_bytes(volume_id, &coverage.doomed_paths);

        let writer = match self.writers.writer_for(&self.data_dir, volume_id) {
            Ok(w) => w,
            Err(e) => {
                log::warn!(target: "media_index", "reclaim prune: writer for '{volume_id}' failed: {e}");
                return PruneOutcome::default();
            }
        };
        let deleted = writer.prune_paths(coverage.doomed_paths).unwrap_or(0);
        if deleted > 0 {
            // Reclaim the pages, then drop the derived caches so a later search / slider
            // preview rebuilds honestly.
            let _ = writer.vacuum();
            vector::cache::invalidate(&db_path);
            coverage::invalidate(volume_id);
            log::info!(
                target: "media_index",
                "reclaim prune on '{volume_id}' at threshold {threshold}: {} removed (~{})",
                crate::pluralize::pluralize(deleted as u64, "row"),
                crate::pluralize::pluralize(freed_bytes, "byte")
            );
        }
        PruneOutcome {
            deleted_rows: deleted as u64,
            freed_bytes,
        }
    }
}
