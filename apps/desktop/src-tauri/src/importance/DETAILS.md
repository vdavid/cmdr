# Importance subsystem — details

The deterministic, cheap folder-importance score that any expensive feature consumes (the in-app agent, the media-ML
enrichment scheduler, future disk-cleanup / prefetch). Full design and milestone plan:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

M1 ships only the pure heart: the [`scorer`](scorer/mod.rs) and its tunable [`Weights`](scorer/weights.rs). Storage
(`importance.db`), the lifecycle bus, the scheduler, and the read API land in M2–M4 (see the plan).

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

## What M1 deliberately leaves out

- No storage, no `importance.db` (M2).
- No lifecycle bus, no scheduler, no recompute (M2).
- No read API, no `explain` over persisted data, no subscriptions (M3).
- No `record_visit`, no `kMDItemLastUsedDate` sampling — the two optional signals stay `None` (M2).
- No IPC surface, no user-facing strings, no i18n.

## Testing

All M1 tests are pure (`scorer/tests.rs`): no FFI, no DB, a fixed `NOW`. They assert each signal's contribution
DIRECTION (the plan's M1 list), the explain-sums-to-score invariant, missing-signal redistribution, the serde round-trip
(load-bearing for M2), the fixture-tree shape, and a proptest that the score is always finite and in `[0,1]`.
