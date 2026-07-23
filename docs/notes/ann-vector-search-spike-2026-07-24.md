# ANN vector-search spike: sqlite-vec vs usearch (M6, 2026-07-24)

Evidence behind the M6 recommendation in `../specs/resource-use-plan.md`. Empirical comparison of the two candidate
vector-search engines on a synthetic CLIP-scale corpus, plus the brute-force f16 baseline that shows what ANN buys.

**Recommendation: `usearch`.** At 200k vectors it answers in 0.30 ms (p50) at 0.994 recall@10, where `sqlite-vec` 0.1.9
turns out not to be ANN at all (exact linear scan, 141 ms p50 at 200k, ~1.4 s extrapolated at 2M). Details below; the
decision summary lives in the plan's M6 addendum.

## Environment

- Apple M3 Max, 64 GB RAM, macOS 26.5.2 (build 25F84), rustc 1.97.1, release builds, 2026-07-24.
- Harness (standalone cargo project, fixed seeds, reproducible):
  `/private/tmp/claude-501/-Users-veszelovszki-projects-git-vdavid-cmdr/733dfd6f-550d-4e88-80f8-df2cf9cf56ca/scratchpad/m6-ann-spike/`
  (session scratchpad; its `README.md` has exact run commands, `results/results.jsonl` the raw records). The scratchpad
  is session-scoped, so treat this note as the durable record.
- Engines: `sqlite-vec` 0.1.9 (crates.io, MIT OR Apache-2.0, via `rusqlite` 0.40.1 bundled) and `usearch` 2.26.0
  (crates.io, Apache-2.0; NEON acceleration active, reported by `index.hardware_acceleration()`).

## Method

- Corpus: 200,000 synthetic 512-d unit-norm vectors mimicking CLIP embeddings: 200 Gaussian cluster centers (seed 7),
  vector = center + per-dim N(0, 0.025) noise, normalized; corpus seed 42. Queries: 1,000 held-out vectors from the same
  distribution (seed 43).
- Ground truth: exact f32 brute-force cosine top-10 per query, computed once and cached. Recall@10 = mean fraction of
  the true top-10 found in each engine's top-10.
- Latency: single query at a time, warm (100-query warmup), p50/p95 over 1,000 queries. RAM: process RSS deltas, each
  bench in its own process, 1 s settle after index load.
- Upsert: add vectors 190,000..200,000 to a prebuilt 190k index (sqlite-vec in batches of 100 inside transactions,
  usearch one by one), wall time per vector.
- Storage matches the app where each engine allows: usearch stores f16 (`ScalarKind::F16`, matching M3's f16 blobs);
  sqlite-vec stores f32 (see finding below). Brute-force baseline scans resident f16, widening inline like `cosine_f16`.

## Results (200k vectors, 512-d, cosine)

| Metric                             | Brute-force f16 (baseline) | sqlite-vec 0.1.9            | usearch 2.26.0 (HNSW)                                      |
| ---------------------------------- | -------------------------- | --------------------------- | ---------------------------------------------------------- |
| Query p50                          | 74.4 ms                    | 140.9 ms                    | 0.21 ms (ef 64) / 0.30 ms (ef 128)                         |
| Query p95                          | 84.6 ms                    | 160.9 ms                    | 0.37 ms (ef 64) / 0.38 ms (ef 128)                         |
| Recall@10                          | 0.9987 (f16 rounding)      | 1.0 (exact scan)            | 0.9604 (ef 64) / 0.9939 (ef 128)                           |
| RAM at rest (RSS delta)            | 204.8 MB (resident f16)    | 9.2 MB (reads from disk)    | 257 MB heap-loaded; mmap `view`: 47 MB at rest (see below) |
| On-disk index                      | none (vectors in media DB) | 416.4 MB (2,082 B/vec, f32) | 234.5 MB (1,173 B/vec, f16 + graph)                        |
| Full build (200k)                  | n/a                        | 3.8 s (bulk insert)         | 71.2 s (single thread)                                     |
| Incremental upsert (10k into 190k) | n/a                        | 0.39 s total, 39 µs/vec     | 4.7 s total, 467 µs/vec                                    |
| License                            | n/a                        | MIT OR Apache-2.0           | Apache-2.0                                                 |
| Binary-size delta                  | n/a                        | +91 KB                      | +1.03 MB                                                   |

Binary-size deltas are release arm64 bins vs a 2.26 MB baseline that already links `rusqlite` bundled (Cmdr ships SQLite
anyway); probes in the harness `size-probes/`. The usearch mmap `view` RSS grows to ~246 MB once all 1,100 queries have
touched pages, but those pages are file-backed page cache and evictable under memory pressure.

**2M extrapolations** (linear in corpus size, labeled as extrapolations): brute-force f16 ~2.05 GB resident and ~744 ms
p50; sqlite-vec ~1.4 s p50 (it scans linearly); usearch disk ~2.3 GB, heap-loaded RSS ~2.6 GB, mmap-at-rest small until
queries touch pages. HNSW query latency grows ~logarithmically; the 1M measured run below supports that (0.30 → 0.68 ms
p50 for 5x the corpus).

## 1M sanity run (usearch, measured, not extrapolated)

Same method, `SPIKE_N=1000000` (1M corpus, fresh ground truth):

- **Build (single thread)**: 768.5 s (12.8 min); disk 1.17 GB (the same 1,173 B/vector, linear).
- **Query p50 / p95 (ef 128)**: 0.73 ms / 1.32 ms heap-loaded, 0.68 ms / 0.88 ms mmap view.
- **Recall@10 decays at fixed expansion and recovers by raising it** (mmap view): 0.895 at ef 128 (0.68 ms p50), 0.958
  at ef 256 (1.08 ms), 0.982 at ef 512 (1.57 ms). Latency stays low single-digit-ms even at ef 512.
- **RSS**: 1,286 MB heap-loaded; mmap view 206 MB at rest, ~1,231 MB once all 1,100 queries touched pages (page cache,
  evictable).
- Caveat: this corpus packs 1M points into the same 200 clusters (5,000 near neighbors per cluster), a deliberately
  hard, dense case for HNSW recall; real CLIP corpora spread wider. Treat the ef-vs-recall curve as the shape to
  re-measure on real embeddings, not as absolute numbers.

## Findings

1. **sqlite-vec 0.1.9 is not ANN.** Its `vec0` KNN is an exact linear scan: recall 1.0 and 141 ms p50 at 200k, twice the
   latency of our own resident-f16 scan (it reads f32 from SQLite pages and can't keep the vectors resident). At 2M
   that's ~1.4 s per keystroke-triggered search: it fails M6's "single-digit-ms at 2M" gate outright. ANN (IVF/HNSW) is
   on sqlite-vec's roadmap but not in any stable release (verified against v0.1.9 behavior + the 0.1.10-alpha series on
   crates.io, 2026-07-24).
2. **sqlite-vec silently ignores f16 storage.** `CREATE VIRTUAL TABLE ... (embedding float16[512])` parses, but blobs
   are interpreted as f32 (a 1,024-byte f16 blob for a 4-dim probe column errors with "Expected 4 dimensions but
   received 2"), and disk comes out f32-sized (2,082 B/vector). Measured with the harness `probe` subcommand.
3. **usearch delivers the M6 promise.** 0.30 ms p50 at 0.994 recall@10 (expansion_search 128), an mmap-backed `view`
   mode whose at-rest RSS is ~47 MB with pages faulted in on demand and evictable under pressure, and f16 storage
   matching M3's format. Defaults (connectivity 16, expansion_search 64) give 0.96 recall; expansion_search 128 buys
   0.994 for +0.1 ms. Recall at fixed expansion decays as the corpus grows (the 1M run above), so treat expansion_search
   as a corpus-size-scaled tuning knob (128 at 200k, 256–512 toward 1M+), re-measured on real embeddings; latency stays
   low single-digit-ms across that whole range.
4. **Upserts are cheap in both, build is the cost that matters.** usearch's 467 µs/vector upsert is three orders of
   magnitude faster than enrichment produces vectors (ANE-bound at ~8 images/s, M2 spike), so writer-fed incremental
   upserts are a non-issue. The 200k full build at 71 s single-thread (usearch `add` is thread-safe; a parallel rebuild
   can cut this) sets the cost of the full-rebuild path.
5. **Licenses are clean for BSL-commercial Cmdr.** MIT OR Apache-2.0 (sqlite-vec) and Apache-2.0 (usearch, including its
   vendored SimSIMD kernels); nothing copyleft. Versions verified on crates.io 2026-07-24, both ≥3 days old (sqlite-vec
   0.1.9 published 2026-03-31, usearch 2.26.0 published 2026-07-10).

## Caveats

- Synthetic clusters are friendlier than real CLIP output; recall on real embeddings should be re-checked against the
  importance-evals harness + dog-search QA set during integration (the plan already requires this).
- Latencies are per-process warm numbers on an idle M3 Max; a busy app shares memory bandwidth, but at 0.3 ms there is
  two orders of magnitude of headroom to the ~30 ms interactivity bar.
- The mmap `view` RSS grows toward the full index size as queries touch pages; the win is that those pages are
  file-backed and evictable, unlike the brute-force resident cache.
