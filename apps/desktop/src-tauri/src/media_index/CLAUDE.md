# Media index subsystem

Image-ML enrichment: makes a volume's images searchable by their content. A read-consumer of `indexing/`, sibling to
`importance/` and `search/`. **Off by default. On-device OCR text + Vision tags + image-similarity embeddings (M2).
Local volumes enrich by default; opt-in SMB volumes conservatively (M1.5); MTP never background-sweeps. Real macOS Vision;
a fake for tests.** Plan: [`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md).
Port rationale, network-fetch, GC safety, FFI, schema, M2 depth: [DETAILS.md](DETAILS.md).

## Module map

- `predicate.rs` — PURE image-qualification (`qualify_dir`): images, Live Photos, `.aae` sidecars, RAW+JPEG.
- `store/` — per-volume `media.db` (ported from `importance/store/`): `media_status` + `media_ocr` (FTS5: OCR + folded
  tags) + `media_tags` + `media_embedding`; `needs_enrichment` + embedding codec.
- `writer.rs` + `writer_registry.rs` — the ONE writer thread per volume, and its registry.
- `backend/` — the `VisionBackend` seam (`ocr`, `analyze`, stamps), `FakeVisionBackend`, and (macOS) `vision/`, the real
  backend (OCR + classify + feature print on one 8 MB-stack thread).
- `scheduler/` — bus-driven coalesced pass + `enrich.rs` (walk + importance-order + enrich + GC).
- `network/` — SMB byte-fetch (M1.5): `fetch.rs`, `policy.rs`, `config.rs` (opt-in/override/exclude/paused), `enrich.rs`;
  `foreground.rs` = the idle signal.
- `vector/` — brute-force `VectorStore` (cosine top-k, dedup) + the resident `cache`.
- `coverage.rs` — the slider covered-count (cached counts + threshold math).
- `read/` — `MediaIndex`, the ONLY consumer entry (OCR/tag search, find-similar, dedup).
- `commands.rs` — the thin IPC surface; `gate.rs` — master-toggle, emergency-stop, threshold atomics.

## Must-knows

- **A deliberate PORT of `importance/`** (store, writer registry, scheduler, coalescer, read API). Mirror its patterns;
  read `importance/CLAUDE.md`.
- **Path-keyed, disposable cache.** Rows key on the index path identity (no stable entry id); a schema bump/corruption
  delete-and-recreates `media.db` (no migrations; `SCHEMA_VERSION` is `2`). Staleness is `(path, mtime, size)` PLUS the
  analyze provenance stamp.
- **`analyze` is the enrichment entry, not `ocr`** — OCR + tags + feature print from ONE decode (Decision 5). Its
  `analysis_stamp` (OCR engine + tag taxonomy + feature-print revisions, in the `engine_version` column) drives
  staleness, so an OS taxonomy change re-tags. Tag labels fold into `media_ocr` (a `source` column) for keyword search.
- **NEVER persist the lifecycle-bus `generation`** (a transient wake counter, resets to 1 each launch); there is NO
  per-row scan-generation column. Bus single-sourced in [`indexing/DETAILS.md`](../indexing/DETAILS.md).
- **GC is deletion-driven AND edge-triggered — a data-safety line.** A pass GCs ONLY on a `Completed` bus edge (via
  `borrow_and_update`, NEVER a `borrow()` poll) or the Fresh sweep — post-flush, tree whole. Don't GC on volume-absence
  (a mid-`Scanning` truncate or a disconnect must never sweep). A deferred below-threshold image stays in the GC
  `current` set, so only vanished files are GC'd.
- **Importance-prioritized (the headline).** The scheduler orders + filters by `ImportanceIndex` at the slider threshold
  (`gate::importance_threshold`). `folder_scores` is `None` when importance never scored the volume; the fallback DIFFERS:
  LOCAL enriches all, NETWORK enriches override-only (never drag a whole NAS). An EXCLUDED folder is a hard veto (beats
  override); floored junk has no importance row, so it's skipped at any threshold.
- **Inference behind `VisionBackend`.** Production selects real Vision in `scheduler::start`; every test injects
  `FakeVisionBackend` via `MediaScheduler::new`, never `start`. The real backend runs ALL Vision/ImageIO on ONE 8 MB-stack
  thread (never rayon for macOS frameworks); a hostile image returns a typed `VisionError`, never a panic/hang.
- **Off by default + ONE shared memory ceiling.** The master toggle (`mediaIndex.enabled`) defaults off; the scheduler
  no-ops until on. Cancellation hooks the EXISTING indexing watchdog (`register_subsystem_stop_hook`), which also drops
  the vector caches — do NOT add a second ceiling. Query-time work runs OFF the IPC thread; the vector cache is
  load-once, invalidated per completed pass.
- **A disconnect is NOT a bad file (data-safety).** A mid-pass SMB unmount PAUSES (keeps completed rows, no GC, no
  `Failed` — that's reserved for a genuinely bad file: a good read but a decode/analysis failure).
- **`search/` reaches `media.db` ONLY through `MediaIndex`** (Decision 8) — no raw `rusqlite` dep. Commands register in
  BOTH `ipc.rs` and `ipc_collectors.rs` (regen with `pnpm bindings:regen`).

## Not yet

The M2 frontend has landed (slider + covered-count preview, per-volume progress, find-similar; [DETAILS.md](DETAILS.md)
§ M2 frontend). Still open: the per-FOLDER exclude + "always index" FE triggers (a native-menu follow-up; setters +
settings ready), MTP on-demand, and M3+ (CLIP, faces, durable identity store, captions).
