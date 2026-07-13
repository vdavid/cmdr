//! A lazy per-volume registry of long-lived [`MediaWriter`] threads.
//!
//! Ported from `importance/writer_registry.rs`: the "ONE writer thread per DB"
//! invariant must hold in spirit. The scheduler's enrichment passes and (later)
//! the disable-and-purge path both write a volume's `media.db`; this registry hands
//! them a SHARED, long-lived writer per volume, created on first use and living for
//! the process. Keyed by volume id, independent of the index registry, so a writer
//! outlives an unmount.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::store::{MediaStoreError, media_db_path};
use super::writer::MediaWriter;
use crate::ignore_poison::IgnorePoison;

/// The long-lived per-volume writers, keyed by volume id.
#[derive(Default)]
pub struct WriterRegistry {
    writers: Mutex<HashMap<String, MediaWriter>>,
}

impl WriterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (or lazily create) the shared writer for `volume_id`, whose `media.db`
    /// lives under `data_dir`. Returns a cloned handle sharing the one writer
    /// thread. The DB file + schema are opened first ([`super::store::MediaStore`]
    /// owns the schema stamp) so the writer's connection finds the tables. Creation
    /// reserves the slot outside the map lock, so two concurrent first-uses can't
    /// race two threads onto one DB.
    pub fn writer_for(&self, data_dir: &Path, volume_id: &str) -> Result<MediaWriter, MediaStoreError> {
        if let Some(writer) = self.writers.lock_ignore_poison().get(volume_id).cloned() {
            return Ok(writer);
        }
        let db_path = media_db_path(data_dir, volume_id);
        super::store::MediaStore::open(&db_path)?; // create file + schema.
        let built = MediaWriter::spawn(&db_path)?;

        let mut map = self.writers.lock_ignore_poison();
        let entry = map.entry(volume_id.to_string()).or_insert(built);
        Ok(entry.clone())
    }

    /// Shut down and forget every writer (app teardown). Idempotent.
    pub fn shutdown_all(&self) {
        let writers: Vec<MediaWriter> = self.writers.lock_ignore_poison().drain().map(|(_, w)| w).collect();
        for w in writers {
            w.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_index::predicate::MediaKind;
    use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};

    /// Two `writer_for` calls for one volume return handles to the SAME writer
    /// thread, and a write through one is visible via the store.
    #[test]
    fn writer_for_shares_one_writer_per_volume() {
        let dir = tempfile::tempdir().expect("temp dir");
        let registry = WriterRegistry::new();

        let w1 = registry.writer_for(dir.path(), "root").expect("writer 1");
        let w2 = registry.writer_for(dir.path(), "root").expect("writer 2");
        assert_eq!(w1.db_path(), w2.db_path(), "both handles serve the same DB");

        w1.upsert(
            MediaStatusRow {
                path: "/p.jpg".to_string(),
                mtime: Some(10),
                size: Some(20),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
            },
            Some("hello".to_string()),
        )
        .expect("upsert");
        w2.flush_blocking().expect("flush through the shared writer");

        let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("open");
        assert!(
            store.status_for("/p.jpg").expect("read").is_some(),
            "a write through the shared writer is persisted"
        );
        registry.shutdown_all();
    }
}
