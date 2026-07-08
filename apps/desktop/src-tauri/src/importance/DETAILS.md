# Importance subsystem — details

The deterministic, cheap folder-importance score that any expensive feature consumes (the in-app agent, the media-ML
enrichment scheduler, future disk-cleanup / prefetch). Full design and milestone plan:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

M1 shipped the pure heart: the [`scorer`](scorer/mod.rs) and its tunable [`Weights`](scorer/weights.rs). **M2 adds
storage (`importance.db`), the scheduler that fills it on scan completion, and the navigation-visit signal** (see
"Storage", "The scheduler", and "The visit signal" below). The consumable read API and incremental recompute land in M3;
SMB / offline-unmounted reads in M4 (see the plan).

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
- **The startup registry sweep** (`indexing::ready_volume_ids`): a volume already Fresh at launch never re-fires
  `ScanCompleted`, so a bus-only scheduler would miss the common restart case. The sweep enqueues those once.

`PassCoordinator` is the pure, unit-tested coalescing core: it guarantees ONE pass per `volume_id` at a time — a request
arriving mid-pass sets a single re-run flag rather than starting a second pass (so the sweep + a concurrent
`ScanCompleted` collapse to one pass, then at most one re-run). The recompute itself (`recompute_from_pool`) is
full-volume: walk the index tree through the read pool (`get_read_pool_for`), assemble a `FolderSignals` per folder
(`signals::signals_for_dir`), run the pure scorer, and write every row at a freshly-bumped generation. It runs on a
blocking background task (SQLite + scoring), never on the IPC thread; a `None` read pool (index not registered) is a
no-op. **Local (`root`) only in M2** — SMB scoring is M4.

**Signal assembly agrees with the fixtures by construction.** The categorical signals (denylist, path class, project
marker, hidden) come from the shared [`classify`](classify.rs) module that BOTH `signals::signals_for_dir` (production)
and `fixtures::signals_for` (tests) call — so the M1 formula's test stand-in and the M2 real assembler can't drift on what
a signal means (the fixtures doc's standing warning, now enforced by shared code).

### `kMDItemLastUsedDate` sampling (`last_used.rs`, macOS-local)

The one potentially-slow input. We SAMPLE, not sweep: cap at `SAMPLE_CAP` folders per pass, query `MDItemCopyAttribute`
on a DEDICATED 8 MB-stack OS thread wrapped in `objc2::rc::autoreleasepool` (never rayon — a synchronous macOS-framework
round-trip; `src-tauri/CLAUDE.md`). An un-sampled local folder is *available but unsampled* (contributes 0, drags the
reachable max down), distinct from an SMB folder where the signal is *unavailable* and its weight redistributes — the
`SignalSet` the scheduler passes encodes which. Off macOS the sample is empty and `last_used` is unavailable.

## The visit signal (`commands.rs` + `store` visits table, M2, plan Decision 3)

A typed `record_visit(Location)` IPC command the frontend's navigation-commit point calls fire-and-forget (the
`persistLastUsedPath` hook in `pane/persistence-subscriber.svelte.ts`, alongside the existing last-used-path save). It
persists a compact per-volume `visits` row: **counts and timestamps only, no content, local-only** (the privacy-sane shape
— noted in `docs/security.md`). The scorer's visit-activity signal reads it on the next recompute. Fire-and-forget and
failure-silent by contract: a visit that can't be recorded must never block or break navigation, so the command returns
`Ok(())` even on a write hiccup. Local (`root`) only in M2. The agent spec's planned `user_action_log` is this signal's
future superset — when it lands, `record_visit` folds into it (never two parallel recorders).

## What M2 still leaves out

- No consumable read API (`weight_for`/`top_n`/`above_threshold`/`explain`/`signals_for`), no recompute subscription (M3).
- No incremental (changed-subtree) recompute — full-volume on scan completion only (M3).
- No dev tuning surface (M3).
- No SMB scoring, no offline-unmounted reads (M4).
- No IPC surface beyond `record_visit`; no user-facing strings, no i18n (`record_visit` is invisible).

## Testing

All M1 tests are pure (`scorer/tests.rs`): no FFI, no DB, a fixed `NOW`. They assert each signal's contribution
DIRECTION (the plan's M1 list), the explain-sums-to-score invariant, missing-signal redistribution, the serde round-trip
(load-bearing for M2), the fixture-tree shape, and a proptest that the score is always finite and in `[0,1]`.
