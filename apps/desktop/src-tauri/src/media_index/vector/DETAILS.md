# Vector store + resident caches — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## The store (plan Decision 2)

Brute-force cosine in Rust, NO `sqlite-vec` (a loadable extension our `rusqlite` isn't built for; a real build+signing
project adopted only if a library outgrows brute force, behind this same `VectorStore` trait). The store holds vectors
as **`f16`** (plan M3 — half the RAM of an f32 cache; the encoding and the precision evidence are in
`../store/DETAILS.md` § The f16-embedding + integer-id decisions). `cosine` (f32↔f32, the query↔query case) and
`cosine_f16` (an f32 query vs an f16 stored vector, widened per element) both guard degenerate inputs (zero magnitude /
length mismatch → `0.0`, never `NaN`).

`BruteForceVectorStore::top_k` linearly ranks by `cosine_f16` (source excluded, ties by path); `dedup_clusters` groups
near-duplicates by single-linkage union-find over pairs at/above a cosine threshold (default 0.9), widening each vector
to f32 once (O(n), not per pair) then comparing via `cosine_f16`, returning clusters of two or more.

**Why score against f16 directly (widen per element), not widen-on-load:** widening on load would keep the cache f32 and
forfeit the RAM halving; the brute-force scan is memory-bandwidth-bound, so f16 entries (half the bytes) keep or improve
query latency while halving RAM — the plan's "measure, pick the simpler one that keeps latency" resolved in f16's favor
by that bandwidth argument.

## The resident cache (`cache.rs`)

`cache` keeps a load-once `BruteForceVectorStore` per volume PER embedding table (keyed
`(media.db path, EmbeddingTable)`, so the Vision feature print and CLIP have independent warm stores), mirroring
`search/`'s warm `SEARCH_INDEX` arena, so a find-similar/dedup/semantic query doesn't reload the BLOBs each call (all
query-time work runs OFF the IPC thread via `spawn_blocking`).

- **`invalidate(db_path)`** drops both of a volume's stores AND the ANN route/view cache. It's the ONE choke point for
  "this volume's derived query caches changed", which is what lets every existing invalidation seam (pass completion,
  live tick, reclaim prune, retro-delete, purge) stay correct without naming the ANN layer.
- **Invalidated per COMPLETED enrichment pass, not per write** — per-write would thrash-reload mid-pass; the plan
  accepts eventual consistency until a pass completes.
- **`clear_all`** drops every cached store (plus the ANN caches) from the memory-watchdog stop hook, so the resident
  vectors are counted against the ONE shared resident-memory ceiling rather than growing beside it. The next query
  reloads lazily.

Above `ANN_MIN_VECTORS` the semantic query routes to the ANN index instead of this exact scan; the threshold and the
measurements behind it are in `../ann/DETAILS.md`.

## Testing

`tests.rs` is pure and real red→green on the risky bits: cosine (including the degenerate guards), `top_k` ranking and
source exclusion, dedup grouping, and the f16-vs-f32 scoring agreement.
