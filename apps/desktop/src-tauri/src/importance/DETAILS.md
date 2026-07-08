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

Three tables. `weights` is **path-keyed** (the index's real identity is the path, not the rebuild-unstable entry id): each
row holds the scalar `score`, the serialized raw `FolderSignals` vector (so a future consumer can re-weight under its own
profile without a rescan — plan Decision 2), and the **as-of `recompute_generation`** the pass stamped. `visits` holds the
navigation-visit signal (see below). `meta` holds `schema_version` and the per-volume `recompute_generation` counter, bumped
once per full pass. **The staleness predicate is `row.as_of_generation < store.recompute_generation()`** — a weight from an
older pass than the current one is stale (the honest as-of marker consumers caveat with; M3's read API surfaces it, M4 uses
it for offline-unmounted reads). `ImportanceWriter`'s surface: `write_weights(generation, rows)` (one transaction: upsert
every row + advance the generation, so a reader never sees a bumped generation with un-written rows), `purge_volume`
(forget), `record_visit`.

## The scheduler (`scheduler/`, M2)

Recomputes a volume's folder weights when its index finishes scanning. Two triggers, unified through one coalescing core:

- **The lifecycle bus** ([`indexing/lifecycle_bus.rs`](../indexing/DETAILS.md) — the mechanism is documented there,
  single-source): the scheduler subscribes per volume; a `ScanCompleted` publish ⇒ recompute.
- **The startup registry sweep** (`indexing::ready_volumes_with_kind`): a volume already Fresh at launch never re-fires
  `ScanCompleted`, so a bus-only scheduler would miss the common restart case. The sweep enqueues those once, WITH each
  volume's typed kind (so MTP is excluded and SMB degrades correctly — see "Multi-volume" below).
- **The registration bus** (`indexing::lifecycle_bus::subscribe_registrations`): a volume that registers AFTER the sweep
  (a share mounted mid-session) is wired then. The scheduler subscribes to it BEFORE the sweep, so no volume registering
  in the gap is lost (plan M4 late-registering volumes).

`PassCoordinator` is the pure, unit-tested coalescing core: it guarantees ONE pass per `volume_id` at a time — a request
arriving mid-pass sets a single re-run flag rather than starting a second pass (so the sweep + a concurrent
`ScanCompleted` collapse to one pass, then at most one re-run). The recompute itself is full-volume: walk the index tree
through the read pool (`get_read_pool_for`), assemble a `FolderSignals` per folder (`signals::signals_for_dir`), run the
pure scorer, and write every row at a freshly-bumped generation. It runs on a blocking background task (SQLite +
scoring), never on the IPC thread; a `None` read pool (index not registered) is a no-op.

### The walk is O(dirs), not O(entries) (`walk_index_folders`)

The full-recompute walk materializes **directories only** (`IndexStore::all_directories`) and STREAMS file rows
(`IndexStore::for_each_file_child`) into a small per-parent accumulator — distinct-extension set, file count, and the
direct-marker flag — collapsed to a `ChildAggregate` per folder. So pass memory is O(dirs), a small fraction of a
multi-million-entry NAS index, not O(entries) (an `all_entries` walk went transiently into the hundreds of MB on exactly
the NAS-sized volumes SMB scoring now enables). Directory children still come from the directory set (a `.git`/`.hg`/`.svn`
marker is a directory), so `has_direct_marker` folds both the streamed file children and the sibling directory children.
`signals_for_dir` takes the `ChildAggregate`, not child rows. `has_marker_below` is one upward propagation after the walk
(a `.git` deep in a tree raises its ancestors, plan Decision 3).

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
- `top_n(n)` / `above_threshold(t)` — ranked folders (score DESC, ties by path). `above_threshold` is INCLUSIVE at the
  bound (a folder exactly at `t` is returned) — the agent's summary gate and media-ML's enrich-important-first, one
  query with an optional `LIMIT` / `WHERE score >= t`.
- `signals_for(path)` — the stored raw vector, for a consumer applying its own weighting profile (plan Decision 2).
- `explain(path, now)` — the per-signal breakdown, **recomputed from the STORED signals via the pure scorer**
  ([`explain`](scorer/mod.rs)), so there's ONE formula and the breakdown can't drift from the stored scalar.

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

**Rescoping + the ancestor cap.** For each changed path, the touched set is the folder itself plus its ancestor chain
(`touched_folder_set`), because a project marker or size/mtime change can raise parents. The ancestor walk is capped at
`ANCESTOR_WALK_CAP` (32) levels per changed path: a project marker appearing deep in a tree could otherwise raise every
ancestor to the root and rescope half the volume (plan open-question). The pass walks the index once, filters to the
touched subset, rescopes the scorer over just those, and writes them.

**Generation semantics.** An incremental pass writes its rows at the CURRENT generation and does NOT bump it
(`write_weights_incremental`, `advance_generation == false`), so every untouched folder keeps its as-of marker and the
volume doesn't turn wholesale-stale after a one-file change. Only a full pass advances the generation. A `"/"` sentinel in
the changed set (a full-refresh emit) escalates to a full pass rather than resolving `/` as one folder.

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
