//! What a test outside this subsystem needs to drive media enrichment, behind
//! the same `testing` feature the index uses.
//!
//! ❌ Not part of the API. One app-side integration test fetches image bytes off
//! a real SMB share through the enrichment path's own fetcher, to prove the two
//! agree; `#[cfg(test)]` can't reach across a crate boundary to let it.

/// Pulling image bytes for enrichment: the trait, the volume-backed
/// implementation the network pass uses, and what it fails with.
pub use crate::media_index::network::fetch::{ByteFetcher, FetchError, VolumeByteFetcher};

/// Stores `path` as an enriched image with `ocr_text` in `volume_id`'s `media.db` under
/// `data_dir`, for the app's content tools to be tested against real stored text. Panics on a
/// store it can't write: it's a fixture.
pub fn seed_ocr_row(data_dir: &std::path::Path, volume_id: &str, path: &str, ocr_text: &str) {
    use crate::media_index::predicate::MediaKind;
    use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
    use crate::media_index::writer::{MediaWriter, UpsertAnalysis};

    let db_path = media_db_path(data_dir, volume_id);
    MediaStore::open(&db_path).expect("open the store");
    let writer = MediaWriter::spawn(&db_path, volume_id).expect("spawn the writer");
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis {
                ocr_text: ocr_text.to_string(),
                ..Default::default()
            }),
        )
        .expect("seed the row");
    writer.flush_blocking().expect("flush the row");
    writer.shutdown();
}
