# Media index subsystem

Image-ML enrichment: images searchable by content. A read-consumer of `indexing/`, off by default. On-device OCR +
Vision tags + similarity embeddings. Local enriches by default; SMB opt-in conservative; MTP never; LocalExternal
(USB/SD) parked (skip, never treat as Local).

## Module map

- `predicate.rs` PURE `qualify_dir`. `store/` per-volume `media.db` + ONE writer thread (`writer_registry`). `backend/`
  the `VisionBackend` seam (`FakeVisionBackend`, macOS `vision/`).
- `scheduler/` bus-driven coalesced pass: `mod.rs` (`MediaScheduler` + pass bodies), `coordinator.rs`
  (`PassCoordinator`), `lifecycle.rs` (`start`/kick/wire/spawn), `live.rs`, `enrich.rs`, `reclaim.rs` (coverage
  + prune). `network/` SMB
  byte-fetch + `config` (opt-in/override/exclude/paused). `vector/` `VectorStore` + resident `cache`; `coverage.rs`
  covered-count.
- `read/` `MediaIndex` (the ONLY consumer entry). `commands.rs` IPC; `gate.rs` toggle + threshold atomics.

## Must-knows

Depth for every point is in [DETAILS.md](DETAILS.md) (§ sections inline).

- **PORT of `importance/`'s patterns** (store, writer, scheduler, read API). Read `importance/CLAUDE.md`.
- **Disposable, path-keyed cache.** A schema bump/corruption delete-and-recreates `media.db` (no migrations). Staleness
  = `(path, mtime, size)` + the analyze stamp. `analyze` (NOT `ocr`) is the enrichment entry: one decode → OCR + tags +
  feature print (`engine_version` drives staleness).
- **GC is deletion-driven + edge-triggered (data-safety).** GC ONLY on a `Completed` bus edge (`borrow_and_update`,
  never a `borrow()` poll) or the Fresh sweep; never on volume-absence. Deferred/below-threshold rows stay; only
  vanished files collect. Never persist the lifecycle-bus `generation`.
- **Three deletions bypass the `Completed` edge** (privacy retro-delete, reclaim prune, live-tick scoped GC). ❌ The
  exclusion veto reads LIVE `is_excluded`, never the pass snapshot, re-checked before each upsert (in-flight TOCTOU). ❌
  NEVER whole-store `gc_targets`/`enrich_and_gc` on a live tick (wipes every row OUTSIDE the touched dirs).
- **Importance-prioritized (headline; § Defer-until-scored).** Filter + order by `ImportanceIndex` at the slider
  threshold. `folder_scores` `None` (unscored) defers to override-only, NEVER enrich-all (a first-run race over-indexes
  permanently); `wire_volume` re-kicks once scored. EXCLUDED = hard veto.
- **What starts a pass**: a `Completed` bus edge, or a user kick (`kick_all_ready_passes` on toggle-on / restart /
  threshold DECREASE; `kick_network_pass` on opt-in). The sweep only WIRES subs, so the kick enriches, not the sweep.
  Plus **live index updates** (LOCAL only): a throttled, touched-dirs-SCOPED tick on a distinct `#live` coordinator key.
- **`FakeVisionBackend` via `MediaScheduler::new`, never `start`.** Real backend: ALL Vision/ImageIO on ONE 8 MB-stack
  thread (never rayon); a hostile image gives a typed `VisionError`, never a panic.
- **Off by default + ONE shared memory ceiling** (§ Disabling stops the running pass). Scheduler no-ops until on;
  cancellation hooks the EXISTING indexing watchdog, don't add a second. The between-images cancel hook is
  `gate::should_stop` (watchdog `is_cancelled` OR toggle OFF), so disabling stops the RUNNING pass (rows kept); don't
  narrow it to `is_cancelled`.
- **A disconnect is NOT a bad file**: a mid-pass SMB unmount PAUSES (keeps rows, no GC, no `Failed`).
- **`search/` reaches `media.db` ONLY through `MediaIndex`.** Commands register in BOTH `ipc.rs` + `ipc_collectors.rs`;
  events (`events.rs`) register in `collect_events!` only.
- **A pass publishes progress** (§ Progress events): throttled `media-enrich-progress` over the ENRICHABLE subset (never
  `images.len()`) + one `media-enrich-terminal` on EVERY exit path (a `Drop`-guard emits `Failed` on an error bubble). A
  vanished source (`VisionError::Missing`, ENOENT) is skipped quietly but counts as processed.
- **CLIP semantic search (M3) is a SEPARATE vector space** (`clip/`, macOS Core ML; § CLIP semantic search):
  `media_clip_embedding` + a `clip_stamp` column with INDEPENDENT staleness (`needs_clip`, decoupled from Vision's
  `engine_version`). A pass runs the stale side(s) from ONE decode via `analyze_media(want_vision, want_clip)`. NEVER
  compare CLIP against the Vision feature print (different spaces). Off unless a model is installed (on-demand,
  SHA-256-verified); `search_semantic` returns `[]` with no model.

Still open: per-folder always-index trigger, MTP on-demand, faces/captions.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
