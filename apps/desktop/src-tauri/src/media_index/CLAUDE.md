# Media index subsystem

Image-ML enrichment: images searchable by content. A read-consumer of `indexing/`. Off by default. On-device OCR +
Vision tags + similarity embeddings; local enriches by default, opt-in SMB conservative, MTP never. Real macOS Vision, a
fake for tests.

## Module map

- `predicate.rs` PURE `qualify_dir`. `store/` per-volume `media.db` + ONE writer thread (`writer_registry`). `backend/`
  the `VisionBackend` seam (`FakeVisionBackend`, macOS `vision/`).
- `scheduler/` bus-driven coalesced pass + `enrich.rs` + `reclaim.rs` (M4 stored-coverage split + prune). `network/` SMB byte-fetch + `config`
  (opt-in/override/exclude/paused). `vector/` `VectorStore` + resident `cache`; `coverage.rs` covered-count.
- `read/` `MediaIndex` (the ONLY consumer entry). `commands.rs` IPC; `gate.rs` toggle + threshold atomics.

## Must-knows

- **PORT of `importance/`'s patterns** (store, writer, scheduler, coalescer, read API). Read `importance/CLAUDE.md`.
- **Disposable, path-keyed cache.** A schema bump/corruption delete-and-recreates `media.db` (`SCHEMA_VERSION` 2, no
  migrations). Staleness = `(path, mtime, size)` + the analyze stamp. `analyze` (NOT `ocr`) is the enrichment entry: one
  decode → OCR + tags + feature print (`engine_version` drives staleness).
- **GC is deletion-driven + edge-triggered (data-safety).** GC ONLY on a `Completed` bus edge (`borrow_and_update`,
  never a `borrow()` poll) or the Fresh sweep; never on volume-absence. Deferred/below-threshold rows stay; only vanished
  files collect. Never persist the lifecycle-bus `generation` (a transient wake counter).
- **Two USER-EXPLICIT deletions bypass the edge** (they derive from SETTINGS state, not scan/bus/gate state, so they
  need no `Completed` edge): the privacy retro-delete (excluding a folder) and the reclaim prune (M4). Both go through
  the writer (`prune_under_folder` / `prune_paths` + `VACUUM`), delete all four row kinds atomically, and drop the
  vector + coverage caches. ❌ The exclusion veto reads LIVE `network::config::is_excluded`, NEVER the pass snapshot, and
  re-checks before each upsert (the in-flight-analyze TOCTOU) — else a pass already running re-inserts what the
  retro-delete removed. D.md § Privacy retro-delete.
- **Importance-prioritized (headline).** Filter + order by `ImportanceIndex` at the slider threshold. `folder_scores`
  `None` (unscored) defers to override-only, NEVER enrich-all (forward-only, so a first-run race would over-index
  permanently); the `wire_volume` importance-subscribe bridge re-kicks once scored. "Scored" = live weight rows OR a
  generation (`coverage::importance_scored`). EXCLUDED = hard veto; floored junk has no row. D.md § Defer-until-scored.
- **What starts a pass**: a `Completed` bus edge, or a user kick (`kick_all_ready_passes` on toggle-on / restart /
  threshold DECREASE; `kick_network_pass` on opt-in). The sweep only WIRES subs (a Fresh-at-launch bus stays `Pending`),
  so the kick, not the sweep, enriches.
- **`FakeVisionBackend` via `MediaScheduler::new`, never `start`.** Real backend: ALL Vision/ImageIO on ONE 8 MB-stack
  thread (never rayon); a hostile image gives a typed `VisionError`, never a panic.
- **Off by default + ONE shared memory ceiling.** Scheduler no-ops until on; cancellation hooks the EXISTING indexing
  watchdog, so don't add a second.
- **A disconnect is NOT a bad file.** A mid-pass SMB unmount PAUSES (keeps rows, no GC, no `Failed`).
- **`search/` reaches `media.db` ONLY through `MediaIndex`.** Commands register in BOTH `ipc.rs` + `ipc_collectors.rs`
  (`pnpm bindings:regen`); events (`events.rs`) register in `ipc.rs`'s `collect_events!` only.
- **A pass publishes progress to the top-right indicator** (`events.rs`, plan M5): throttled `media-enrich-progress` over
  the ENRICHABLE subset (never `images.len()`) + one `media-enrich-terminal` on EVERY exit path (a `Drop`-guard emits
  `Failed` on an error bubble). A VANISHED source (typed `VisionError::Missing`, ENOENT at analyze) is skipped quietly
  (DEBUG, no row) and counts as processed. `run_pass_blocking` needs the `AppHandle` (`None` in unit tests ⇒ no emit).
  D.md § Progress events.

Still open: per-FOLDER always-index trigger (setter ready; the exclude trigger shipped as a folder context-menu item),
MTP on-demand, CLIP/faces/captions.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
