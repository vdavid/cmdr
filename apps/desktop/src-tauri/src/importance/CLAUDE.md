# Importance subsystem

Deterministic, cheap folder-importance scoring for expensive features (agent, media-ML, future cleanup/prefetch). A pure
read-consumer of `indexing/`, sibling to `search/`. Design + depth for every must-know below: `DETAILS.md` and
`docs/specs/later/importance-subsystem-plan.md`.

## Module map

- `scorer/` — the PURE formula (`score` + `explain`), `types.rs`, `weights.rs`.
- `store/` — per-volume `importance.db`, its single writer, the writer registry.
- `read.rs` — `ImportanceIndex`, the consumer read API + recompute subscription.
- `scheduler/` — bus-driven full + incremental recompute (coalesced per volume); `signals.rs`, `classify.rs`,
  `last_used.rs`, `commands.rs`.
- `fixtures.rs` (`cfg(test)`) — `SyntheticHome`. `evals/` — ranking-quality suite + weight-tuning corpus.

## Must-knows

Scorer:

- **PURE** (no `rusqlite`/`Volume`/fs/clock; "now" is a `u64` arg). `score` delegates to `explain` ⇒ ONE formula; the
  breakdown sums to the score when unfloored.
- **Three FLOOR overrides cap the score at `0.0` OUTSIDE the signal sum**: `name_denylisted`, `hidden_or_system`,
  `under_floored_ancestor` (floors the whole subtree; floor beats marker). Missing optional signals REDISTRIBUTE, never
  fabricate.
- **Classification is typed, never a string branch** (`no-string-matching`): `PathClass`/`SignalSet`/`SignalKind` enums;
  denylist reuses `search::SYSTEM_DIR_EXCLUDES`.
- **Default `Weights` are UNVALIDATED** — pass a `Weights`, don't hardcode; changes can fail the `evals/` soft floor.

Storage + scheduler:

- **Disposable cache** (delete-and-recreate on `SCHEMA_VERSION` mismatch). Rows key on a **BINARY `path_folded` PK**, ❌
  never a `platform_case`-collated `path` PK (full-scans the incremental subtree-clear and pegs a core). ONE long-lived
  `ImportanceWriter` per volume (scheduler's `WriterRegistry`); visits + recomputes route through it. A full pass
  REPLACES the whole table + bumps the generation in ONE transaction; each row carries its as-of generation.
- **Floored folders get NO row** — the read side derives `Floored`; don't reintroduce a `0.0` row. **`FolderSignals`
  serde shape is load-bearing** (camelCase, `specta::Type`, per-field `skip_serializing_if`).
- **Full walk is O(dirs), not O(entries)**: materialize dirs, STREAM file rows into a per-parent `ChildAggregate`; no
  `all_entries` walk (hundreds of MB on NAS).
- **Categorical signals live in `classify.rs`**, shared by prod + fixtures/evals — don't re-derive.
- **Drive full recompute off the bus `ScanCompleted` + sweep, NEVER phase events** (network never emits them). Coalesce
  per volume. A volume Fresh at launch never re-fires `ScanCompleted`, so the sweep ALSO runs
  `enqueue_initial_full_pass_if_unscored`: a store with no generation (fresh / schema-recreated / incremental-only) gets
  one full pass. The "unscored?" check binds to `store::needs_initial_full_pass`, which forces the WRITE-path open
  (triggering the lazy schema recreate) BEFORE reading the generation — ❌ never a sweep-time read probe (it reads the OLD
  schema's stamped generation, skips, then the recreate wipes it: the stuck-at-generation-0 prod-upgrade trap).
- **Volume kind ⇒ policy TYPED** (`ScoringPolicy::for_kind`): Local + SMB scored, **MTP excluded**. ❌ NEVER a filesystem
  syscall against an SMB/MTP mount — read the local DB only.

Read API + incremental:

- **`ImportanceIndex` (`read/`) is the ONLY consumer entry** (no raw `rusqlite`); reads the DB directly so weights stay
  queryable OFFLINE after unmount.
- **Incremental writes at the CURRENT generation, does NOT bump it, and NEVER escalates to a full pass.** Clears each
  changed subtree then re-inserts only non-floored folders. Every live batch carries the bare root `/`
  (`collect_ancestor_paths`); `sanitize_incremental_batch` drops it — ❌ don't reintroduce a `/`→full-pass escalation (it
  pegged a core with continuous recomputes). Throttled to ≤1 walk per `INCREMENTAL_THROTTLE_WINDOW`.

Adding a signal, the signal catalog, and every "why": `DETAILS.md`.
