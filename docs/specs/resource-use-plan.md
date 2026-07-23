# Resource-use plan: make media indexing fast, small, and honest at NAS scale

**Goal**: cut the time, RAM, and disk cost of indexing and searching large image corpora (the measured reference:
David's NAS with 2,176,908 images of 2,571,575 files), while keeping the "respect the user's resources" principle. Ten
work items, ordered by decreasing impact. Items 1–5 are the "before the big index run" set: 3 and 4 change the media
DB's on-disk format via a plain `SCHEMA_VERSION` bump (the store is a disposable cache — delete-and-recreate, no
migrations), which is only cheap while corpora are small (9.7k local + 2.7k NAS rows today re-enrich in under an hour;
2M rows would re-enrich in days).

**Measured baselines feeding this plan** (2026-07-16, dev build, M3 Max, gigabit NAS):

- Enrichment throughput: ~60–80 images/min sustained, single worker (Vision + CLIP; the wire is ~4% utilized — decode +
  inference is ~85–90% of per-image wall time).
- Media DB cost: ~13 KB/image all-in (CLIP 2,048 B + Vision embedding 3,072 B + OCR/tags/FTS/status; measured 128 MB /
  9.75k images).
- Search RAM at 2M images, brute-force: ~4.4 GB (CLIP f32) + ~6.7 GB (Vision f32) if fully cached; latency ~100–300 ms
  per query (memory-bandwidth-bound).
- Coverage preview: ~1.4 s warm floor (importance reads), ~42 s cold walk after launch.
- CLIP model dir: 1.1 GB (keeps both the downloaded `.mlpackage` and the compiled `.mlmodelc`).
- Importance WAL: ~100% of DB size (103 MB WAL on a 101 MB root DB).

Execution model: one agent per milestone (M1–M2 sequential — same pass code; M5 fully parallel-safe; M7–M9 parallel-safe
among themselves; details per item). Run `pnpm check -q` per milestone, `--include-slow` at the very end.

---

## M1. Unstick NAS enrichment: right read path + honest error classification

**Why first**: NAS media enrichment is fully stalled today. Reproduced twice (19:31 and 20:49 on 2026-07-16): the
network pass opens `/Volumes/naspi/...` via the OS mount with `std::fs`, gets `EPERM` (macOS TCC "network volumes" — the
unsigned dev binary has no grant; rebuilds shed grants), the error is classified as _disconnected_, and the whole pass
pauses after 0 images. Everything else in this plan is moot for network volumes until this lands.

**Intent**: two defects, and they MUST land together — fix 2 alone would make the NAS pass silently _complete_ with
every file skipped (0 enriched), and a completed pass is worse than today's pause: the SMB auto-resume machinery retries
paused-disconnected passes, not completed ones.

1. **Read through the volume backend, not the OS mount.** When the volume has a Direct smb2 session, enrichment must
   read image bytes via the `Volume` trait (the session Cmdr owns — no TCC, no foreign-mount surprises). The OS-mount
   `std::fs` read stays only as the path for volumes that are _not_ direct (plain OS-mounted shares), where it's
   legitimate. This is NOT a drop-in seam swap: today's `ByteFetcher::fetch(os_path, timeout)` is
   sync-on-a-throwaway-thread and takes an OS path; the direct route goes through the async `Volume::open_read_stream`
   and carries a volume handle + index-relative path, so it needs a sync bridge. It also **overturns a documented
   decision**: `media_index/DETAILS.md` § "The byte-fetch decision — why the OS mount, not a direct smb2 client" must be
   superseded (rewritten with the new rationale: TCC denies the OS mount for unsigned/dev binaries and can regress
   per-binary; the direct session is the connection Cmdr already owns and health-checks), never left contradicting the
   code.
2. **Per-file errors never pause the pass.** `EPERM`/access-denied on one file = skip it, count it, log the total
   honestly at pass end ("N skipped: unreadable"). Only a _typed_ connection-loss signal
   (`VolumeError::DeviceDisconnected` or the smb2 session reporting loss) pauses the pass as "disconnected". No string
   matching — typed variants only (`no-string-matching` rule; the current classification path is the thing to fix, not
   to imitate).

**Interaction with the just-landed SMB auto-resume** (commit `499027b5`): a pass paused by a _genuine_ disconnect now
gets revived by the reconnect → registration-bus chain. That machinery is verified; M1 makes sure a _non_-disconnect
error can't masquerade as one and dodge it.

**Tests (TDD, red first)**:

- Unit: classification — a per-file `PermissionDenied` produces skip-and-continue; a typed disconnect produces the
  pause. Write against the pass's error-handling seam with a fake volume.
- Integration: fake network volume where file k of n EPERMs → pass completes with n−1 enriched, 1 skipped.
- Live QA after merge: the NAS pass runs past `/Volumes/naspi/_todo_pics/...` and enrichment counts climb.

**Docs**: `media_index/DETAILS.md` decision (read-path routing + why), `CLAUDE.md` one-liner only if it clears the
must-know bar.

---

## M2. Parallel enrichment workers + Settings slider

**Why**: the single-worker pipeline leaves 4–6x on the table (compute-bound, wire at 4%). 3–4 weeks for the full NAS →
under a week.

**Intent**: a worker pool for the decode+inference stage; the user picks how much machine to give it.

- **Setting**: `mediaIndex.parallelism` (integer, default **2**, min 1, max = `std::thread::available_parallelism()`
  read at runtime — 16 on David's M3 Max). Slider in Settings next to the importance-threshold slider. Live-applied via
  the settings-applier (mirror how `mediaIndex.importanceThreshold` applies live). Label microcopy in sentence case,
  i18n keys in all 10 locales.
- **Backend shape — this is a Vision-backend threading redesign, not a scheduler-level pool.** Today ALL Vision work
  funnels through ONE dedicated 8 MB-stack thread that deliberately serializes calls and confines `!Send` CF objects to
  itself (see `media_index/DETAILS.md` and `backend/mod.rs`); you cannot feed it concurrently. Parallelism means N such
  workers, each with its own thread, stack, autoreleasepool, and request handlers — the serialize-per-thread and
  rayon-unsafe (XPC stack) constraints hold per worker. Results funnel to the existing single SQLite writer (keep the
  single-writer invariant — parallelize compute, never DB writes). **Prefetch bounded by BYTES, not file count** (a
  per-file cap is 256 MB — `network/fetch.rs::MAX_FETCH_BYTES` — and each in-flight decode holds a ~36 MB bitmap, so a
  count-based queue could buffer gigabytes on a RAW-heavy corpus): pick a byte budget that counts against the memory
  watchdog's ceiling. On a network volume the prefetch reads serialize on the one smb2 session — fine, compute
  dominates; don't expect wire-level parallelism. Cancellation: pass-stop must drain workers promptly (everything
  cancelable — design principle 3).
- **Politeness**: the existing enrichment throttles are network idle-gating, the bandwidth cap, and the shared memory
  watchdog — the pool inherits those. There is NO thermal throttle today (thermal state exists only as a diagnostics
  field); N workers pounding the ANE will heat an M3 Max, so **add thermal backoff as explicit new scope**:
  `NSProcessInfo.thermalState` is already wired and thread-safe in `diagnostics_snapshot.rs` — back the effective worker
  count down at `serious`/`critical`. Default **1** — genuinely today's behavior; the slider is explicit user consent
  for more (principle 5: never take more machine by default).
- **Measure before promising**: Vision (OCR + classify + feature-print) and CLIP both hit the ANE, which serializes, so
  inference may not scale much past N=2–3; decode (CPU) is the part most likely to scale. Start with a small spike
  measuring decode-vs-inference scaling at N ∈ {1, 2, 4, 8} on a 200-image NAS fixture, and set the milestone's success
  metric from that measurement (append it to this doc) rather than asserting a multiplier up front. The slider's _max_
  stays the CPU count.

**Tests**: pool respects N (including live-apply mid-pass), no double-enrichment of one path under concurrency (assert
via the status table), cancellation drains within a bound, bench fixture (100 local images) before/after with numbers in
the commit message. TDD the concurrency-correctness tests (red first with a deliberately racy fake), not the bench.

**Docs**: `media_index/DETAILS.md` architecture section; settings docs.

**Depends on M1** (same pass code; M1's read routing feeds the prefetcher). Sequential, same agent or consecutive
agents.

### Spike results and success metric (measured 2026-07-23, M3 Max, 200 local images from `~/Downloads`)

Measured with the ignored spike `media_index::backend::vision::spike` (decode-only vs full-analyze scaling at N ∈ {1, 2, 4, 8}, same image set, ANE warmed once):

- **Decode-only** (ImageIO decode, the CPU part): 58.7 → 117.9 → 208.4 → 316.7 img/s = **1.00x → 2.01x → 3.55x → 5.40x**. Scales near-linearly to N=2, sublinearly to ~5.4x at N=8 (performance-core count).
- **Full analyze** (decode + OCR + classify + feature-print): 6.3 → 8.0 → 8.0 → 7.5 img/s = **1.00x → 1.26x → 1.26x → 1.18x**. Plateaus at ~1.25x by N=2 and mildly REGRESSES at N=8 (scheduling/thermal contention).

**Reading**: at N=1, full analyze is ~159 ms/image and decode alone is ~17 ms/image, so inference (OCR + classify + feature-print, all on the ANE) is ~89% of per-image wall time and decode is ~11%. The ANE serializes inference in the framework, so it does NOT parallelize; decode scales with cores but is too small a share to move the total. **The plan's "4–6x on the table" premise (from the wire at 4%) does not hold for compute — the bottleneck is the ANE, not the wire.** The wire-at-4% only means the NAS fetch can overlap compute; it can't make the ANE faster.

**Success metric (measured, not asserted)**: parallel enrichment tops out at **~1.25x throughput at N=2** on this machine, with no gain (slight loss) beyond N=2. So:

- Default **1** is correct (conservative, byte-for-byte today's behavior); N=2 is the practical sweet spot (~25% faster); the slider's higher stops exist for future hardware / more-parallel inference backends, not present ANE gains.
- Thermal backoff capping the effective worker count LOW is aligned with the ANE ceiling, not a tax on throughput.
- On network volumes the extra win is fetch↔compute overlap (a fetch of image k+1 proceeds while image k infers), bounded by the same ANE ceiling; the byte-bounded prefetch is what makes that overlap safe, not a multiplier.

---

## M3. f16 embeddings

**Why**: halves the biggest per-image storage item (5 KB of f32 vectors → 2.5 KB) and halves search RAM. Precision loss
is far below ranking noise (cosine-similarity delta ~1e-3).

**Intent**: store CLIP (512-d) and Vision (768-d) vectors as f16 blobs; convert on write; score either by widening to
f32 on read or directly in f16 via Accelerate (implementer's choice — measure, pick the simpler one that keeps query
latency).

- **No migration machinery.** The media store is a disposable cache by explicit invariant (`media_index/store/mod.rs`:
  any `SchemaMismatch` → `delete_and_recreate()`, "no migrations"). M3 is a `SCHEMA_VERSION` bump; existing rows
  re-enrich on the next pass. That's the "do it while corpora are small" premise applied honestly: ~12.5k rows today
  re-enrich in well under an hour, and the big NAS run writes f16 from the start. Don't build
  migration/format-marker/interrupted-recovery infrastructure the subsystem is designed not to have.
- Keep the `dims` column so the blob width is self-describing.

**Tests (TDD)**: round-trip precision bound (cosine delta ≤ 1e-3 on a fixture); top-k search order preserved vs f32 on a
100-vector fixture; new writes are f16.

**Coordinate with M4 and M5**: M3+M4 are the same DB and land as **one `SCHEMA_VERSION` bump** (one agent). M5's
palettized model also changes embeddings, so sequence the M3/M4 bump and the M5 model-pin bump to land **before the same
next pass** — one re-enrich, not two. Never two agents in this schema concurrently.

### Status — done (branch `david/media-schema-slim`, with M4 in the same `SCHEMA_VERSION = 4` bump)

- Embeddings (CLIP 512-d + Vision feature print 768-d) persist as `f16` le blobs; `dims` column kept. `encode_embedding`
  → f16; `decode_embedding` widens to f32 (query direction); `decode_embedding_f16` loads f16 for the resident cache.
- **Scoring choice: score against `f16` directly, not widen-on-load.** The resident `BruteForceVectorStore` holds `f16`
  (halving RAM as well as disk); `cosine_f16` widens each stored element inline (no temp `Vec`). The brute-force scan is
  memory-bandwidth-bound, so half the bytes keeps or improves latency — the plan's "measure, pick the simpler one that
  keeps latency" resolved by that bandwidth argument rather than a micro-bench.
- Tests: f16 round-trip cosine ≤ 1e-3 (512-d fixture); top-k order preserved vs the exact f32 reference over 100 vectors
  (0.008 score gaps, scrambled path order); the store round-trip / tag-filter tests now assert within f16 tolerance.

---

## M4. Integer-id keying in the media DB

**Why**: every media table (`media_status`, `media_ocr*`, `media_tags`, `media_embedding`, `media_clip_embedding`) keys
rows by the full file path. NAS paths average ~80 B and appear once per table — gigabytes of pure duplication at 2M,
plus string-compare joins.

**Intent**: one `media_file(id INTEGER PRIMARY KEY, path TEXT UNIQUE)` table; children reference `file_id`. All lookups
by path go through one resolved id. Renames/moves become an UPDATE of one row.

- **Same no-migration story as M3**: part of the same single `SCHEMA_VERSION` bump; delete-and-recreate re-enriches. No
  data-carrying migration.
- Touch every read path: photo search, coverage counting, GC/retro-delete, the enrichment writer. Grep-audit for raw
  `path =` queries when done.

**Tests (TDD one representative read path)**: search_photos and coverage return identical results on a fixture written
under the new schema vs expected values; rename handling (one-row UPDATE reflected across children).

**Merges with M3** (one schema bump, one agent).

### Status — done (in the same `SCHEMA_VERSION = 4` bump as M3)

- `media_file(id INTEGER PRIMARY KEY, path TEXT UNIQUE)` is the identity table; `media_status`, `media_ocr`,
  `media_tags`, `media_embedding`, `media_clip_embedding` all key on `file_id`. Decided against merging `media_status`
  into `media_file` (they're 1:1) to keep identity separate from enrichment-state and match the plan's shape.
- **Read-path audit (every raw `path =` query became a `media_file` join):** store (`read_status`, `read_all_status`,
  `read_status_paths`, `sum_bytes_for_paths`, `read_all_embeddings_from`, `read_embedding_for`, `read_tag_matches`);
  read API (`search_ocr`, `facts_for_paths`, `images_with_tag`); coverage (`scan_accounted`); writer (upsert /
  upsert_clip / GC / prune / prune-prefix / purge). The Rust layer above the store stays path-addressed (reads join back
  to a path).
- Rename is `MediaWriter::rename_path`: a one-row `UPDATE media_file.path`, children follow via `file_id`; maintains the
  accounted aggregate across a parent-dir change. It's the seam a future rename-following hook calls (renames still
  manifest as GC+re-enrich until one is wired).
- Tests: rename moves all five children + the CLIP embedding and only those, refuses a taken destination / missing
  source, and moves the accounted unit between dirs; the existing search/coverage/facts read tests now exercise the
  join-based schema (read-path equivalence). All 229 `media_index` tests pass.

---

## M5. CLIP model slimming: palettize + drop the `.mlpackage` after compile

**Why**: the model dir is 1.1 GB per user because we keep the downloaded `.mlpackage` next to the compiled `.mlmodelc`,
and weights are un-palettized. Target ~350 MB and a faster first load.

**Intent**, two halves:

1. **Delete the `.mlpackage` after a verified compile** (guard: the compiled model must load and produce a sane
   embedding before the source is deleted; keep the sha-pinned zip contract intact for re-downloads). ~30-minute win,
   `media_index/clip/install.rs`.
2. **Palettize in the conversion pipeline** (`apps/desktop/scripts/convert-clip-model/`): 6-bit (or 8-bit if quality
   demands) weight palettization via coremltools, re-upload both towers to the HF repo as new files, bump the sha256
   pins + URLs in `install.rs`. Validate on the existing QA queries (the dog-search set) — embedding cosine vs the f32
   reference ≥ 0.99 on a 50-image fixture, and eyeball the top-10 for 3 queries.

**Tests**: install-flow unit tests (download → verify → compile → verified-load → delete source; failure at any step
keeps the source), embedding sanity harness in the convert pipeline.

**Parallel-safe in code** (different files) — but **its model-pin bump must land before the same next pass as M3+M4's
schema bump** (hard constraint, not a preference): a palettized model changes embeddings slightly, so re-embedding on
the next pass is the clean story, and landing the two bumps around the same pass means the corpus re-embeds ONCE. An
agent landing M5's pins ahead of the schema bump causes a second full re-embed.

### Status

- **M5a (delete `.mlpackage` after a verified compile) — done** (`clip/install.rs`, `clip/macos.rs`, branch
  `david/media-schema-slim`). On the `clip-worker`'s first load, once both towers load AND a zero-input encode is sane
  (512-d, all-finite — `verify_sane`, guarding against a NaN model), each `.mlpackage` source is deleted
  (`reclaim_source_package`), keeping only the compiled `.mlmodelc` (~550 MB saved). Fallback: `load_tower` prefers the
  cached `.mlmodelc`; a stale one (OS upgrade) is dropped and recompiled from the `.mlpackage` if present; if NEITHER a
  loadable compiled model nor a source remains it drops the stale compiled and returns `NotAvailable`, so `is_installed`
  (now **`.mlpackage` OR `.mlmodelc` per tower**) flips to `false` and `media_index_download_clip_model` refetches the
  pinned zip. Tradeoff (≈550 MB/user saved vs a rare ~200 MB re-download) documented in `install.rs` + `DETAILS.md`. The
  filesystem logic is unit-tested; the Core ML FFI around it isn't (needs a real model).
- **M5b (palettization) — done** (2026-07-23). The image tower is 8-bit palettized and pinned live in `install.rs`
  (uploaded to Hugging Face as `clip-image-p8.mlpackage.zip`, checksum-verified); `CLIP_MODEL_ID` is `-img8p`, so the
  corpus re-embeds on the next pass (the known, accepted second re-enrich). Per-tower 8/6-bit k-means palettization
  measured against the torch fp32 reference over a 50-image NAS fixture (6 reference prompts + 5 top-10 queries).
  Results:
  - **(a) image-8bit + text-fp — WINNER.** Image cosine min **0.9988** / mean **0.9995** (≥ 0.99 gate passes); text
    tower unchanged (cosine 1.0). Top-10 set overlap **9.6/10** mean (near-identical result sets, minor intra-set
    reordering). Download **~267 MB** (image 207.9 → **83.4 MB**, −60%; text stays 183.7 MB, now the dominant cost).
  - **(b) image-6bit + text-fp — rejected.** Image cosine min **0.9568** / mean **0.9838** — below the 0.99 gate. Top-10
    overlap 8.4/10. 6-bit is too lossy for the vision tower.
  - **(c) image-8bit + text-8bit — rejected, confirms the prior finding.** The 8-bit text tower's Core ML inference is
    **all-NaN** (build succeeds; predict NaNs), so top-10 is N/A. Text stays fp; only the image tower palettizes. Winner
    is image-only 8-bit — exactly the "image-only palettization is a legitimate win even if text stays fp" case. Pinned
    image tower: `clip-image-p8.mlpackage.zip`, sha `61ff585e…`, 83,447,408 B. Reproduce with `convert.py` at its
    defaults (`CLIP_IMAGE_NBITS=8`, `CLIP_TEXT_NBITS=0`); it parallelizes k-means via `num_kmeans_workers` and
    palettizes only the image tower, because coremltools closes its worker pool after the first `palettize_weights`
    call, so two palettize calls in one process raise "Pool not running".

---

## M6. ANN vector search

**Why**: brute-force at 2M is ~100–300 ms but needs the vectors resident (~2–4.5 GB after M3). ANN makes 2M-image search
single-digit-ms with a mostly-on-disk index — the gate between "works on a 64 GB dev machine" and "shippable to 16 GB
users".

**Intent**: pick the engine by a **spike, not a doctrine**: build a 200k-synthetic-vector benchmark comparing
`sqlite-vec` (SQL-native, simple ops story) vs `usearch` (HNSW, strong recall/latency) on: RAM at rest, query latency,
recall@10 vs brute force, incremental-upsert cost, index-rebuild time, license (`cargo deny check`), and binary-size
delta. Recommendation lands in this doc as an addendum before implementation.

- Integrate behind the existing photo-search API: per-volume index files next to the media DB, upserts fed by the
  enrichment writer, full-rebuild path for corruption/version bumps, brute-force fallback below ~50k vectors (simpler,
  and exact).
- Verify with the existing eval corpus (importance-evals harness + dog-search QA set).

**Biggest single effort in the plan — and the one item that may legitimately wait.** Post-M3 the brute-force resident
cost at 2M is ~2–4.5 GB with 100–300 ms queries: tolerable on the dev machine, not shippable as a default. If the
near-term goal is David's corpus rather than shipping 2M-scale search to users, M6 can sit behind a flag until a real
user approaches the wall — decide at execution time. Ordering that stays fixed either way: M5 (final model) before M6,
since the ANN index must be built over the final-model vectors. Start after M1–M5; independent of M7–M10.

---

## M7. `ext` column + partial index for the coverage cold walk

**Why**: the first coverage preview after launch walks the whole drive index (~42 s on the 5M-entry root DB) because
image-ness is computed from the name at query time.

**Intent**: add an `ext` column to the drive-index `entries` table (lowercased final extension) plus a partial index
covering image extensions **including RAW formats**. Writing `ext` touches TWO writer sites beyond the ALTER — the
throttled live upsert and the reconciler's rename pre-pass — each needing its own test; scope the writer change
explicitly, not just the reader. The walk (`media_index/coverage.rs` + `scheduler/enrich.rs::walk_image_entries`) uses
it to **prune directories with no image-extension files** — it does NOT replace per-directory qualification.
`qualify_dir` is sibling-aware (a RAW beside a same-stem JPEG defers; Live-Photo stills get tagged) and still runs over
the reduced set. This works because the one count-changing sibling relation (RAW vs JPEG) is between two image-extension
files that are both in the pruned index; a Live Photo's dropped `.mov` sibling doesn't change the enrichable count.

- **This is the first exception to `indexing/`'s no-migrations invariant** (`indexing/CLAUDE.md`: schema mismatch →
  delete and rebuild). It's justified here because a rebuild is a full filesystem rescan (very expensive on a 5M-entry
  NAS index), unlike the disposable media DB. Own the exception: a stored format marker so the ALTER runs once, and
  delete-and-rebuild stays the fallback on any failure.
- **Build in the background post-launch by default** (progress log line), not during startup — a stored column + index
  build on a 1.9 GB DB rewrites a lot of pages. Measure on a copy of the real root DB first; checkpoint the WAL after
  the build.

**Tests**: walk equivalence on a mixed fixture — TDD this, it's the correctness core, and it MUST cover the RAW+JPEG
same-stem pair and the Live-Photo still + `.mov` sibling explicitly; the once-only marker; cold-walk timing before/after
noted in the commit.

---

## M8. Importance-read caching

**Why**: the ~1.4 s warm floor on every coverage preview (slider drag) is attributed to importance reads — **verify this
first**. The qualifying-count walk is already cached (`coverage::get_or_build`), so measure where the 1.4 s actually
goes (the `above_threshold` importance read vs something else) before building; caching the wrong layer is wasted work.

**Intent** (assuming the measurement confirms importance reads): an in-memory per-volume snapshot of the weights the
coverage preview needs, invalidated on recompute. **Hook the subscription `media_index` already holds**: `wire_volume`
subscribes per-volume via `importance::read::subscribe(volume_id)` (the defer-until-scored bridge) — ride that, don't
add a second subscriber. (`search/index.rs`'s recompute subscriber is the same _pattern_ but is root-only and belongs to
search.) Target: warm preview < 100 ms.

**Tests**: snapshot invalidation on recompute (subscribe fires → next read is fresh), preview equivalence vs direct
reads on a fixture.

---

## M9. WAL checkpoint hygiene

**Why**: the importance WAL sits at ~100% of DB size (103 MB on root today) because the every-60s incremental rescore
churns pages and nothing truncates.

**Intent**: `PRAGMA wal_checkpoint(TRUNCATE)` at natural quiet points — pass completion and recompute completion — on
the importance and media DBs. Why the WAL sits at ~100%: no `wal_autocheckpoint` override is set, so SQLite's default
_passive_ autocheckpoint copies pages back but reuses the WAL file in place and never shrinks it; TRUNCATE is the
correct shrink. **Run it on the single writer thread** (never a side connection — the single-writer invariant), and
tolerate an occasional block from a long-lived reader snapshot without a retry loop. Cap expectation: WAL ≤ ~16 MB at
rest.

**Tests**: unit around the checkpoint hook (fires at completion, tolerates busy), manual before/after size check.

**Smallest item; bundle with M8 in one agent if convenient.**

### Status: shipped

`PRAGMA wal_checkpoint(TRUNCATE)` now runs on each DB's own writer thread at its natural quiet point: after every
importance recompute (full pass + the every-60s incremental, in `importance/scheduler/recompute.rs`) and after every
media enrichment pass that wrote rows (local + network seams in `media_index/scheduler/mod.rs`). Both writers gained a
`checkpoint_wal()` method + `run_wal_checkpoint` helper (`{importance,media_index}/writer.rs`) that brackets the
truncate with a short 250 ms busy timeout (mirroring the index writer's cap), degrades to PASSIVE on a reader-blocked
truncate, logs at debug, and never errors the pass. Decision/Why in each area's `DETAILS.md`.

---

## M10. Gate the Vision embedding (optional, last)

**Why**: the 768-d Vision feature-print costs 3 KB/image (1.5 KB post-M3) plus inference time, and serves **two**
features: similar-images AND near-duplicate detection (`dedup_clusters` via `vector/`). On a 2M bulk corpus that's ~3 GB
and real hours.

**Intent**: make it a choice rather than a tax — skip the feature-print on _network bulk_ passes by default, with a
setting to opt back in. Local volumes unchanged. Gating it degrades BOTH similar-images and dedup on network volumes —
the default is a two-feature tradeoff, present it to David that way. If either feature later needs a gap-fill, the
independent-staleness backfill pattern (the same one CLIP used after its install) already covers it.

**Deliberately last**: it trades against features; David decides the default after seeing M2+M3's numbers — it may be
unnecessary.

---

## Order and parallelization summary

- **Sequential spine**: M1 → M2 (same pass code; M2 starts with its scaling spike), then M3+M4 (one `SCHEMA_VERSION`
  bump, one agent).
- **Parallel-safe anytime**: M5 (model pipeline files only) — but sequence its model-pin bump with M3+M4's schema bump
  so the corpus re-enriches ONCE, not twice.
- **Parallel-safe cluster after the spine**: M7, M8+M9 (different DBs/modules; keep M7 and M9's checkpoint touch-points
  coordinated if simultaneous; M8 starts with its where-does-1.4s-go measurement).
- **M6** after M1–M5, standalone (biggest; spike first, addendum to this doc, then decide ship-now vs behind-a-flag).
- **M10** last, only if still wanted (two-feature tradeoff: similar-images AND dedup).

Every milestone: colocated `CLAUDE.md`/`DETAILS.md` updates ride along (per the docs rule — decision + why in DETAILS,
must-knows only in CLAUDE), `pnpm check -q` green before hand-back, allowlist warns surfaced never silenced, no pushes.

**Success criteria for the plan as a whole** (re-measure after M1–M5): NAS enrichment running unattended (no manual
revives) at the throughput M2's spike establishes (decode should scale near-linearly; inference is ANE-bound — the spike
sets the honest number); per-image disk ~9 KB (honest arithmetic: 13 KB baseline − 2.5 KB from f16 vectors − ~1–1.5 KB
from int-keying; the remaining ~6.5 KB is OCR text + FTS, untouched by this plan — M10 would shave another ~1.5 KB on
network volumes, and an OCR/FTS compaction is a possible future item, not promised here); CLIP model dir ≤ 400 MB;
coverage preview < 100 ms warm / < 5 s cold; importance WAL ≤ 16 MB at rest.
