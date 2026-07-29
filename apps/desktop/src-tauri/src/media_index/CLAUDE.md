# Media index subsystem

Image-ML enrichment making images searchable by content; a read-consumer of `indexing/`, off by default. Local by
default, SMB opt-in, MTP never, external drives parked. A PORT of `importance/`'s patterns (`importance/CLAUDE.md`
first).

## Areas (routing map)

Each area subdir has its own `CLAUDE.md` (must-knows) + `DETAILS.md` (depth). Touch a dir and its `CLAUDE.md`
autoloads; read it before non-trivial work there.

- **`scheduler/CLAUDE.md`** — the pass machinery: full, network, and live-tick passes, bus wiring, the parallel worker
  pool, importance ordering, the reclaim prune. **`network/CLAUDE.md`** — SMB byte-fetch, the conservative fetch
  policy, the opt-in / override / exclusion config.
- **`backend/CLAUDE.md`** — the fakeable `VisionBackend` inference seam and the real macOS Vision impl.
  **`clip/CLAUDE.md`** — CLIP semantic search (model install, Core ML towers, the query encode). **`ann/CLAUDE.md`** —
  the per-volume usearch HNSW index serving it at scale.
- **`store/CLAUDE.md`** — `media.db` schema, connections, staleness. **`read/CLAUDE.md`** — `MediaIndex`, the ONLY
  consumer entry, `search/` included. **`vector/CLAUDE.md`** — brute-force cosine + the resident vector caches.

Top-level leaves this file owns: `commands/` (the IPC surface, one module per family: `search`, `state`, `reclaim`,
`file_status`, `clip_model`, `thumbnail`, and `policy` for the coverage-changing setters; `mod.rs` re-exports them all,
so command paths in `ipc.rs` never move), `coverage.rs` (the eligible/accounted caches), `gate.rs` (toggle / scope /
threshold / parallelism atomics), `writer.rs` + `writer_registry.rs` (ONE writer thread per volume), `events.rs`,
`progress.rs`, `thermal.rs`, and `predicate.rs`, whose `qualify_dir` stays PURE.

## Subsystem-wide must-knows

- **Disposable, integer-id-keyed cache.** A schema bump or corruption delete-and-recreates `media.db`; no migrations.
  Paths live ONCE in `media_file(id, path)` and every other table keys on `file_id`, so a raw `path =` query against a
  media table is the bug, and a rename is `rename_path`, one row.
- **Deletion is data-safety.** Exactly four paths delete a row: whole-store GC (ONLY on a `Completed` bus edge or the
  Fresh sweep, never on volume-absence), the live tick's dir-scoped GC, the reclaim prune, and the privacy
  retro-delete. ❌ Nothing else may run whole-store `gc_targets` / `enrich_and_gc`. Uncovered rows STAY: narrowing a
  setting deletes nothing. The exclusion veto reads LIVE `is_excluded`, ❌ never a pass snapshot.
- **Coverage = scope + importance.** Scope is an EXPLICIT `gate::IndexScope`, ❌ never a sentinel threshold. Resolve it
  ONLY via `lifecycle::pass_coverage`, and hand the SAME scope to `coverage::stored_row_survives`, or reclaim drifts
  from what a pass keeps. EXCLUDED beats every override.
- **Counts stream; polls never build them.** Aggregate through the `for_each_qualifying_image` sink, ❌ never over
  `walk_image_entries`: one path `String` per image is the 50 GB launch runaway. Polls and startup read
  `coverage::cached`; only user-initiated settings reads call `get_or_build`. `None` means "no number yet", ❌ never
  `0`. `accounted` is INCREMENTAL (writer ±1) in its own cache, ❌ never rebuilt from a walk.
- **Cancellation hooks the EXISTING indexing watchdog** (❌ no second one; one shared memory ceiling). The
  between-images hook is `gate::should_stop` (watchdog OR toggle OFF); ❌ don't narrow it to `is_cancelled`.
- **CLIP is a SEPARATE vector space from the Vision feature print**; ❌ never cosine-compare the two.
- **Every pass emits `media-enrich-progress`** over the ENRICHABLE subset (❌ never `images.len()`) and
  `media-enrich-terminal` on EVERY exit path. New commands register in BOTH `ipc.rs` and `ipc_collectors.rs`, events in
  `collect_events!`.

Port rationale, the GC safety argument, the scope model, the coverage caches, settings, events, and the frontend map:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
