//! The full-rebuild path: regenerate a volume's ANN index from the media DB's f16
//! blobs. The index is a disposable derivative, so this is the ONE recovery for
//! every unusable-index case (missing, corrupt, format/model mismatch, crashed
//! session). Kicked by the query-side route when it falls back to brute force;
//! search keeps answering (exactly, just slower) until the rebuild lands.
//!
//! Runs on its own background thread, single-threaded on purpose (the spike's 71 s
//! per 200k; usearch `add` is thread-safe, so a parallel build is a future lever if
//! real corpora make this feel slow — respecting the M2 discipline of never taking
//! more machine unasked). It streams rows (no whole-corpus `Vec`), polls an
//! injected stop hook every [`super::REBUILD_STOP_CHECK_EVERY`] adds (production
//! wires the memory watchdog's cancel — see [`kick`]) so a heap-building rebuild
//! yields under pressure, and installs via temp+rename under the per-file lock so
//! it can't interleave with a writer flush.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use super::{AnnError, AnnMeta, AnnSpace, engine_err};
use crate::ignore_poison::IgnorePoison;
use crate::media_index::store;

/// The in-flight set: one rebuild per index file at a time. A query that finds the
/// route unusable while a rebuild is already running just keeps brute-forcing.
static IN_FLIGHT: LazyLock<Mutex<HashSet<(PathBuf, AnnSpace)>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Whether a rebuild for `db_path`/`space` is currently running. The writer's flush
/// consults this to RETAIN its buffered ops for the whole rebuild window (see
/// `writer::AnnPending::flush`): a flush landing mid-rebuild would either apply to a
/// file the install is about to overwrite, or be dropped against a missing/stale
/// file whose replacement was snapshotted BEFORE those rows committed — either way
/// the ops would be silently lost.
pub(crate) fn is_in_flight(db_path: &Path, space: AnnSpace) -> bool {
    IN_FLIGHT.lock_ignore_poison().contains(&(db_path.to_path_buf(), space))
}

/// Test seam: mark a rebuild in flight without running one, so tests exercise the
/// writer's retain-during-rebuild behavior deterministically.
#[cfg(test)]
pub(crate) fn test_hold_in_flight(db_path: &Path, space: AnnSpace) {
    IN_FLIGHT.lock_ignore_poison().insert((db_path.to_path_buf(), space));
}

/// Test seam: release a [`test_hold_in_flight`] marker.
#[cfg(test)]
pub(crate) fn test_release_in_flight(db_path: &Path, space: AnnSpace) {
    IN_FLIGHT.lock_ignore_poison().remove(&(db_path.to_path_buf(), space));
}

/// Kick a background rebuild for `db_path`/`space` unless one is already running.
pub(crate) fn kick(db_path: &Path, space: AnnSpace, model_id: &str) {
    let key = (db_path.to_path_buf(), space);
    {
        let mut in_flight = IN_FLIGHT.lock_ignore_poison();
        if !in_flight.insert(key.clone()) {
            return; // already rebuilding
        }
    }
    let db_path = db_path.to_path_buf();
    let model_id = model_id.to_string();
    let spawned = std::thread::Builder::new()
        .name("media-ann-rebuild".into())
        .spawn(move || {
            let started = Instant::now();
            // The stop hook is the memory watchdog's cancel, deliberately NOT
            // `gate::should_stop`: a rebuild only ever starts from a live query, and
            // the search commands short-circuit while the master toggle is off, so
            // the toggle adds nothing here — while the watchdog's cancel is exactly
            // the "release resources now" signal a heap-building rebuild must obey.
            match rebuild_blocking(&db_path, space, &model_id, &crate::media_index::gate::is_cancelled) {
                Ok(rows) => log::info!(
                    target: "media_index",
                    "ann rebuild for {} done: {} in {:.1?}",
                    db_path.display(),
                    crate::pluralize::pluralize(rows, "vector"),
                    started.elapsed()
                ),
                Err(AnnError::Stopped) => log::info!(
                    target: "media_index",
                    "ann rebuild for {} stopped early (watchdog/toggle); will retry on a later query",
                    db_path.display()
                ),
                Err(e) => log::warn!(
                    target: "media_index",
                    "ann rebuild for {} did not complete: {e}",
                    db_path.display()
                ),
            }
            IN_FLIGHT.lock_ignore_poison().remove(&(db_path.clone(), space));
            // Re-decide the route either way: a success promotes queries to the new
            // index; an abort clears the cached brute-force route so a later query
            // can kick a fresh attempt.
            super::cache::invalidate(&db_path);
        });
    if let Err(e) = spawned {
        log::warn!(target: "media_index", "ann rebuild thread spawn failed: {e}");
        IN_FLIGHT.lock_ignore_poison().remove(&key);
    }
}

/// Build the index from every stored embedding of `space` and install it. Returns
/// the vector count. Public within the module tree for tests (which run it
/// synchronously instead of through [`kick`]).
pub(crate) fn rebuild_blocking(
    db_path: &Path,
    space: AnnSpace,
    model_id: &str,
    stop: &dyn Fn() -> bool,
) -> Result<u64, AnnError> {
    let conn = store::open_read_connection(db_path)?;
    let total = store::embedding_count_on(&conn, space.table())?;

    // Dims come from the first stored row (the module is dimension-generic); an
    // empty corpus builds nothing — the route below the threshold never asks for it.
    let mut index: Option<usearch::Index> = None;
    let mut dims = 0usize;
    let mut added = 0usize;
    let mut widened: Vec<f32> = Vec::new();
    store::for_each_embedding_with_id(&conn, space.table(), |file_id, vector| {
        if index.is_none() {
            dims = vector.len();
            let fresh = usearch::new_index(&super::index_options(dims)).map_err(engine_err)?;
            fresh.reserve(total as usize).map_err(engine_err)?;
            index = Some(fresh);
        }
        let Some(index) = index.as_ref() else {
            return Ok(());
        };
        if vector.len() != dims {
            // A row from a different-dimension world (mid-model-change); skip it —
            // the re-embed pass will upsert it into the right space.
            return Ok(());
        }
        if added.is_multiple_of(super::REBUILD_STOP_CHECK_EVERY) && stop() {
            return Err(AnnError::Stopped);
        }
        widened.clear();
        widened.extend(vector.iter().map(|v| v.to_f32()));
        index.add(file_id as u64, &widened).map_err(engine_err)?;
        added += 1;
        Ok(())
    })?;
    let Some(index) = index else {
        return Ok(0);
    };

    // Install under the per-file lock so a concurrent writer flush can't load the
    // old file, lose this rename, and save over it. Writer flushes additionally
    // RETAIN their buffered ops for the whole in-flight window (see
    // `writer::AnnPending::flush`) and replay them idempotently on top of this
    // install at the next seam, so rows committed after this snapshot are never
    // lost.
    let lock = super::file_lock(db_path, space);
    let _guard = lock.lock_ignore_poison();
    super::save_index_atomically(&index, &super::index_path(db_path, space))?;
    let rows = index.size() as u64;
    super::write_meta(
        db_path,
        space,
        &AnnMeta {
            format: super::ANN_FORMAT_VERSION,
            model_id: model_id.to_string(),
            dims,
            rows,
            checksum: super::index_file_checksum(db_path, space)?,
        },
    )?;
    Ok(rows)
}
