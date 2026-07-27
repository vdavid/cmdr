# Importance read API — details

The canonical consumer entry point for folder importance. Read this before any non-trivial work here: editing,
planning, reorganizing, or advising.

`ImportanceIndex` mirrors `search/`→`indexing/`: a read-only handle that owns a `platform_case`-registered read
connection over `importance.db` (thread-local, reopened lazily, keyed by DB path), so no consumer takes a raw
`rusqlite` dep on the store. The agent and media-ML plans point here rather than restating (single-source,
`.claude/rules/docs.md`).

`open(data_dir, volume_id, available)` doesn't touch the DB until the first read, so it's cheap and never fails on a
missing file. `open_at(db_path, available)` serves a caller that already has a path (the dev tuning surface).
`with_home` overrides the home dir the floored-vs-unscored derivation needs (path-class and hidden/system priors are
home-relative; defaults to `$HOME`); `with_weights` overrides the weights `explain` re-scores with.

## The calls

- `weight_for(path)` — one folder's `ScoredWeight` (scalar + deserialized `FolderSignals` + as-of generation), or
  `None`. A floored folder has no row, so it reads `None` here too.
- `lookup(path)` — the typed `WeightLookup::{Scored, Floored(FloorReason), Unscored}`. A floored folder has no row, so
  the reason (`nameDenylisted` / `hiddenOrSystem` / `underFlooredAncestor`, in that precedence) is derived live from the
  path — the single derivation `explain`'s floored breakdown also uses. A caller that only wants the number can use
  `WeightLookup::score()`, which flattens floored and unscored to `0.0`.
- `top_n(n)` / `above_threshold(t)` / `top_above_threshold(n, t)` — ranked folders (score DESC, ties by path ASC).
  `above_threshold` is INCLUSIVE at the bound (a folder exactly at `t` is returned); `top_above_threshold` combines the
  `LIMIT` and `WHERE score >= t` in one bounded query, which is how the MCP resource's capped threshold read fetches
  `cap + 1` to detect truncation without loading the whole tail. The agent's summary gate and media-ML's
  enrich-important-first.
- `scored_folder_count()` — the `weights` row count (a `COUNT(*)`, no deserialization), for the overview surface.
- `signals_for(path)` — the stored raw vector, for a consumer applying its own weighting profile (plan Decision 2).
- `for_each_nonzero_weight(visit)` — STREAMS every `(path, score)` with a non-zero score (floored folders omitted), for
  a consumer that folds one snapshot into its own in-memory form and ranks many candidates against it rather than
  querying per item. It streams rather than returning a map because a `path → score` map is far wider than what the
  consumer keeps (58 MB for a measured 368,043-folder NAS volume), and each `path` borrows the row buffer, so a row
  allocates nothing.
- `explain(path, now)` — the per-signal breakdown, **recomputed from the STORED signals via the pure scorer**, so
  there's ONE formula and the breakdown can't drift from the stored scalar. A floored folder (no row, floors by path)
  reports a floored `Explanation` (score `0.0`, `floored == true`) whose flag says WHY, derived live. That breakdown
  loses the "would-have-contributed" additive terms, which is acceptable and deliberate: tuning cares about the
  non-floored ranking, and floored-with-reason is what a consumer needs.
- `recompute_generation()` — the store's current generation (`0` if never scored).

A malformed stored signal vector degrades to `FolderSignals::neutral()` rather than failing the read: the scalar is
still good, and only a re-weighting consumer loses the raw vector for that one row.

## The consumers

- **Search ranking.** `search/` blends these weights into result ordering (a file takes its parent folder's weight),
  streaming one `for_each_nonzero_weight` snapshot per recompute via `subscribe`. Match quality dominates; importance is
  a within-band boost. The blend design, weight-map lifecycle, and degradation contract live in `../../search/DETAILS.md`
  § "Importance ranking" (single-source).
- **The MCP `cmdr://importance` resource.** It exposes `lookup` / `top_n` / `above_threshold` / `top_above_threshold` /
  `explain` / `scored_folder_count` to agents, enumerating scored volumes offline via `scored_volume_ids` (the
  `importance-{id}.db` files on disk) and opening each index with the kind's `signal_availability` mask so `explain`
  sums to the stored score. It's the offline-unmounted read made a user-facing feature. Builder and modes:
  `../../mcp/DETAILS.md` § "Resources" (the `cmdr://importance` builder).
- **Media-ML enrichment.** It orders its passes by importance and asks "has importance scored this volume?" through the
  row count, not the generation (`../../media_index/coverage.rs`'s `importance_scored`).

`scored_volume_ids(data_dir)` lists the volume ids with an `importance.db` on disk, root first then the rest sorted.
The stores outlive their volume's mount by design, so this is the offline-capable roster: no live scheduler, index
registry, or mount needed. MTP is never background-scored, so no `importance-mtp-*.db` exists to list.

## Offline-unmounted reads (plan Decision 2)

`ImportanceIndex` reads a volume's `importance.db` (a local per-volume file) directly and NEVER touches the index
registry, so a volume's weights stay queryable after it unmounts (its index registration gone, `get_read_pool_for` now
`None`). `weight_for` / `top_n` / `recompute_generation` answer from the on-disk store, each weight carrying the as-of
generation it was scored at — the staleness caveat ("as of the last scan before the NAS went offline"). Proven end to
end by `offline_unmounted_read_returns_stored_weights_after_index_gone` (score a volume, delete its index DB, assert the
read API still returns weights at the right generation). When the OS purges the cache the file vanishes and the read
returns `None`; the next mount plus scan regenerates it — weights are disposable, identical to the index-purge path.

## Staleness

Every result carries `as_of_generation`; a consumer compares it to `recompute_generation()` to caveat "as of the last
scan" (agent-spec D7). The read API never hides a stale weight. Only a full pass advances the generation, so `0` does
not mean "unscored" — read `../scheduler/DETAILS.md` § Generation semantics before keying on it.

## The recompute subscription

`subscribe(volume_id)` returns a `tokio::sync::watch<u64>` receiver carrying the last completed generation. The
scheduler calls `notify_recompute_completed` after each full or incremental pass, so a consumer awaits `changed()`
instead of polling (subscribe-don't-poll). It retains the last value for a late subscriber (`send_replace`) and fires
exactly once per completion. The senders live in a process-global keyed by volume id (surviving an unmount), like the
indexing lifecycle bus. A crate-visible test shim lets a consumer's subscribe→reload wiring be tested without widening
the production notifier past the scheduler.

## Dev tuning surface (`importance-tune`, plan Decision 6)

A dev-only binary extending the `index-query` pattern (a `cmdr_lib`-linking CLI with the collation registered). It reads
a volume's `importance.db` through this SAME read API and prints the ranked folders WITH their `explain` breakdowns, so
David can eyeball the ranking against his real home directory and tune `Weights` (agent-spec §18.3). No write path — it
reads stored signals and re-scores.

```
cargo run -p index-query --bin importance-tune -- <path-to-importance-root.db> [top_n]
```

Find the DB under the app data dir as `importance-root.db` (beside `index-root.db`); `top_n` defaults to 30. The
printout lists each folder's score, then per-signal `weight`, `raw`, and `contribution` (skipping signals redistributed
to zero), so a mis-ranked folder's cause is visible. Measuring ranking QUALITY (rather than eyeballing it) is
`../evals/DETAILS.md`.
