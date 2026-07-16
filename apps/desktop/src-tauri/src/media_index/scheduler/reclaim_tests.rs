//! Reclaim-space tests: the single-source stored-coverage
//! partition over a seeded `media.db` + importance store, and the prune round-trip
//! (doomed rows gone, surviving rows intact, honest counts returned). Deletion is
//! data-safety-critical, so these are real red→green.

use super::*;
use crate::importance::store::{ImportanceStore, importance_db_path};
use crate::importance::writer::{ImportanceWriter, WeightRow};
use crate::media_index::backend::fake::FakeVisionBackend;
// Share the kick tests' tiny-index builder + gate reset (same shape).
use super::kick_tests::{build_index, reset_gate};
use crate::media_index::network::config::NetworkEnrichConfig;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::{MediaWriter, UpsertAnalysis};

const ROOT: &str = "root";

fn fake_backend() -> Arc<dyn VisionBackend> {
    Arc::new(FakeVisionBackend::new())
}

/// Seed a FULL-pass importance store (generation stamped) so `importance_scores` reads
/// it as scored.
fn seed_importance(data_dir: &std::path::Path, volume_id: &str, rows: &[(&str, f64)]) {
    let path = importance_db_path(data_dir, volume_id);
    ImportanceStore::open(&path).expect("open importance store");
    let writer = ImportanceWriter::spawn(&path).expect("importance writer");
    let rows = rows
        .iter()
        .map(|(p, s)| WeightRow {
            path: p.to_string(),
            score: *s,
            signals_json: "{}".to_string(),
        })
        .collect();
    writer.write_weights(1, rows).expect("write weights");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

/// Seed one fully-enriched `media.db` row (OCR text + a tag + an embedding, so a prune
/// has real content bytes to free) for `volume_id` at `path`.
fn seed_media_row(data_dir: &std::path::Path, volume_id: &str, path: &str) {
    use crate::media_index::backend::Tag;
    let db_path = media_db_path(data_dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    let writer = MediaWriter::spawn(&db_path).expect("media writer");
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(10),
                size: Some(10),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
            },
            Some(UpsertAnalysis {
                ocr_text: "some text".to_string(),
                tags: vec![Tag {
                    label: "beach".to_string(),
                    score: 0.9,
                }],
                embedding: Some(vec![0.1, 0.2, 0.3, 0.4]),
            }),
        )
        .expect("seed row");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

fn stored_exists(data_dir: &std::path::Path, volume_id: &str, path: &str) -> bool {
    MediaStore::open(&media_db_path(data_dir, volume_id))
        .expect("open")
        .status_for(path)
        .expect("read")
        .is_some()
}

#[test]
fn stored_coverage_partitions_rows_by_the_threshold_and_counts_qualifying() {
    // /keep scores high, /drop scores low. At threshold 0.5 the /keep rows survive and
    // the /drop rows are doomed; `covered_qualifying` counts the drive-index images in
    // the covered folders (a DIFFERENT number from surviving stored rows).
    let _guard = crate::indexing::test_read_pool_lock();
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    // The drive index: two images in /keep, one in /drop.
    build_index(
        &index_path,
        &[("/keep", "a.jpg"), ("/keep", "b.jpg"), ("/drop", "c.jpg")],
    );
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // The covered-count cache is process-global and keyed by volume id ("root"); a prior
    // test may have cached a different index's counts, so drop it for a fresh build.
    crate::media_index::coverage::invalidate(ROOT);
    seed_importance(dir.path(), ROOT, &[("/keep", 0.9), ("/drop", 0.1)]);
    // media.db carries one stored row in each folder (only a.jpg enriched in /keep).
    seed_media_row(dir.path(), ROOT, "/keep/a.jpg");
    seed_media_row(dir.path(), ROOT, "/drop/c.jpg");
    network::config::set_config(NetworkEnrichConfig::default());

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let cov = sched.stored_coverage(ROOT, "/", 0.5).expect("scored");
    assert_eq!(cov.surviving_stored, 1, "the /keep row survives");
    assert_eq!(cov.doomed_stored, 1, "the /drop row is doomed");
    assert_eq!(cov.doomed_paths, vec!["/drop/c.jpg".to_string()]);
    assert_eq!(
        cov.surviving_stored + cov.doomed_stored,
        2,
        "the partition covers every stored row"
    );
    // Covered qualifying counts the DRIVE-INDEX images in /keep (2), not the 1 stored row.
    assert_eq!(cov.covered_qualifying, 2, "qualifying images in covered folders");

    crate::indexing::test_uninstall_root_read_pool();
    network::config::set_config(NetworkEnrichConfig::default());
}

#[test]
fn stored_coverage_is_none_when_importance_is_unscored() {
    // No importance store ⇒ can't partition safely ⇒ `None` (the command reports pending
    // and the reclaim UI stays hidden rather than proposing a destructive number).
    let dir = tempfile::tempdir().expect("temp");
    seed_media_row(dir.path(), ROOT, "/keep/a.jpg");
    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    assert!(sched.stored_coverage(ROOT, "/", 0.0).is_none());
}

#[test]
fn prune_below_threshold_deletes_the_doomed_set_and_keeps_the_rest() {
    // The prune round-trip: the /drop row goes, the /keep row stays, and the outcome
    // reports one deleted row and a positive freed-byte estimate.
    let _guard = crate::indexing::test_read_pool_lock();
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg"), ("/drop", "c.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // The covered-count cache is process-global and keyed by volume id ("root"); a prior
    // test may have cached a different index's counts, so drop it for a fresh build.
    crate::media_index::coverage::invalidate(ROOT);
    seed_importance(dir.path(), ROOT, &[("/keep", 0.9), ("/drop", 0.1)]);
    seed_media_row(dir.path(), ROOT, "/keep/a.jpg");
    seed_media_row(dir.path(), ROOT, "/drop/c.jpg");
    network::config::set_config(NetworkEnrichConfig::default());

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let outcome = sched.prune_below_threshold(ROOT, "/", 0.5);
    assert_eq!(outcome.deleted_rows, 1, "one doomed row deleted");
    assert!(outcome.freed_bytes > 0, "a positive freed-byte estimate");

    assert!(stored_exists(dir.path(), ROOT, "/keep/a.jpg"), "covered row survives");
    assert!(!stored_exists(dir.path(), ROOT, "/drop/c.jpg"), "doomed row gone");

    crate::indexing::test_uninstall_root_read_pool();
    network::config::set_config(NetworkEnrichConfig::default());
}

#[test]
fn a_pass_enriching_covered_rows_and_a_prune_touch_disjoint_sets() {
    // The concurrent-pass sanity: a pass only enriches AT-or-above-threshold rows and the
    // prune only deletes BELOW-threshold rows, so the two sets are disjoint. Running a
    // pass (which enriches the covered /keep image) and then the prune (which removes the
    // doomed /drop row) leaves /keep enriched and /drop gone — neither steps on the other.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    gate::set_importance_threshold(0.5);

    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg"), ("/drop", "c.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // The covered-count cache is process-global and keyed by volume id ("root"); a prior
    // test may have cached a different index's counts, so drop it for a fresh build.
    crate::media_index::coverage::invalidate(ROOT);
    seed_importance(dir.path(), ROOT, &[("/keep", 0.9), ("/drop", 0.1)]);
    // media.db starts with only the doomed /drop row (the covered /keep row is enriched by
    // the pass below — the "new rows during prune").
    seed_media_row(dir.path(), ROOT, "/drop/c.jpg");

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    // The pass enriches the covered /keep image (it defers /drop, below threshold).
    sched.run_pass_blocking(ROOT).expect("pass");
    assert!(
        stored_exists(dir.path(), ROOT, "/keep/a.jpg"),
        "the pass enriched the covered row"
    );

    // The prune removes the doomed /drop row, leaving the just-enriched /keep row intact.
    let outcome = sched.prune_below_threshold(ROOT, "/", 0.5);
    assert_eq!(outcome.deleted_rows, 1, "only the doomed /drop row is pruned");
    assert!(
        stored_exists(dir.path(), ROOT, "/keep/a.jpg"),
        "the covered row survives the prune"
    );
    assert!(
        !stored_exists(dir.path(), ROOT, "/drop/c.jpg"),
        "the doomed row is gone"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn prune_leaves_an_override_covered_row_below_threshold() {
    // An "always index" override keeps a low-scoring folder's rows even at a high
    // threshold — the prune honors the same precedence enrichment does.
    let _guard = crate::indexing::test_read_pool_lock();
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/archive", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // The covered-count cache is process-global and keyed by volume id ("root"); a prior
    // test may have cached a different index's counts, so drop it for a fresh build.
    crate::media_index::coverage::invalidate(ROOT);
    seed_importance(dir.path(), ROOT, &[("/archive", 0.1)]);
    seed_media_row(dir.path(), ROOT, "/archive/a.jpg");
    // Override /archive so it's covered regardless of its low score.
    network::config::set_config(NetworkEnrichConfig {
        always_index_folders: ["/archive".to_string()].into_iter().collect(),
        ..Default::default()
    });

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let outcome = sched.prune_below_threshold(ROOT, "/", 0.8);
    assert_eq!(outcome.deleted_rows, 0, "the override-covered row is not pruned");
    assert!(
        stored_exists(dir.path(), ROOT, "/archive/a.jpg"),
        "override row survives"
    );

    crate::indexing::test_uninstall_root_read_pool();
    network::config::set_config(NetworkEnrichConfig::default());
}
