# Importance scorer — details

The pure heart of the subsystem: values in, a `0.0..=1.0` score out. Read this before any non-trivial work here:
editing, planning, reorganizing, or advising. Subsystem-wide context (the floor doctrine, `classify.rs`, the volume-kind
policy) is `../DETAILS.md`.

- `score(inputs, available, weights, now_secs) -> Score` — the scalar.
- `explain(inputs, available, weights, now_secs) -> Explanation` — the same scalar plus the per-signal
  `SignalContribution` breakdown. `score` delegates here, so there is one formula, not two.

No `rusqlite`, no `Volume`, no filesystem, no clock: "now" is passed in as a `u64` so recency is deterministic in tests.
Full design: `docs/specs/importance-subsystem-plan.md`.

## Signal catalog

`FolderSignals` carries the raw signal vector.

- **name denylist** (`name_denylisted`): a set-membership check on the folded folder name against the shared
  `indexing::SYSTEM_DIR_EXCLUDES` list (`node_modules`, `.git`, caches, build output). A FLOOR override, not an additive
  term. Set-membership, never a substring match.
- **hidden / system** (`hidden_or_system`): also a FLOOR override. A dotfile or system-owned folder scores `0.0`. The
  soft, non-floor side ("being visible is mildly positive") is the separate additive `Visibility` term.
- **under a floored ancestor** (`under_floored_ancestor`): the third FLOOR override, `true` when a self-flooring
  ancestor sits above this folder. Derivation and the vendored-repo nuance: `../DETAILS.md` § The floor propagates to
  descendants.
- **extension diversity** (`distinct_extension_count` + `file_count`): mixed folders score above monocultures.
  Normalized as `distinct / min(file_count, 5)`, so three files of three kinds already reads as diverse while 200 `.log`
  files (one extension) reads as a monoculture. Zero files is neutral (`0.0`).
- **mtime recency** (`mtime_secs`): exponential half-life decay (`0.5 ^ (age / half_life)`), default half-life 30 days.
  `None` is neutral; a future timestamp (clock skew) clamps to `1.0`.
- **project markers** (`has_project_marker`): `1.0` when a `.git`/`Cargo.toml`/`package.json`/… sits in the folder or a
  descendant, raising the whole subtree.
- **path-class prior** (`path_class`): a typed `PathClass` — `ProjectRoot` (1.0) > `UserContent` (0.8) > `Neutral` (0.4)
  > `SystemOrCache` (0.0). The caller classifies the path once; the scorer reads the variant (no path-substring branch
  > in the scorer).
- **visit activity** (`visit_count`, optional): linear up to a saturation count (default 10), then flat.
- **Spotlight last-used** (`last_used_secs`, optional): recency decay, default half-life 14 days. `None` on SMB/MTP (no
  Spotlight) and `None` before sampling has run.

`extension_count(file_names)` is the convenience the callers assembling a `FolderSignals` from a listing use: it folds
each extension to lowercase and counts the distinct set, with no-extension files in a single bucket.

## Missing-signal redistribution

`SignalSet` marks which optional signals are AVAILABLE for a volume, independent of their value. When a signal is
unavailable (SMB has no Spotlight), its coefficient is removed and the remaining coefficients are scaled up so they sum
to the same total: the folder is never penalized for a signal its backend can't produce. Availability is distinct from a
`None` value — a local folder whose `kMDItemLastUsedDate` sampling simply hasn't run yet is _available but unsampled_
(contributes `0.0`, drags the reachable max down), whereas an SMB folder is _unavailable_ (its weight redistributes).
`redistribution_preserves_total_weight` pins the conservation; `missing_optional_signal_redistributes_not_penalizes`
pins the SMB-vs-local direction.

The five listing signals are always available; only the two backend-dependent optional signals ever redistribute. The
degenerate all-unavailable case can't occur (listing signals are always present), but `effective_weights` guards the
divide-by-zero anyway.

## The explain invariant

For an unfloored folder, the `SignalContribution` list sums (then clamps) to exactly the `Score`, and each
`contribution == weight * raw`. When a FLOOR override fires, `Explanation::floored` is `true` and the additive terms are
reported at the values they _would_ have contributed (so a tuner still sees the signal shape) while the score is `0.0`.
Pinned by `explain_contributions_sum_to_score_unfloored` and the proptest.

## Tunable weights (`weights.rs`)

The formula is unproven: the defaults are a STARTING POINT to tune against real trees, not validated values. So the
coefficients are data (`Weights`, serde-serializable, defaulted), not hardcoded constants — the dev tuning surface
overrides them (`../read/DETAILS.md` § Dev tuning surface), and a future per-consumer profile can ship its own set. The
seven additive weights sum to `1.0` at their defaults, so a folder that maxes every signal (and hits no floor) reaches
`1.0`; the scorer does not require that at runtime, and the redistribution and explain invariants hold for any values.

The largest default weights sit on the signals that most cleanly separate "matters" from "machine output": path class
(0.25) and project markers (0.20). Half-lives and the visit-saturation count are shape parameters, not additive weights.

Whether a weight change is an improvement is a measured question, not a vibe: `../evals/DETAILS.md`.

## Adding a signal (step-by-step)

Add the field to `FolderSignals` (+ `neutral()`), a `SignalKind` variant (+ `ALL`), a `Weights` coefficient (+
`additive_weight` and `EffectiveWeights::weight_of` arms), and a `raw_signal_value` arm; if the signal is optional
(backend-dependent), add a `SignalSet` flag + a `signal_available` arm so it redistributes when absent. Then cover its
contribution DIRECTION with a test and keep the explain-sums invariant green. A categorical signal also needs a
`classify.rs` classifier (shared by production, fixtures, and evals) and an assembly line in `signals.rs`.

## Testing

`tests.rs` is entirely pure: no FFI, no DB, a fixed `NOW`. It asserts each signal's contribution DIRECTION, the
explain-sums-to-score invariant, missing-signal redistribution, the `FolderSignals` serde round-trip (load-bearing for
the store's trimmed JSON), the fixture-tree shape, and a proptest that the score is always finite and in `[0,1]`.
