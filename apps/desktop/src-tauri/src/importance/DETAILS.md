# Importance subsystem — details

The deterministic, cheap folder-importance score that any expensive feature consumes (the in-app agent, the media-ML
enrichment scheduler, future disk-cleanup / prefetch). Full design and milestone plan:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

M1 shipped the pure heart: the [`scorer`](scorer/mod.rs) and its tunable [`Weights`](scorer/weights.rs). M2 added storage
(`importance.db`), the scheduler that fills it on scan completion, and the navigation-visit signal. **M3 adds the
consumable [`ImportanceIndex`](read/mod.rs) read API (the canonical "how consumers reach importance" — see "The read API"),
incremental recompute on live listing changes, the shared per-volume writer registry, and the dev tuning surface.** SMB /
offline-unmounted reads are M4 (see the plan).

## Why a separate subsystem

Importance is a scoring **policy** (tunable weights, an explain breakdown, a formula that iterates) consumed by three
unrelated features. Folding it into the indexing aggregator would couple a churny formula to the one place a bug ships
wrong directory sizes and force every tweak through a `SCHEMA_VERSION` bump. So `importance/` is a sibling of `search/`:
a pure read consumer of `indexing/` with its own (later) store. Plan Decision 1 has the full rationale.

## The scorer (M1, `scorer/`)

Pure functions, values-in / values-out — no `rusqlite`, no `Volume`, no filesystem, no clock. "Now" is passed in as a
`u64` so recency is deterministic in tests (plan Decision 3, agent-spec §6.3 / §15 testability seams).

- `score(inputs, available, weights, now_secs) -> Score` — the scalar, `0.0..=1.0`.
- `explain(inputs, available, weights, now_secs) -> Explanation` — the same scalar plus the per-signal
  `SignalContribution` breakdown. `score` delegates to `explain`, so there is one formula, not two.

### Signal catalog (§5.1)

`FolderSignals` carries the raw signal vector. M1 covers the listing-derived signals; two optional signals are typed but
stay `None` until M2 wires their sources.

- **name denylist** (`name_denylisted`): a set-membership check on the folded folder name against the shared
  `search::SYSTEM_DIR_EXCLUDES` list (`node_modules`, `.git`, caches, build output). A FLOOR override, not an additive
  term — a denylisted folder scores `0.0` regardless of its other signals. Set-membership, never a substring match
  (`no-string-matching` rule).
- **hidden / system** (`hidden_or_system`): also a FLOOR override. A dotfile or system-owned folder scores `0.0`. The
  soft, non-floor side ("being visible is mildly positive") is the separate additive `Visibility` term.
- **under a floored ancestor** (`under_floored_ancestor`): the third FLOOR override — `true` when a self-flooring
  ancestor (denylisted / hidden / system) sits above this folder, so the whole subtree under a `node_modules`, a `.git`,
  or a cache floors, not just the named folder. See "The floor propagates to descendants" below for the derivation and
  the vendored-repo nuance.
- **extension diversity** (`distinct_extension_count` + `file_count`): mixed folders score above monocultures. Normalized
  as `distinct / min(file_count, 5)`, so three files of three kinds already reads as diverse while 200 `.log` files (one
  extension) reads as a monoculture. Zero files is neutral (`0.0`).
- **mtime recency** (`mtime_secs`): exponential half-life decay (`0.5 ^ (age / half_life)`), default half-life 30 days.
  `None` is neutral; a future timestamp (clock skew) clamps to `1.0`.
- **project markers** (`has_project_marker`): `1.0` when a `.git`/`Cargo.toml`/`package.json`/… sits in the folder or a
  descendant, raising the whole subtree (plan Decision 3).
- **path-class prior** (`path_class`): a typed `PathClass` — `ProjectRoot` (1.0) > `UserContent` (0.8) > `Neutral` (0.4)
  > `SystemOrCache` (0.0). The caller classifies the path once; the scorer reads the variant (no path-substring branch in
  the scorer).
- **visit activity** (`visit_count`, optional): linear up to a saturation count (default 10), then flat. `None` in M1.
- **Spotlight last-used** (`last_used_secs`, optional): recency decay, default half-life 14 days. `None` on SMB/MTP (no
  Spotlight) and `None` in M1.

### Missing-signal redistribution (plan Decision 3)

`SignalSet` marks which optional signals are AVAILABLE for a volume, independent of their value. When a signal is
unavailable (SMB has no Spotlight), its coefficient is removed and the remaining coefficients are scaled up so they sum
to the same total — the folder is never penalized for a signal its backend can't produce. Availability is distinct from
a `None` value: a local folder whose `kMDItemLastUsedDate` sampling simply hasn't run yet is *available but unsampled*
(contributes 0, drags the reachable max down), whereas an SMB folder is *unavailable* (its weight redistributes). The
`redistribution_preserves_total_weight` test pins the conservation; `missing_optional_signal_redistributes_not_penalizes`
pins the SMB-vs-local direction.

The five listing signals are always available; only the two backend-dependent optional signals ever redistribute. The
degenerate all-unavailable case can't occur (listing signals are always present), but the code guards the divide-by-zero
anyway.

### The explain invariant

For an unfloored folder, the `SignalContribution` list sums (then clamps) to exactly the `Score`, and each
`contribution == weight * raw`. When a FLOOR override fires, `Explanation::floored` is `true` and the additive terms are
reported at the values they *would* have contributed (so a tuner still sees the signal shape) while the score is `0.0`.
Pinned by `explain_contributions_sum_to_score_unfloored` and the proptest.

## Tunable weights (`scorer/weights.rs`)

The formula is unproven (agent-spec §18.3, plan open-question 1): the defaults are a STARTING POINT to tune against real
trees, not validated values. So the coefficients are data (`Weights`, serde-serializable, defaulted), not hardcoded
constants — the M3 dev-tuning surface will override them, and a future per-consumer profile can ship its own set. The
seven additive weights sum to `1.0` at their defaults so a folder that maxes every signal (and hits no floor) reaches
`1.0`; the scorer does not require that at runtime, and the redistribution and explain invariants hold for any values.

The largest default weights sit on the signals that most cleanly separate "matters" from "machine output": path class
(0.25) and project markers (0.20). Half-lives and the visit-saturation count are shape parameters, not additive weights.

## Synthetic-home fixture generator (`fixtures.rs`, `cfg(test)`)

`SyntheticHome::canonical(now)` builds the tree the plan names (a mixed Downloads, a `.git` project with a
`node_modules`, a monoculture logs folder, a Documents/invoices tree, a Library/Caches) as `FileEntry`s, and
`signals_for(path)` derives a `FolderSignals` for any folder in it. `volume()` materializes an `InMemoryVolume` over the
same tree for tests that want the real `Volume` listing surface. It owns its clock (`now_secs`) so a test scores against
the same "now" the mtimes were built from.

This is test-support code, not a production path: M2's scheduler is where signals get assembled from the real index.
The two must agree on what each signal means; this generator is the M1 stand-in that pins the formula's behavior
(`fixture_ranking_matches_expected_importance_order` is the end-to-end ranking assertion).

## Storage (`store/`, M2)

Per-volume `importance-{volume_id}.db`, a sibling of the drive index's `index-{volume_id}.db` in the app data dir. It
carries the index's disposable-cache discipline verbatim (plan Decision 2): the shared `platform_case` collation (reused
from `indexing::store` — the SAME filesystem case/normalization rule) registered on every connection, delete-and-recreate
on a `SCHEMA_VERSION` mismatch (no migrations — weights are regenerable), and ONE writer thread per DB
(`ImportanceWriter`, mirroring `IndexWriter`).

Three tables. `weights` is **keyed by the folded path** (`path_folded`, the BINARY primary key — the index's real
identity is the path, not the rebuild-unstable entry id) with the verbatim `path` kept as a plain column for return
values. Each row also holds the scalar `score`, the serialized raw `FolderSignals` vector (so a future consumer can
re-weight under its own profile without a rescan — plan Decision 2), and the **as-of `recompute_generation`** the pass
stamped. See "The folded-key primary key" below for why the key is precomputed rather than a `platform_case`-collated
`path`. `visits` holds the
navigation-visit signal (see below). `meta` holds `schema_version` and the per-volume `recompute_generation` counter, bumped
once per full pass. Every weight carries the **as-of `recompute_generation`** it was scored at — the honest staleness marker
an offline-unmounted read caveats with (all rows from the last full pass share it; the read API surfaces it). A full pass
REPLACES the whole `weights` table (see below), so a surviving row is never at an older generation than the store's.
`ImportanceWriter`'s surface: `write_weights(generation, rows)` (a full pass: clear + insert + advance the generation in one
transaction, so a reader never sees a bumped generation with un-written rows), `write_weights_incremental(generation, rows,
delete_subtrees)` (an incremental pass, below), `purge_volume` (forget), `record_visit`.

### The folded-key primary key (Decision/Why)

**Decision:** the primary key is a precomputed `path_folded` column — `normalize_for_comparison(path)` (the SAME fold the
`platform_case` collation applies: NFD-normalize then lowercase on macOS, identity elsewhere) — with a plain BINARY
collation, and the verbatim `path` rides along as a non-key column for return values. Every write folds the path once
(`insert_rows`, `apply_visit`, single-sourced through `normalize_for_comparison`); every read binds `folded(query)`
against `path_folded`. This reuses the index store's own `name_folded` pattern (`indexing/store`), for the same reason.

**Why not a `platform_case`-collated `path` PK (the old shape):** a custom collation on the key defeats SQLite's b-tree
range and LIKE-prefix optimizations. The incremental subtree-clear DELETE (`writer::apply_incremental`) therefore
FULL-SCANNED the whole `weights` table and re-ran the NFD-folding `platform_case_compare` on every row, per changed
prefix — CPU profiling put an incremental's entire cost in that comparison over the scan, and on the root volume (near-
continuous FSEvent churn ⇒ incrementals firing constantly) it pegged a CPU core. With a BINARY `path_folded` PK the same
DELETE is index-served: `EXPLAIN QUERY PLAN` shows `SEARCH weights USING PRIMARY KEY` for both the equality and the
half-open descendant range (a `MULTI-INDEX OR`), instead of `SCAN weights`. Pinned by
`subtree_clear_delete_is_index_served`.

**Correctness is preserved exactly.** `path_folded` is byte-identical to what the collation computed, so which case/NFD
variants collide into one row is unchanged; case/NFD-insensitive lookup still resolves (`weight_lookup_is_platform_case_insensitive`,
`incremental_write_resolves_a_case_and_nfd_variant`). Ranking is unaffected: the score is pure Rust (never touches SQL
collation), the search ranker looks up the verbatim `path` in a `HashMap`, and `ORDER BY score DESC, path ASC` is a
determinism tiebreak on the verbatim path. On case-sensitive volumes `normalize_for_comparison` is identity, so
`path_folded == path` (same fold-collision behavior as before — no regression). The `platform_case` collation stays
registered on every connection for parity with the index store; no importance query relies on it now.

Measurements (the index-served DELETE, plus why the full walk stays deferred rather than targeted):
[`docs/notes/idle-cpu-indexing-streamlining-2026-07.md`](../../../../../docs/notes/idle-cpu-indexing-streamlining-2026-07.md).

### Storage model: no floored rows, trimmed JSON (compaction)

Two decisions keep the store small (an older DB just recreates fresh on the next scan — it's a disposable cache):

- **A floored folder gets NO row.** On a dev home ~76% of folders floor (a `node_modules`, a `.git`, a cache, and their
  whole subtrees), and storing a `0.0` weight plus a full signal blob for each is pure waste. Floored-ness is derivable
  from the PATH STRING alone (`classify::self_floors` + the ancestor walk — pure name/path classification, no index or
  listing data), so the store simply omits floored folders and the read side re-derives them:
  - The full recompute skips writing a floored folder; the incremental pass clears a changed subtree and re-inserts only
    its non-floored folders (below).
  - `ImportanceIndex::lookup(path)` returns a typed `WeightLookup::{Scored, Floored(FloorReason), Unscored}`: `Scored`
    when a row exists, else `Floored` (carrying WHY, derived live) when the path floors by the shared classifiers
    (`classify::floors_by_path`), else `Unscored`.
    The scalar helpers stay compatible (`weight_for` reads `None` for a floored path, `WeightLookup::score()` flattens
    floored/unscored to `0.0`), but the typed `lookup` is the documented surface. `all_nonzero_weights` already omitted
    zeros, so search ranking is unaffected — the map just has fewer rows.
  - `explain` on a floored path reports a floored breakdown DERIVED LIVE from the path (score `0.0`, `floored == true`,
    the flag reflecting which classifier fired), not the stored "would-have-contributed" additive terms — a floored folder
    no longer stores those. Acceptable and documented: tuning cares about the non-floored ranking.
  - **The derive-on-read invariant that makes deletion safe**: for every folder the walk produces,
    `classify::floors_by_path(path)` (what the read side uses when a row is absent) agrees with the pure scorer's floor
    over that folder's full signals (what the pre-compaction store would have persisted). Pinned by
    `floored_by_path_matches_the_scorer_floor_for_every_walked_folder` over the whole synthetic home.
- **Trimmed JSON for kept rows.** `FolderSignals` serializes only its non-default fields (`#[serde(skip_serializing_if)]`
  on every field, plus `#[serde(default)]` so any subset deserializes). A neutral vector serializes to `{}`; a typical
  kept row carries two or three set fields, roughly halving the stored JSON. Deserialization is compatible in both
  directions — a verbose pre-compaction row and a trimmed one parse to the identical value (pinned by the
  full-vs-sparse round-trip test), so the store can hold a mix without a migration.

### Transition semantics on the incremental path (the subtle part)

An incremental pass (`write_weights_incremental`) CLEARS each changed subtree (an index-served range over `path_folded`;
see "The subtree clear" below), then inserts only the non-floored folders in the touched set (the changed subtrees plus
each changed path's capped ancestor chain), at the CURRENT generation without bumping it. Clearing-then-inserting handles
every floor transition in one model:

- a folder RENAMED AWAY or DELETED: its old-path row is cleared and never re-inserted (it's not in the current walk);
- a folder that BECAME floored (e.g. renamed to `node_modules`) and its now-under-floored descendants: cleared, then
  skipped on re-insert because they floor — so no stale positive-score row survives under a fresh `node_modules`;
- a folder that STOPPED being floored (e.g. `node_modules` renamed to an ordinary name) and its descendants: cleared (they
  had no row anyway), then inserted because they now score.

**The subtree clear.** It's an index-served BINARY range over the folded PK: `path_folded = folded(P)` for the changed
folder itself, plus `folded(P) + "/" <= path_folded < folded(P) + "0"` for every descendant (`"0"` at 0x30 is one past
`"/"` at 0x2f, and `/` is an ASCII boundary that folding never crosses, so the range holds exactly `P`'s descendants). The
`/` boundary means clearing `/a` never touches a sibling like `/ab`. Both floor directions are TDD'd
(`incremental_deletes_rows_that_become_floored`, `incremental_scores_rows_that_stop_being_floored`) — the likeliest bug
site. A full pass replaces the whole table instead (a full pass rewrites every folder, so clearing first purges any folder
that floored or vanished since the last pass).

The measurement/tuning entry point for this is `scheduler::recompute_index_to_db` (walk a real index read-only, score,
write an `importance.db` — the full-pass core without the registry), wrapped by the `importance-measure` dev bin, which
reports the row count, store size, and the phase wall-clock split (walk+score vs write+flush). The live full pass logs
that same split at info (`run_pass_blocking`, `target: "importance"`), so a regression in a real recompute's cost shows
up in the logs — the write phase dominates on a local root (compaction roughly halves it there by dropping ~76% of rows).

## The scheduler (`scheduler/`, M2)

Recomputes a volume's folder weights when its index finishes scanning. Two triggers, unified through one coalescing core:

- **The lifecycle bus** ([`indexing/lifecycle_bus.rs`](../indexing/DETAILS.md) — the mechanism is documented there,
  single-source): the scheduler subscribes per volume; a `ScanCompleted` publish ⇒ recompute.
- **The startup registry sweep** (`indexing::ready_volumes_with_kind`): a volume already Fresh at launch never re-fires
  `ScanCompleted` (its retained bus value stays `Pending`), so wiring its subscription alone never recomputes it — the
  common restart case. The sweep wires each ready volume WITH its typed kind (so MTP is excluded and SMB degrades
  correctly — see "Multi-volume" below), then runs `enqueue_initial_full_pass_if_unscored` per volume to actually score a
  fresh/recreated store (see § The initial full pass).
- **The registration bus** (`indexing::lifecycle_bus::subscribe_registrations`): a volume that registers AFTER the sweep
  (a share mounted mid-session) is wired then. The scheduler subscribes to it BEFORE the sweep, so no volume registering
  in the gap is lost (plan M4 late-registering volumes).

`PassCoordinator` is the pure, unit-tested coalescing core: it guarantees ONE pass per `volume_id` at a time — a request
arriving mid-pass sets a single re-run flag rather than starting a second pass (so the sweep + a concurrent
`ScanCompleted` collapse to one pass, then at most one re-run). The recompute itself is full-volume: walk the index tree
through the read pool (`get_read_pool_for`), assemble a `FolderSignals` per folder (`signals::signals_for_dir`), run the
pure scorer, and write every row at a freshly-bumped generation. It runs on a blocking background task (SQLite +
scoring), never on the IPC thread; a `None` read pool (index not registered) is a no-op.

### The initial full pass (the fresh/recreated-store trigger)

**Generation-stamp semantics.** `recompute_generation` (a `meta` counter) is stamped ONLY by a full pass
(`write_weights` → `apply_full_pass`, in the same transaction as the table replace). The incremental path
(`write_weights_incremental`) deliberately never bumps or stamps it. So "generation 0" does NOT mean "no weights": a
store maintained only by incremental rescores holds hundreds of thousands of usable weight rows at generation 0, and a
schema-recreated store sits at generation 0 until its first full pass. Consumers that must tell "genuinely unscored" from
"scored but no generation" key on the weight-row count, not the generation (media's `coverage::importance_scored`).

**A fresh/recreated store must get a full pass — the invariant.** Because a Fresh-at-launch volume never fires
`ScanCompleted`, the bus subscription alone never scores it. The sweep therefore runs
`enqueue_initial_full_pass_if_unscored` per ready volume: it enqueues a full recompute IFF the store carries no
generation. Gating on "no generation" (not an unconditional kick) is deliberate — importance is expensive, so an
unconditional kick would rescore every volume on every launch; media's kick is unconditional because a redundant
enrichment pass is a cheap staleness no-op. The policies differ on purpose.

**The recreate-ordering trap, and why the decision binds to the write-path open.** The schema delete-and-recreate happens
LAZILY, only inside `ImportanceStore::open` on a WRITE-path open (`open_write_connection`); the read path never
recreates. So on the prod schema-3 upgrade launch, the db is still on the OLD schema at sweep time WITH its old stamped
generation. A naive sweep-time generation READ would read that non-zero generation, decide "already scored", skip the
full pass — and THEN the recreate fires on the first incremental write, wiping the generation, leaving the volume stuck
at "never scored" forever. `store::needs_initial_full_pass` avoids this by opening the store on the WRITE path FIRST
(forcing the recreate), then reading the generation, so the decision reflects the current schema. ❌ Never probe the
generation via the read path before the write-path open. The store test drives this exact ordering (old-schema db with a
stamped generation → read probe sees it → the write-path-bound probe recreates and reports "needs a full pass").

### The walk is O(dirs), not O(entries) (`walk_index_folders`)

The full-recompute walk materializes **directories only** (`IndexStore::all_directories`) and STREAMS file rows
(`IndexStore::for_each_file_child`) into a small per-parent accumulator — distinct-extension set, file count, and the
direct-marker flag — collapsed to a `ChildAggregate` per folder. So pass memory is O(dirs), a small fraction of a
multi-million-entry NAS index, not O(entries) (an `all_entries` walk went transiently into the hundreds of MB on exactly
the NAS-sized volumes SMB scoring now enables). Directory children still come from the directory set (a `.git`/`.hg`/`.svn`
marker is a directory), so `has_direct_marker` folds both the streamed file children and the sibling directory children.
`signals_for_dir` takes the `ChildAggregate`, not child rows. `has_marker_below` is one upward propagation after the walk
(a `.git` deep in a tree raises its ancestors, plan Decision 3); `under_floored_ancestor` is its downward twin, a second
pass over the same parent map that floors every folder below a self-flooring one (see "The floor propagates to
descendants").

**Signal assembly agrees with the fixtures by construction.** The categorical signals (denylist, path class, project
marker, hidden) come from the shared [`classify`](classify.rs) module that BOTH `signals::signals_for_dir` (production)
and `fixtures::signals_for` (tests) call — so the M1 formula's test stand-in and the M2 real assembler can't drift on what
a signal means (the fixtures doc's standing warning, now enforced by shared code).

### `kMDItemLastUsedDate` sampling (`last_used.rs`, macOS-local)

The one potentially-slow input. We SAMPLE, not sweep: cap at `SAMPLE_CAP` folders per pass, query `MDItemCopyAttribute`
on a DEDICATED 8 MB-stack OS thread wrapped in `objc2::rc::autoreleasepool` (never rayon — a synchronous macOS-framework
round-trip; `src-tauri/CLAUDE.md`). An un-sampled local folder is *available but unsampled* (contributes 0, drags the
reachable max down), distinct from an SMB folder where the signal is *unavailable* and its weight redistributes — the
`SignalSet` the scheduler passes encodes which. **Sampling runs ONLY when the volume's mask says `last_used_available`**:
SMB has no Spotlight, and sampling would issue `MDItem` queries against the mount, which the scheduler must never do (it
reads only the local index). Off macOS the sample is empty and `last_used` is unavailable.

## The visit signal (`commands.rs` + `store` visits table, M2, plan Decision 3)

A typed `record_visit(Location)` IPC command the frontend's navigation-commit point calls fire-and-forget (the
`persistLastUsedPath` hook in `pane/persistence-subscriber.svelte.ts`, alongside the existing last-used-path save). It
persists a compact per-volume `visits` row: **counts and timestamps only, no content, local-only** (the privacy-sane shape
— noted in `docs/security.md`). The scorer's visit-activity signal reads it on the next recompute. Fire-and-forget and
failure-silent by contract: a visit that can't be recorded must never block or break navigation, so the command returns
`Ok(())` even on a write hiccup. **Recorded for any background-scored volume — Local and SMB** (M4); an unregistered or
MTP volume is skipped (recording a visit no recompute reads is dead weight), gated on the registered volume's TYPED kind
(`indexing::volume_kind`), never its id string. The agent spec's planned `user_action_log` is this signal's future
superset — when it lands, `record_visit` folds into it (never two parallel recorders).

## The read API (`read.rs`, M3) — the canonical consumer entry point

`ImportanceIndex` is the ONE way any consumer (the agent, media-ML enrichment, future cleanup/prefetch) reaches folder
importance — the agent and media-ML plans point here rather than restating (single-source, `docs.md`). It mirrors
`search/`→`indexing/`: a read-only handle that owns a `platform_case`-registered read connection over `importance.db`
(thread-local, reopened lazily), so no consumer takes a raw `rusqlite` dep on the store. Calls:

- `weight_for(path)` — one folder's `ScoredWeight` (scalar + deserialized `FolderSignals` + as-of generation), or `None`.
- `lookup(path)` — the typed `WeightLookup::{Scored, Floored(FloorReason), Unscored}`. A floored folder has no row, so
  the reason (`nameDenylisted` / `hiddenOrSystem` / `underFlooredAncestor`, in that precedence) is derived live from the
  path — the single derivation `explain`'s floored breakdown also uses.
- `top_n(n)` / `above_threshold(t)` / `top_above_threshold(n, t)` — ranked folders (score DESC, ties by path).
  `above_threshold` is INCLUSIVE at the bound (a folder exactly at `t` is returned); `top_above_threshold` combines the
  `LIMIT` and `WHERE score >= t` in one bounded query (the resource's capped threshold read fetches `cap + 1` to detect
  truncation). The agent's summary gate and media-ML's enrich-important-first.
- `scored_folder_count()` — the `weights` row count (a `COUNT(*)`, no deserialization), for the overview surface.
- `signals_for(path)` — the stored raw vector, for a consumer applying its own weighting profile (plan Decision 2).
- `all_nonzero_weights()` — the bulk `path → score` map (non-zero scores only; floored folders omitted), for a consumer
  that loads one snapshot and ranks many candidates in memory rather than querying per item.
- `explain(path, now)` — the per-signal breakdown, **recomputed from the STORED signals via the pure scorer**
  ([`explain`](scorer/mod.rs)), so there's ONE formula and the breakdown can't drift from the stored scalar.

**First consumer: search ranking.** `search/` blends these weights into result ordering (a file takes its parent
folder's weight), loading one `all_nonzero_weights` snapshot per recompute via `subscribe`. Match quality dominates;
importance is a within-band boost. The blend design, weight-map lifecycle, and degradation contract live in
[`search/DETAILS.md`](../search/DETAILS.md) § Importance ranking (single-source).

**Second consumer: the MCP `cmdr://importance` resource.** It exposes `lookup` / `top_n` / `above_threshold` /
`top_above_threshold` / `explain` / `scored_folder_count` to agents, enumerating scored volumes offline via
`read::scored_volume_ids` (the `importance-{id}.db` files on disk) and opening each index with the kind's
`signal_availability` mask so `explain` sums to the stored score. It's the offline-unmounted read made a user-facing
feature. Builder + modes: [`mcp/DETAILS.md`](../mcp/DETAILS.md) § Resources (`cmdr://importance`).

**Staleness is first-class.** Every result carries `as_of_generation`; a consumer compares it to `recompute_generation()`
to caveat "as of the May 28 scan" (agent-spec D7; M4's offline-unmounted read leans on this). The read API never hides a
stale weight.

**The recompute subscription** (`read::subscribe(volume_id)`) is a `tokio::sync::watch<u64>` receiver carrying the last
completed generation. The scheduler calls `notify_recompute_completed` after each full or incremental pass, so a consumer
awaits `changed()` instead of polling (subscribe-don't-poll). It retains the last value for a late subscriber and fires
exactly once per completion. The senders live in a process-global keyed by volume id (survives an unmount), like the
indexing lifecycle bus.

## Incremental recompute (`scheduler`, M3, plan Decision 5)

A full-volume recompute on `ScanCompleted` stays the default. On top of it, live listing changes drive an **incremental
rescore** of only the touched folders, so a single file edit doesn't re-walk-and-rescore the whole volume.

**The event source (documented choice).** There is no clean in-process per-directory hook in `indexing/`: the reconciler
reports directory changes only via `IndexDirUpdatedEvent` to the frontend, and the writer/aggregation `emit_dir_updated`
sites aren't uniformly volume-aware. So — exactly as M2 added `publish_scan_completed` alongside the frontend `.emit` —
M3 adds a per-volume `dir-changed` channel to [`indexing/lifecycle_bus`](../indexing/DETAILS.md)
(`publish_dirs_changed`), published from the **live-change sites where `volume_id` is in scope**: the live event loop
(FSEvents batches) and the per-navigation verifier. The scan-completion `/`-refresh emits are left on the full-recompute
path (already covered by `ScanCompleted`), so incremental captures exactly the "listing changed while running" signal.
The scheduler subscribes via `subscribe_dirs_changed` and coalesces bursts per volume (accumulating paths into a pending
set, one pass plus at most one re-run — a distinct coordinator key from the full pass so the two don't block each other).

**Rescoping + the ancestor cap.** For each changed path the touched set is the folder itself plus its ancestor chain
(`touched_folder_set`, because a project marker or size/mtime change can raise parents) UNION each changed path's whole
descendant subtree (because a floor transition flips the whole subtree — see the storage model's transition semantics).
The ancestor walk is capped at `ANCESTOR_WALK_CAP` (32) levels per changed path: a project marker appearing deep in a tree
could otherwise raise every ancestor to the root and rescope half the volume (plan open-question); the downward side is
bounded by the subtree that actually changed. The pass walks the index once, filters to the touched subset, clears each
changed subtree, and re-inserts only its non-floored folders.

**Generation semantics.** An incremental pass writes its rows at the CURRENT generation and does NOT bump it, so every
untouched folder keeps its as-of marker and the volume doesn't turn wholesale-stale after a one-file change. Only a full
pass advances the generation.

**The incremental never escalates on `/`.** Every live `dir-changed` batch carries the bare root `/`:
`reconciler::collect_ancestor_paths` walks each change up to `/` so the frontend can refresh every ancestor's displayed
size, so `/` sits in essentially every batch as the *universal ancestor* — not a signal that the whole volume changed.
`sanitize_incremental_batch` drops `/` (and empty strings) at the incremental boundary before `touched_folder_set` /
`write_weights_incremental` see it; a batch that was only `/` is a no-op. Full recomputes are `ScanCompleted`-driven
only — the incremental path never calls `run_pass_blocking`. **Gotcha/Why:** treating `/` as a full-refresh sentinel
(escalate to a whole-volume rewrite) meant that because the root volume live-watches `/`, where macOS FSEvent churn is
near-continuous, `/` arrived in almost every batch and full recomputes ran back-to-back forever — pegging a core and
starving the index-DB WAL checkpoint (its `wal_checkpoint(TRUNCATE)` kept losing to importance's long read), which
surfaced as `stall_probe::sqlite_busy` WARN bursts. ❌ Don't reintroduce a `/`→full-pass escalation.

**Debounce (leading + trailing).** Each incremental still walks the whole index (O(dirs)) before rescoping to the
touched subset; that walk now dominates each incremental's cost, because the targeted write is index-served against the
BINARY `path_folded` PK (sub-millisecond even on a 166k-row store). **Gotcha/Why:** before the folded-key column the
subtree-clear DELETE keyed on a `platform_case`-collated `path` PK, which defeats SQLite's b-tree range/LIKE
optimization — so it FULL-SCANNED every row and re-ran the NFD-folding comparison on each, per changed prefix. On the
root volume, where FSEvent churn keeps incrementals firing, that single DELETE pegged a CPU core (CPU profiling put the
whole incremental's cost in `platform_case_compare` over the scan). ❌ Don't revert the PK to a collated `path` or the
clear to a `LIKE` prefix — both reintroduce the full scan. So `spawn_incremental` debounces per volume: the first pass
of a burst runs immediately (leading edge), and under sustained change it runs at most once per
`INCREMENTAL_THROTTLE_WINDOW` (60 s; a throttle, NOT a debounce that never fires under constant change). Coalesced
requests accumulate during the wait and the next drain folds them all in. Importance is a background signal, so the lag
is invisible to consumers. **Ideal follow-up (deferred):** a targeted walk reading only the changed subtree's directory
+ child rows would make each incremental ~O(touched) and remove the need to debounce; it's deferred because computing
`has_marker_below` / `under_floored_ancestor` correctly across the subtree boundary (an ancestor outside the subtree can
floor it) is a real correctness surface.

## The shared writer registry (`writer_registry.rs`, M3)

The subsystem's one-writer-per-DB invariant must hold in spirit, not be papered over by WAL busy-timeouts: both
`record_visit` and every recompute write to a volume's `importance.db`. `WriterRegistry` (owned by the `ImportanceScheduler`,
in Tauri managed state) hands both a SHARED long-lived `ImportanceWriter` per volume, created lazily on first use and
living for the process. `record_visit` reaches it via `app.try_state::<Arc<ImportanceScheduler>>()`; the scheduler's
recompute reaches it via `writer_for`. Creation reserves the slot then builds outside the map lock, so two concurrent
first-uses can't race two threads onto one DB. `next_generation()` reads the current generation on the writer thread's own
connection (not a separate reader), keeping the generation a single-writer-owned value.

## Dev tuning surface (`crates/index-query`'s `importance-tune` bin, M3, plan Decision 6)

A minimal dev-only binary extending the `index-query` pattern (a `cmdr_lib`-linking CLI with the collation registered).
It reads a volume's `importance.db` through the SAME `ImportanceIndex` read API and prints the ranked folders WITH their
`explain` breakdowns, so David can eyeball the ranking against his real home directory and tune `Weights` (agent-spec
§18.3). No write path — it reads stored signals and re-scores. Usage:

```
cargo run -p index-query --bin importance-tune -- <path-to-importance-root.db> [top_n]
```

Find the DB under the app data dir as `importance-root.db` (beside `index-root.db`); `top_n` defaults to 30. The printout
lists each folder's score, then per-signal `weight`, `raw`, and `contribution` (skipping signals redistributed to zero),
so a mis-ranked folder's cause is visible.

## Ranking-quality evals (`evals/`, the weight-tuning instrument)

The `importance-tune` bin above eyeballs a ranking; the `evals/` module MEASURES ranking quality, so a weight change
becomes a number instead of a vibe. It's the measurement instrument for the plan's open-question 1 (unvalidated default
weights). The full David-facing how-to (running, reading the score, adding a scenario, the snapshot/label/tune loop, the
privacy contract) is [`docs/guides/importance-evals.md`](../../../../../docs/guides/importance-evals.md); the design in
brief:

- **A `Scenario` is the shared unit**: folders + their derived `FolderSignals` + two tiers of ranking expectations
  (`scenario.rs`). NOT a synthetic tree — the pure scorer only needs signals, and this is exactly what a real-index dump
  can export. So synthetic scenarios (`scenarios.rs`) and anonymized corpus dumps (`corpus.rs`) load into the SAME type
  and score through the same path.
- **Hard vs. soft, and the floor** (`constraints.rs`): hard constraints are ordering facts asserted as `#[test]`s (a
  violation fails CI); soft constraints are counted into a scalar quality score. The aggregate soft score is pinned to a
  FIXED floor constant (`SOFT_SCORE_FLOOR` in `evals/tests.rs`), consciously raised when tuning improves quality — never
  a self-updating ratchet. `score_scenario(scenario, weights) -> f64` is the pure, fast fitness function a grid-search
  could optimize.
- **The corpus tool reuses production signal-derivation** (`corpus.rs` calls `scheduler::walk_index_folders` +
  `signals::signals_for_dir`), so a dumped scenario's signals match what the live scheduler computes. The
  `importance-snapshot` bin (`crates/index-query`) wraps it: read a real `index-{volume_id}.db` READ-ONLY, derive
  signals, anonymize every folder name, and write a `.scenario.json` + a `.labels.json` template into a GITIGNORED
  corpus dir (`apps/desktop/src-tauri/tests/importance-corpus/`). Real dumps are NEVER committed; the suite is green with
  zero corpus files present.
- **Anonymization is the privacy crux** (`corpus.rs`): the scorer reads a folder name ONLY through the classifiers, so
  every name that doesn't feed one becomes a stable `dir-<hash>` placeholder with zero effect on the score. Kept
  verbatim: denylist hits, dot-prefixed names, path-class anchors (as home children), project markers. The home root
  itself becomes a synthetic `/home` or `/volume`. Pinned by the `corpus/tests.rs` privacy tests.

### The floor propagates to descendants (the descendant-floor rule)

A floored folder floors its whole SUBTREE, not just the named folder. Three flags floor a folder to `0.0`:
`name_denylisted`, `hidden_or_system`, and `under_floored_ancestor` — the last is `true` when any self-flooring ancestor
(a denylisted, hidden, or system folder) sits above it. So a `node_modules/<pkg>/dist` floors even though `dist` isn't
itself denylisted, and a `.git/refs/heads` floors under the `.git`. Without this, scoring David's real `index-root.db`
(646k folders) ranked deep machine-output folders at the TOP: the 312k folders living under a `node_modules` inherited a
project-root prior from an ancestor `.git` and scored ~0.85, dwarfing real content.

- **Derivation is shared** (the `classify.rs` must-know): `classify::self_floors` decides the seed, and both the
  production walk (`scheduler/recompute.rs`, a downward propagation over the same `id → parent_id` map path
  reconstruction uses — the twin of the upward `has_marker_below`) and the fixtures / evals scenario builder
  (`classify::under_floored_paths`, pure path math) derive `under_floored_ancestor` from it, so synthetic scenarios
  exercise the exact rule production applies.
- **Floor beats marker (the vendored-repo nuance).** A folder that IS itself a project root — a repo vendored inside a
  `node_modules`, carrying its own `.git` — stays floored when it sits under a floored ancestor. The floor is a hard cap
  outside the additive sum, so `has_project_marker`/`ProjectRoot` can't rescue it. That's the intended behavior: a
  vendored dependency is machine output, project markers and all.
- **Persistence.** `under_floored_ancestor` is a `#[serde(default)]` field on `FolderSignals`, so a vector persisted
  before it existed still deserializes (its absence reads as `false`); such a row is a stale generation and gets
  overwritten on the next full pass. The eval scenarios' hard constraints (a `ScoreAtMost 0.0` on every folder under a
  `node_modules`/`.git`/cache, plus the vendored-repo case) regression-guard the rule.

## Adding a signal (step-by-step)

Add the field to [`FolderSignals`](scorer/types.rs) (+ `neutral()`), a `SignalKind` variant (+ `ALL`), a `Weights`
coefficient (+ `additive_weight`), and a `raw_signal_value` arm in [`scorer`](scorer/mod.rs); if the signal is optional
(backend-dependent), add a `SignalSet` flag + a `signal_available` arm so it redistributes when absent. Then cover its
contribution DIRECTION with a test and keep the explain-sums invariant green. The categorical signals also need a
`classify.rs` classifier (shared by prod + fixtures) and an assembly line in `signals.rs`.

## Multi-volume, kind-aware scoring + offline-unmounted reads (M4)

The scheduler scores **any** background-scored volume, not just the local `root`. The typed volume kind
(`indexing::IndexVolumeKind`, retained on the registry instance) decides the policy at a single seam
(`ScoringPolicy::for_kind`), never by inspecting the volume-id string (`no-string-matching`):

- **Local** — background-scored; both optional signals available (visits + Spotlight where the OS has it).
- **SMB** — background-scored, but **Spotlight is unavailable** (no `kMDItemLastUsedDate` over a share), so `last_used`'s
  weight redistributes onto the listing signals (the M1 scorer's redistribution makes this honest: a missing signal
  spreads, never fabricates). Visits still apply — they come from Cmdr navigation, not the mount.
- **MTP** — an explicit **exclusion**, not an accident of gating: a phone/camera is on-demand only, never
  background-scored. The scheduler skips it at every entry point (sweep, registration, bus subscription), and
  `record_visit` skips it too.

**Network-mount discipline.** The scheduler never issues a filesystem syscall against an SMB/MTP mount — it reads only
the local index DB. Spotlight sampling is gated on the mask (`last_used_available`), so it never runs for SMB (which would
have meant `MDItem` queries against the mount).

**Offline-unmounted reads (the headline, plan Decision 2).** `ImportanceIndex` reads a volume's `importance.db` (a local
per-volume file) directly and NEVER touches the index registry — so a volume's weights stay queryable after it unmounts
(its index registration gone, `get_read_pool_for` now `None`). `weight_for`/`top_n`/`recompute_generation` answer from the
on-disk store, each weight carrying the **as-of generation** it was scored at (the staleness caveat: "as of the last scan
before the NAS went offline"). Proven end to end by `offline_unmounted_read_returns_stored_weights_after_index_gone`
(score a volume, delete its index DB, assert the read API still returns weights at the right generation). When the OS
purges the cache the file vanishes and the read returns `None`; the next mount + scan regenerates it — weights are
disposable, identical to the index-purge path.

**Late-registering volumes.** A share mounted mid-session registers on the lifecycle bus
([`indexing/DETAILS.md` § the bus](../indexing/DETAILS.md)); the scheduler subscribes to registrations once (before its
startup sweep, closing the gap) and wires the new volume's scan-completion + dir-changed subscriptions on arrival. The
registration event carries the typed kind so the scheduler applies the same score/degrade/exclude policy.

## What v1 still leaves out

- No IPC surface beyond `record_visit`; no user-facing strings, no i18n (`record_visit` and the tuning bin are invisible
  to the app UI).
- Weight tuning against real trees, and the `kMDItemLastUsedDate` sampling cost, are unmeasured — see the plan's
  open-questions.

## The dir-changed `watch` can drop a batch under bursts (accepted)

The incremental trigger rides the per-volume `dir-changed` `watch` channel (`indexing/lifecycle_bus`). A `watch` is
last-value-wins: if two `publish_dirs_changed` batches land between the scheduler's `borrow_and_update` reads, the
consumer sees only the later batch's paths — the earlier batch's paths can be dropped. This is **acceptable and by
design**: importance is advisory, disposable derived data, and the next full recompute (on the next `ScanCompleted`)
heals any folder a dropped incremental batch missed. We don't add an unbounded queue to make incremental lossless; the
full pass is the backstop.

## Testing

All M1 tests are pure (`scorer/tests.rs`): no FFI, no DB, a fixed `NOW`. They assert each signal's contribution
DIRECTION (the plan's M1 list), the explain-sums-to-score invariant, missing-signal redistribution, the serde round-trip
(load-bearing for M2), the fixture-tree shape, and a proptest that the score is always finite and in `[0,1]`.

M4's scheduler tests (`scheduler/tests.rs`, over synthetic indexes, no FFI, no registry): `ScoringPolicy` scores
Local/SMB and excludes MTP; SMB's recompute degrades Spotlight and redistributes (never fabricates); the O(dirs) walk's
`ChildAggregate` matches a whole-tree oracle (the memory-fix characterization); the offline read returns stored weights
at the right as-of generation after the index DB is deleted; a multi-volume recompute scores each volume into its own
store. The registration bus's late-volume delivery is covered in `indexing/lifecycle_bus`.
