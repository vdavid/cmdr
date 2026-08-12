# Media index subsystem

Image-ML enrichment making images searchable by content; a read-consumer of `indexing/`, off by default. Local by
default, SMB opt-in, MTP never, external drives parked. A PORT of `importance/`'s patterns (`importance/CLAUDE.md`
first).

## Areas (routing map)

Eight area subdirs, each with its own `CLAUDE.md` + `DETAILS.md`; read the area's before non-trivial work there:
`scheduler/` (the passes), `network/` (SMB fetch + opt-in config), `backend/` (the `VisionBackend` seam), `clip/`,
`ann/`, `store/` (`media.db`), `read/` (`MediaIndex`, the ONLY consumer entry), `vector/`. What each owns: `DETAILS.md`
§ Area map.

This file owns the leaves: `coverage/`, `gate.rs` (toggle / scope / threshold / parallelism atomics), `paths.rs`,
`writer/` + `writer_registry.rs` (ONE writer thread per volume), `events.rs`, `progress.rs`, `thermal.rs`,
`predicate.rs` (`qualify_dir` stays PURE).

## Subsystem-wide must-knows

- **The IPC surface lives app-side in `../commands/media_index/`**, not here: commands carry `tauri::` and this
  subsystem must not. It reaches back in through `read/`, `gate.rs`, and `network::config`, and ❌ nothing here reads a
  settings file: the app builds an `IndexConfig` and `indexing::host::config::set_config` applies its media half to
  `gate.rs` and `network/config.rs`. `MediaScheduler::start()` hands the scheduler to the host; it never registers
  itself.
- **Disposable, integer-id-keyed store.** A schema bump or corruption delete-and-recreates `media.db`; no migrations.
  Paths live ONCE in `media_file(id, path)`, so a raw `path =` query against another media table is the bug.
  `store/CLAUDE.md`.
- **Deletion is data-safety.** Exactly four paths delete a row: whole-store GC (ONLY on a `Completed` bus edge or the
  Fresh sweep, never on volume-absence), the live tick's dir-scoped GC, the reclaim prune, the privacy retro-delete. ❌
  Nothing else may run whole-store `gc_targets` / `enrich_and_gc`. Uncovered rows STAY: narrowing a setting deletes
  nothing. The exclusion veto reads LIVE `is_excluded`, ❌ never a pass snapshot. DETAILS § GC safety argument.
- **Coverage = scope + importance.** Scope is an EXPLICIT `gate::IndexScope`, ❌ never a sentinel threshold. Resolve it
  ONLY via `lifecycle::pass_coverage` and hand the SAME scope to `coverage::stored_row_survives`, or reclaim drifts from
  what a pass keeps. EXCLUDED beats every override.
- **Counts stream; polls never build them.** Aggregate through the `for_each_qualifying_image` sink, ❌ never over
  `walk_image_entries` (one path `String` per image was the 50 GB launch runaway). Polls and startup read
  `coverage::cached`; only user-initiated settings reads call `get_or_build`. `None` means "no number yet", ❌ never
  `0`. `accounted` is INCREMENTAL (writer ±1), ❌ never rebuilt from a walk nor merged into `eligible`.
- **Scores reach a UI path ONLY via `coverage::importance_scores`** (cached off the recompute subscription). ❌ Never
  `above_threshold` direct: it sorts every scored folder, and per badge query that froze the app.
- **Cancellation hooks the EXISTING indexing watchdog** (❌ no second one; one shared memory ceiling), between images
  via `gate::should_stop` (watchdog OR toggle OFF); ❌ don't narrow it to `is_cancelled`.
- **CLIP is a SEPARATE vector space from the Vision feature print**; ❌ never cosine-compare the two.
- **Every pass emits `media-enrich-progress`** over the ENRICHABLE subset (❌ never `images.len()`) and
  `media-enrich-terminal` on EVERY exit path. New commands register in BOTH `ipc.rs` and `ipc_collectors.rs`, events in
  `collect_events!`.

Port rationale, the GC safety argument, the scope model, the coverage caches, settings, events, and the frontend map:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
