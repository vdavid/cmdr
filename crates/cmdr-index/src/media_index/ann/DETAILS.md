# ANN vector search — details

The depth behind `CLAUDE.md` (plan M6). Read this before any non-trivial work here: editing, planning, reorganizing, or
advising.

## The engine choice

CLIP text→image search over a per-volume `usearch` HNSW index, so semantic search stays low-ms as a corpus scales past
what the exact resident-f16 scan can serve. Engine chosen by a measured spike
(`docs/notes/ann-vector-search-spike-2026-07-24.md`): at 200k vectors usearch answers in 0.30 ms p50 at 0.994 recall@10
from an mmap-backed view, where `sqlite-vec` 0.1.9 turned out to be an exact linear scan (141 ms p50, not ANN at all).
Files: `media-{id}.clip.usearch` beside the media DB, plus a JSON sidecar (`….usearch.meta`: format version, model id,
dims, rows, SHA-256 of the index file) and a transient dirty marker (`….usearch.dirty`). The module is
dimension-generic: `AnnSpace` names the space (table + file suffix + model identity), so the 768-d Vision feature print
(similar-images/dedup) adopts ANN later by adding a variant — deliberately NOT wired now.

## Decisions

**The writer thread owns incremental index mutations (single-writer discipline).** The `MediaWriter` loop buffers an
`AnnOp` (upsert/remove, keyed by the `media_file` id as `u64`) beside every CLIP write/delete it commits, and lands the
batch via `flush_ann_index()` at exactly the seams that invalidate the resident vector cache (local/network pass
completion, live tick, reclaim prune, retro-delete), plus an in-writer auto-flush at 8,192 pending ops. usearch has no
in-place file mutation, so a flush loads the index to the heap, applies the ops in order (an upsert removes the key
first, so re-embeds overwrite), and saves temp+rename — a live mmap view keeps the old inode, and a crash never leaves a
torn file. The one writer-external mutator is the background rebuild, and its install serializes with flushes on a
per-file mutex (`ann::file_lock`). Accepted cost: a flush's load+save is linear in index size (~235 MB of I/O and
transient heap at 200k), paid once per pass seam, not per image.

**A flush RETAINS its buffer while a rebuild is in flight (`rebuild::is_in_flight`), never applies or drops.** The
rebuild's DB snapshot predates rows committed after it opened its read connection, so a flush landing mid-rebuild would
lose those ops every way it could resolve: applied to the old file (the install then overwrites it), dropped against a
missing file (`NoIndex` — but the in-flight rebuild's snapshot doesn't have the rows either), or dropped with a
stale-file wipe. So the writer keeps the ops AND the dirty marker for the whole rebuild window and replays the batch
idempotently on the installed index at the next seam flush (upserts overwrite, removes of absent keys no-op — replay
safety is what makes retention sufficient). The reverse race is benign: a rebuild kicked right after `is_in_flight`
returned false snapshots AFTER the flush's DB writes committed, so it includes them. A writer shutdown mid-rebuild also
retains — deliberately: the marker survives the session and the next spawn wipes the possibly-lagging index for a fresh
rebuild, conservative over silent loss. The pending buffer may exceed its auto-flush bound during the window, bounded by
the rebuild's duration (minutes at worst).

**Crash detection is a dirty marker, not a row-count compare.** The writer creates the marker BEFORE the first buffered
op's DB write commits and the flush removes it after a successful save, so a session that dies with unflushed ops leaves
it behind and the next writer spawn wipes the index (the next query rebuilds from the DB, the truth). A count-compare at
query time would misread normal mid-pass write lag as corruption and rebuild-storm during enrichment. Until the wipe, a
lagging index only under-returns (missing newest vectors); it can never return wrong paths, because hits resolve ids
through the DB.

**Verify a SHA-256 of the index file before EVERY load/view.** usearch trusts the bytes it maps: viewing a garbage file
SIGSEGVs (observed in tests), so corruption cannot be caught at open time by the engine itself. The sidecar carries the
checksum from the last save; a mismatch fails closed (`AnnError::Corrupt` → exact-scan fallback + background rebuild).
Cost: one streamed hash per open/flush (~0.1 s per GB), paid at pass seams, not per query.

**Brute force below `ANN_MIN_VECTORS` = 50,000 vectors.** Below it the exact scan is ≤ ~19 ms from a ≤ ~50 MB resident
cache (74 ms/205 MB measured at 200k, linear) — exact, with no index file to build, maintain, or store. At/above it,
latency and RAM grow linearly while HNSW stays sub-ms over an evictable mmap. No index file is ever created below the
threshold (a flush with no index drops its ops); crossing it makes the first query kick the rebuild.

**Over-fetch 4× + exact re-rank, so ORDERING stays exact-quality.** The ANN route fetches `k × 4` candidates, reads each
candidate's stored f16 vector and CURRENT path from the DB, re-scores with the same `cosine_f16` the exact scan uses,
and returns the top `k`. HNSW recall dips as corpora grow (0.895–0.982 at 1M depending on `expansion_search` — spike);
re-ranking the over-fetched set exactly restores exact ordering for the k callers see, and the DB join both follows
renames (keys are stable `media_file` ids — a rename needs NO index touch) and silently drops ghost keys (a key whose
row is gone yields nothing). Measured on the real corpora below: the re-rank's read-and-rescore adds well under a
millisecond at k = 10.

## Rebuild, versioning, lifecycle

**Rebuild** (`rebuild.rs`): background thread, single-flight per index file, streams `(file_id, f16)` rows from the DB
(no whole-corpus `Vec`), single-threaded on purpose (spike: 71 s per 200k; usearch `add` is thread-safe, so a parallel
build is a future lever), polls the memory watchdog's cancel every 1,024 adds (deliberately NOT `gate::should_stop`:
queries can't kick a rebuild while the master toggle is off, and the watchdog cancel is the "release resources now"
signal that matters). Triggered by the query-side route whenever the index is missing, corrupt, or sidecar-incompatible;
search answers exactly via the fallback until it lands. `expansion_search` scales stepwise with corpus size
(`expansion_search_for`: 128 ≤ 300k, 256 ≤ 700k, 512 beyond — the spike's recall-vs-ef curve).

**Versioning + lifecycle:** the sidecar pins `ANN_FORMAT_VERSION` and the space's model id (`CLIP_MODEL_ID`, no OS
component — OS-drift re-embeds flow through writer upserts row by row), so a model bump or index-format change reads as
`MetaIncompatible` and rebuilds. The index is versioned independently of `SCHEMA_VERSION`, and the disposable-cache
paths all take it along: `MediaStore::delete_and_recreate` (schema wipe), `PruneAllClip` (delete model), `PurgeVolume`,
and the crashed-session wipe. `cache` holds the query-side route + warm view per volume, invalidated and dropped through
`vector::cache`'s `invalidate` / `clear_all` rather than its own seams (`../vector/DETAILS.md`), so every existing
invalidation site stays correct without naming this layer.

## Testing

`tests.rs` pins the flush/rebuild races real red→green:
`a_flush_during_an_in_flight_rebuild_retains_ops_and_replays_them_after`,
`a_shutdown_during_an_in_flight_rebuild_keeps_the_marker_and_the_next_spawn_wipes`, and
`a_rename_touches_neither_the_index_nor_the_dirty_marker_and_hits_follow`. **Measured on real embeddings** (M6
verification, 2026-07-24, M3 Max): the harness `real_corpus_recall_and_latency` runs against copies of the real
`media.db`s and records recall@10 and before/after latency numbers.
