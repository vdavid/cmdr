# Media index subsystem

Image-ML enrichment making images searchable by content; a read-consumer of `indexing/`, off by default. Local by
default, SMB opt-in, MTP never, external drives parked. A PORT of `importance/`'s patterns (`importance/CLAUDE.md`
first).

## Module map

- `scheduler/` runs the bus-driven coalesced pass over `backend/` (the `VisionBackend` seam), `clip/`, and `network/`
  (SMB byte-fetch + opt-in config); `store/` holds each volume's `media.db` behind ONE writer thread
  (`writer_registry`); `vector/` + `ann/` serve search.
- `read/`'s `MediaIndex` is the ONLY consumer entry, `search/` included. `commands.rs` is read IPC,
  `commands/policy.rs` coverage-changing IPC, `gate.rs` the toggle / scope / threshold atomics, and
  `predicate.rs`'s `qualify_dir` stays PURE.

## Must-knows

- **Disposable, integer-id-keyed cache.** A schema bump or corruption delete-and-recreates `media.db`; no migrations.
  Paths live ONCE in `media_file(id, path)` and every other table keys on `file_id`, so a raw `path =` query against a
  media table is the bug, and a rename is `rename_path`, one row.
- **GC is deletion-driven and edge-triggered (data-safety).** ONLY on a `Completed` bus edge (`borrow_and_update`, never
  a `borrow()` poll) or the Fresh sweep, never on volume-absence. Uncovered rows stay; only vanished files collect.
  ❌ Never persist the lifecycle-bus `generation`. ❌ NEVER whole-store `gc_targets` / `enrich_and_gc` on a live tick
  (wipes every row OUTSIDE the touched dirs). The exclusion veto reads LIVE `is_excluded`, never the pass snapshot,
  re-checked before each upsert.
- **Coverage = scope + importance.** Scope is an EXPLICIT `gate::IndexScope`, ❌ never a sentinel threshold. Resolve it
  ONLY via `lifecycle::pass_coverage` and pass it to `coverage::stored_row_survives`, or reclaim drifts. `folder_scores`
  `None` ⇒ override-only, ❌ NEVER enrich-all (a first-run race over-indexes permanently); `wire_volume` re-kicks once
  scored. EXCLUDED is a hard veto. Narrowing DELETES NOTHING.
- **Counts stream; polls never build them.** Aggregate through the `for_each_qualifying_image` sink
  (`coverage::count_qualifying_images`, O(folders)), ❌ never over `walk_image_entries`: one path `String` per image is
  the 50 GB launch runaway. Polls and startup use `coverage::cached` (no walk); only user-initiated settings reads use
  `get_or_build`. `None` means "no number yet", ❌ never `0`. The per-folder `accounted` aggregate is INCREMENTAL
  (writer ±1, seeded at spawn), never rebuilt from a walk, in its own `ACCOUNTED` cache, not `COUNTS`.
- **Parallel enrichment is N INDEPENDENT backends** (`scheduler/pool.rs`): ❌ never feed one backend concurrently (CF
  confinement), ❌ never fan out the single writer. Tests inject `FakeVisionBackend` via `MediaScheduler::new`, never
  `start`.
- **Cancellation hooks the EXISTING indexing watchdog** (❌ no second one; one shared memory ceiling). The
  between-images hook is `gate::should_stop` (watchdog OR toggle OFF); ❌ don't narrow it to `is_cancelled`.
- **ONLY a typed disconnect pauses a network pass** (rows kept, no GC, no `Failed`). Every other per-file read error is
  `FetchError::Unreadable`: skip-and-count, ❌ never a pause.
- **CLIP is a SEPARATE vector space from the Vision feature print** (`clip/`); ❌ never compare the two.
  `gate::semantic_search_enabled` is the SINGLE CLIP-write seam. **`ann/`'s index files are memory-mapped**, so a
  corrupt view SIGSEGVs. Both areas carry more invariants than fit here: read DETAILS § CLIP semantic search and
  § ANN vector search BEFORE touching either.
- **Every pass emits `media-enrich-progress`** over the ENRICHABLE subset (❌ never `images.len()`) and
  `media-enrich-terminal` on EVERY exit path. New commands register in BOTH `ipc.rs` and `ipc_collectors.rs`, events in
  `collect_events!`.

Architecture, flows, decisions, and the depth behind every line here: `DETAILS.md`. Read it before any non-trivial work:
editing, planning, reorganizing, or advising.
