# Importance subsystem

Deterministic, cheap folder-importance scoring consumed by expensive features (the agent, media-ML enrichment, future
cleanup/prefetch). A pure read-consumer of `indexing/`, sibling to `search/`. Full design + milestones:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

**Scope now (through M3): the pure scorer, per-volume storage, the scheduler (full recompute on scan completion +
incremental on live changes), the visit signal, and the `ImportanceIndex` read API.** SMB / offline reads are M4.

## Module map

- `scorer/` — the PURE formula: `score` + `explain`. `types.rs` (`FolderSignals`, `SignalSet`, `PathClass`), `weights.rs`.
- `store/` — per-volume `importance.db`: `ImportanceStore`, path-keyed `weights` + `visits` + `meta`. `writer.rs`
  (`ImportanceWriter`, ONE thread per DB), `writer_registry.rs` (lazy per-volume long-lived writers).
- `read.rs` — `ImportanceIndex`, the consumer read API + recompute subscription.
- `scheduler/` — bus-driven full + incremental recompute, coalesced per `volume_id`. `signals.rs`, `classify.rs`
  (shared classifiers), `last_used.rs` (Spotlight sampling), `commands.rs` (`record_visit`).
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome`. Dev tuning: `crates/index-query`'s `importance-tune` bin.

## Must-knows

- **The scorer is PURE: no `rusqlite`, no `Volume`, no filesystem, no clock.** "Now" is a `u64` arg (unit-testable
  without a running app). `score` delegates to `explain`, so there's ONE formula.
- **`FolderSignals` is the persisted raw signal vector; its serde shape is load-bearing** — renaming / reordering a field
  changes what the store reads back. Keep it `serde` + `specta::Type`, camelCase, roundtrip test green.
- **Denylist and hidden/system are FLOOR overrides, not additive terms** — they cap the score at `0.0`, OUTSIDE the
  `SignalContribution` sum. The additive `Visibility` term is the separate soft signal.
- **Missing optional signals REDISTRIBUTE, never fabricate**: an unavailable signal's weight scales onto the available
  ones (SMB has no Spotlight ⇒ `last_used` spreads). Availability (`SignalSet`) is distinct from a `None` value.
- **Classification is typed, never a string/substring branch** (`no-string-matching`): `PathClass`, `SignalSet`,
  `SignalKind` are enums; the denylist is set-membership on the folded name (reuses `search::SYSTEM_DIR_EXCLUDES`).
- **The default `Weights` are UNVALIDATED** (tune with the `importance-tune` bin). Don't hardcode them; pass a `Weights`.
- **The `explain` breakdown sums to the score when unfloored** (each `contribution == weight * raw`). Hold the invariant
  when adding a signal.

### Storage + scheduler (depth in [DETAILS.md](DETAILS.md))

- **`store/` carries the index's disposable-cache discipline verbatim**: `platform_case` on EVERY connection,
  delete-and-recreate on `SCHEMA_VERSION` mismatch, path-keyed rows.
- **ONE shared long-lived `ImportanceWriter` per volume, from the scheduler's `WriterRegistry`** (Tauri managed state).
  Both `record_visit` and every recompute route through it — don't spawn a per-call writer (breaks one-writer-per-DB).
- **Full pass: write all rows AND bump the generation in ONE transaction** (`write_weights`); split them and a reader
  sees a bumped generation with un-written rows. Staleness: `row.as_of_generation < recompute_generation()`.
- **Categorical signals live in `classify.rs`, shared by `signals` (prod) AND `fixtures` (tests)** — don't re-derive
  denylist / path-class / project-marker; they MUST agree.
- **Drive full recompute off the bus's `ScanCompleted` + the startup sweep (`ready_volume_ids`), NEVER off phase events**
  (network volumes never emit `Aggregating`/`Reconciling`). Coalesce per `volume_id`.
- **`record_visit` is fire-and-forget + failure-silent** (never blocks navigation). `kMDItemLastUsedDate` sampling:
  dedicated OS thread + autoreleasepool, capped — never rayon, never inline on IPC.

### Read API + incremental (depth in [DETAILS.md](DETAILS.md))

- **`ImportanceIndex` (`read.rs`) is the ONLY consumer entry point** — none takes a raw `rusqlite` dep on `importance.db`.
  `explain` re-scores the STORED `FolderSignals` via the pure scorer (one formula, so the breakdown can't drift).
- **Incremental writes at the CURRENT generation and does NOT bump it** (`write_weights_incremental`), so untouched
  folders' as-of markers stay intact. Never route incremental through `write_weights` (that bumps).
- **Incremental is driven by the `dir-changed` bus** (`lifecycle_bus::publish_dirs_changed`, from the live event-loop +
  verifier), not per-directory index hooks (none exist); the ancestor walk is capped (`ANCESTOR_WALK_CAP`).

## Adding a signal

The step-by-step (which types + arms to touch) and the signal catalog live in [DETAILS.md](DETAILS.md); keep the
explain-sums invariant.
