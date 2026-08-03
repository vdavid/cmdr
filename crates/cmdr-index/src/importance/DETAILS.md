# Importance subsystem — details

The deterministic, cheap folder-importance score that any expensive feature consumes (the in-app agent, the media-ML
enrichment scheduler, future disk-cleanup / prefetch). Full design and milestone plan:
`docs/specs/later/importance-subsystem-plan.md`. This doc covers what's SUBSYSTEM-WIDE plus the top-level files no area
subdir owns; each area documents itself.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

**Where the area depth lives:** `scorer/DETAILS.md` (the formula, the signal catalog, redistribution, the weights),
`store/DETAILS.md` (the schema, the folded key, storage compaction), `scheduler/DETAILS.md` (triggers, the O(dirs) walk,
incremental rescore, the kind policy), `read/DETAILS.md` (`ImportanceIndex`, offline reads, the consumers, the tuning
bin), `evals/DETAILS.md` (ranking quality and the corpus).

## Why a separate subsystem

Importance is a scoring **policy** (tunable weights, an explain breakdown, a formula that iterates) consumed by three
unrelated features. Folding it into the indexing aggregator would couple a churny formula to the one place a bug ships
wrong directory sizes, and force every tweak through the index's `SCHEMA_VERSION` bump. So `importance/` is a sibling of
`search/`: a pure read consumer of `indexing/` with its own store.

`media_index/` is a later PORT of these patterns, so a change to the shared shape here is worth checking against
`../media_index/DETAILS.md` § Why a port of `importance/`.

## The floor propagates to descendants (the descendant-floor rule)

A floored folder floors its whole SUBTREE, not just the named folder. Three flags floor a folder to `0.0`:
`name_denylisted`, `hidden_or_system`, and `under_floored_ancestor` — the last is `true` when any self-flooring ancestor
(a denylisted, hidden, or system folder) sits above it. So a `node_modules/<pkg>/dist` floors even though `dist` isn't
itself denylisted, and a `.git/refs/heads` floors under the `.git`. Without this, scoring David's real `index-root.db`
(646k folders) ranked deep machine-output folders at the TOP: the 312k folders living under a `node_modules` inherited a
project-root prior from an ancestor `.git` and scored ~0.85, dwarfing real content.

- **Derivation is shared.** `classify::self_floors` decides the seed, and both the production walk (a downward
  propagation over the same `id → parent_id` map path reconstruction uses, the twin of the upward `has_marker_below` —
  `scheduler/DETAILS.md` § The walk) and the fixtures / evals scenario builder (`classify::under_floored_paths`, pure
  path math) derive `under_floored_ancestor` from it, so synthetic scenarios exercise the exact rule production applies.
  `classify::floors_by_path` is the read side's live derivation for a folder with no row.
- **Floor beats marker (the vendored-repo nuance).** A folder that IS itself a project root — a repo vendored inside a
  `node_modules`, carrying its own `.git` — stays floored when it sits under a floored ancestor. The floor is a hard cap
  outside the additive sum, so `has_project_marker` / `ProjectRoot` can't rescue it. That's the intended behavior: a
  vendored dependency is machine output, project markers and all.
- **Persistence.** `under_floored_ancestor` is a `#[serde(default)]` field on `FolderSignals`, so a vector persisted
  before it existed still deserializes (its absence reads as `false`); such a row is a stale generation and gets
  overwritten on the next full pass. The eval scenarios' hard constraints (a `ScoreAtMost 0.0` on every folder under a
  `node_modules` / `.git` / cache, plus the vendored-repo case) regression-guard the rule.

## The shared classifiers (`classify.rs`)

Pure path/name classifiers — `leaf_name`, `is_denylisted`, `is_hidden_or_system`, `self_floors`, `floors_by_path`,
`under_floored_paths`, `is_project_marker`, `path_class` — shared by production signal assembly (`signals.rs`), the test
fixtures, and the evals scenario builder. Keeping them in ONE place is load-bearing: the test stand-in and the real
assembler must agree on what each signal means, and the only way to guarantee that is shared code, not re-derivation.

They run once per folder in the recompute walk and again per folder in scoring, so they hold no allocation on the ASCII
path: the folded denylist is a process-wide `LazyLock`, and `path_class` strips prefixes rather than formatting a
candidate path per check. A non-ASCII name still takes the exact `to_lowercase` path, because it can fold ONTO an ASCII
name (U+212A KELVIN SIGN lowercases to `k`).

## Signal assembly (`signals.rs`)

`signals_for_dir` is the production counterpart to the fixtures' `signals_for`: values in (an mtime, a `ChildAggregate`,
a reconstructed path, the home, the optional visit/last-used inputs), a `FolderSignals` out. No I/O — the caller reads
the index; this only classifies. `ChildAggregate` is the three scalars a folder's direct children collapse to (distinct
extension count, file count, direct-marker flag), which is why the walk never has to hold child rows.

## Sampled `kMDItemLastUsedDate` (`last_used.rs`, macOS-local)

The one potentially-slow input. We SAMPLE, not sweep: cap at `SAMPLE_CAP` (500) folders per pass and query
`MDItemCopyAttribute` on a DEDICATED 8 MB-stack OS thread wrapped in `objc2::rc::autoreleasepool` — never rayon, since a
synchronous macOS-framework round-trip can blow rayon's 2 MB worker stack (`src-tauri/CLAUDE.md`), and never inline on
the caller. A folder with no `kMDItemLastUsedDate` (never opened, or Spotlight has no record) is simply absent from the
returned map.

An un-sampled local folder is _available but unsampled_, which is NOT the same as an SMB folder where the signal is
_unavailable_ (`scorer/DETAILS.md` § Missing-signal redistribution); the `SignalSet` the scheduler passes encodes which.
**Sampling runs ONLY when the volume's mask says `last_used_available`**: SMB has no Spotlight, and sampling would issue
`MDItem` queries against the mount, which the scheduler must never do (it reads only the local index). Off macOS
`is_available()` is `false`, the sample is empty, and the weight redistributes.

`SAMPLE_CAP` is a guess until measured on a real home; the caller hands the sampler only the paths it can use, not the
whole volume's, which is worth ~60 MB of transient on a local volume.

## The visit signal (`apps/desktop/src-tauri/src/commands/importance.rs` + the store's `visits` table)

A typed `record_visit(Location)` IPC command the frontend's navigation-commit point calls fire-and-forget (the
`persistLastUsedPath` hook in `apps/desktop/src/lib/file-explorer/pane/persistence-subscriber.svelte.ts`, alongside the
existing last-used-path save). It persists a compact per-volume `visits` row: **counts and timestamps only, no content,
local-only** — the privacy-sane shape, noted in `docs/security.md`. The scorer's visit-activity signal reads it on the
next recompute.

Fire-and-forget and failure-silent by contract: a visit that can't be recorded must never block or break navigation, so
the command returns `Ok(())` even on a write hiccup (it logs at debug). If the scheduler isn't in managed state yet
(startup raced ahead of `start`), the visit is dropped and the next navigation records it. **Recorded for any
background-scored volume, Local and SMB**; an unregistered or MTP volume is skipped (recording a visit no recompute
reads is dead weight), gated on the registered volume's TYPED kind (`indexing::volume_kind`), never its id string.

The agent spec's planned `user_action_log` is this signal's future superset — when it lands, `record_visit` folds into
it, never two parallel recorders.

## The writer (`writer.rs`)

`ImportanceWriter` mirrors the index's `IndexWriter`: exactly ONE writer thread owns the single write connection per DB,
and all writes cross a bounded channel (1,024 messages, plenty because a whole pass is one message, while still giving
backpressure on a pathological visit storm). The handle is cloneable; every clone shares the one channel and thread.
Each message is applied under a single transaction, so a crash mid-pass leaves the prior generation intact — recompute
is idempotent and re-runs from the bus on the next scan completion.

- `write_weights(generation, rows)` — a full pass: clear the table, insert, and advance the generation in ONE
  transaction, so a reader never sees a bumped generation with un-written or stale rows. Clearing first is what purges a
  folder that has since floored or vanished, which the compacted store requires.
- `write_weights_incremental(generation, rows, rescored_subtrees)` — an incremental pass in one transaction: READ each
  rescored subtree, then write back only what moved (upsert the rows whose signals differ, delete the stored rows the
  pass no longer scores) at the CURRENT generation without bumping it. Every stored row in the subtree takes exactly one
  `StoredRowFate`, which is what keeps the delete and the insert from disagreeing. The transition model this implements
  is `scheduler/DETAILS.md` § Transition semantics, the skip and its signals-not-score equality key are the same file's
  § "Only what moved is written", and the index-served range math is `store/DETAILS.md` § The folded-key primary key.
- `purge_volume` — drop all weights and visits (a consumer forgot the volume); the schema stays.
- `record_visit(path, at_secs)` — bump a path's visit count and last-visit timestamp.
- `next_generation()` — read the current generation on the writer thread's OWN connection and return `current + 1`, so
  the generation stays a single-writer-owned value with no reader racing a concurrent write.
- `flush_blocking`, `checkpoint_wal`, `shutdown`.

### WAL checkpoint at recompute completion (Decision/Why)

**Decision:** the writer runs `PRAGMA wal_checkpoint(TRUNCATE)` (`writer::run_wal_checkpoint`, driven by
`ImportanceWriter::checkpoint_wal`) at every recompute completion — after both a full pass (`recompute_folders`) and an
incremental rescore (`incremental_rescore`), once the write is flushed. It runs on the writer thread's own connection
(the single-writer invariant; never a side connection), in autocommit: every message commits before the loop reads the
next, so the TRUNCATE, which SQLite refuses inside a transaction, is always legal there.

**Why:** no `wal_autocheckpoint` override is set, so SQLite's default PASSIVE autocheckpoint copies frames back into the
main DB but reuses the WAL file in place and never shrinks it. A full pass REPLACES the whole `weights` table and the
throttled incremental churns pages, so the WAL climbed to ~100% of the DB size (100 MB DB, 100 MB WAL observed on the
dev `importance-root.db`) and stayed there. Only an explicit TRUNCATE reclaims that on-disk space, and a recompute
completion is the natural quiet point to take it, keeping the WAL small (≤ ~16 MB at rest).

**Busy tolerance, no retry loop:** a long-lived reader snapshot can block the truncate. The checkpoint brackets itself
with a short busy timeout (250 ms, mirroring the index writer's cap in `indexing/writer/maintenance.rs`) so it can't
stall the writer thread for the connection's default 5 s, then degrades to PASSIVE (`busy = 1`): the frames still
checkpoint into the main DB, the file just doesn't shrink this time, and the next recompute retries. It logs at debug
and moves on; the recompute callers `let _ =` the result, so a checkpoint hiccup never fails a pass.

## The shared writer registry (`writer_registry.rs`)

The one-writer-per-DB invariant must hold in spirit, not be papered over by WAL busy-timeouts: both `record_visit` and
every recompute write to a volume's `importance.db`. `WriterRegistry` (owned by the `ImportanceScheduler`, in Tauri
managed state) hands both a SHARED long-lived `ImportanceWriter` per volume, created lazily on first use and living for
the process. `record_visit` reaches it via `app.try_state::<Arc<ImportanceScheduler>>()`; the scheduler's recompute
reaches it via `writer_for`. Creation reserves the slot then builds outside the map lock, so two concurrent first-uses
can't race two threads onto one DB. Keyed by volume id and independent of the index registry, so a writer outlives an
unmount and a late `record_visit` or queued recompute still has one writer to go through.

## Synthetic-home fixture generator (`fixtures.rs`, `cfg(test)`)

`SyntheticHome::canonical(now)` builds the tree the plan names (a mixed Downloads, a `.git` project with a
`node_modules`, a monoculture logs folder, a Documents/invoices tree, a Library/Caches) as `FileEntry`s, and
`signals_for(path)` derives a `FolderSignals` for any folder in it. `volume()` materializes an `InMemoryVolume` over the
same tree for tests that want the real `Volume` listing surface. It owns its clock (`now_secs`), so a test scores
against the same "now" the mtimes were built from.

This is test-support code, not a production path. It and the production assembler must agree on what each signal means,
which is enforced by both going through `classify.rs` rather than by convention.
`fixture_ranking_matches_expected_importance_order` (in `scorer/tests.rs`) is the end-to-end ranking assertion over the
canonical tree.

## Dev and measurement tooling

Three `crates/index-query` binaries, each documented next to the code it drives:

- `importance-tune` — eyeball a real volume's ranking with `explain` breakdowns (`read/DETAILS.md`).
- `importance-measure` — a full pass's row count, store size, phase split, and memory growth (`scheduler/DETAILS.md`).
- `importance-snapshot` — dump an anonymized eval scenario from a real index (`evals/DETAILS.md`).

## What v1 still leaves out

- No IPC surface beyond `record_visit`; no user-facing strings, no i18n (`record_visit` and the dev bins are invisible
  to the app UI).
- Weight tuning against real trees, and the `kMDItemLastUsedDate` sampling cost, are unmeasured — see the plan's
  open-questions.
