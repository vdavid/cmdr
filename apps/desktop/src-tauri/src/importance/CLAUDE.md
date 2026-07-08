# Importance subsystem

Deterministic, cheap folder-importance scoring consumed by expensive features (the agent, media-ML enrichment, future
cleanup/prefetch). A pure read-consumer of `indexing/`, sibling to `search/`. Full design + milestones:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

**M1 scope (what exists now): the pure scorer only.** Storage, lifecycle bus, scheduler, and read API are M2–M4 and not
built yet. Don't wire consumers to importance yet — there's no persisted store or read API to reach.

## Module map

- `scorer/` — the PURE formula: `score(inputs, available, weights, now_secs) -> Score` and `explain(...) -> Explanation`.
  `types.rs` (`FolderSignals`, `SignalSet`, `PathClass`, `Explanation`), `weights.rs` (tunable `Weights`).
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome` builder over `InMemoryVolume` + per-folder signal derivation.

## Must-knows

- **The scorer is PURE: no `rusqlite`, no `Volume`, no filesystem, no clock.** "Now" is passed in as a `u64`. Keep it
  that way — it's the whole reason the formula is unit-testable and tunable without a running app (plan Decision 3).
  `score` delegates to `explain`, so there's ONE formula; don't fork a second scalar path.
- **`FolderSignals` is the persisted raw signal vector (M2, plan Decision 2). Its serde shape is load-bearing.** Adding /
  renaming / reordering a field changes what M2 stores and reads back. Keep it `serde` + `specta::Type`, camelCase, and
  keep `folder_signals_serde_roundtrips` green.
- **Denylist and hidden/system are FLOOR overrides, not additive terms.** They cap the score at `0.0` regardless of the
  weighted sum, and live OUTSIDE the `SignalContribution` sum. The additive `Visibility` term is the separate soft signal.
- **Missing optional signals REDISTRIBUTE, never fabricate.** An unavailable signal's weight scales onto the available
  ones (SMB has no Spotlight ⇒ `last_used` weight spreads); availability (`SignalSet`) is distinct from a `None` value.
  Don't fill a missing signal with a default value — that's fabrication the plan forbids.
- **Classification is typed, never a string/substring branch** (`no-string-matching`): `PathClass`, `SignalSet`,
  `SignalKind` are enums. The name denylist is set-membership on the folded name (reuses `search::SYSTEM_DIR_EXCLUDES`),
  not a substring match.
- **The default `Weights` are an UNVALIDATED starting point** (agent-spec §18.3). They'll be tuned against real trees in
  M3. Don't treat the current numbers as correct, and don't hardcode them elsewhere — pass a `Weights` through.
- **The `explain` breakdown must sum to the score when unfloored** (each `contribution == weight * raw`). Pinned by tests
  + a proptest; don't break the invariant when adding a signal.

## Adding a signal

Add the field to `FolderSignals` (+ `neutral()`), a `SignalKind` variant (+ `ALL`), a `Weights` coefficient (+
`additive_weight`), and a `raw_signal_value` arm; if it's optional, add a `SignalSet` flag and a `signal_available` arm.
Cover its contribution direction with a test and keep the explain-sums invariant. Depth, rationale, and the signal
catalog: [DETAILS.md](DETAILS.md).
