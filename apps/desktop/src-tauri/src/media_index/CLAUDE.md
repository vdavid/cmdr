# Media index subsystem

Image-ML enrichment: images searchable by content. A read-consumer of `indexing/`, off by default. On-device OCR +
Vision tags + similarity embeddings. Local by default; SMB opt-in conservative; MTP never; LocalExternal (USB/SD)
parked (❌ never treat as Local).

A PORT of `importance/`'s patterns (store, writer, scheduler, read API): read `importance/CLAUDE.md` first.

## Module map

- `predicate.rs` PURE `qualify_dir`. `store/` per-volume `media.db` + ONE writer thread (`writer_registry`).
  `backend/` the `VisionBackend` seam (`FakeVisionBackend`, macOS `vision/`).
- `scheduler/` bus-driven coalesced pass: `mod.rs` (`MediaScheduler` + pass bodies), `coordinator.rs`, `lifecycle.rs`
  (`start`/kick/wire/spawn/`pass_coverage`), `live.rs`, `enrich.rs`, `reclaim.rs` (coverage + prune). `network/` SMB
  byte-fetch + `config` (opt-in/override/exclude/paused). `vector/` + `coverage.rs` counts.
- `read/` `MediaIndex` (the ONLY consumer entry). `commands.rs` read IPC + `commands/policy.rs` coverage-changing IPC;
  `gate.rs` toggle/scope/threshold atomics.

## Must-knows

- **Disposable, path-keyed cache.** A schema bump/corruption delete-and-recreates `media.db` (no migrations).
  Staleness = `(path, mtime, size)` + the analyze stamp. `analyze` (NOT `ocr`) is the entry: one decode → OCR + tags +
  feature print (`engine_version` drives staleness).
- **GC is deletion-driven + edge-triggered (data-safety).** ONLY on a `Completed` bus edge (`borrow_and_update`, never
  a `borrow()` poll) or the Fresh sweep; never on volume-absence. Uncovered rows stay; only vanished files collect.
  ❌ Never persist the lifecycle-bus `generation`. Three deletions bypass the edge: privacy retro-delete, reclaim
  prune, live-tick scoped GC. The exclusion veto reads LIVE `is_excluded`, never the pass snapshot, re-checked before
  each upsert (in-flight TOCTOU). ❌ NEVER whole-store `gc_targets`/`enrich_and_gc` on a live tick (wipes every row
  OUTSIDE the touched dirs).
- **Coverage = scope + importance** (§ The indexing scope, § Defer-until-scored). Scope is an EXPLICIT
  `gate::IndexScope`, ❌ never a sentinel threshold; default `ChosenFolders` = override-only, importance unread.
  `ByImportance` also filters + orders by `ImportanceIndex` at the threshold. Resolve ONLY via
  `lifecycle::pass_coverage` (narrow ⇒ `scores: None` AND no defer-marking) and pass the scope to
  `coverage::stored_row_survives`, or reclaim drifts from the gate. `folder_scores` `None` (unscored) ⇒ override-only,
  ❌ NEVER enrich-all (a first-run race over-indexes permanently); `wire_volume` re-kicks once scored. EXCLUDED = hard
  veto. Narrowing DELETES NOTHING (rows fall into the kept/reclaim offer).
- **What starts a pass**: a `Completed` bus edge, or a user kick (`kick_all_ready_passes` on toggle-on / restart /
  threshold DECREASE / scope BROADEN / a folder added; `kick_network_pass` on opt-in). The sweep only WIRES subs. Plus
  **live index updates** (LOCAL only): a throttled, touched-dirs-SCOPED tick on a distinct `#live` coordinator key.
- **`FakeVisionBackend` via `MediaScheduler::new`, never `start`.** Real backend: ALL Vision/ImageIO on ONE 8 MB-stack
  thread (never rayon); a hostile image gives a typed `VisionError`, never a panic.
- **Off by default + ONE shared memory ceiling** (§ Disabling stops the running pass). Cancellation hooks the EXISTING
  indexing watchdog; ❌ don't add a second. The between-images hook is `gate::should_stop` (watchdog OR toggle OFF), so
  disabling stops the RUNNING pass; ❌ don't narrow it to `is_cancelled`.
- **A disconnect is NOT a bad file**: a mid-pass SMB unmount PAUSES (rows kept, no GC, no `Failed`).
- **`search/` reaches `media.db` ONLY through `MediaIndex`.** Commands register in BOTH `ipc.rs` +
  `ipc_collectors.rs`; events in `collect_events!` only.
- **A pass publishes progress** (§ Progress events): `media-enrich-progress` over the ENRICHABLE subset (❌ never
  `images.len()`) + `media-enrich-terminal` on EVERY exit path.
- **CLIP semantic search is a SEPARATE vector space** (`clip/`, macOS Core ML; § CLIP semantic search):
  `media_clip_embedding` + `clip_stamp`, INDEPENDENT staleness (`needs_clip`). One decode runs the stale side(s) via
  `analyze_media(want_vision, want_clip)`. ❌ NEVER compare CLIP against the Vision feature print. Off with no model
  installed (`search_semantic` returns `[]`).

Still open: per-folder COUNTS (no cheap scan; the pane readout voices coverage, not completion), MTP, faces/captions.

Depth for every § above, plus architecture and decisions: [DETAILS.md](DETAILS.md). Read it before non-trivial work.
