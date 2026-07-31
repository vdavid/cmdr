# Importance ranking-quality evals

The weight-tuning instrument: `scenario.rs` (the serde `Scenario` format), `scenarios.rs` (committed synthetic homes),
`constraints.rs` (the `Constraint` vocabulary and `Ranking`), `corpus.rs` (anonymized real-index dumps), `mod.rs`
(`rank_scenario` / `score_scenario` / `aggregate_score`). The David-facing how-to is `docs/guides/importance-evals.md`.

## Must-knows

- **`SOFT_SCORE_FLOOR` is a FIXED floor, not a self-updating ratchet.** A weight change that drops the aggregate soft
  score below it fails the suite; when tuning genuinely improves quality, raise the constant CONSCIOUSLY in the same
  commit. ❌ Never lower it to make a change pass.
- **Hard constraints are ordinary `#[test]`s (a violation fails CI); soft constraints are counted into a scalar.** Both
  tiers speak the SAME `Constraint` vocabulary — the only difference is how a caller treats a violation.
- **`score_scenario(scenario, weights) -> f64` must stay PURE and fast**: no I/O, no clock (the scenario carries its own
  `now_secs`), no globals. It's the fitness function a grid search or hill-climb would call in a loop.
- **A `Scenario` is folders + their derived `FolderSignals` + expectations, NOT a synthetic tree.** That's all the pure
  scorer needs and exactly what a real-index dump can export, so synthetic and corpus scenarios load into the SAME type
  and score through the same path. ❌ Don't add a tree-walking scenario kind.
- **The corpus tool derives signals through PRODUCTION code** (`scheduler::walk_index_folders` +
  `signals::signals_for_dir`), so a dump scores identically to the live volume. ❌ Don't re-derive signals here.
- **Anonymization is the privacy crux.** A folder name survives verbatim ONLY when a classifier reads it (denylist hits,
  dot-prefixed names, home-child path-class anchors, project markers); everything else becomes a stable `dir-<hash>`.
  Real dumps land in a GITIGNORED corpus dir and are NEVER committed, and the suite must stay green with ZERO corpus
  files present (CI has none). `corpus/tests.rs` pins the privacy rules.

The constraint catalog, the scenario format, the snapshot/label/tune loop, and the `importance-snapshot` bin:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
