# Importance scorer

The pure formula: `mod.rs` (`score`, `explain`, the per-signal math), `types.rs` (`FolderSignals`, `SignalKind`,
`SignalSet`, `Explanation`, `Score`), `weights.rs` (the tunable coefficients). The purity contract and the three FLOOR
overrides are in `../CLAUDE.md`.

## Must-knows

- **`score` delegates to `explain`** ⇒ ONE formula, never two. ❌ Don't add a second scoring path.
- **Keep the explain invariant green**: unfloored, the contributions sum (then clamp) to exactly the `Score`; floored,
  `Explanation::floored` is `true` and the terms still report what they WOULD have contributed. ❌ Don't zero them.
- **A missing optional signal REDISTRIBUTES, never fabricates.** `SignalSet` says which optional signals a volume's
  backend CAN produce; an unavailable one's coefficient is dropped and the rest scale up to the same total. ❌ Don't
  substitute a default value for an unavailable signal, and ❌ don't conflate _unavailable_ with an unsampled `None`
  (which contributes `0.0` and drags the reachable max down).
- **`SignalKind::ALL` drives every loop.** A new signal needs the `ALL` array plus arms in `raw_signal_value`,
  `Weights::additive_weight`, and `EffectiveWeights::weight_of` — miss one and the signal silently weighs nothing. Full
  recipe: `DETAILS.md` § Adding a signal.
- **Default `Weights` are UNVALIDATED**, a starting point rather than tuned values. Pass a `Weights` through; ❌ don't
  hardcode a coefficient at a call site. A change here can fail `evals/`'s `SOFT_SCORE_FLOOR`, so run that suite.
- **`FolderSignals`'s serde shape is load-bearing**: `camelCase` + `specta::Type` + per-field `skip_serializing_if` (the
  trimmed JSON the store persists) + `#[serde(default)]` (any subset deserializes, so a row written before a field
  existed still parses). ❌ Don't drop either attribute.

The signal catalog, redistribution's conservation argument, the weights rationale, and the add-a-signal recipe:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
