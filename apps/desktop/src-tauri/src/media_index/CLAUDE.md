# Media index subsystem

Image-ML enrichment: images searchable by content. A read-consumer of `indexing/`, sibling to `importance/`. **Off by
default. On-device OCR text + Vision tags + image-similarity embeddings. Local volumes enrich by default; opt-in SMB
volumes conservatively; MTP never background-sweeps. Real macOS Vision; a fake for tests.** Plan:
[`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). Port, network-fetch, GC-safety,
FFI, schema: [DETAILS.md](DETAILS.md).

## Module map

- `predicate.rs` — PURE image-qualification (`qualify_dir`): images, Live Photos, `.aae` sidecars, RAW+JPEG.
- `store/` — per-volume `media.db`: `media_status`, `media_ocr` (FTS5: OCR + folded tags), `media_tags`,
  `media_embedding`. `writer.rs` + `writer_registry.rs` — ONE writer thread per volume + registry.
- `backend/` — the `VisionBackend` seam (`ocr`, `analyze`, stamps), `FakeVisionBackend`, (macOS) `vision/`. `scheduler/`
  — bus-driven coalesced pass + `enrich.rs` (walk + importance-order + enrich + GC).
- `network/` — SMB byte-fetch (`fetch`, `policy`, `config` opt-in/override/exclude/paused, `enrich`); `foreground.rs` =
  idle signal.
- `vector/` — brute-force `VectorStore` (cosine top-k, dedup) + resident `cache`; `coverage.rs` — covered-count.
  `read/` — `MediaIndex`, the ONLY consumer entry (search, find-similar, dedup).
- `commands.rs` — thin IPC surface; `gate.rs` — toggle, emergency-stop, threshold atomics.

## Must-knows

- **A deliberate PORT of `importance/`'s patterns** (store, writer, scheduler, coalescer, read API); read
  `importance/CLAUDE.md`.
- **Path-keyed, disposable cache.** A schema bump/corruption delete-and-recreates `media.db` (no migrations;
  `SCHEMA_VERSION` is `2`). Staleness is `(path, mtime, size)` PLUS the analyze stamp.
- **`analyze` is the enrichment entry, not `ocr`** — OCR + tags + feature print from ONE decode. Its `analysis_stamp`
  (`engine_version`) folds the OCR/taxonomy/feature-print revisions and drives staleness (an OS taxonomy change re-tags).
- **NEVER persist the lifecycle-bus `generation`** (a transient wake counter); there is NO per-row scan-generation
  column. Bus single-sourced in [`indexing/DETAILS.md`](../indexing/DETAILS.md).
- **GC is deletion-driven AND edge-triggered — a data-safety line.** A pass GCs ONLY on a `Completed` bus edge (via
  `borrow_and_update`, NEVER a `borrow()` poll) or the Fresh sweep — post-flush, tree whole. Never GC on volume-absence
  (a mid-`Scanning` truncate / disconnect). A deferred below-threshold image stays in the GC `current` set, so only
  vanished files are collected.
- **Importance-prioritized (the headline).** Orders + filters by `ImportanceIndex` at the slider threshold. `folder_scores`
  `None` (unscored) ⇒ BOTH kinds DEFER to override-only, NEVER enrich-all (else a first-run race over-indexes permanently;
  forward-only); the unscored→scored bridge in `wire_volume` re-kicks once scored, and `waitingForImportance` surfaces the
  wait. "Scored" = weight rows OR a generation (`coverage::importance_scored`; incremental-only stores sit at generation
  0). EXCLUDED = hard veto; floored junk has no row. DETAILS § Defer-until-scored.
- **What starts a pass**: a `Completed` bus edge, or a user kick (`kick_all_ready_passes` on toggle-on / restart /
  threshold DECREASE; `kick_network_pass` on opt-in). The sweep only WIRES subscriptions — a Fresh-at-launch bus stays
  `Pending` — so the kick, not the sweep, enriches (old dead-start).
- **Inference behind `VisionBackend`.** Every test injects `FakeVisionBackend` via `MediaScheduler::new`, never `start`.
  The real backend runs ALL Vision/ImageIO on ONE 8 MB-stack thread (never rayon for macOS frameworks); a hostile image
  returns a typed `VisionError`, never a panic.
- **Off by default + ONE shared memory ceiling.** The master toggle (`mediaIndex.enabled`) defaults off; the scheduler
  no-ops until on. Cancellation hooks the EXISTING indexing watchdog (`register_subsystem_stop_hook`), which drops the
  vector caches — do NOT add a second.
- **A disconnect is NOT a bad file (data-safety).** A mid-pass SMB unmount PAUSES (keeps completed rows, no GC, no
  `Failed` — reserved for a genuinely bad decode/analysis, not a read failure).
- **`search/` reaches `media.db` ONLY through `MediaIndex`** — no raw `rusqlite` dep. Commands register in BOTH `ipc.rs`
  and `ipc_collectors.rs` (regen with `pnpm bindings:regen`).

## Not yet

Frontend landed (slider, per-volume progress, find-similar). Open: per-FOLDER exclude + "always index" triggers (setters
ready), MTP on-demand, later work (CLIP, faces, captions). [DETAILS.md](DETAILS.md) § What's left for later.
