# Importance store (`importance.db`)

The per-volume SQLite cache: `mod.rs` (schema, `SCHEMA_VERSION`, the read helpers, `needs_full_pass`),
`connection.rs` (the read/write connection factories). `../writer.rs` owns every write, on ONE thread per volume. The
disposable-cache rule and the floor doctrine are in `../CLAUDE.md`.

## Must-knows

- **Rows key on a BINARY `path_folded` PK** (the precomputed `normalize_for_comparison` fold), with the verbatim `path`
  as a plain column for return values. ❌ Never go back to a `platform_case`-collated `path` PK, and ❌ never make the
  incremental's subtree query a `LIKE` prefix: a custom collation defeats SQLite's b-tree range optimization, so that
  query full-scans the table and pegs a CPU core. `subtree_read_is_index_served` pins the plan.
- **The `platform_case` collation is still registered on every connection** for parity with the index store, but no
  importance query relies on it. Every write folds once through `normalize_for_comparison`; every read binds
  `folded(query)`. ❌ Don't add a query that compares `path` under the collation.
- **`SCHEMA_VERSION` is a cache version, not a migration target.** A bump delete-and-recreates every user's
  `importance-{volume_id}.db` on next launch (one recompute, nothing lost). ❌ Don't write a migration. Bump it for a
  change to what rows or JSON the store persists, not only for a `CREATE TABLE` change.
- **`needs_full_pass` must force the WRITE-path open FIRST, then read the meta table.** ❌ Never a read-path
  generation probe: the recreate is lazy and write-path only, so a read probe sees the OUTGOING schema's stamped
  generation, skips the pass, and the recreate then wipes it — the volume sticks at "never scored" forever. The ordering
  and why the sweep binds here: `../scheduler/DETAILS.md` § The initial full pass.
- **`SCORING_POLICY_KEY` is what makes a classification fix reach rows a user already has.** It stamps
  `classify::scoring_policy_fingerprint()` (a content hash of every classifier list plus `SCORING_RULES_VERSION`); a
  mismatch or an absent stamp makes `needs_full_pass` say yes. Nothing else revisits stored rows: a full pass runs
  once, an incremental only touches changed folders. ❌ Write it in `apply_full_pass` and nowhere else: an incremental
  can't vouch for the rows it didn't touch. A `SCHEMA_VERSION` bump would force a rescore too, but it deletes the DB and
  `visits` with it, which is real user data. Details: `DETAILS.md` § The scoring-policy stamp.
- **A floored folder gets NO row**, and a kept row's `FolderSignals` JSON is trimmed to its non-default fields. A read
  that assumes one row per folder is the bug; the read side re-derives floored-ness (`../read/CLAUDE.md`).

The table-by-table schema, the folded-key decision and its measurements, and the storage-compaction model: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
