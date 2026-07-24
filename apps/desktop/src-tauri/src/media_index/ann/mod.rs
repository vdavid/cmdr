//! ANN vector search (plan M6): a per-volume usearch HNSW index over the CLIP
//! embeddings, so text→image search stays low-ms as a corpus grows past what the
//! exact resident-f16 scan can serve (74 ms p50 and ~205 MB resident at 200k,
//! linear beyond — the spike, `docs/notes/ann-vector-search-spike-2026-07-24.md`).
//!
//! ## Shape
//!
//! - **One index file per volume per embedding space**, beside `media-{id}.db`:
//!   `media-{id}.clip.usearch` plus a small JSON sidecar
//!   (`….usearch.meta`) carrying the format version, the embedding model id, the
//!   dimension count, and the row count at last save. The module is
//!   dimension-generic ([`AnnSpace`] names the space and its staleness identity), so
//!   the 768-d Vision feature print can adopt it later by adding a variant — only
//!   CLIP is wired today.
//! - **Keys are the `media_file` integer ids** (plan M4) cast to `u64`. A rename is a
//!   one-row `media_file.path` update with the id unchanged, so renames never touch
//!   the index; query hits join ids back to CURRENT paths through the DB, which also
//!   drops ghost keys (a key whose row is gone resolves to nothing and falls out of
//!   the result).
//! - **f16 storage, cosine metric** ([`ScalarKind::F16`]) matching the M3 on-disk
//!   blobs; queries read the file in mmap `view` mode so the at-rest RSS stays small
//!   and pages are evictable ([`cache`]).
//!
//! ## Ownership (the single-writer discipline)
//!
//! Incremental mutations flow through the ONE `MediaWriter` thread, exactly like DB
//! writes: the writer buffers an [`AnnOp`] per CLIP write/delete and applies the
//! batch via [`flush_ops`] at the same seams that invalidate the resident vector
//! cache (pass/tick completion, prunes), plus an in-writer auto-flush at
//! [`ANN_PENDING_FLUSH_LIMIT`] ops so a long first pass can't hold an unbounded
//! buffer. usearch has no in-place file mutation, so a flush loads the index to the
//! heap, applies the ops, and saves temp+rename (safe-overwrite: a live mmap view
//! keeps the old inode; a crash never leaves a torn file). The background rebuild
//! ([`rebuild`]) is the one writer-external mutator, and it serializes with flushes
//! on the per-file [`file_lock`].
//!
//! ## Disposable derivative + crash detection
//!
//! The index is rebuilt from the media DB's f16 blobs whenever it's missing,
//! corrupt, or its sidecar disagrees on format/model/dims ([`cache`] detects,
//! [`rebuild`] heals in the background; search falls back to the exact brute-force
//! scan meanwhile — a bad index NEVER breaks search). Crash detection is a dirty
//! marker file: the writer creates it before the first buffered op's DB write
//! commits and [`flush_ops`] removes it after a successful save, so a session that
//! crashed with unflushed ops leaves the marker behind and the next writer spawn
//! wipes the stale index (next query rebuilds). `MediaStore`'s schema-mismatch
//! delete-and-recreate removes the index files too, so a `SCHEMA_VERSION` bump takes
//! the derivative with it.

pub(crate) mod cache;
pub(crate) mod rebuild;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use usearch::{IndexOptions, MetricKind, ScalarKind};

use super::store::MediaStoreError;
use crate::ignore_poison::IgnorePoison;

/// Below this many stored vectors, semantic search keeps the EXACT resident-f16
/// brute-force scan; at or above it, the ANN index takes over. Rationale (spike,
/// `docs/notes/ann-vector-search-spike-2026-07-24.md`): the exact scan measured
/// 74 ms p50 at 200k vectors and scales linearly, so at 50k it's ~19 ms from a
/// ~50 MB resident cache — exact results, no index file to maintain, RAM and
/// latency both still comfortably inside budget. Past 50k both grow linearly while
/// HNSW stays sub-ms over an evictable mmap, so that's where the index starts
/// paying for its build cost and disk footprint.
pub(crate) const ANN_MIN_VECTORS: usize = 50_000;

/// ANN over-fetch factor: fetch `k × 4` approximate candidates, then re-rank them
/// EXACTLY (cosine against the stored f16 blobs) and return the top `k`. HNSW
/// recall dips as the corpus grows (0.994 at 200k, 0.895–0.982 at 1M depending on
/// `expansion_search` — the spike), and the misses are overwhelmingly at the tail
/// of the candidate list; a 4× over-fetch re-scored exactly restores exact-quality
/// ORDERING for the k the caller sees at a per-query cost of reading ~4k small
/// blobs (sub-ms; measured in the M6 integration harness).
pub(crate) const ANN_OVERFETCH_FACTOR: usize = 4;

/// The writer's in-flight [`AnnOp`] buffer flushes itself past this many ops, so a
/// long first enrichment pass holds at most ~17 MB of pending vectors
/// (8,192 × 512 dims × 4 B) rather than the whole pass's output.
pub(crate) const ANN_PENDING_FLUSH_LIMIT: usize = 8_192;

/// Bump to invalidate every on-disk ANN index (a change to the index options,
/// key scheme, or sidecar shape). A mismatch wipes and rebuilds — the index is a
/// disposable derivative, exactly like `SCHEMA_VERSION` for `media.db`.
pub(crate) const ANN_FORMAT_VERSION: u32 = 1;

/// How often the rebuild loop polls `gate::should_stop` (every N vectors added).
const REBUILD_STOP_CHECK_EVERY: usize = 1_024;

/// An embedding space with an ANN index. Names the DB table, the file suffix, and
/// the model identity the sidecar pins. Only CLIP is wired today; the Vision
/// feature print (768-d, similar-images/dedup) adds a variant here when it adopts
/// ANN — everything else in this module is dimension-generic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AnnSpace {
    /// CLIP text→image embeddings (`media_clip_embedding`, 512-d today).
    Clip,
}

impl AnnSpace {
    /// The file-name suffix distinguishing this space's index beside the media DB.
    fn suffix(self) -> &'static str {
        match self {
            AnnSpace::Clip => "clip",
        }
    }

    /// The embedding table this space indexes.
    pub(crate) fn table(self) -> super::store::EmbeddingTable {
        match self {
            AnnSpace::Clip => super::store::EmbeddingTable::Clip,
        }
    }

    /// The CURRENT model identity for this space — the sidecar pins it so an index
    /// built from a different model's vectors is detected and rebuilt rather than
    /// silently searched with queries from the new model's space. Deliberately the
    /// model id alone (no OS component): an OS upgrade re-embeds rows via
    /// `clip_stamp` staleness and those re-embeds flow through writer upserts.
    pub(crate) fn current_model_id(self) -> &'static str {
        match self {
            AnnSpace::Clip => super::clip::install::CLIP_MODEL_ID,
        }
    }
}

/// Errors from the ANN layer. Typed so callers and tests classify by variant, never
/// by message (`no-string-matching`); `Engine` wraps usearch's opaque exception text
/// and is never matched on.
#[derive(Debug)]
pub(crate) enum AnnError {
    /// The index or sidecar file doesn't exist.
    Missing,
    /// The sidecar exists but disagrees on format version, model id, or dims — the
    /// index was built for a different world and must be rebuilt.
    MetaIncompatible,
    /// The index file's bytes don't match the sidecar's checksum. usearch mmap-views
    /// TRUST the file (a corrupt body segfaults at search time — observed with a
    /// garbage file in tests), so corruption must be caught HERE, before any
    /// load/view ever touches the bytes.
    Corrupt,
    /// Filesystem trouble around the index/sidecar files.
    Io(std::io::Error),
    /// A media-store read failed (the rebuild's source).
    Store(MediaStoreError),
    /// usearch reported an engine-level failure (corrupt file, bad add). Opaque.
    Engine(String),
    /// A rebuild aborted because `gate::should_stop` went true (watchdog/toggle).
    Stopped,
}

impl std::fmt::Display for AnnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnError::Missing => write!(f, "ann index missing"),
            AnnError::MetaIncompatible => write!(f, "ann index meta incompatible"),
            AnnError::Corrupt => write!(f, "ann index checksum mismatch"),
            AnnError::Io(e) => write!(f, "ann index io: {e}"),
            AnnError::Store(e) => write!(f, "ann index store read: {e}"),
            AnnError::Engine(e) => write!(f, "ann engine: {e}"),
            AnnError::Stopped => write!(f, "ann rebuild stopped"),
        }
    }
}

impl From<std::io::Error> for AnnError {
    fn from(e: std::io::Error) -> Self {
        AnnError::Io(e)
    }
}

impl From<MediaStoreError> for AnnError {
    fn from(e: MediaStoreError) -> Self {
        AnnError::Store(e)
    }
}

/// Map a usearch engine failure (a `cxx::Exception`, not a type we name — the crate
/// doesn't re-export it) into the opaque [`AnnError::Engine`] variant.
pub(crate) fn engine_err(e: impl std::fmt::Display) -> AnnError {
    AnnError::Engine(e.to_string())
}

// ── File naming ─────────────────────────────────────────────────────────────

/// The index file for a volume's space, beside its `media-{id}.db`:
/// `media-{id}.clip.usearch`.
pub(crate) fn index_path(db_path: &Path, space: AnnSpace) -> PathBuf {
    sibling(db_path, space, "usearch")
}

/// The JSON sidecar pinning format/model/dims/rows: `media-{id}.clip.usearch.meta`.
pub(crate) fn meta_path(db_path: &Path, space: AnnSpace) -> PathBuf {
    sibling(db_path, space, "usearch.meta")
}

/// The crash-detection marker: present while the writer holds unflushed ops, so a
/// session that dies mid-pass leaves it behind: `media-{id}.clip.usearch.dirty`.
pub(crate) fn dirty_path(db_path: &Path, space: AnnSpace) -> PathBuf {
    sibling(db_path, space, "usearch.dirty")
}

fn sibling(db_path: &Path, space: AnnSpace, ext: &str) -> PathBuf {
    let stem = db_path.file_stem().and_then(|s| s.to_str()).unwrap_or("media");
    db_path.with_file_name(format!("{stem}.{}.{ext}", space.suffix()))
}

/// Delete a volume's index, sidecar, and dirty marker for one space. Used by the
/// disposable-cache paths: the media DB's schema-mismatch recreate, the CLIP
/// delete-model prune, a volume purge, and the crashed-session wipe.
pub(crate) fn delete_index_files(db_path: &Path, space: AnnSpace) {
    for path in [
        index_path(db_path, space),
        meta_path(db_path, space),
        dirty_path(db_path, space),
    ] {
        if path.exists()
            && let Err(e) = std::fs::remove_file(&path)
        {
            log::warn!(target: "media_index", "removing ann file {} failed: {e}", path.display());
        }
    }
}

// ── Sidecar meta ────────────────────────────────────────────────────────────

/// The sidecar's contents: everything needed to decide "is this index file still
/// the right derivative?" without opening it. `rows` is informational (the count at
/// last save); staleness detection rides the dirty marker, not a count compare, so
/// a mid-pass query never mistakes normal write lag for corruption.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AnnMeta {
    pub(crate) format: u32,
    pub(crate) model_id: String,
    pub(crate) dims: usize,
    pub(crate) rows: u64,
    /// SHA-256 of the index file at last save. Verified before EVERY load/view:
    /// usearch trusts the bytes it maps, so a corrupt body would otherwise crash at
    /// search time instead of failing closed into the brute-force fallback.
    pub(crate) checksum: String,
}

/// Hash the index file and compare against the sidecar's checksum ([`AnnMeta`]).
/// The one gate that makes a corrupt index fail CLOSED (fallback + rebuild) instead
/// of crashing inside a usearch mmap view.
pub(crate) fn verify_index_checksum(db_path: &Path, space: AnnSpace, meta: &AnnMeta) -> Result<(), AnnError> {
    let actual = index_file_checksum(db_path, space)?;
    if actual != meta.checksum {
        return Err(AnnError::Corrupt);
    }
    Ok(())
}

/// SHA-256 of the on-disk index file (streamed; reuses the CLIP installer's hasher).
fn index_file_checksum(db_path: &Path, space: AnnSpace) -> Result<String, AnnError> {
    super::clip::install::sha256_file(&index_path(db_path, space))
        .map_err(|e| AnnError::Io(std::io::Error::other(e.to_string())))
}

/// Read and validate the sidecar against the CURRENT format and `model_id`.
/// `Missing` when absent, `MetaIncompatible` when it parses but pins a different
/// world (or doesn't parse — same remedy: rebuild).
pub(crate) fn read_meta(db_path: &Path, space: AnnSpace, model_id: &str) -> Result<AnnMeta, AnnError> {
    let path = meta_path(db_path, space);
    if !path.exists() {
        return Err(AnnError::Missing);
    }
    let bytes = std::fs::read(&path)?;
    let meta: AnnMeta = serde_json::from_slice(&bytes).map_err(|_| AnnError::MetaIncompatible)?;
    if meta.format != ANN_FORMAT_VERSION || meta.model_id != model_id || meta.dims == 0 {
        return Err(AnnError::MetaIncompatible);
    }
    Ok(meta)
}

/// Write the sidecar via temp+rename (safe-overwrite, like the index itself).
pub(crate) fn write_meta(db_path: &Path, space: AnnSpace, meta: &AnnMeta) -> Result<(), AnnError> {
    let path = meta_path(db_path, space);
    let tmp = path.with_extension("meta.tmp");
    let bytes = serde_json::to_vec(meta).map_err(|e| AnnError::Engine(e.to_string()))?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

// ── usearch construction ────────────────────────────────────────────────────

/// The index options every open path shares: cosine metric + f16 storage (matching
/// the M3 f16 blobs), library-default connectivity/expansions (the spike's
/// configuration; `expansion_search` is then tuned per open via
/// [`expansion_search_for`]).
pub(crate) fn index_options(dims: usize) -> IndexOptions {
    IndexOptions {
        dimensions: dims,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F16,
        connectivity: 0,     // library default
        expansion_add: 0,    // library default
        expansion_search: 0, // library default; tuned per open
        multi: false,
    }
}

/// Scale usearch's `expansion_search` (ef) with the corpus size. Spike numbers
/// (`docs/notes/ann-vector-search-spike-2026-07-24.md`): at 200k, ef 128 gives
/// 0.994 recall@10 at 0.30 ms p50; at 1M, recall at fixed ef decays (0.895 at 128)
/// and recovers by raising it (0.958 at 256, 0.982 at 512, still ≤1.6 ms p50). So:
/// 128 up to ~300k, 256 to ~700k, 512 beyond — latency stays low single-digit-ms
/// across the whole range, so erring toward higher recall is cheap.
pub(crate) fn expansion_search_for(count: usize) -> usize {
    match count {
        0..=300_000 => 128,
        300_001..=700_000 => 256,
        _ => 512,
    }
}

/// Save `index` to `final_path` atomically: write a `.tmp` sibling, then rename.
/// A crash never leaves a torn index, and a live mmap `view` of the old file keeps
/// the old inode until it's dropped.
pub(crate) fn save_index_atomically(index: &usearch::Index, final_path: &Path) -> Result<(), AnnError> {
    let tmp = final_path.with_extension("usearch.tmp");
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| AnnError::Io(std::io::Error::other("non-utf8 ann index path")))?;
    index.save(tmp_str).map_err(engine_err)?;
    std::fs::rename(&tmp, final_path)?;
    Ok(())
}

// ── The per-file mutation lock ──────────────────────────────────────────────

/// One mutex per index file, serializing the two file mutators: the writer thread's
/// [`flush_ops`] and the background [`rebuild`]'s install. Both replace the file via
/// temp+rename; the lock ensures a flush can't load the file, lose a rebuild's
/// rename, and save over it.
static FILE_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn file_lock(db_path: &Path, space: AnnSpace) -> Arc<Mutex<()>> {
    let key = index_path(db_path, space);
    Arc::clone(FILE_LOCKS.lock_ignore_poison().entry(key).or_default())
}

// ── Writer-fed incremental ops ──────────────────────────────────────────────

/// One buffered index mutation, produced by the writer thread as it commits the
/// matching DB write. Keys are `media_file` ids as `u64`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AnnOp {
    /// Insert or overwrite the vector under `key` (a CLIP embed or re-embed).
    Upsert { key: u64, vector: Vec<f32> },
    /// Remove `key` (GC/prune of the row, or a CLIP write that cleared the
    /// embedding). Removing an absent key is a no-op.
    Remove { key: u64 },
}

/// What a flush did — for logging and the writer's bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlushOutcome {
    /// Nothing pending; untouched.
    NoOps,
    /// No index file exists (small corpus, or awaiting its first rebuild): the ops
    /// were dropped — the rebuild that creates the index reads the DB, which
    /// already holds them.
    NoIndex,
    /// The existing file/sidecar was stale or unloadable, so the files were deleted
    /// (the next query kicks a rebuild) and the ops dropped.
    DroppedStale,
    /// The ops were applied and the index saved; `rows` is the new live count.
    Flushed { rows: u64 },
}

/// Apply a batch of buffered ops to the on-disk index: load it to the heap, apply
/// in arrival order (an upsert removes the key first, so re-embeds overwrite), save
/// temp+rename, refresh the sidecar, and clear the dirty marker. Runs under the
/// per-file lock. Ops are idempotent under replay (an upsert overwrites, a remove
/// of an absent key is a no-op), which is what makes the rebuild race benign: ops
/// buffered while a rebuild snapshot was being built simply re-apply on top of it.
///
/// Any unusable existing file (bad sidecar, unloadable index) is DELETED here so
/// the query path sees a clean "missing" and rebuilds — a failed flush must never
/// leave a half-truth on disk.
pub(crate) fn flush_ops(db_path: &Path, space: AnnSpace, model_id: &str, ops: Vec<AnnOp>) -> FlushOutcome {
    if ops.is_empty() {
        // Nothing pending means the on-disk index agrees with every committed write,
        // so a dirty marker left by an op-less mutation (a GC that deleted nothing)
        // can be cleared rather than triggering a pointless wipe next session.
        remove_dirty(db_path, space);
        return FlushOutcome::NoOps;
    }
    let lock = file_lock(db_path, space);
    let _guard = lock.lock_ignore_poison();

    let idx_path = index_path(db_path, space);
    if !idx_path.exists() {
        // No index to keep current: the ops are already in the DB, and the rebuild
        // that eventually creates the index reads the DB.
        remove_dirty(db_path, space);
        return FlushOutcome::NoIndex;
    }
    let meta = match read_meta(db_path, space, model_id).and_then(|meta| {
        verify_index_checksum(db_path, space, &meta)?;
        Ok(meta)
    }) {
        Ok(meta) => meta,
        Err(e) => {
            log::info!(
                target: "media_index",
                "ann index at {} is stale ({e}); deleting for rebuild",
                idx_path.display()
            );
            delete_index_files(db_path, space);
            return FlushOutcome::DroppedStale;
        }
    };
    match apply_ops_to_file(&idx_path, &meta, &ops).and_then(|rows| {
        // Re-stamp the sidecar for the freshly-saved bytes (rows + checksum). This
        // MUST succeed for the flush to count: a stale checksum would read as
        // corruption on the next open and waste a rebuild.
        let checksum = index_file_checksum(db_path, space)?;
        write_meta(
            db_path,
            space,
            &AnnMeta {
                rows,
                checksum,
                ..meta.clone()
            },
        )?;
        Ok(rows)
    }) {
        Ok(rows) => {
            remove_dirty(db_path, space);
            FlushOutcome::Flushed { rows }
        }
        Err(e) => {
            log::warn!(
                target: "media_index",
                "ann flush failed for {} ({e}); deleting for rebuild",
                idx_path.display()
            );
            delete_index_files(db_path, space);
            FlushOutcome::DroppedStale
        }
    }
}

/// Load, mutate, and atomically re-save the index file. Returns the live row count
/// after the ops.
fn apply_ops_to_file(idx_path: &Path, meta: &AnnMeta, ops: &[AnnOp]) -> Result<u64, AnnError> {
    let idx_str = idx_path
        .to_str()
        .ok_or_else(|| AnnError::Io(std::io::Error::other("non-utf8 ann index path")))?;
    let index = usearch::new_index(&index_options(meta.dims)).map_err(engine_err)?;
    index.load(idx_str).map_err(engine_err)?;
    let adds = ops.iter().filter(|op| matches!(op, AnnOp::Upsert { .. })).count();
    index.reserve(index.size() + adds).map_err(engine_err)?;
    for op in ops {
        match op {
            AnnOp::Upsert { key, vector } => {
                // A vector whose dims don't match the index can't be stored; skip it
                // (the row itself is fine in the DB — a dims change is a model change,
                // which the sidecar's model pin turns into a rebuild).
                if vector.len() != meta.dims {
                    log::warn!(
                        target: "media_index",
                        "ann upsert skipped: {}-dim vector into a {}-dim index",
                        vector.len(),
                        meta.dims
                    );
                    continue;
                }
                // Overwrite semantics: usearch `add` doesn't replace an existing key,
                // so drop any prior vector first (absent key ⇒ no-op).
                let _ = index.remove(*key).map_err(engine_err)?;
                index.add(*key, vector).map_err(engine_err)?;
            }
            AnnOp::Remove { key } => {
                let _ = index.remove(*key).map_err(engine_err)?;
            }
        }
    }
    save_index_atomically(&index, idx_path)?;
    Ok(index.size() as u64)
}

// ── Dirty marker ────────────────────────────────────────────────────────────

/// Create the dirty marker. The writer calls this BEFORE the DB write of the first
/// buffered op commits, so a crash after the commit but before the flush is always
/// detectable (marker present ⇒ the on-disk index may lag the DB).
pub(crate) fn mark_dirty(db_path: &Path, space: AnnSpace) {
    let path = dirty_path(db_path, space);
    if let Err(e) = std::fs::write(&path, b"") {
        log::warn!(target: "media_index", "ann dirty marker write failed: {e}");
    }
}

fn remove_dirty(db_path: &Path, space: AnnSpace) {
    let path = dirty_path(db_path, space);
    if path.exists()
        && let Err(e) = std::fs::remove_file(&path)
    {
        log::warn!(target: "media_index", "ann dirty marker remove failed: {e}");
    }
}

/// The writer-spawn crash check: a dirty marker with no live writer means the
/// previous session died with unflushed ops, so the on-disk index silently lags the
/// DB — wipe it (the next query rebuilds from the DB, which is the truth). Called
/// once per writer spawn, before any write.
pub(crate) fn wipe_if_crashed(db_path: &Path, space: AnnSpace) {
    if dirty_path(db_path, space).exists() {
        log::info!(
            target: "media_index",
            "ann index for {} has a dirty marker from a previous session; wiping for rebuild",
            db_path.display()
        );
        delete_index_files(db_path, space);
    }
}
