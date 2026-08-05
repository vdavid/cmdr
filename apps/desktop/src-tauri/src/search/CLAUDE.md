# Search module

In-memory filename search + AI query translation, **one volume per search**. A scope routes to the volume that owns
it; unscoped means the boot volume.

## Module map

- `execute.rs` routes and runs (index-only, live, MCP); `live.rs` + `live/` the runs in flight and the walk pump.
- `engine.rs` scans the arena (`index.rs`, held per volume by `volumes.rs`); `matcher.rs`, `excludes.rs`, `ranking.rs`
  judge and order a row.
- `types.rs`/`query.rs` the data, `history.rs` recent searches, `ai/` NL translation (`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB); `types.rs` stays free of logic; `search/` consumes
  `indexing/` ONE WAY — a live search may ASK for a walk, ❌ but no matcher, query, or search type goes inside
  `cmdr-index`.
- **One matcher and one exclusion set, two evaluators each**: the arena scan (ancestor IDS) and a live walk (the
  entry's own PATH). ❌ Never re-derive case folding or NFD normalization elsewhere — that fork is how an unindexed
  drive answers differently. ❌ In neither: directory sizes, the include-root filter. The broad-query guard is per
  evaluator: a walk refuses and takes the whole RUN with it, ❌ never answering from the index alone.
- **A live search asks coverage BEFORE loading the arena, and reloads when a walk wrote behind it** (Decision 12):
  "covered" promises the arena holds those rows, and breaking it makes the NEXT query return fewer, silently. Both
  guards matter — the walk mark and the coverage token.
- **Superseding a run ≠ cancelling it**: events stop, the walk runs on. Cancel is the dialog close (which SPARES
  `keep_run_id`), Escape, or quit, ❌ never the arena idle-drop. ❌ Both reach `RunOrigin::Dialog` only: an agent's run
  must not silence a person's.
- **MCP takes the SAME live run, folded into one reply**: ❌ no walk-versus-don't parameter (Decision 10), and its wait
  is a transport budget — when it runs out the walk KEEPS GOING.
- **One volume is the CEILING, enforced at the API** (`resolve_target`), not just in the UI. ❌ No fan-out: it's the
  only way a search can silently omit a drive (`docs/specs/unindexed-search-plan.md` Decision 4).
- **Non-root indices are mount-relative**: PREFIX the mount root onto read paths, STRIP it from scope paths (a
  mount-root scope means the WHOLE volume). Mount root = the `volume_path` meta OR the live registry, ❌ never assume
  the meta is set. One NAS reached two ways keeps two index DBs claiming one path: routing picks the live id.
- **Honesty is TYPED**, ❌ never a string match: `uncovered_scopes` (an unindexed volume, index-only runs only — a live
  run walks it instead), `unresolved_scopes` (❌ never "doesn't exist": it can't tell a typo from a not-yet-walked
  folder), and a live run's `SearchRunCoverage`, in which `walk: Completed` ≠ exhaustive.
- **`prepare_search_index`'s `loading` says whether an event is COMING**: `loading: false, ready: false` is the
  terminal "no index here", without which a machine that declined indexing waits forever.
- **A stale root arena is SERVED**, refreshed in the background. ❌ Never reload-on-mismatch: root's generation ticks
  several times a second, costing 2.6 s per search.
- **Count-only** returns an exact total and no rows — except under a dir-size filter, where `run_blocking` MUST
  `fill_dir_sizes` then `count_only_volume_total`, or it over-counts.
- **Memory is the design constraint**: arena-allocated names (❌ no owned `String`s), importance keyed on `hash_path`
  and ❌ never the path, ranking per MATCH and so top-k.
- **`history.rs` holds two locks** (a cache `Mutex`, then `DISK_LOCK`): ❌ no `fs` call or `.await` under a guard.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
