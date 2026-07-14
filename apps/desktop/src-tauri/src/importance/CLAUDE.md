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
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome`. `evals/` — ranking-quality suite + corpus tooling for weight tuning.

## Must-knows

- **The scorer is PURE** (no `rusqlite`/`Volume`/filesystem/clock; "now" is a `u64` arg). `score` delegates to `explain`,
  so there's ONE formula; the breakdown sums to the score when unfloored — hold that when adding a signal.
- **Three FLOOR overrides cap the score at `0.0`, OUTSIDE the `SignalContribution` sum**: `name_denylisted`,
  `hidden_or_system`, and `under_floored_ancestor` (a denylisted/hidden/system ancestor floors its whole subtree; floor
  beats marker; see [DETAILS.md](DETAILS.md)). **Missing optional signals REDISTRIBUTE, never fabricate** — availability
  (`SignalSet`) is distinct from a `None` value.
- **Classification is typed, never a string/substring branch** (`no-string-matching`): `PathClass`, `SignalSet`,
  `SignalKind` are enums; the denylist is folded-name set-membership (reuses `search::SYSTEM_DIR_EXCLUDES`).
- **The default `Weights` are UNVALIDATED**; don't hardcode them, pass a `Weights`. Changing them can fail the `evals/`
  soft-score floor (the tuning instrument — see [DETAILS.md](DETAILS.md)).

### Storage + scheduler (depth in [DETAILS.md](DETAILS.md))

- **`store/` carries the index's disposable-cache discipline verbatim** (`platform_case`, delete-and-recreate on
  `SCHEMA_VERSION` mismatch, path-keyed rows). ONE shared long-lived `ImportanceWriter` per volume from the scheduler's
  `WriterRegistry`; `record_visit` + every recompute route through it. A full pass REPLACES the whole table + bumps the
  generation in ONE transaction; every row carries its as-of generation (the offline-read marker).
- **Floored folders get NO row; the read side derives them** (`ImportanceIndex::lookup` →
  `WeightLookup::{Scored,Floored,Unscored}` via `classify::floors_by_path`). Don't reintroduce a `0.0` row.
  **`FolderSignals` serde shape is load-bearing**: camelCase, `specta::Type`, every field
  `#[serde(default, skip_serializing_if)]` (serializes only non-defaults). See DETAILS storage model.
- **The full-recompute walk is O(dirs), NOT O(entries)**: materialize directories (`all_directories`), STREAM file rows
  (`for_each_file_child`) into a per-parent `ChildAggregate`. Don't reintroduce an `all_entries` walk (hundreds of MB on
  NAS-sized volumes).
- **Categorical signals live in `classify.rs`**, shared by `signals` (prod) AND `fixtures`/`evals` (tests) — don't
  re-derive denylist / path-class / marker / descendant-floor (`self_floors` + `under_floored_paths`).
- **Drive full recompute off the bus's `ScanCompleted` + the sweep (`ready_volumes_with_kind`), NEVER off phase events**
  (network volumes never emit `Aggregating`/`Reconciling`). Coalesce per volume.
- **Volume kind decides the policy TYPED, never by id string** (`ScoringPolicy::for_kind`): Local + SMB scored (SMB drops
  Spotlight ⇒ `last_used` redistributes), **MTP an explicit exclusion**; `record_visit` shares the gate.
- **NEVER a filesystem syscall against an SMB/MTP mount** — read only the local index DB. Spotlight sampling is
  mask-gated (never for SMB); local sampling is a dedicated OS thread, never rayon.

### Read API + incremental (depth in [DETAILS.md](DETAILS.md))

- **`ImportanceIndex` (`read/`) is the ONLY consumer entry point** (no raw `rusqlite` dep). `explain` re-scores the STORED
  `FolderSignals` via the pure scorer (one formula, no drift). It reads the DB directly, never the index registry, so
  weights stay queryable OFFLINE after a volume unmounts.
- **Incremental (`write_weights_incremental`) writes at the CURRENT generation, does NOT bump it, and NEVER escalates to
  a full pass.** It CLEARS each changed subtree then re-inserts only non-floored folders (a `node_modules` floor
  transition leaves no stale row); never route it through `write_weights` (bumps). `dir-changed`-driven, ancestor walk
  capped. Every live batch carries the bare root `/` (universal ancestor via `collect_ancestor_paths`);
  `sanitize_incremental_batch` drops it — ❌ don't reintroduce a `/`→full-pass escalation (it rewrote all folders every
  batch: continuous full recomputes, pegged core, index WAL-checkpoint stalls). Full passes are `ScanCompleted`-only;
  `spawn_incremental` throttles to ≤1 index walk per `INCREMENTAL_THROTTLE_WINDOW`.

Adding a signal, and the signal catalog: [DETAILS.md](DETAILS.md).
