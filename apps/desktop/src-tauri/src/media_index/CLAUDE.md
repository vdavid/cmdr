# Media index subsystem

Image-ML enrichment: images searchable by content. A read-consumer of `indexing/`, off by default. On-device OCR + Vision tags +
embeddings. Local by default; SMB opt-in; MTP never; LocalExternal (USB/SD) parked (❌ never Local).

A PORT of `importance/`'s patterns (`importance/CLAUDE.md` first).

## Module map

- `predicate.rs` PURE `qualify_dir`. `store/` per-volume `media.db` + ONE writer thread (`writer_registry`).
  `backend/` the `VisionBackend` seam (`FakeVisionBackend`, macOS `vision/`).
- `scheduler/` bus-driven coalesced pass: `mod.rs` (`MediaScheduler` + pass bodies), `coordinator.rs`, `lifecycle.rs`
  (`start`/kick/wire/spawn/`pass_coverage`), `live.rs`, `enrich.rs`, `reclaim.rs` (coverage + prune). `network/` SMB
  byte-fetch + `config` (opt-in/override/exclude/paused). `vector/` + `ann/` search; `coverage.rs` counts.
- `read/` `MediaIndex` (the ONLY consumer entry). `commands.rs` read IPC + `commands/policy.rs` coverage-changing IPC;
  `gate.rs` toggle/scope/threshold atomics.

## Must-knows

- **Disposable, integer-id-keyed cache.** A schema bump/corruption delete-and-recreates `media.db` (no migrations).
  `media_file(id, path)` holds each path ONCE; every other table keys on `file_id`, and reads join back to a path (a
  raw `path =` query against a media table is the bug; a rename is `rename_path`, one row). Embeddings are `f16`
  (disk + resident cache). Staleness = `(path, mtime, size)` + the analyze stamp. `analyze` (NOT `ocr`) is the entry: one
  decode → OCR + tags + feature print.
- **GC is deletion-driven + edge-triggered (data-safety).** ONLY on a `Completed` bus edge (`borrow_and_update`, never
  a `borrow()` poll) or the Fresh sweep; never on volume-absence. Uncovered rows stay; only vanished files collect.
  ❌ Never persist the lifecycle-bus `generation`. Bypassing the edge: privacy retro-delete, reclaim prune, live-tick
  scoped GC. The exclusion veto reads LIVE `is_excluded` (never the pass snapshot),
  re-checked before each upsert (TOCTOU). ❌ NEVER whole-store `gc_targets`/`enrich_and_gc` on a live tick (wipes every row
  OUTSIDE the touched dirs).
- **Coverage = scope + importance** (§ The indexing scope, § Defer-until-scored). Scope is an EXPLICIT
  `gate::IndexScope`, ❌ never a sentinel threshold; default `ChosenFolders` = override-only, importance unread.
  `ByImportance` also filters + orders by `ImportanceIndex`. Resolve ONLY via `lifecycle::pass_coverage` (narrow ⇒
  `scores: None`, no defer-marking) + pass the scope to `coverage::stored_row_survives`, or reclaim drifts.
  `folder_scores` `None` ⇒ override-only, ❌ NEVER enrich-all (a first-run race over-indexes permanently);
  `wire_volume` re-kicks once scored. EXCLUDED = hard veto. Narrowing DELETES NOTHING.
- **What starts a pass**: a `Completed` bus edge, or a user kick (`kick_all_ready_passes` on toggle-on / restart /
  threshold DECREASE / scope BROADEN / folder added; `kick_network_pass` on opt-in). The sweep only WIRES subs. Plus
  **live updates** (LOCAL only): a throttled touched-dirs-SCOPED tick on a distinct `#live` coordinator key.
- **`FakeVisionBackend` via `MediaScheduler::new`, never `start`.** Real backend: each Vision/ImageIO worker on its OWN
  8 MB-stack thread (never rayon), CF objects confined; hostile images ⇒ typed `VisionError`s, never panics.
- **Parallel enrichment = N INDEPENDENT backends** (`scheduler/pool.rs`): ❌ never feed one backend concurrently (CF
  confinement) or fan out the single writer. `gate::parallelism` (default 1) capped by `thermal`; network prefetch is
  byte-bounded (`network/budget.rs`).
- **Off by default + ONE shared memory ceiling** (§ Disabling stops the running pass): cancellation hooks the EXISTING
  indexing watchdog (❌ no second one). The between-images hook is `gate::should_stop` (watchdog OR toggle OFF);
  ❌ don't narrow to `is_cancelled`.
- **ONLY a typed disconnect pauses a network pass** (rows kept, no GC, no `Failed`); any other per-file read error is
  `FetchError::Unreadable` = skip-and-count, ❌ never a pause. Direct-SMB fetches via the app's OWN smb2 session (no
  TCC); mount-only via the OS mount (`network/fetch.rs`).
- **`search/` reaches `media.db` ONLY through `MediaIndex`.** Commands register in BOTH `ipc.rs` +
  `ipc_collectors.rs`; events in `collect_events!` only.
- **A pass publishes progress** (§ Progress events): `media-enrich-progress` over the ENRICHABLE subset (❌ never
  `images.len()`) + `media-enrich-terminal` on EVERY exit path.
- **CLIP semantic search is a SEPARATE vector space** (`clip/`, macOS Core ML; § CLIP semantic search):
  `media_clip_embedding` + `clip_stamp`, INDEPENDENT staleness (`needs_clip`). One decode runs the stale side(s) (`analyze_media(want_vision, want_clip)`). ❌ NEVER compare CLIP against the Vision feature print. `gate::semantic_search_enabled` (ON by default) gates both:
  `search_semantic` returns `[]`, and `clip::current_stamp` returns `None` so no pass embeds CLIP (the single
  CLIP-write seam; ❌ don't re-gate `want_clip`). Delete-model prunes every volume's clip embeddings + stamps
  (`prune_all_clip`); Vision kept, off ≠ delete. Post-compile the `.mlpackage` source is deleted:
  `is_installed` = `.mlpackage` OR `.mlmodelc` per tower (❌ not package-only); unloadable + no source ⇒ re-download.
- **ANN (`ann/`, § ANN vector search)**: ≥50k CLIP vectors ⇒ per-volume `.usearch` index keyed by `media_file` ids
  (rename = NO index touch); ops writer-buffered, `flush_ann_index` at every cache-invalidate seam. ❌ Never mutate
  index files outside the writer/rebuild seams (per-file lock). Unusable ⇒ exact fallback + rebuild, NEVER a search
  failure; ❌ no open without the checksum verify (corrupt views SIGSEGV).
- **Per-folder `accounted` aggregate** (`coverage.rs`; feeds `media_index_file_status`/`_folder_coverage`):
  ❌ INCREMENTAL (writer `+1`/`-1`, seeded at spawn), never rebuilt from a walk; SEPARATE `ACCOUNTED` cache, not
  `COUNTS`.

Depth for every §, plus architecture and decisions: `DETAILS.md`; read it before non-trivial work.
