# Vector store + resident caches

Brute-force cosine in Rust over the stored embeddings, plus the per-volume resident caches the query paths read.
`mod.rs` is the math + `BruteForceVectorStore`, `cache.rs` the per-volume warm stores.

## Must-knows

- **The cache holds `f16` and scores against it directly.** `cosine_f16(query_f32, stored_f16)` widens each stored
  element inline, no temp `Vec`. ❌ Don't widen on load "for simplicity": that keeps the cache f32 and forfeits the RAM
  halving, and the brute-force scan is memory-bandwidth-bound, so half the bytes is also the faster shape.
- **Both cosine fns guard degenerate inputs** (zero magnitude or a length mismatch → `0.0`). ❌ Never let a `NaN` reach
  the ranking; it poisons the sort silently.
- **`cache::invalidate` is the ONE choke point** for "this volume's derived query caches changed" — it drops BOTH
  embedding tables' stores AND the ANN route/view cache, so every existing seam stays correct without naming the ANN
  layer. ❌ Don't drop a single cache entry directly; add your seam to this call.
- **Invalidate per COMPLETED pass, ❌ never per write** — per-write would thrash-reload mid-pass. Eventual consistency
  until a pass completes is accepted and deliberate.
- **`clear_all` is the memory-watchdog stop hook**, so resident vectors count against the ONE shared memory ceiling.
  Anything new and resident here must be droppable from it.
- **Widen ONCE, not per pair.** `dedup_clusters` widens each vector to f32 a single time (O(n)) before comparing; a
  per-pair widen is O(n²) work for the same answer.

The brute-force ranking, dedup clustering, the f16 rationale, and the cache lifecycle: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
