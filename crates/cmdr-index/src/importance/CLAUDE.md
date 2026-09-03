# Importance subsystem

Deterministic, cheap folder-importance scoring for expensive features (the in-app agent, media-ML enrichment, future
cleanup/prefetch). A pure read-consumer of `indexing/`, sibling to `search/`.

## The public surface

3 public modules, 23 public items, plus two gated doors. `importance::tooling` (the `tooling` feature) is the ONLY way
the `index-query` binaries reach the evaluation corpus and the measurement entry points; `importance::testing` is the
only way an app-side test stages a scored folder. ❌ Don't widen a module to let a caller in — take one of the four
dispositions in `../indexing/handle/DETAILS.md` § "The other two subsystems".

## Areas (routing map)

Each area subdir has its own `CLAUDE.md` (must-knows) + `DETAILS.md` (depth).

- **`scorer/CLAUDE.md`** — the pure formula: `score` / `explain`, `FolderSignals`, the tunable `Weights`.
  **`store/CLAUDE.md`** — per-volume `importance.db`: the schema, the folded PK, what earns a row.
- **`scheduler/CLAUDE.md`** — bus-driven full and incremental recompute, the O(dirs) full walk and the O(touched) scoped
  one, the kind policy. **`read/CLAUDE.md`** — `ImportanceIndex`, the ONLY consumer entry, plus the recompute
  subscription.
- **`evals/CLAUDE.md`** — the ranking-quality suite and the anonymized real-index corpus.

Top-level leaves this file owns: `classify.rs` (the shared categorical classifiers), `signals.rs` (index rows ⇒
`FolderSignals`), `last_used.rs` (sampled Spotlight `kMDItemLastUsedDate`), `writer.rs`

- `writer_registry.rs` (ONE writer thread per volume), and `fixtures.rs` (`cfg(test)`, `SyntheticHome`).

## Subsystem-wide must-knows

- **The scorer is PURE** — no `rusqlite`, no `Volume`, no filesystem, no clock ("now" is a `u64` argument). ❌ Don't
  hand it a connection or let it read the wall clock; every caller passes values in.
- **Three FLOOR overrides cap a folder at `0.0` OUTSIDE the additive sum**: `name_denylisted`, `hidden_or_system`, and
  `under_floored_ancestor`, which floors the whole subtree. **Floor beats marker**: a repo vendored inside a
  `node_modules` stays floored. **A floored folder gets NO row** — every read derives `Floored` from the path
  (`classify::floors_by_path`); ❌ don't reintroduce a `0.0` row.
- **Categorical signals come from `classify.rs`**, shared by production, fixtures, and evals — ❌ never re-derive them.
  Classification is typed (`PathClass` / `SignalKind`), never a string branch, and the denylist reuses
  `indexing::SYSTEM_DIR_EXCLUDES`. Marker promotion lives in `path_class_with_marker` alone; it declines at `$HOME`, a
  volume root, and a `SystemOrCache` path. ❌ Never floor `$HOME`: that propagates home-wide and disables the feature.
- **A classifier change is INERT until `store::SCORING_POLICY_KEY` re-arms stores** (a full pass runs once, an
  incremental only touches changed folders). It hashes the lists plus `SCORING_RULES_VERSION`; bump the latter by hand
  for a rule no list can see.
- **`importance-{volume_id}.db` is a disposable cache**: a `SCHEMA_VERSION` mismatch delete-and-recreates it, no
  migrations. ONE long-lived `ImportanceWriter` per volume through `writer_registry`; visits AND recomputes both route
  through it. ❌ Never a second writer thread on one DB.
- **Volume kind ⇒ policy, TYPED** (`scheduler::ScoringPolicy::for_kind`): Local and SMB scored, **MTP excluded** at
  every entry point. ❌ NEVER a filesystem syscall against an SMB or MTP mount — read the local index DB only.
- **Nothing here is cancelable, so don't assume a pass stops.** No `CancellationToken`, no stop hook, so
  `stop_all_indexing` (memory watchdog, shutdown) doesn't reach a running recompute; it walks the whole index to the
  end. Known gap with a `TODO(importance)` in `scheduler/recompute.rs`; ❌ don't add a second primitive to fix it.
- **Only a FULL pass stamps `recompute_generation`**, so generation `0` does NOT mean "no weights" (an incremental-only
  store holds hundreds of thousands of rows at generation 0). A consumer asking "genuinely unscored?" keys on the row
  count.

Why it's a separate subsystem, the writer and its registry, the WAL checkpoint, `record_visit`, Spotlight sampling, the
floor-propagation rule, and the fixtures: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
