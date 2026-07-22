//! Command-layer tests. The seeded-`media.db` → `OcrHit`-with-snippet round-trip the
//! command delegates to is covered end-to-end in `read/tests.rs`
//! (`search_finds_the_image_by_ocr_text_and_survives_unmount`); here we cover the
//! command-specific limit resolution.

use super::policy::{folder_override_should_kick, scope_change_should_kick, threshold_change_should_kick};
use super::{DEFAULT_LIMIT, FileIndexState, MAX_LIMIT, classify_all, classify_one, resolve_limit};
use crate::media_index::gate::IndexScope;

#[test]
fn a_missing_limit_takes_the_default() {
    assert_eq!(resolve_limit(None), DEFAULT_LIMIT as usize);
}

#[test]
fn a_supplied_limit_is_honored_below_the_ceiling() {
    assert_eq!(resolve_limit(Some(25)), 25);
}

#[test]
fn an_oversized_limit_is_clamped_to_the_ceiling() {
    assert_eq!(resolve_limit(Some(100_000)), MAX_LIMIT as usize);
}

// ── The threshold-change kick decision (item 2c) ─────────────────────────────
// `media_index_set_importance_threshold` needs an `AppHandle` to kick, so the
// decide-then-kick logic is extracted here. The pure direction check
// (`gate::threshold_decreased`) has its own test in `gate`; these pin the combined
// decision the command actually makes.

#[test]
fn a_threshold_decrease_while_enabled_kicks() {
    // Lowering the threshold broadens coverage, so newly-covered folders enrich now.
    assert!(threshold_change_should_kick(0.6, 0.3, true));
}

#[test]
fn a_threshold_raise_never_kicks() {
    // A raise only defers future work (forward-only): nothing to enrich now.
    assert!(!threshold_change_should_kick(0.3, 0.6, true));
}

#[test]
fn an_unchanged_threshold_never_kicks() {
    assert!(!threshold_change_should_kick(0.5, 0.5, true));
}

#[test]
fn a_decrease_while_disabled_never_kicks() {
    // With the feature off there is no pass to run.
    assert!(!threshold_change_should_kick(0.6, 0.3, false));
}

// ── The chosen-folder and scope kick decisions ───────────────────────────────
// Same shape: the commands need an `AppHandle` to kick, so the decision is extracted.

#[test]
fn choosing_a_folder_while_enabled_kicks_a_pass() {
    // Without this the folder sits unindexed until some volume happens to rescan, and
    // the whole "add the folder, watch it index" promise falls over.
    assert!(folder_override_should_kick(true, true));
}

#[test]
fn removing_a_chosen_folder_never_kicks() {
    // Coverage only narrows; the rows persist (forward-only) until a reclaim.
    assert!(!folder_override_should_kick(false, true));
}

#[test]
fn choosing_a_folder_while_disabled_never_kicks() {
    assert!(!folder_override_should_kick(true, false));
}

#[test]
fn broadening_the_scope_while_enabled_kicks_a_pass() {
    assert!(scope_change_should_kick(
        IndexScope::ChosenFolders,
        IndexScope::ByImportance,
        true
    ));
}

#[test]
fn narrowing_the_scope_never_kicks() {
    // Narrowing has nothing new to enrich; the now-uncovered rows stay searchable and
    // surface as the reclaim offer instead.
    assert!(!scope_change_should_kick(
        IndexScope::ByImportance,
        IndexScope::ChosenFolders,
        true
    ));
}

#[test]
fn broadening_the_scope_while_disabled_never_kicks() {
    assert!(!scope_change_should_kick(
        IndexScope::ChosenFolders,
        IndexScope::ByImportance,
        false
    ));
}

// ── The per-file classification (the file-icon overlay's states) ─────────────
// `classify_one`/`classify_all` are pure over already-resolved inputs, so every state
// is testable without an index, a `media.db`, or an app.

mod file_status {
    use super::*;
    use crate::media_index::network::config::NetworkEnrichConfig;
    use crate::media_index::predicate::MediaKind;
    use crate::media_index::scheduler::enrich::ImageEntry;
    use crate::media_index::store::{EnrichmentState, MediaStatusRow};
    use std::collections::HashMap;

    /// A qualifying-image index entry with a given `(mtime, size)`.
    fn entry(path: &str, mtime: u64, size: u64) -> ImageEntry {
        ImageEntry {
            path: path.to_string(),
            mtime: Some(mtime),
            size: Some(size),
            kind: MediaKind::Image,
        }
    }

    /// A stored `media_status` row at a given state and `(mtime, size, engine)`.
    fn row(path: &str, state: EnrichmentState, mtime: u64, size: u64, engine: &str) -> MediaStatusRow {
        MediaStatusRow {
            path: path.to_string(),
            mtime: Some(mtime),
            size: Some(size),
            media_kind: MediaKind::Image,
            state,
            engine_version: engine.to_string(),
            clip_stamp: String::new(),
        }
    }

    /// Classify one path against optional entry + optional stored row, an "all covered"
    /// override config, and the current engine stamp `e1`.
    fn classify(
        entry: Option<&ImageEntry>,
        stored: Option<&MediaStatusRow>,
        config: &NetworkEnrichConfig,
    ) -> FileIndexState {
        classify_one("/photos/a.jpg", entry, stored, Some("e1"), None, config, "vol")
    }

    fn cover_all() -> NetworkEnrichConfig {
        NetworkEnrichConfig {
            always_index_volumes: ["vol".to_string()].into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_non_qualifying_path_is_not_applicable() {
        // No index entry (a video/doc/folder or a vanished path) ⇒ no badge, whatever a
        // stray stored row might say.
        assert_eq!(classify(None, None, &cover_all()), FileIndexState::NotApplicable);
        let r = row("/photos/a.jpg", EnrichmentState::Done, 1, 2, "e1");
        assert_eq!(classify(None, Some(&r), &cover_all()), FileIndexState::NotApplicable);
    }

    #[test]
    fn a_current_done_row_is_indexed() {
        let e = entry("/photos/a.jpg", 1, 2);
        let r = row("/photos/a.jpg", EnrichmentState::Done, 1, 2, "e1");
        assert_eq!(classify(Some(&e), Some(&r), &cover_all()), FileIndexState::Indexed);
    }

    #[test]
    fn a_changed_file_reads_stale() {
        // The live file's size moved since indexing ⇒ `needs_enrichment` ⇒ stale.
        let e = entry("/photos/a.jpg", 1, 999);
        let r = row("/photos/a.jpg", EnrichmentState::Done, 1, 2, "e1");
        assert_eq!(classify(Some(&e), Some(&r), &cover_all()), FileIndexState::Stale);
    }

    #[test]
    fn an_engine_bump_reads_stale() {
        // Same file, but the analyze engine advanced (an OS Vision upgrade) ⇒ stale.
        let e = entry("/photos/a.jpg", 1, 2);
        let r = row("/photos/a.jpg", EnrichmentState::Done, 1, 2, "e0");
        assert_eq!(classify(Some(&e), Some(&r), &cover_all()), FileIndexState::Stale);
    }

    #[test]
    fn a_failed_row_reads_failed() {
        let e = entry("/photos/a.jpg", 1, 2);
        let r = row("/photos/a.jpg", EnrichmentState::Failed, 1, 2, "e1");
        assert_eq!(classify(Some(&e), Some(&r), &cover_all()), FileIndexState::Failed);
    }

    #[test]
    fn an_uncovered_stored_row_still_reads_from_its_row() {
        // Forward-only: an indexed image the current setting no longer covers stays
        // "indexed" (searchable), not "excluded".
        let e = entry("/photos/a.jpg", 1, 2);
        let r = row("/photos/a.jpg", EnrichmentState::Done, 1, 2, "e1");
        let uncovered = NetworkEnrichConfig::default(); // covers nothing
        assert_eq!(classify(Some(&e), Some(&r), &uncovered), FileIndexState::Indexed);
    }

    #[test]
    fn an_un_enriched_covered_image_is_pending() {
        let e = entry("/photos/a.jpg", 1, 2);
        assert_eq!(classify(Some(&e), None, &cover_all()), FileIndexState::Pending);
    }

    #[test]
    fn an_un_enriched_uncovered_image_is_excluded() {
        let e = entry("/photos/a.jpg", 1, 2);
        let uncovered = NetworkEnrichConfig::default();
        assert_eq!(classify(Some(&e), None, &uncovered), FileIndexState::Excluded);
    }

    #[test]
    fn an_excluded_folder_beats_coverage_for_an_un_enriched_image() {
        // The privacy veto: even an "always index" override can't make an excluded
        // folder's un-enriched image pending.
        let e = entry("/photos/a.jpg", 1, 2);
        let config = NetworkEnrichConfig {
            always_index_volumes: ["vol".to_string()].into_iter().collect(),
            excluded_folders: ["/photos".to_string()].into_iter().collect(),
            ..Default::default()
        };
        assert_eq!(classify(Some(&e), None, &config), FileIndexState::Excluded);
    }

    #[test]
    fn classify_all_preserves_request_order_and_mixes_states() {
        // A folder, an indexed image, and an un-enriched covered image, returned 1:1 in
        // the SAME order the caller asked.
        let paths = vec![
            "/photos/folder".to_string(),
            "/photos/indexed.jpg".to_string(),
            "/photos/pending.jpg".to_string(),
        ];
        let indexed = entry("/photos/indexed.jpg", 1, 2);
        let pending = entry("/photos/pending.jpg", 1, 2);
        let qualifying: HashMap<String, ImageEntry> = [
            ("/photos/indexed.jpg".to_string(), indexed),
            ("/photos/pending.jpg".to_string(), pending),
        ]
        .into_iter()
        .collect();
        let stored: HashMap<String, MediaStatusRow> = [(
            "/photos/indexed.jpg".to_string(),
            row("/photos/indexed.jpg", EnrichmentState::Done, 1, 2, "e1"),
        )]
        .into_iter()
        .collect();

        let out = classify_all(&paths, &qualifying, &stored, Some("e1"), None, &cover_all(), "vol");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].path, "/photos/folder");
        assert_eq!(out[0].state, FileIndexState::NotApplicable, "the folder gets no badge");
        assert_eq!(out[1].state, FileIndexState::Indexed);
        assert_eq!(out[2].state, FileIndexState::Pending);
    }
}
