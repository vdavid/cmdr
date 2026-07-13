# Media index subsystem

Image-ML enrichment: makes a volume's images searchable by content. A read-consumer of `indexing/`, sibling to
`importance/`. **Off by default. On-device OCR text + Vision tags + image-similarity embeddings. Local
volumes enrich by default; opt-in SMB volumes conservatively; MTP never background-sweeps. Real macOS Vision; a fake for
tests.** Plan: [`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). Port rationale,
network-fetch, GC safety, FFI, and schema: [DETAILS.md](DETAILS.md).

## Module map

- `predicate.rs` — PURE image-qualification (`qualify_dir`): images, Live Photos, `.aae` sidecars, RAW+JPEG.
- `store/` — per-volume `media.db` (ported from `importance/store/`): `media_status`, `media_ocr` (FTS5: OCR + folded
  tags), `media_tags`, `media_embedding`. `writer.rs` + `writer_registry.rs` — the ONE writer thread per volume + registry.
- `backend/` — the `VisionBackend` seam (`ocr`, `analyze`, stamps), `FakeVisionBackend`, and (macOS) `vision/`, the real
  backend. `scheduler/` — bus-driven coalesced pass + `enrich.rs` (walk + importance-order + enrich + GC).
- `network/` — SMB byte-fetch (`fetch`, `policy`, `config` opt-in/override/exclude/paused, `enrich`); `foreground.rs` =
  idle signal.
- `vector/` — brute-force `VectorStore` (cosine top-k, dedup) + resident `cache`. `coverage.rs` — slider covered-count.
  `read/` — `MediaIndex`, the ONLY consumer entry (OCR/tag search, find-similar, dedup).
- `commands.rs` — the thin IPC surface; `gate.rs` — master-toggle, emergency-stop, threshold atomics.

## Must-knows

- **A deliberate PORT of `importance/`'s patterns** (store, writer registry, scheduler, coalescer, read API); read
  `importance/CLAUDE.md`.
- **Path-keyed, disposable cache.** A schema bump/corruption delete-and-recreates `media.db` (no migrations;
  `SCHEMA_VERSION` is `2`). Staleness is `(path, mtime, size)` PLUS the analyze stamp.
- **`analyze` is the enrichment entry, not `ocr`** — OCR + tags + feature print from ONE decode. Its `analysis_stamp`
  (the `engine_version` column) folds the OCR/taxonomy/feature-print revisions and drives staleness, so an OS taxonomy
  change re-tags.
- **NEVER persist the lifecycle-bus `generation`** (a transient wake counter, resets to 1 each launch); there is NO
  per-row scan-generation column. Bus single-sourced in [`indexing/DETAILS.md`](../indexing/DETAILS.md).
- **GC is deletion-driven AND edge-triggered — a data-safety line.** A pass GCs ONLY on a `Completed` bus edge (via
  `borrow_and_update`, NEVER a `borrow()` poll) or the Fresh sweep — post-flush, tree whole. Don't GC on volume-absence
  (a mid-`Scanning` truncate or a disconnect must never sweep). A deferred below-threshold image stays in the GC
  `current` set, so only vanished files are collected.
- **Importance-prioritized (the headline).** The scheduler orders + filters by `ImportanceIndex` at the slider threshold
  (`gate::importance_threshold`). When `folder_scores` is `None` (importance never scored the volume) the fallback
  DIFFERS: LOCAL enriches all, NETWORK enriches override-only (never drag a whole NAS). An EXCLUDED folder is a hard veto
  (beats override); floored junk has no importance row, so it's skipped at any threshold.
- **Inference behind `VisionBackend`.** Every test injects `FakeVisionBackend` via `MediaScheduler::new`, never `start`
  (which picks real Vision). The real backend runs ALL Vision/ImageIO on ONE 8 MB-stack thread (never rayon for macOS
  frameworks); a hostile image returns a typed `VisionError`, never a panic.
- **Off by default + ONE shared memory ceiling.** The master toggle (`mediaIndex.enabled`) defaults off; the scheduler
  no-ops until on. Cancellation hooks the EXISTING indexing watchdog (`register_subsystem_stop_hook`), which drops the
  vector caches — do NOT add a second. Query-time work runs off the IPC thread.
- **A disconnect is NOT a bad file (data-safety).** A mid-pass SMB unmount PAUSES (keeps completed rows, no GC, no
  `Failed` — `Failed` is reserved for a genuinely bad file: a good read but a bad decode/analysis).
- **`search/` reaches `media.db` ONLY through `MediaIndex`** — no raw `rusqlite` dep. Commands register in BOTH `ipc.rs`
  and `ipc_collectors.rs` (regen with `pnpm bindings:regen`).

## Not yet

Frontend has landed (slider, per-volume progress, find-similar). Still open: the per-FOLDER exclude + "always index"
frontend triggers (setters ready), MTP on-demand, and later work (CLIP, faces, captions). [DETAILS.md](DETAILS.md)
§ Frontend surface, § What's left for later.
