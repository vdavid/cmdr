# Operation log — details

Depth for the operation-log subsystem. Must-knows and the module map: [CLAUDE.md](CLAUDE.md). The full design, every
decision (D1–D10), and the milestone breakdown: [`docs/specs/operation-log-plan.md`](../../../../../docs/specs/operation-log-plan.md)
— this doc captures what shipped and the durable rationale a future agent needs on hand; the plan holds the rest.

## What this is

A durable, cross-volume journal of every file mutation Cmdr performs: one `operations` row per user-level batch (1:1
with the pipeline's `operation_id`), many `operation_items` rows beneath it. It answers "what did I do to my files, and
can I undo it?" — provenance, rollback, indexed name search, and retention. Shipped: the durable store, capture at
the chokepoint, the rollback engine, the read/search API + retention enforcement, and the MCP tools
(`operations_list` / `operations_get` / `operations_rollback` — see `mcp/DETAILS.md`). The UI (the Debug panel and the
alpha dialog) builds on the read side.

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
  `FinalizeOutcome { rollback_unit_rows, search_only_rows }` — the durably-written counts per role. The **capture
  layer** compares them against the `record_item` calls it actually *issued* (items reached — NOT the planned
  `item_count`, which a canceled op never reaches) and, on a shortfall, degrades: a missing `rollback_unit` row ⇒
  `not_rollbackable(journal_incomplete)`; a missing `search_only` row ⇒ `search_coverage = top_level_only`
  (`search_row_incomplete`). The writer supplies the numbers; it does not itself compute eligibility.
- **Eligibility lives upstream.** The net-new flag and `archive_edit` subkind the capturing driver knows (Finding 3) feed
  the capture layer's eligibility computation, which passes the already-typed `rollback_state` + reason + subkind into
  `FinalizeOperation`. `writer.rs` stores what it's given — keeping data-safety-critical business logic in the TDD'd
  capture/rollback layers, not the writer. (This is a deliberate divergence from a "finalize computes eligibility"
  reading of the plan: storing the terminal state the caller computed keeps the writer single-responsibility and avoids a
  dead net-new field it would ignore.)

## Capture — journaling every mutation at the chokepoint

`capture.rs` is the journal half of the pipeline observer seam (D4); the write pipeline's glue lives in
`file_system/write_operations/journal.rs`. Together they make every managed mutation journal itself with per-item rows,
two-axis status, and computed rollback eligibility — without measurably slowing the op.

### The seam: a global journal reached by `op_id` (recorded deviation from D4)

**D4's plan bundles the journal WITH the sink into an `OperationObservers` context threaded down the pipeline. This
implementation does NOT do that.** Instead the journal is a **process-global singleton** (`operation_log::JOURNAL`,
installed at `start`) reached BY `op_id` through free functions — `journal_open`, `journal_record_items`,
`journal_note_coverage`, `journal_finalize` — exactly mirroring the op-keyed `update_operation_status(op_id, …)` status
cache that the same record points already write, and the `manager()` operation-manager singleton.

- **Why the deviation.** Threading an observers context replaces `Arc<dyn OperationEventSink>` through the entire
  transfer/delete signature chain (`copy_single_item`, the transfer driver, the volume paths, …) — a large, high-risk
  refactor of the app's most data-safety-critical code. The op-id-keyed global is (a) how `update_operation_status`
  already works at the identical call sites, (b) how `manager()` already works, and (c) zero-churn to those signatures.
  D4's HARD constraint — never extend `OperationEventSink` — is kept; only its suggested *mechanism* changed.
- **Testability holds.** `set_journal` / `clear_journal` install a `CapturingJournal` or a temp-DB `WriterJournal` per
  test; nextest isolates each test in its own process, so the global is hermetic.
- **Lifecycle.** `journal_open` is called when the op actually STARTS (inside the manager's deferred), not at
  registration, so a queued op that's canceled before admission never journals and leaks no accumulator. `journal_finalize`
  removes the per-op accumulator.
- **Header aggregates (`item_count` / `items_done` / `bytes_total`).** `open` writes a provisional `item_count` (the
  top-level source count, known before the scan) plus the destination volume; `finalize_op` then refines the three
  aggregates from the live `OperationStatus` cache (the same one the queue UI drives — `header_totals_from_status`), so
  a finished op carries the scanned leaf total and completed count, not zeros. When no status row exists (an instant
  create, an archive edit, or a direct-call test), `item_count` stays at the open value and `items_done`/`bytes_total`
  stay 0. These fields are informational (the alpha dialog renders "Copy N items" from `item_count`), NOT the rollback
  yardstick — completeness still compares ISSUED vs written per `row_role` (above).

### The two decisions the capture layer owns (the writer doesn't)

- **Eligibility (D3), `compute_eligibility`** — pure, tested in isolation: copy/move rollbackable iff nothing overwrote;
  delete never (`permanent_delete`); trash/rename/create-folder/create-file open rollbackable (rechecked at rollback
  time, in the rollback engine); compress rollbackable iff net-new (`archive_overwrite` otherwise); zip-inner edit not yet
  (`zip_edit_unsupported`). `execution_status` is deliberately NOT an input — a failed/canceled op stays rollbackable for
  what it reached (D4).
- **Completeness (D4), `apply_completeness`** — the per-`row_role` issued-vs-written check. The `WriterJournal`
  accumulates the count of `record_item` calls it ISSUED per role; `finalize` compares them to the writer's durable
  counts and, on a shortfall, downgrades: a missing `rollback_unit` row ⇒ `not_rollbackable(journal_incomplete)` (a
  lossy journal must never claim rollbackability); a missing `search_only` row ⇒ `search_coverage = top_level_only`
  (`search_row_incomplete`). Compared against ISSUED, never the planned `item_count` (Finding 1). The correcting
  re-finalize fires only on a real drop, so it's rare.

The `WriterJournal` also **batches** rows (a per-op buffer flushed at `RECORD_BATCH` or finalize, so a huge op coalesces
into batched writer transactions) and **auto-assigns `seq`** in recording order, so record points never track it.

### Per-kind record points and granularity (D-granularity)

Each point is where the op already stats the item, so journaling is near-zero marginal cost (no new syscalls):

- **copy** — per-leaf `rollback_unit` rows at `transfer/copy/single_item.rs` (right where each file commits to the
  `CopyTransaction`, carrying the free source `mtime` + the overwrite flag), plus the **created-directory rows** from
  `CopyTransaction::created_dirs` at the success commit in `copy/mod.rs`. Files record during the op, dirs after, so dir
  `seq` > their contents' `seq` (Finding 2); the rollback engine removes files before dirs.
- **delete** — per-leaf at `delete/walker.rs`, one row per file, deliberately (a 1M-file delete journals ~1M rows on the
  order of tens-to-~150 MB — leaf search is the requirement, and retention, D9/D10, manages the cost, NOT a row cap).
  Delete is never rollbackable, so these rows exist purely for "when did I delete `dog.jpg`".
- **same-FS move + trash** — the **top-level** `rollback_unit` row (one rename-back / one restore reverses the whole
  subtree) at `transfer/move_op.rs` / `delete/trash.rs`. Trash also captures the OS `resultingItemURL` (the in-trash
  location) as the row's dest, so the rollback restore knows where to move it back from. The subtree's `search_only` leaves
  come from the **drive index**, not a filesystem walk — see § Search-leaf enumeration.
- **cross-FS move** — per-leaf via `copy_single_item` (it stages a copy), same as copy; the op's `kind` is `move`.
- **compress** (`archive_edit`) — spawns directly (not through `start_write_operation`), so `copy_into.rs`'s deferred
  carries its OWN open/finalize bracket. The compress driver supplies the `archive_edit` subkind + a net-new flag
  (probed before the seed overwrites the target) via `ArchiveProvenance` — the journal can't derive them, both compress
  and zip-inner edit cross IPC as `ArchiveEdit` (Finding 3). A net-new compress records the created archive as its single
  `rollback_unit` item (with a size/mtime snapshot for the rollback drift recheck) and finalizes `rollbackable`; an overwrite
  of a prior archive is `not_rollbackable`; a plain into-archive edit journals its header only (not rollbackable in v1).

### Search-leaf enumeration (`file_system/write_operations/journal_search.rs`)

For same-FS move + trash, the subtree's descendant leaves are read from the DRIVE INDEX (zero extra filesystem I/O) and
recorded as `search_only` rows so "when did I trash `dog.jpg`" hits inside a trashed folder. Two hard ordering rules:

- **Enumerate BEFORE the OS mutation**, buffered in memory: the index reconciler prunes the subtree the instant it sees
  the trash/rename FSEvent, so a later read would find the rows gone and wrongly stamp `full`.
- **Persist only AFTER the top-level item succeeds**: both loops process per item with partial failure, so a failed item
  must record no leaves (search has no per-item outcome filter — leaves for a never-trashed subtree would return a trash
  that never happened).

`search_coverage = full` is gated on the subtree being PRESENT + CURRENT (`min_subtree_epoch > 0` AND `== current_epoch`,
read via the sanctioned `ReadPool` — never a raw rusqlite dep on the index DB) AND the volume index `Live` (active +
`Fresh`); otherwise it downgrades to `top_level_only` with a typed reason (`index_absent | index_stale | volume_not_live
| capped`). The leaf read is bounded by `SEARCH_LEAF_CAP` (50,000) via `IndexStore::list_children_on_limited`'s `LIMIT
cap + 1`, so a 1M-child folder pays a bounded (~59 ms) synchronous read before the sub-second rename, not a 1M-row one;
over the cap ⇒ top-level row only + `capped` (rollback unaffected — the top-level row is the undo unit regardless).
Numbers + the cap-tuning rationale: [`docs/notes/operation-log-capture-bench.md`](../../../../../docs/notes/operation-log-capture-bench.md).

### The bypass boundary

- **`run_instant` ops (rename / mkdir / mkfile)** don't flow through the sink; capture hooks the managed functions
  directly as single-item ops.
- **`paste_clipboard`** (paste-as-file) now runs INSIDE `manager::run_instant` with a `CreateFile` descriptor, so it
  journals a one-item `CreateFile` op for free (the write loop is unchanged, just bracketed by the managed op).
- **The single `move_to_trash_sync` in `rename.rs`** goes through `trash::trash_single_journaled`, journaling a one-item
  trash op that mirrors the batch path (in-trash dest + drive-index search leaves).
- **Volume (SMB / MTP) copy/move/delete + volume `run_instant`** journal through the same seam, but under the REAL
  volume id (not `"root"`). They spawn through their own `volume_copy.rs` / `volume_move.rs` / delete-walker deferreds,
  so each deferred brackets the op with `journal::open_volume_op` / `finalize_op` and the per-item record points call the
  `record_volume_*` siblings of the local helpers. The record points inside the shared `copy_volumes_with_progress` /
  `move_volumes_with_progress` bodies read the `(source_volume_id, dest_volume_id)` off `WriteOperationState::journal_volumes`
  (the deferred sets it) rather than taking new params — those bodies have ~80 test call sites, and this mirrors how
  `op_id` already reaches them. `run_instant` (create/rename) unifies local + volume via `open_volume_op` with the id
  (`"root"` for local), so there's no local/volume branch. See § Volume capture below.
- **Native drag-out** is explicitly OUT of scope — the destination is another app, outside Cmdr; there's nothing to roll
  back to.
- **Archive move-OUT** (`route_archive_move_out`, a compound extract-then-archive-rewrite) does NOT open an
  operation-log op; its extract phase runs `copy_volumes_with_progress` with no journal target set, so it doesn't journal.
  A copy/move INTO an archive DOES journal (via the compress/edit `ArchiveProvenance` path), but the plain
  into-archive-edit convenience wrapper (`route_archive_copy_into`) defaults its `initiator` to `user` — an
  MCP-initiated into-archive edit records `user`, a minor provenance gap on a rare, already-`not_rollbackable` path.

### Volume capture — carrying the real volume id + honest overwrite

Two things the volume paths need that the local paths don't:

- **The real volume id, threaded via op state.** `journal.rs` gained `open_volume_op` + `record_volume_leaf` /
  `record_volume_transfer_source` / `record_created_dirs_on` / `record_search_leaf` — the volume-aware siblings of the
  `_local_` helpers, taking explicit source/dest volume ids. The deferreds call `open`/`finalize` directly (they have the
  ids); the per-leaf points inside the shared `*_with_progress` bodies read `WriteOperationState::journal_volumes`
  (`Some((src, dst))`, set by the deferred; `None` in tests / the both-local shortcut ⇒ no journaling). Honesty invariant,
  TDD-guarded: a volume op's rows carry the real id, never `"root"` (a wrong id silently corrupts history —
  `volume_copy_journals_under_the_real_volume_ids_not_root`, `cross_volume_move_journals_per_leaf_move_rows`).
- **Overwrite detection for eligibility.** A copy/move that overwrote isn't rollbackable (deleting the copies can't
  restore the overwritten original), so the volume paths surface "did anything overwrite": the top-level file→file
  safe-replace is known at the call site; deep-merge child overwrites are counted in `CreatedPaths::overwrote_files`
  (copy/cross-volume-move) or a `RenameMergeCtx::overwrote` flag (same-volume move), and the same-volume resolver records
  a top-level file overwrite in a shared `overwritten_sources` set (it runs in a separate driver callback from the record
  point). The recorded row's `overwrote` bit is the OR of these; `compute_eligibility` reads it op-wide. Per-inner-file
  granularity isn't tracked (op-wide eligibility is all that's consumed).

Granularity mirrors local (D-granularity): cross-volume copy + cross-volume move + volume delete are per-leaf; a
same-volume move is a same-FS-style move (top-level `rollback_unit` row + drive-index `search_only` leaves, which
downgrade to `index_absent` on a volume with no index — verified by the gate in `enumerate_subtree_for_search`).

### Provenance — initiator threads through every write-start command

Every write-start command now carries an optional `initiator` (default `user`): the local commands (provenance threading) plus the volume
commands (`copy_between_volumes`, `move_between_volumes`, `compress_files`) and the `run_instant` commands (`create`,
`rename`). The FE `mcp-listeners.ts` tags MCP-originated write dispatches `ai_client` (threaded through the typed command
bus alongside `autoConfirm`/`onConflict`, mirroring navigation's `source: 'mcp'`). The one gap: an into-archive-edit via
a volume command defaults to `user` (see the bypass boundary above).

### Row-volume tradeoff

Per-leaf delete/copy rows are the search requirement, so there is **no row cap** on them; the only cap is the
`search_only` leaf enumeration for trash/same-FS-move (search-leaf enumeration, a per-op tunable). Retention (D9/D10) reclaims the space, not
a row cap.

## Rollback — reversing an operation as recheck-then-act inverses

`rollback.rs` is the data-safety-critical engine that undoes a journaled operation. It never runs a bespoke reversal:
each item's inverse is applied through the `Volume` trait (so local and remote reverse uniformly), and the whole inverse
is itself a journaled operation linked back via `rolls_back_op_id`. The write-pipeline glue that spawns it as a
cancelable managed op is `file_system/write_operations/rollback.rs::dispatch_rollback`.

### The two data-safety guards (D7)

Every item passes two independent guards before anything is touched; failing either SKIPS the item (never operates on
it), feeding a `partially_rolled_back` result:

1. **Snapshot recheck** (`verify_snapshot`). The item must still match the size/mtime the journal recorded. Every
   recorded field must have a present, equal live value; a recorded field whose live counterpart is absent (an MTP/SMB
   mtime the backend can't prove) is **Unverifiable** — a skip, not a guess. Nothing recorded ⇒ Unverifiable too. A
   recorded field that differs ⇒ **Drift** — skip. This is why a copy leaf that recorded only `size` (volume transfers
   carry no per-leaf mtime) still verifies on size, while an item whose only field (mtime) is absent live is skipped.
   Directories aren't cheaply verifiable, so a dir's recheck is existence-only.
2. **Pinned non-destructive restore.** A restore-move (move/trash/rename undo) NEVER overwrites: if the restore target
   is occupied by a DIFFERENT entry it skips (`RestoreTargetOccupied`). The one exception is a **case-only
   self-collision** (`is_self_collision`): restoring `dog.JPG` → `dog.jpg` on a case-insensitive volume sees the target
   "exist" because it IS the same entry — same inode (`LocalPosixVolume`) or, without inodes (MTP), the same
   case-folded path **within one volume** (the `same_volume` gate is load-bearing — a cross-volume restore to the same
   relative path is a genuinely different file and must never be treated as self).

### Per-kind inverse table

The op kind + item entry-type map to one of three inverse actions (`inverse_action`):

- **copy** → file: `RemoveFileIfUnchanged` (delete the copied dest if it still matches the snapshot); dir: the created
  dir is `RemoveDirIfEmpty`. A copy of a whole tree removes its copied files, then its created dirs.
- **create_file / compress** (`archive_edit`) → `RemoveFileIfUnchanged` (delete the created file / net-new archive only
  if unchanged — a later zip-edit drifts the archive, so it's left untouched, Finding 5).
- **create_folder** → `RemoveDirIfEmpty` (remove only if still empty — a file added since ⇒ keep, partial).
- **move / trash / rename** → `RestoreMove` (move the item back FROM where it landed, `dest`, TO its original,
  `source`). Trash's `dest` is the recorded in-trash location; a same-volume undo is a `rename`, a cross-volume one
  streams the bytes back then deletes the source side.
- **delete** → refused op-level (a permanent delete can't be restored).

The inverse op's own eligibility is computed like any op: a delete-the-copies undo is `not_rollbackable`, a move/rename
undo is `rollbackable` again (redo falls out — D-undo).

### Streaming + ordering

Reversal streams the original op's `rollback_unit` rows via `store::read_rollback_units_page` (a `seq DESC` paged
cursor, `before_seq` = the smallest seq of the prior page), so a 1M-item op never materializes its list.
`search_only` leaves are excluded (they're for search, never reversal). Removal happens in two phases matching
`CopyTransaction::rollback`: all **files** first (streamed), then the buffered **created-dir** rows deepest-path-first —
a dir removes only once its contents are gone, and pure `seq DESC` would hit a deep dir before the files it holds. Dirs
are a small fraction of an op (interning shares them), so buffering just the dir rows stays bounded.

### The `rolling_back` state machine + startup reconcile (Finding 7 + 3)

`rollback_operation` (the entry) reads the op, gates it (`check_rollbackable`: `UnknownOperation` / `AlreadyRollingBack`
/ `NotRollbackable(reason)` / `VolumeUnavailable{volume_id}` — the cross-volume gate is computed from live mount state,
never stored), then sets `rolling_back` **as late as possible** (right before the spawn) and hands the plan to the
injected `spawn`. The double-rollback guard is automatic: a second request reads `rolling_back` and refuses. On a
**synchronous spawn failure** (a volume dropped between the gate and the spawn, so the inverse never starts) it resets
`rolling_back → rollbackable` in the same call before returning the typed error, so a retry isn't wedged.

`execute_rollback` resolves the original op at the end: `RolledBack` (nothing skipped), `PartiallyRolledBack` (any skip,
even if nothing could be reversed — those skips won't clear on retry), or back to `Rollbackable` **only** when a run was
CANCELED with nothing reversed (a clean retry). It marks the original's items `rolled_back`/`skipped` and journals the
inverse op's own item rows (reversed ⇒ `done`, skipped ⇒ `skipped`), which drive the reconcile.

**Startup reconcile** (`reconcile_rolling_back_on_open`, called at `start` beside the migration-ladder open path)
resolves any op a crash left `rolling_back`: from its unfinalized inverse op's recorded outcomes (any `done` item ⇒
`partially_rolled_back`, else `rollbackable`), or — when **no inverse op row exists** (crashed after setting
`rolling_back` but before the spawn) — straight back to `rollbackable`. Either way a re-issued rollback resumes
idempotently: every per-item inverse is a recheck-then-act, and an already-reversed item reads as `AlreadyGone` (counted
as a no-op success).

### The retention race it closes (Finding 6)

The paged cursor spans successive short-lived read connections, not one WAL snapshot, so a concurrent `Prune` could
delete rows between pages and silently under-restore. The fix is NOT a long read transaction (it would block WAL
checkpointing for the whole file-I/O duration) — it's the `rolling_back` state: retention skips any op in `rolling_back`
(and its `rolls_back_op_id` target), so a live rollback's streamed source rows can't vanish mid-stream (see `writer.rs`
`handle_prune`).

### Known snapshot-completeness limit

Volume (SMB/MTP) transfers record `size`/`mtime` only for TOP-LEVEL files, not for the inner leaves of a copied/moved
directory (the capture path doesn't cheaply have them). So a rollback of a cross-volume directory copy/move verifies
and reverses the top-level items but SKIPS inner leaves as `UnverifiablePrecondition` — a safe partial, never a wrong
delete. Local-FS copy/move record per-leaf mtime, so their directory rollbacks are complete. Closing this needs the capture layer to
capture inner-leaf snapshots for volume transfers.

The case-only rename self-collision logic is unit-tested through the pure `is_self_collision` helper (inode plus
path-fold cases) but NOT exercised end-to-end on a real case-insensitive filesystem — the `InMemoryVolume` fixtures are
case-sensitive. A macOS-gated tempdir integration test on the real FS is the named follow-up.

### Future: Cmd+Z (D-undo, designed-for, not built)

A later Cmd+Z is `SELECT op_id FROM operations WHERE initiator='user' AND rollback_state='rollbackable' ORDER BY
ended_at DESC LIMIT 1` then `dispatch_rollback`. The two-axis status + `rolls_back_op_id` linkage make it a query, not a
new engine; because a rollback is itself a journaled user op, "undo the undo" (redo) falls out too. Don't build it;
don't preclude it.

## Query API + search

`query.rs` is the read side: short-lived read-only connections, index-served name search, and paged reads for the Debug
panel / alpha dialog / MCP tools. Two kinds of caller open their own short-lived read connection and
forward to `query.rs` (business logic — filtering, paging, dir-path resolution — lives there, never in the callers): the
FE IPC surface is two thin pass-throughs (`commands/operation_log.rs`): `get_recent_operation_log_entries(limit, offset)`
and `get_operation_log_detail(operation_id, item_limit, item_offset)`; the MCP `operations_list` / `operations_get`
handlers (`mcp/executor/operation_log.rs`) do the same off the MCP task.

- **Name search is an indexed folded-name lookup, not FTS (D8).** The product headline — "when did I delete `dog.jpg`?"
  — is exact/prefix name equality, so `search_operations` joins `operation_items` to `operations` and matches the
  indexed `source_name_folded` column. The benchmark query
  (`source_name_folded = ? AND kind IN (delete, trash) ORDER BY ended_at DESC`) is served by
  `operation_items_source_name` + the `operations` PK, never a full table scan — pinned by an `EXPLAIN QUERY PLAN` test
  (`query::tests::delete_dog_jpg_is_index_served`). FTS5 stays a clean later add if substring/fuzzy is ever wanted.
- **Search spans every `row_role`, deliberately.** A trashed folder records the top-level `rollback_unit` plus its
  subtree's `search_only` leaves (D-granularity), so "when did I trash `dog.jpg`" hits even when `dog.jpg` sat inside a
  trashed folder — the asymmetry a uniform-granularity design would leave as a silent miss. The one uncovered case (a
  subtree that couldn't be enumerated) is flagged `search_coverage = top_level_only`, a queryable known gap
  (`coverage_is_complete`), not a false negative.
- **Names repeat, so they're NOT interned.** Item names duplicate massively across a 1M-file op; a b-tree index handles
  duplicate keys fine, and (unlike dirs) names must stay directly queryable. Prefix match is a folded-range scan on the
  same index (`>= prefix AND < prefix⁺`), never `LIKE` (which wouldn't use the index).
- **Item views resolve interned dirs to full paths.** `OperationRow` is the summary wire type (no `dir_id`s); item rows
  carry interned prefixes, so `get_operation` returns `OperationItemView`s with `source_path`/`dest_path` reconstructed —
  the frontend never sees a `dir_id`.

## Alpha UI — the "Operation log" dialog

The thin alpha surface (requirement 6b): a menu- and shortcut-triggered soft dialog listing recent operations, newest
first, each expandable to its per-item rows. **Debugging/demo quality by design** (it may become a sidebar later), so it
is deliberately un-gold-plated — but i18n, style-guide copy, and a11y basics are not optional. It lives entirely in the
frontend (`apps/desktop/src/lib/operation-log/`) over the existing read API; no new backend command.

- **Modeled on the What's-new dialog** (the menu-triggered, list-rendering, App-scoped soft-dialog template):
  - `operation-log-trigger.svelte.ts` — the reactive `$state({ open, entries, loading, loadError, hasMore, loadingMore })`
    plus `openOperationLog()` (fetch page 1 = 50 via `getRecentOperationLogEntries`), `loadMoreOperations()` (append the
    next 50), `closeOperationLog()`. **Paging offset is `entries.length`** (one source of truth), so an append can't
    desync or duplicate. Opens even on a read failure (`loadError`) so the menu item never feels dead.
  - `OperationLogDialog.svelte` — the `ModalDialog` (`dialogId="operation-log"`, registered in `dialog-registry.ts`),
    mounted in `routes/(main)/+page.svelte` against `operationLogState.open` (and added to that page's
    `isModalDialogOpen()` guard). One collapsible row per operation; expanding lazily fetches its items via
    `getOperationLogDetail` (cached per `opId` for the dialog's lifetime).
  - `operation-log-labels.ts` — PURE label mapping. **Every label derives from a TYPED enum, never a backend-rendered
    string** (`no-string-matching`): the per-operation summary ("Moved 214 items") is formatted client-side from
    `kind` + `archiveSubkind` + `itemCount` via an ICU plural key, so it localizes per viewer and shows a thousands
    separator (Finding 3 — the backend ships no rendered English summary for the dialog). Status, kind, initiator, and
    item-outcome labels each map their enum to a catalog key with an exhaustive switch (a new variant is a compile error
    until mapped). Style-guide: `failed` statuses/outcomes read "Didn't finish", never "failed".
  - IPC wrappers: `$lib/tauri-commands/operation-log.ts` unwraps the two `Result`-returning read commands (the Debug
    panel calls the raw bindings directly — it's dev-only and bindings-import-exempt).
- **ALPHA badge** via `StatusBadge` keyed off the `operation-log` entry in the repo-root `feature-status.json`
  (`"status": "alpha"`).
- **Menu + command wiring (the "four places", plus the `COMMAND_IDS` id):** command `log.operationLog` (App scope) — the
  `command-registry.ts` entry, the `app-dialog-handlers.ts` handler (`openOperationLog()`), the `mod.rs`
  `menu_id_to_command`/`command_id_to_menu_id` mappings for `OPERATION_LOG_ID` + the `MenuItem::with_id` in BOTH
  `macos.rs` and `linux.rs` (**View menu**, after the command palette), and `menuCommands` in `shortcuts-store.ts`.
- **Default shortcut ⌘⌥L** (configurable via the registry `shortcuts` field). The plan's first pick ⌥⌘O is already bound
  to `file.showInFinder`, so `L` (for "log") is the mnemonic. The modifier order is **Command-then-Option (⌘⌥)**, not the
  Apple display order (⌥⌘): `formatKeyCombo` emits ⌘⌥ and the JS keydown dispatch (`shortcut-dispatch.ts`, keyed off
  `getEffectiveShortcuts` → `toPlatformShortcut`) matches that, so the shortcut fires via the in-app dispatch on macOS
  and Linux — an ⌥⌘-order default would fire only via the native menu accelerator. Guarded by a collision test in
  `command-registry.test.ts`.

**Convergence with the agent log (Naming section / D7).** The dialog is the mutation half of a future unified timeline:
when the in-app agent ships, its decision log (`agent_log`) joins the same user-facing surface, and whether that merged
surface is later *labelled* "Activity" is a UI-copy call for then. "Operation" stays the entity/row name; "action" stays
reserved for the agent spec's navigation/intent stream. The dialog is intentionally throwaway so that convergence can
reshape it (likely into a sidebar) without sunk cost.

## Retention (D9) — prune by age + size, GC dirs, reclaim

Retention runs the writer's `Prune` on startup and on a periodic timer (`retention.rs`, every 6 h), with the age/size
limits read fresh from settings each tick (`load_operation_log_retention_limits`) so a settings change takes effect on
the next tick with no restart or applier case. Defaults: **age = forever, size = 3 GB** (D10). The settings contract:
`operationLog.maxAge` (duration ms; `0` = forever; absent ⇒ forever) and `operationLog.maxSize` (bytes; absent ⇒ 3 GB;
`0` = unlimited). The user-facing controls are the "Operation log" settings section (`OperationLogSection.svelte` + the
`operationLog.*` entries in `settings-registry.ts`), which persist exactly these keys — see
[`adding-a-new-setting.md`](../../../../../docs/guides/adding-a-new-setting.md) for the registry pattern.

A prune (`handle_prune`) is: **age prune** (delete whole finished ops older than the cutoff) → **size prune** (delete the
oldest whole ops until the DB fits the budget) → **dir GC** → **reclaim**.

- **Prune whole operations only.** Never orphan an item from a kept op; never leave a dangling `rolls_back_op_id` (null a
  surviving op's link to a pruned op BEFORE the delete, or the self-FK rejects it). A rolled-back pair prunes together
  (`rollback_pair_component` pulls in the seed's inverse/original), and any protected partner is excluded with its link
  nulled.
- **Never prune an in-flight rollback's rows.** `protected_ops_fragment` excludes any op in `rolling_back` (the original,
  which a live rollback streams across successive read connections) and its `rolls_back_op_id` target; the unfinished
  inverse is separately excluded by the `ended_at IS NOT NULL` gate. This closes the Finding 6/7 race without a long read
  transaction.
- **Size budget is measured as live bytes.** `live_size_bytes = (page_count - freelist) * page_size` — the size the file
  would have after a full vacuum — so the delete loop makes progress before pages are physically reclaimed (each delete
  grows the freelist). It stops when under budget or nothing prunable remains (everything left protected).
- **Dir GC — the referenced-plus-ancestors closure.** Interning keeps a dir row forever, so pruning ops alone leaves a
  monotonically-growing `dirs` floor. GC iterates leaf-up: delete dirs referenced by no item AND no child dir, repeat
  until stable — exactly the complement of the referenced-dirs-plus-their-ancestors closure, so a referenced dir's whole
  parent chain survives (path reconstruction walks it to the root).
- **Reclaim.** An age-only prune runs one **bounded** `incremental_vacuum` slice (tiered `pick_vacuum_cap`, mirroring
  `indexing/writer/maintenance.rs`) and lets the periodic timer drain the rest over ticks, never stalling the
  lossless-with-backpressure writer. A **size** prune must actually shrink the file to honor the budget, so it drains the
  ENTIRE freelist (`reclaim_fully`, ignoring the cap floor) then `wal_checkpoint(TRUNCATE)` so the truncation reaches the
  physical file (in WAL mode `incremental_vacuum`'s page-count drop otherwise lands only in the WAL). Importance sets
  `auto_vacuum = INCREMENTAL` but never calls `incremental_vacuum`; this DB must, or it grows unboundedly. Both slices go
  through `crate::sqlite_util::run_incremental_vacuum` because `incremental_vacuum` frees one page per step — see the
  canonical gotcha in [`../indexing/DETAILS.md`](../indexing/DETAILS.md) § row-yielding pragmas.

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
