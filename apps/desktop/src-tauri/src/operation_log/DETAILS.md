# Operation log — details

Depth for the operation-log subsystem. Must-knows and the module map: [CLAUDE.md](CLAUDE.md). The full design, every
decision (D1–D10), and the milestone breakdown: [`docs/specs/operation-log-plan.md`](../../../../../docs/specs/operation-log-plan.md)
— this doc captures what M1 shipped and the durable rationale a future agent needs on hand; the plan holds the rest.

## What this is

A durable, cross-volume journal of every file mutation Cmdr performs: one `operations` row per user-level batch (1:1
with the pipeline's `operation_id`), many `operation_items` rows beneath it. It answers "what did I do to my files, and
can I undo it?" — provenance, rollback, indexed name search, and retention. M1 is the store foundation; capture and
rollback build on it.

## Why a separate durable DB (D1)

Every other on-disk store here is a disposable cache: the drive index and `importance.db` live in `~/Library/Caches/`,
are per-volume, and delete-and-recreate on any schema change (Time Machine skips them, the OS may purge them). A mutation
history is the opposite — valuable user data that must survive for years and span volumes (a copy from disk A to disk B
is ONE operation with one identity). So it's its own `operation-log.db` beside the durable app data
(`resolved_app_data_dir`), which Time Machine backs up normally.

The heavy-churn backup cost is accepted; retention (default 3 GB) bounds it. The named escape hatch, if it ever bites, is
a future "exclude operation log from backups" toggle (an exclusion attribute on the file) — not built, just the
identified reversible lever.

It is itself sensitive: a map of the user's file activity. It stays local, never transmitted (noted in
[`docs/security.md`](../../../../../docs/security.md)).

## The migration ladder (D2) — the reusable template

This is the codebase's **first forward-migration system**, and the template every future durable DB should copy. It
lives in `store/migrations.rs`.

- **Shape.** `MIGRATIONS` is an ordered `&[Migration]`, each `{ version, description, up: fn(&Transaction) }`. `up` for
  version N transforms the schema from N-1 to N (create tables, `ALTER TABLE`, backfill, add an index).
- **The runner (`run_migrations`).** Bootstraps the `meta` anchor table, reads the stored `schema_version` (absent ⇒ 0),
  and for each step newer than the stored version runs `up` + stamps the new version **in one transaction**, oldest
  first. Stepwise commits mean a crash between steps leaves a consistent intermediate version the next open resumes from.
- **Never destroy on a version gap.** A stored version *older* than the ladder migrates up. A stored version *newer* (a
  downgrade — the user ran a newer build, then an older one) is refused with a typed `SchemaDowngrade` error and the file
  is left untouched, because it may hold data this build can't represent. Delete-and-recreate is reserved for a genuinely
  **unparseable** file, classified by the typed sqlite error code (`NotADatabase` / `DatabaseCorrupt`), never a message
  string.

Rules for adding a migration: append a step with the next version; **never edit or renumber a shipped step** (users' DBs
already ran it — editing it silently diverges their schema); write `up` against the schema the prior steps produce, not
the latest Rust structs.

**Why no delete-and-recreate here** (the whole point): the index/importance caches wipe on a `SCHEMA_VERSION` mismatch
because their data is regenerable derived state. The operation log's data is the user's history — wiping it loses
undo-ability, which is exactly what makes the log valuable. So it migrates.

## Case folding, not a collation (D2)

The index and importance stores register a `platform_case` collation for case-insensitive `UNIQUE`, which is why the
`index-query` tool exists at all (stock `sqlite3` can't read those columns). The operation log wants
`sqlite3`-inspectability, so it stores a precomputed `name_folded` column instead: `fold_name` lowercases (Unicode) then
NFC-normalizes, once in Rust at insert. It's a *record* key, so it may differ slightly from a given filesystem's exact
case rules — acceptable for history, not a live mirror. Search and dir identity query the folded column with a plain
b-tree index.

## Schema

Three tables plus `meta` (the migration anchor). Full DDL: `store/migrations.rs::migrate_v1_initial`. The shape and its
non-obvious choices:

- **`dirs` — interned directory prefixes.** `dir_id`, `volume_id`, nullable `parent_dir_id`, `name`, `name_folded`. A
  1M-file operation under one tree shares a handful of directories; interning stores each hot dir once, forever (reused
  across operations), and item rows reference a `dir_id` + a leaf name — so the DB and its backup don't bloat with
  repeated path prefixes. A **volume root** is a single row with `name = ''` and NULL parent, so a file directly at the
  root still has a dir to reference; `reconstruct_dir_path` walks `parent_dir_id` to it.
  - **Identity gotcha.** SQLite treats NULLs as distinct in a plain `UNIQUE`, so `UNIQUE(volume_id, parent_dir_id,
    name_folded)` would fail to dedupe root-level dirs (NULL parent). The identity is a **unique expression index** on
    `(volume_id, IFNULL(parent_dir_id, 0), name_folded)` instead — dir ids start at 1, so 0 is a safe stand-in for a
    NULL parent. `intern_dir` inserts with `ON CONFLICT` on that expression, then reads the id back.
- **`operations` — one row per batch.** `op_id` (the pipeline UUID), typed `kind` + nullable `archive_subkind`,
  `initiator`, the two-axis status (`execution_status` + `rollback_state` + nullable `not_rollbackable_reason`),
  `rolls_back_op_id` (the rollback linkage), source/dest volume ids, timestamps, `item_count` (the **planned** total,
  informational — NOT the completeness yardstick), `items_done`, `bytes_total`, `search_coverage` +
  `search_coverage_reason`, and an optional dev-only `dev_summary`. **No stored rendered summary**: the UI label is
  formatted client-side from the typed fields so it localizes per viewer (D2); `dev_summary` is dev-only and never shown
  in the alpha dialog.
- **`operation_items` — per-item rows.** `seq` (order within the op, for grouped display and reverse-order rollback),
  typed `entry_type` (file/dir) and `row_role` (`rollback_unit` / `search_only`), interned `source_dir_id` +
  `source_name` (+ folded) and nullable dest equivalents, `size`, `mtime`, typed `outcome`, `overwrote`. Directories the
  op created are **first-class `dir` rows** sequenced after their contents, so a `seq DESC` rollback removes files before
  the dirs that held them. Names are indexed folded (search) and, unlike dirs, **not interned** — a b-tree handles the
  massive duplication across a large op fine, and names must stay directly queryable.

## The writer (`writer.rs`)

One dedicated OS thread owns the single write connection; the cloneable `OperationLogWriter` handle is the only way in.
Message surface: `OpenOperation` (insert the header, `Running`), `RecordItems` (batched insert of a slice in one
transaction, interning dirs + folding names), `FinalizeOperation` (write terminal state, return per-`row_role` durable
counts), `Prune` (retention), `Flush` (barrier), `Shutdown`.

- **Lossless with backpressure (D4).** The channel is a bounded `sync_channel`; `record_items` blocks briefly if the
  writer falls behind rather than dropping. Safe for "logging never slows an op": a batched row insert is far cheaper
  than the per-item file I/O the op already does, so the writer outpaces every real op and the block is a theoretical
  backstop. A DB *error* on one row (not fullness) logs and drops THAT row — the op never fails for a journal problem.
- **Completeness accounting is the writer's contribution, not its judgment.** `finalize_operation` returns
  `FinalizeOutcome { rollback_unit_rows, search_only_rows }` — the durably-written counts per role. The **M2 capture
  layer** compares them against the `record_item` calls it actually *issued* (items reached — NOT the planned
  `item_count`, which a canceled op never reaches) and, on a shortfall, degrades: a missing `rollback_unit` row ⇒
  `not_rollbackable(journal_incomplete)`; a missing `search_only` row ⇒ `search_coverage = top_level_only`
  (`search_row_incomplete`). The writer supplies the numbers; it does not itself compute eligibility.
- **Eligibility lives upstream.** The net-new flag and `archive_edit` subkind the capturing driver knows (Finding 3) feed
  the M2 layer's eligibility computation, which passes the already-typed `rollback_state` + reason + subkind into
  `FinalizeOperation`. `writer.rs` stores what it's given — keeping data-safety-critical business logic in the TDD'd
  capture/rollback layers, not the writer. (This is a deliberate divergence from a "finalize computes eligibility"
  reading of the plan: storing the terminal state the caller computed keeps the writer single-responsibility and avoids a
  dead net-new field it would ignore.)

## Retention (mechanism here; enforcement in M4)

The `Prune` message is the mechanism; M4 wires the periodic timer, the settings-driven age/size limits, and the
size-budget loop. What M1 lands: prune **whole operations** older than an age cutoff (never orphan an item from a kept
op), null any now-dangling `rolls_back_op_id`, **skip ops in `rolling_back`** (and their target) so a live rollback's
streamed source rows can't vanish mid-stream, then **GC interned dirs** and run a **bounded** `incremental_vacuum` slice.

- **Dir GC — the referenced-plus-ancestors closure.** Interning keeps a dir row forever, so pruning operations alone
  leaves a monotonically-growing `dirs` floor. GC iterates leaf-up: delete dirs referenced by no item AND no child dir,
  repeat until stable. This deletes exactly the complement of the referenced-dirs-plus-their-ancestors closure — a
  referenced dir's whole parent chain survives (path reconstruction walks it to the root).
- **Bounded vacuum.** Mirrors `indexing/writer/maintenance.rs`: a tiered `pick_vacuum_cap(freelist)` (skip below a floor,
  a steady cap for a modest freelist, ramp for a real backlog) so a big prune drains in bounded slices between insert
  batches and never stops the world — the one thing that could stall the lossless-with-backpressure writer. Importance
  sets `auto_vacuum = INCREMENTAL` but never calls `incremental_vacuum`; this DB must, or it grows unboundedly.

## The dev bin

`cargo run -p index-query --bin operation-log-dump -- <operation-log.db> [limit]` opens the DB read-only and prints
recent operations + their items, decoding the typed tokens and reconstructing interned dir paths through the SAME library
read functions the app uses (`recent_operations`, `read_operation_items`, `reconstruct_dir_path`) — never a
re-implementation. Because there's no collation, a stock `sqlite3` also opens the file directly; the bin adds the typed
rendering.

## Evidence anchors

- The `dirs` NULL-parent `UNIQUE` gotcha and the IFNULL expression-index fix: verified by
  `store::tests::intern_dir_dedups_and_distinguishes_siblings` and `intern_dir_handles_the_volume_root` (2026-07,
  in-tree tests).
- Migration ladder forward/downgrade/unparseable behavior: `store::tests` (`forward_migration_preserves_rows_and_bumps_version`,
  `downgrade_is_refused_not_destroyed`, `unparseable_file_recreates_fresh`).
