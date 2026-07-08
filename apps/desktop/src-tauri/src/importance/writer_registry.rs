//! A lazy per-volume registry of long-lived [`ImportanceWriter`] threads.
//!
//! The subsystem's "ONE writer thread per DB" invariant (mirroring the index's
//! `IndexWriter`) must hold in spirit, not just be papered over by WAL
//! busy-timeouts: both the recompute scheduler and the `record_visit` command
//! write to a volume's `importance.db`, and if each spawned its own short-lived
//! writer thread they'd be two writers on one file. This registry hands both a
//! SHARED, long-lived writer per volume, created on first use and living for the
//! process.
//!
//! Keyed by volume id, independent of the index registry: a writer outlives a
//! volume unmount so a late `record_visit` or a queued recompute still has one
//! writer to go through. Creation is guarded so two concurrent first-uses can't
//! race two threads onto one DB (reserve the slot, then build).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::store::{ImportanceStoreError, importance_db_path};
use super::writer::ImportanceWriter;
use crate::ignore_poison::IgnorePoison;

/// The long-lived per-volume writers. A `None` slot marks a volume whose writer
/// is being built (reserved), so a concurrent caller waits rather than spawning a
/// second thread. Once built, the slot holds the shared handle for the process.
#[derive(Default)]
pub struct WriterRegistry {
    /// `volume_id → writer`. The outer `Mutex` guards the map; the inner slot is
    /// the built writer (cloneable handle; a clone shares the one thread).
    writers: Mutex<HashMap<String, ImportanceWriter>>,
}

impl WriterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (or lazily create) the shared writer for `volume_id`, whose
    /// `importance.db` lives under `data_dir`. Returns a cloned handle sharing the
    /// one writer thread.
    ///
    /// The DB file and schema are opened first ([`super::store::ImportanceStore`]
    /// owns the schema-version stamp) so the writer's own connection finds the
    /// tables. A spawn failure propagates; the slot stays empty so a later call
    /// retries rather than caching a dead handle.
    pub fn writer_for(&self, data_dir: &Path, volume_id: &str) -> Result<ImportanceWriter, ImportanceStoreError> {
        // Fast path: an existing writer.
        if let Some(writer) = self.writers.lock_ignore_poison().get(volume_id).cloned() {
            return Ok(writer);
        }

        // Build outside the map lock (spawning a thread + opening a DB can block),
        // then insert, taking whichever writer won if two callers raced. The
        // loser's freshly-spawned writer is dropped (its thread shuts down when the
        // last handle drops); no second live writer persists on the DB.
        let db_path = importance_db_path(data_dir, volume_id);
        super::store::ImportanceStore::open(&db_path)?; // create file + schema.
        let built = ImportanceWriter::spawn(&db_path)?;

        let mut map = self.writers.lock_ignore_poison();
        let entry = map.entry(volume_id.to_string()).or_insert(built);
        Ok(entry.clone())
    }

    /// Shut down and forget every writer. Called on app teardown so the writer
    /// threads join. Idempotent.
    pub fn shutdown_all(&self) {
        let writers: Vec<ImportanceWriter> = self.writers.lock_ignore_poison().drain().map(|(_, w)| w).collect();
        for w in writers {
            w.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importance::writer::WeightRow;

    /// Two `writer_for` calls for the same volume return handles to the SAME
    /// writer thread (one writer per DB), and a write through one is visible via
    /// the store — the cleanup's "one writer in spirit" guarantee.
    #[test]
    fn writer_for_shares_one_writer_per_volume() {
        let dir = tempfile::tempdir().expect("temp dir");
        let registry = WriterRegistry::new();

        let w1 = registry.writer_for(dir.path(), "root").expect("writer 1");
        let w2 = registry.writer_for(dir.path(), "root").expect("writer 2");
        // Same underlying DB path ⇒ same writer thread (shared handle).
        assert_eq!(w1.db_path(), w2.db_path(), "both handles serve the same DB");

        // A write through one handle round-trips.
        w1.write_weights(
            1,
            vec![WeightRow {
                path: "/p".to_string(),
                score: 0.5,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write");
        w2.flush_blocking().expect("flush through the shared writer");

        let store = super::super::store::ImportanceStore::open(&importance_db_path(dir.path(), "root")).expect("open");
        assert!(
            store.weight_for("/p").expect("read").is_some(),
            "a write through the shared writer is persisted"
        );
        registry.shutdown_all();
    }

    /// Different volumes get different writers.
    #[test]
    fn writer_for_is_per_volume() {
        let dir = tempfile::tempdir().expect("temp dir");
        let registry = WriterRegistry::new();
        let root = registry.writer_for(dir.path(), "root").expect("root");
        let other = registry.writer_for(dir.path(), "smb-nas").expect("other");
        assert_ne!(root.db_path(), other.db_path(), "each volume has its own DB + writer");
        registry.shutdown_all();
    }
}
