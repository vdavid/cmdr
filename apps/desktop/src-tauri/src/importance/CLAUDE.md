# Importance subsystem

Deterministic, cheap folder-importance scoring for expensive features (the agent, media-ML enrichment, future
cleanup/prefetch). A pure read-consumer of `indexing/`, sibling to `search/`. Full design:
[`docs/specs/importance-subsystem-plan.md`](../../../../../docs/specs/importance-subsystem-plan.md).

**Scope (v1): the pure scorer, per-volume storage, the multi-volume kind-aware scheduler (full + incremental recompute),
the visit signal, and the `ImportanceIndex` read API — including SMB scoring and offline-unmounted reads.**

## Module map

- `scorer/` — the PURE formula (`score` + `explain`), `types.rs`, `weights.rs`.
- `store/` — per-volume `importance.db`, its single writer, and the writer registry.
- `read.rs` — `ImportanceIndex`, the consumer read API + recompute subscription.
- `scheduler/` — bus-driven full + incremental recompute (coalesced per volume); `signals.rs`, `classify.rs`,
  `last_used.rs`, `commands.rs`.
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome`. Dev tuning: `index-query`'s `importance-tune` bin.

## Must-knows

- **The scorer is PURE** (no `rusqlite`/`Volume`/filesystem/clock; "now" is a `u64` arg). `score` delegates to `explain`,
  so there's ONE formula; the breakdown sums to the score when unfloored — hold that when adding a signal.
- **`FolderSignals` is the persisted raw signal vector; its serde shape is load-bearing** — keep it `serde` +
  `specta::Type`, camelCase, roundtrip test green (a field rename changes what the store reads).
- **Denylist and hidden/system are FLOOR overrides** (cap at `0.0`, OUTSIDE the `SignalContribution` sum), not the
  additive `Visibility` term. **Missing optional signals REDISTRIBUTE, never fabricate** — availability (`SignalSet`) is
  distinct from a `None` value.
- **Classification is typed, never a string/substring branch** (`no-string-matching`): `PathClass`, `SignalSet`,
  `SignalKind` are enums; the denylist is set-membership on the folded name (reuses `search::SYSTEM_DIR_EXCLUDES`).
- **The default `Weights` are UNVALIDATED** (tune with the `importance-tune` bin). Don't hardcode them; pass a `Weights`.

### Storage + scheduler (depth in [DETAILS.md](DETAILS.md))

- **`store/` carries the index's disposable-cache discipline verbatim** (`platform_case`, delete-and-recreate on
  `SCHEMA_VERSION` mismatch, path-keyed rows). ONE shared long-lived `ImportanceWriter` per volume from the scheduler's
  `WriterRegistry`; `record_visit` + every recompute route through it (don't spawn a per-call writer). A full pass writes
  all rows AND bumps the generation in ONE transaction; staleness is `as_of_generation < recompute_generation()`.
- **The full-recompute walk is O(dirs), NOT O(entries)**: materialize directories (`all_directories`), STREAM file rows
  (`for_each_file_child`) into a per-parent `ChildAggregate`. Don't reintroduce an `all_entries` walk (hundreds of MB
  transient on NAS-sized volumes).
- **Categorical signals live in `classify.rs`, shared by `signals` (prod) AND `fixtures` (tests)** — they MUST agree; don't
  re-derive denylist / path-class / marker.
- **Drive full recompute off the bus's `ScanCompleted` + the sweep (`ready_volumes_with_kind`), NEVER off phase events**
  (network volumes never emit `Aggregating`/`Reconciling`). Coalesce per volume.
- **Volume kind decides the policy TYPED, never by id string** (`ScoringPolicy::for_kind`): Local + SMB scored (SMB drops
  Spotlight ⇒ `last_used` redistributes), **MTP an explicit exclusion** — `record_visit` shares the gate; late volumes
  wire via the registration bus.
- **NEVER a filesystem syscall against an SMB/MTP mount** — read only the local index DB. Spotlight sampling is gated on
  the mask (never runs for SMB); local sampling is a dedicated OS thread + autoreleasepool, never rayon.

### Read API + incremental (depth in [DETAILS.md](DETAILS.md))

- **`ImportanceIndex` (`read.rs`) is the ONLY consumer entry point** (no raw `rusqlite` dep). `explain` re-scores the
  STORED `FolderSignals` via the pure scorer (one formula, no drift). It reads the DB directly, never the index registry —
  so weights stay queryable OFFLINE after a volume unmounts, each carrying its as-of generation.
- **Incremental writes at the CURRENT generation and does NOT bump it** (`write_weights_incremental`) — untouched folders
  keep their as-of markers; never route it through `write_weights` (that bumps). Driven by the `dir-changed` bus; ancestor
  walk capped (`ANCESTOR_WALK_CAP`). A burst can drop a batch (last-value-wins `watch`) — the next full pass heals it.

Adding a signal, and the signal catalog: [DETAILS.md](DETAILS.md).
