# Importance store — details

The on-disk contract for a volume's folder weights and its navigation-visit counts. Read this before any non-trivial
work here: editing, planning, reorganizing, or advising. The writer thread and its transactions live one level up
(`../DETAILS.md` § The writer).

Per-volume `importance-{volume_id}.db`, a sibling of the drive index's `index-{volume_id}.db` in the app data dir
(`importance_db_path`). It carries the index's disposable-cache discipline verbatim: the shared `platform_case`
collation (reused from `indexing::store`, the SAME filesystem case/normalization rule) registered on every connection,
delete-and-recreate on a `SCHEMA_VERSION` mismatch (no migrations, weights are regenerable), and ONE writer thread per
DB.

## The three tables

- **`weights`** — keyed by `path_folded` (the BINARY primary key), with the verbatim `path` as a plain column. Each row
  also holds the scalar `score`, the serialized raw `FolderSignals` vector (so a future consumer can re-weight under its
  own profile without a rescan), and the **as-of `recompute_generation`** the pass stamped.
- **`visits`** — the navigation-visit signal, `path → (count, last-visit seconds)`. Counts and timestamps only, no
  content, local-only. Fed by `record_visit` (`../DETAILS.md` § The visit signal).
- **`meta`** — `schema_version`, the per-volume `recompute_generation` counter (bumped once per full pass), and
  `scoring_policy` (the classification fingerprint that pass scored under; see below).

All three are `WITHOUT ROWID`. Every weight row carries the as-of generation it was scored at, the honest staleness
marker an offline-unmounted read caveats with; all rows from the last full pass share it, and the read API surfaces it.
Because a full pass rewrites the whole table (`../DETAILS.md` § The writer), a surviving row is never at an older
generation than the store's.

`ImportanceStore` is the read side and the schema-lifecycle owner: `open` applies the schema check plus
delete-and-recreate, and hands out a read connection. The writer thread opens its own connection through
`open_write_connection`. Read-only connections can't create tables, so they assume the write path ran first.

## The folded-key primary key (Decision/Why)

**Decision:** the primary key is a precomputed `path_folded` column — `normalize_for_comparison(path)` (the SAME fold
the `platform_case` collation applies: NFD-normalize then lowercase on macOS, identity elsewhere) — with a plain BINARY
collation, and the verbatim `path` rides along as a non-key column for return values. Every write folds the path once
(`insert_rows`, `apply_visit`, single-sourced through `normalize_for_comparison`); every read binds `folded(query)`
against `path_folded`. This reuses the index store's own `name_folded` pattern, for the same reason.

**Why not a `platform_case`-collated `path` PK:** a custom collation on the key defeats SQLite's b-tree range and
LIKE-prefix optimizations. The incremental's per-prefix subtree query (`writer::apply_incremental`) therefore
FULL-SCANNED the whole `weights` table and re-ran the NFD-folding `platform_case_compare` on every row. CPU profiling
put an incremental's entire cost in that comparison over the scan, and on the root volume (near-continuous FSEvent churn
⇒ incrementals firing constantly) it pegged a CPU core. With a BINARY `path_folded` PK the same range is index-served:
`EXPLAIN QUERY PLAN` shows `SEARCH weights USING PRIMARY KEY` for both the equality and the half-open descendant range
(a `MULTI-INDEX OR`), instead of `SCAN weights`. Pinned by `subtree_read_is_index_served`.

**The descendant range.** `path_folded = folded(P)` covers the changed folder itself, and
`folded(P) + "/" <= path_folded < folded(P) + "0"` covers every descendant: `"0"` (0x30) is one past `"/"` (0x2f), and
`/` is an ASCII boundary folding never crosses, so the range holds exactly `P`'s descendants. The `/` boundary is what
keeps a pass over `/a` from touching a sibling like `/ab`.

**Correctness is preserved exactly.** `path_folded` is byte-identical to what the collation computed, so which case/NFD
variants collide into one row is unchanged; case/NFD-insensitive lookup still resolves
(`weight_lookup_is_platform_case_insensitive`, `incremental_write_resolves_a_case_and_nfd_variant`). Ranking is
unaffected: the score is pure Rust (never touches SQL collation), the search ranker looks up the verbatim `path` in a
`HashMap`, and `ORDER BY score DESC, path ASC` is a determinism tiebreak on the verbatim path. On case-sensitive volumes
`normalize_for_comparison` is identity, so `path_folded == path`.

Measurements (the index-served range, plus why the full walk stays deferred rather than targeted):
`docs/notes/idle-cpu-indexing-streamlining-2026-07.md`. What the range costs now that a pass READS it instead of
DELETE-ing it (10 ms against 550–620 ms over a real 51,081-row subtree):
`docs/notes/importance-treadmill-2026-08-04.md`.

## Storage model: no floored rows, trimmed JSON (compaction)

Two decisions keep the store small (an older DB just recreates fresh on the next scan — it's a disposable cache):

- **A floored folder gets NO row.** On a dev home ~76% of folders floor (a `node_modules`, a `.git`, a cache, and their
  whole subtrees), and storing a `0.0` weight plus a full signal blob for each is pure waste. Floored-ness is derivable
  from the PATH STRING alone (`classify::self_floors` plus the ancestor walk — pure name/path classification, no index
  or listing data), so the store omits floored folders and the read side re-derives them (`../read/DETAILS.md`).
  - The full recompute skips writing a floored folder; the incremental pass handles every floor transition by clearing
    first (`../scheduler/DETAILS.md` § Transition semantics).
  - **The derive-on-read invariant that makes deletion safe**: for every folder the walk produces,
    `classify::floors_by_path(path)` (what the read side uses when a row is absent) agrees with the pure scorer's floor
    over that folder's full signals (what the pre-compaction store would have persisted). Pinned by
    `floored_by_path_matches_the_scorer_floor_for_every_walked_folder` over the whole synthetic home.
- **Trimmed JSON for kept rows.** `FolderSignals` serializes only its non-default fields (`skip_serializing_if` on every
  field, plus `#[serde(default)]` so any subset deserializes). A neutral vector serializes to `{}`; a typical kept row
  carries two or three set fields, roughly halving the stored JSON. Deserialization is compatible in both directions — a
  verbose row and a trimmed one parse to the identical value (pinned by the full-vs-sparse round-trip test) — so the
  store can hold a mix without a migration.

The `weights` write phase dominates a full pass on a local root; compaction roughly halves it there by dropping ~76% of
rows.

## `needs_full_pass` and the recreate ordering

`needs_full_pass(data_dir, volume_id)` answers "can this store's weights be trusted, or does it owe a full recompute?"
and is the gate the scheduler's `wire_volume` probe uses. Two independent reasons say yes: no `RECOMPUTE_GENERATION_KEY`
at all, or a `SCORING_POLICY_KEY` that doesn't match this build (below).

It opens the store on the WRITE path first, which is where the lazy schema delete-and-recreate fires, and only then
reads the meta table. A read-path probe would read the outgoing schema's stamped generation and skip the pass, after
which the recreate wipes it. The full trap and its prod-upgrade history: `../scheduler/DETAILS.md` § The initial full
pass. The store test drives the exact ordering (an old-schema DB with a stamped generation → the read probe sees it →
the write-path-bound probe recreates and reports "needs a full pass").

## The scoring-policy stamp

`SCORING_POLICY_KEY` holds `classify::scoring_policy_fingerprint()`, a content hash over every list the classifiers read
(temp roots, mount prefixes, the home-relative path-class folders, project markers, the denylist) plus
`SCORING_RULES_VERSION` for the rules that aren't a list. `apply_full_pass` writes it in the same transaction as the
generation bump; a mismatch (or an absent stamp) makes `needs_full_pass` answer yes.

**Why it exists.** Nothing else ever revisits a scored volume's rows: a full pass runs once, and an incremental only
touches folders the filesystem changed. So a classification fix ships and stays inert over everything already stored. On
2026-09-03 the local `root` volume held 188,760 such rows, among them `/private/tmp` at
`score=0.898, pathClass=projectRoot` and `$HOME` at `score=0.954`, the two scores that had the agent's wake firing
continuously. The stamp is what makes a fix like that reach them.

**Why a stamp rather than a `SCHEMA_VERSION` bump**, which would also force a rescore: the bump deletes the DB file, and
`visits` is the one table here that isn't regenerable. Navigation history is real user data, so a scoring change re-arms
the weights and leaves it alone (`re_arming_the_scoring_policy_keeps_the_visit_history` pins that).

❌ **Stamp it in `apply_full_pass` and nowhere else.** A full pass is the one moment the table provably holds nothing
but rows this build's classifiers produced. An incremental can't vouch for the rows it didn't touch, so stamping there
would strand them under a policy they were never scored by. Same discipline as the index's exclusion stamp, which is
only ever sent right after a `TruncateData`.

The mixing function is shared with that stamp: `crate::fingerprint::fingerprint_of`, FNV-1a, one golden test behind
both. ❌ Don't fold the scorer's `Weights` into the fingerprint: every row persists the `FolderSignals` it was computed
from, so a weight change re-weights stored signals without a rescan; only a change to the SIGNALS invalidates a row.

## Connection pragmas

Both factories open through `crate::sqlite_util`, so the process-wide page-cache slab is installed before SQLite
initializes; ❌ never `rusqlite::Connection::open*` directly. `connection.rs`'s `apply_pragmas` mirrors the index
store's, page-cache budget included: it delegates to `crate::sqlite_util::apply_page_cache`, so a read connection runs
`READ_PAGE_CACHE_KIB` and the writer `WRITE_PAGE_CACHE_KIB`, both served from the shared slab. ❌ Don't set `cache_size`
locally. `importance-root.db` was the single biggest contributor to the 156 connections a profiled prod session
accumulated, which is why the bound lives in the slab rather than in these numbers. Rationale and measurements:
`indexing/store/DETAILS.md` § "SQLite page memory is one process-wide slab".

## Errors

`ImportanceStoreError` mirrors `IndexStoreError`'s shape: a schema mismatch is a distinct, non-failure variant that
triggers delete-and-recreate rather than surfacing to a caller.
