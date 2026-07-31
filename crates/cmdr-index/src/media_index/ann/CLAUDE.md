# ANN vector search

A per-volume `usearch` HNSW index (`media-{id}.clip.usearch` + a JSON sidecar) so CLIP semantic search stays low-ms as a
corpus scales past what the exact resident scan serves. `mod.rs` is the index + flush, `cache.rs` the query-side route +
warm view, `rebuild.rs` the background rebuild. `AnnSpace` keeps it dimension-generic, so the 768-d feature print can
adopt ANN later by adding a variant.

## Must-knows

- **usearch TRUSTS the bytes it maps: viewing a garbage file SIGSEGVs** (observed in tests), so corruption cannot be
  caught at open time by the engine. Verify the sidecar's SHA-256 before EVERY load/view; a mismatch fails closed
  (`AnnError::Corrupt` → exact-scan fallback + background rebuild). ❌ Never open an index without the hash check.
- **The writer thread owns incremental mutations.** It buffers an `AnnOp` beside every CLIP write/delete it commits and
  lands the batch at the same seams that invalidate the resident vector cache. ❌ No other code path mutates the index,
  except the background rebuild's install, which serializes on `ann::file_lock`.
- **A flush during an in-flight rebuild RETAINS its buffer** (`rebuild::is_in_flight`) — ❌ never applies it, never
  drops it. The rebuild's DB snapshot predates those rows, so every other resolution loses them. Replay is idempotent
  (upserts overwrite, removes of absent keys no-op), which is what makes retention sufficient.
- **Crash detection is the dirty marker, ❌ never a row-count compare.** The marker is created BEFORE the first buffered
  op's DB write commits and removed after a successful save, so a session that dies with unflushed ops leaves it and the
  next writer spawn wipes the index. A count-compare would misread normal mid-pass write lag as corruption and
  rebuild-storm during enrichment.
- **Keys are `media_file` ids and hits resolve through the DB**, so a rename needs NO index touch and a ghost key
  (row gone) silently yields nothing. A lagging index can only UNDER-return; it can never return a wrong path.
- **Over-fetch `k × 4` and exactly re-rank** with the same `cosine_f16` the exact scan uses. HNSW recall dips as corpora
  grow, so ❌ don't return raw ANN order — the re-rank is what keeps the k the caller sees exact-quality.
- **No index file exists below `ANN_MIN_VECTORS` (50,000).** A flush with no index drops its ops; crossing the threshold
  makes the first query kick the rebuild. Don't add a code path that creates one early.
- **A rebuild polls the memory watchdog's cancel, ❌ not `gate::should_stop`** — a query can't kick a rebuild while the
  master toggle is off, and the watchdog cancel is the "release resources now" signal that matters.

The engine choice, the flush/rebuild decisions, versioning, and the measured numbers: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
