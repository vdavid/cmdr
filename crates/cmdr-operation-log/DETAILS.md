# Operation log — details

Depth for the `cmdr-operation-log` crate. Read this before any non-trivial work here: editing, planning, reorganizing,
or advising. Must-knows and the module map: `CLAUDE.md`. This doc captures what shipped and the durable rationale a
future agent needs on hand, including every decision behind it. App paths named here (`commands/`, `mcp/`,
`operation_log_host/`) are under `apps/desktop/src-tauri/src/`; the write engine that journals into the log is
`crates/cmdr-operations/`.

## Must-knows, in full

`CLAUDE.md` keeps each of these as one line; this is the full text, with the why each line compresses.

- **The log never names its host**: values or traits come in. Planner end-to-end cells:
  `crates/cmdr-operations/src/rollback/planner_tests/`. § "The crate boundary".
- **DURABLE and MIGRATES; never delete-and-recreate on a version bump.** The ladder (`store/migrations.rs`) is the
  template future durable DBs follow: append a `Migration`, NEVER edit or renumber a shipped step. A downgrade is
  refused; only an unparseable file is rebuilt (typed sqlite code, ❌ not a string).
- **NO `platform_case` collation.** A precomputed `name_folded` column (Unicode-lowercase + NFC) plus plain b-tree
  equality keeps the file `sqlite3`-openable.
- **One writer thread, one cross-volume DB, NO per-volume registry.** `record_items` BLOCKS under backpressure
  (lossless); a DB error drops that ONE row, and the completeness check degrades the op rather than under-reversing it.
- **Classification is typed end to end**: every `kind`, `initiator`, status, `row_role`, `outcome`, and
  `rollback_skip_reason` is a `types.rs` enum with a stable token, mapped ONLY there. Renaming a token is a schema
  change; renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** That reasoning lives in `capture.rs`. A driver
  may also NOTE a reason the rows can't express (`note_not_rollbackable`, say a directory merge), and a note beats the
  per-kind rule.
- **Capture is the operation's `OpJournal`, fixed at `journal_open` and carried in the write engine's op scope** (❌
  never extend `OperationEventSink`).
- **Recording on/off is decided ONCE per op, at open** (`recording.rs`); ❌ never mid-op (half-journaled lies).
- **A multi-operation undo reverses NEWEST FIRST** (`undo_order`): a later rename batch can take a name an earlier one
  freed. Callers pass ids in APPLY order.
- **Rollback FAILS SAFE** (data-safety-critical): recheck each item against its snapshot AND its restore target (a copy:
  its original; a trash: the node it landed as); drift, unverifiable, or occupied target ⇒ SKIP (→
  `partially_rolled_back`), never operate, and a restore-move never overwrites. `verify_snapshot` + `SkipReason` +
  `SkipTally` are SHARED with the in-flight reversals (`crates/cmdr-operations/src/reversal.rs`): ❌ don't fork them.
  `rolling_back` guards double-rollback and the retention race.
- **The engine PLANS; an injected `RollbackRunner` acts** (this crate can't reach the executor). Pause parks BEFORE the
  item is verified (a stale verification must never authorize a destructive act), and a reversal's "should I stop?" is
  NOT `is_cancelled` (`StopMeans`). § "Planner here, executor injected".
- **Op ids are UUIDv7 (`new_operation_id`), never v4**: same-second ops order by `op_id`, and only v7 sorts by time.
- **The journal records what SURVIVES an operation, whatever its terminal state**: a canceled copy's files AND the dirs
  it created, never the mid-write partial (both terminal paths remove it).
- **Search spans every `row_role`; retention prunes whole ops only.** Name search matches `source_name_folded` across
  `rollback_unit` AND `search_only` rows, so a leaf hits inside a trashed folder. Retention never prunes a
  `rolling_back` op or its target.

## What this is

A durable, cross-volume journal of every file mutation Cmdr performs: one `operations` row per user-level batch (1:1
with the pipeline's `operation_id`), many `operation_items` rows beneath it. It answers "what did I do to my files, and
can I undo it?" — provenance, rollback, indexed name search, and retention. Shipped: the durable store, capture at the
chokepoint, the rollback engine, the read/search API + retention enforcement, and the MCP tools (`operations_list` /
`operations_get` / `operations_rollback` — see `mcp/DETAILS.md`). The UI (the Debug panel and the alpha dialog) builds
on the read side.

## Why a separate durable DB

The index's stores are per-volume and delete-and-recreate on any schema change; the biggest, the drive index, lives in
`~/Library/Caches/` (Time Machine skips it, the OS may purge it; `crates/cmdr-index/DETAILS.md` § "Where the stores
live"). A mutation history is the opposite — valuable user data that must survive for years and span volumes (a copy
from disk A to disk B is ONE operation with one identity). So it's its own `operation-log.db` beside the durable app
data (`resolved_app_data_dir`), which Time Machine backs up normally.

The heavy-churn backup cost is accepted; retention (default 3 GB) bounds it. The named escape hatch, if it ever bites,
is a future "exclude operation log from backups" toggle (an exclusion attribute on the file) — not built, just the
identified reversible lever.

It is itself sensitive: a map of the user's file activity. It stays local, never transmitted (noted in
`docs/security.md`).

## The migration ladder — the reusable template

This is the codebase's **first forward-migration system**, and the template every future durable DB should copy. It
lives in `store/migrations.rs`.

- **Shape.** `MIGRATIONS` is an ordered `&[Migration]`, each `{ version, description, up: fn(&Transaction) }`. `up` for
  version N transforms the schema from N-1 to N (create tables, `ALTER TABLE`, backfill, add an index).
- **The runner (`run_migrations`).** Bootstraps the `meta` anchor table, reads the stored `schema_version` (absent ⇒ 0),
  and for each step newer than the stored version runs `up` + stamps the new version **in one transaction**, oldest
  first. Stepwise commits mean a crash between steps leaves a consistent intermediate version the next open resumes
  from.
- **Never destroy on a version gap.** A stored version _older_ than the ladder migrates up. A stored version _newer_ (a
  downgrade — the user ran a newer build, then an older one) is refused with a typed `SchemaDowngrade` error and the
  file is left untouched, because it may hold data this build can't represent. Recreating is reserved for a genuinely
  **unparseable** file, classified by the typed sqlite error code (`NotADatabase` / `DatabaseCorrupt`), never a message
  string. The file goes to the host's `set_aside` first (the app keeps a copy and tells the person, `state_recovery`); a
  caller that keeps nothing (tests and tools pass `&|_| None`) lets it be deleted.

Rules for adding a migration: append a step with the next version; **never edit or renumber a shipped step** (users' DBs
already ran it — editing it silently diverges their schema); write `up` against the schema the prior steps produce, not
the latest Rust structs.

**v2 (`migrate_v2_rollback_skip_reason`) is the ladder's first appended step**, and the worked example of the rules: one
`ALTER TABLE operation_items ADD COLUMN rollback_skip_reason TEXT`, nullable, no default, **no backfill**. Not
backfilling is the decision, not a shortcut: a v1 row's skip has no recorded reason, so any value picked for it would be
a fabricated fact in the user's own history. NULL reads as "reason not recorded" everywhere (`Option<SkipReason>` on the
read side), which is what actually happened.

**Why no delete-and-recreate here** (the whole point): the index/importance caches wipe on a `SCHEMA_VERSION` mismatch
because their data is regenerable derived state. The operation log's data is the user's history — wiping it loses
undo-ability, which is exactly what makes the log valuable. So it migrates.

## Case folding, not a collation

The index and importance stores register a `platform_case` collation for case-insensitive `UNIQUE`, which is why the
`index-query` tool exists at all (stock `sqlite3` can't read those columns). The operation log wants
`sqlite3`-inspectability, so it stores a precomputed `name_folded` column instead: `fold_name` lowercases (Unicode) then
NFC-normalizes, once in Rust at insert. It's a _record_ key, so it may differ slightly from a given filesystem's exact
case rules — acceptable for search, not a live mirror. Search queries the folded column with a plain b-tree index.
**Folding is never an identity**: the log can't know a volume's case or normalization rules, and on a case-sensitive one
(SFTP to Linux, S3, Samba with `case sensitive = yes`, case-sensitive APFS) `Media` and `media` are two folders. So the
`dirs` identity is the exact `name` (below), and the rollback's self-collision guard compares exact spellings (§ "The
two data-safety guards").

## Schema

Three tables plus `meta` (the migration anchor). Full DDL: `store/migrations.rs::migrate_v1_initial`. The shape and its
non-obvious choices:

- **`dirs` — interned directory prefixes.** `dir_id`, `volume_id`, nullable `parent_dir_id`, `name`, `name_folded`. A
  1M-file operation under one tree shares a handful of directories; interning stores each hot dir once, forever (reused
  across operations), and item rows reference a `dir_id` + a leaf name — so the DB and its backup don't bloat with
  repeated path prefixes. A **volume root** is a single row with `name = ''` and NULL parent, so a file directly at the
  root still has a dir to reference; `reconstruct_dir_path` walks `parent_dir_id` to it.
  - **Identity gotcha.** SQLite treats NULLs as distinct in a plain `UNIQUE`, so
    `UNIQUE(volume_id, parent_dir_id, name)` would fail to dedupe root-level dirs (NULL parent). The identity is a
    **unique expression index** on `(volume_id, IFNULL(parent_dir_id, 0), name)` instead — dir ids start at 1, so 0 is a
    safe stand-in for a NULL parent. `intern_dir` inserts with `ON CONFLICT` on that expression, then reads the id back.
  - **The identity is the EXACT name, never `name_folded`** (schema v3). A folded identity merged `/srv/Media` and
    `/srv/media` into one row spelled whichever came first, so on a case-sensitive volume a rollback looked in, or
    deleted from, the wrong twin (#527). `name_folded` stays as a plain column for search. On a case-insensitive volume
    two spellings of one folder now intern twice, which costs a row and nothing else: both resolve to the same folder.
    **Rows merged before v3 stay merged**: only the first spelling was kept and nothing recorded which items meant the
    other, so there's nothing to split them by. An item recorded under the lost spelling still resolves to the first
    one. Pinned by `intern_dir_keeps_case_and_normalization_twins_apart` and
    `v3_migration_keeps_merged_dirs_and_stops_merging_new_twins`.
  - **A remote volume's root is ONE name.** A remote path is rooted at `scheme://authority` (`adb://serial/…`,
    `sftp://ada@nas:22/…`, `smb://nas/share`), and `intern_dir` keeps that head whole as the first name under the
    volume-root row (`split_scheme_root`); `reconstruct_dir_path` renders a chain led by such a name without the leading
    `/`. Splitting it on `/` like the rest drops the empty segment inside `://`, so `adb://serial/x` came back as
    `/adb:/serial/x`: a path no volume answers to, in the item views AND in the `RollbackUnit` the rollback hands to the
    volume. No real folder name can hold `://` (a name has no `/`), so the rule can't misfire on a local path. Rows
    written before it keep their mangled chain. Pinned by `intern_dir_round_trips_a_scheme_rooted_path`.
- **`operations` — one row per batch.** `op_id` (the pipeline UUID), typed `kind` + nullable `archive_subkind`,
  `initiator`, the two-axis status (`execution_status` + `rollback_state` + nullable `not_rollbackable_reason`),
  `rolls_back_op_id` (the rollback linkage), source/dest volume ids, timestamps, `item_count` (the **planned** total,
  informational — NOT the completeness yardstick), `items_done`, `bytes_total`, `search_coverage` +
  `search_coverage_reason`, and an optional dev-only `dev_summary`. **No stored rendered summary**: the UI label is
  formatted client-side from the typed fields so it localizes per viewer; `dev_summary` is dev-only and never shown in
  the alpha dialog.
- **`OperationRow.inverse_op_id` is read-side only, and every reader fills it.** It's the mirror of the stored
  `rolls_back_op_id` (the newest operation reversing THIS one), resolved through the partial `operations_rolls_back`
  index by `store::fill_inverse_op_ids` right after the mapper runs, because the mapper holds a `Row` and not the
  connection. Every reader that hands an `OperationRow` out calls it, so a `None` always means "nothing reverses this"
  rather than "this reader did not look". While a row reads `rolling_back` the field names the LIVE reversal, which is
  what lets the history dialog offer it Pause and Cancel without a second round trip.
- **`operation_items` — per-item rows.** `seq` (order within the op, for grouped display and reverse-order rollback),
  typed `entry_type` (file/dir) and `row_role` (`rollback_unit` / `search_only`), interned `source_dir_id` +
  `source_name` (+ folded) and nullable dest equivalents, `size`, `mtime`, typed `outcome`, `overwrote`, and the
  nullable typed `rollback_skip_reason` (v2). That last column has exactly ONE writer — the rollback engine, on the
  original op's rows, alongside `outcome = skipped` — so its vocabulary can't drift: a `Skipped` outcome written by a
  mutation path means something else entirely and correctly reads NULL, as do pre-v2 rows and the inverse op's mirror
  rows (the reason is a property of the item the engine examined, and the inverse row reaches it via
  `rolls_back_op_id`). The plain name `skip_reason` stays free for a future "why did the OPERATION skip this item". No
  index: read per row, never filtered on. The nullable `dest_inode` + `dest_birth_ns` pair (v4) is the
  `cmdr_fs::volume::NodeIdentity` the item landed as at its dest, both set or both NULL, the inode bit-cast to a signed
  integer; only a trash writes it (§ "A trashed item must still be the one trashed"). Directories the op created are
  **first-class `dir` rows** sequenced after their contents, so a `seq DESC` rollback removes files before the dirs that
  held them. Names are indexed folded (search) and, unlike dirs, **not interned** — a b-tree handles the massive
  duplication across a large op fine, and names must stay directly queryable.

## The writer (`writer.rs`)

One dedicated OS thread owns the single write connection; the cloneable `OperationLogWriter` handle is the only way in.
Message surface: `OpenOperation` (insert the header, `Running`), `RecordItems` (batched insert of a slice in one
transaction, interning dirs + folding names), `FinalizeOperation` (write terminal state, return per-`row_role` durable
counts), `Prune` (retention), `Flush` (barrier), `Shutdown`.

- **Lossless with backpressure.** The channel is a bounded `sync_channel`; `record_items` blocks briefly if the writer
  falls behind rather than dropping. Safe for "logging never slows an op": a batched row insert is far cheaper than the
  per-item file I/O the op already does, so the writer outpaces every real op and the block is a theoretical backstop. A
  DB _error_ on one row (not fullness) logs and drops THAT row — the op never fails for a journal problem.
- **Completeness accounting is the writer's contribution, not its judgment.** `finalize_operation` returns
  `FinalizeOutcome { rollback_unit_rows, search_only_rows }` — the durably-written counts per role. The **capture
  layer** compares them against the `record_item` calls it actually _issued_ (items reached — NOT the planned
  `item_count`, which a canceled op never reaches) and, on a shortfall, degrades: a missing `rollback_unit` row ⇒
  `not_rollbackable(journal_incomplete)`; a missing `search_only` row ⇒ `search_coverage = top_level_only`
  (`search_row_incomplete`). The writer supplies the numbers; it does not itself compute eligibility.
- **Eligibility lives upstream.** The net-new flag and `archive_edit` subkind the capturing driver knows feed the
  capture layer's eligibility computation, which passes the already-typed `rollback_state` + reason + subkind into
  `FinalizeOperation`. `writer.rs` stores what it's given — keeping data-safety-critical business logic in the TDD'd
  capture/rollback layers, not the writer. (This is a deliberate divergence from a "finalize computes eligibility"
  reading of the plan: storing the terminal state the caller computed keeps the writer single-responsibility and avoids
  a dead net-new field it would ignore.)

## Capture — journaling every mutation at the chokepoint

`capture.rs` is the journal half of the pipeline observer seam; the write pipeline's glue lives in
`crates/cmdr-operations/src/journal.rs`. Together they make every managed mutation journal itself with per-item rows,
two-axis status, and computed rollback eligibility — without measurably slowing the op.

### The seam: an operation's journal, fixed at open

**The journal lives in an `OperationLog` handle** (`handle.rs`): its slot, the recording gate, and the writer. The app
builds ONE (`operation_log_host::install`, first thing in `setup`), hands it to the write engine
(`operations_host::install`), and opens it late (`OperationLog::open`, from `operation_log_host::start`).
`OperationLog::journal_open` returns an `OpJournal`, the operation's journal fixed at open, and the write engine keeps
it in the operation's scope (`OpScope`, `crates/cmdr-operations/src/scope.rs`) for every record, note, and the finalize.

- **`open` publishes last.** It migrates the store, spawns the writer, reconciles the operations a crash left
  `rolling_back`, and only THEN puts the journal and the writer in the slot, in one write. So nothing can journal, and
  no rollback can be dispatched, into a reconcile. Once per handle: a second `open` is `AlreadyOpen` and starts no
  second writer, and a failed one leaves the handle without a journal (the app runs without history rather than refusing
  to start).
- **An operation is journaled wholly or not at all, decided at open.** One opened before the journal was (or while
  recording is off) holds an empty `OpJournal`, so every later record, note, and finalize no-ops even if the journal
  arrives mid-op: a tail and a finalize sent to a journal that never opened it would be rows with no header. The type
  carries it, so nothing has to remember. Pinned by `handle/tests.rs`.
- **Why it rides the operation's scope, not the sink.** The write engine already carries one `OpScope` per operation
  (its seams and status cache), so the journal joins it there and reaches every record point the state reaches. The hard
  constraint holds: the sink stays a sink, and nothing here extends `OperationEventSink`.
- **Testability holds.** Every test builds its own handle, so none needs a lock. A test of the operation log builds an
  `OperationLog` directly; a write-pipeline test builds an engine journaling into a temp-DB log of its own
  (`cmdr_operations::testing::TestOperationLog`, `crates/cmdr-operations/DETAILS.md` § "Test isolation: an engine of the
  test's own"). A test that drives an engine function directly opens the journal on the same state's scope it passes in.
- **Lifecycle.** `journal_open` is called when the op actually STARTS (inside the manager's deferred), not at
  registration, so a queued op that's canceled before admission never journals and leaks no accumulator. The finalize
  removes the per-op accumulator.
- **Header aggregates (`item_count` / `items_done` / `bytes_total`).** `open` writes a provisional `item_count` (the
  top-level source count, known before the scan) plus the destination volume; `finalize_op` then refines the three
  aggregates from the live `OperationStatus` cache (the same one the queue UI drives — `header_totals_from_status`), so
  a finished op carries the scanned leaf total and completed count, not zeros. When no status row exists (an instant
  create, an in-ZIP edit, or a direct-call test), `item_count` stays at the open value and `items_done`/`bytes_total`
  stay 0. A fresh compress is the one archive op with real aggregates, and it hands them over itself
  (`finalize_archive_op`'s `PackedTotals`: planned entries, entries written, uncompressed source bytes), because the
  status cache reads zero by then (both finishing phases clear their totals). Its header also names the SOURCE volume
  (`open_compress_op`), where an in-ZIP edit names the archive's volume on both sides. **The unit differs by kind,
  matching that kind's row granularity**: a copy or cross-FS move reports scanned LEAF files, a same-FS move reports
  top-level ITEMS (one rename-back reverses a whole subtree, so that's what its rows count). `move_with_rename` writes
  the cache itself — no scan seeds it and its renames are instant — which is also what keeps a finished move from
  journaling `items_done = 0`. These fields are informational (the alpha dialog renders "Copy N items" from
  `item_count`), NOT the rollback yardstick — completeness still compares ISSUED vs written per `row_role` (above).

### The decisions the capture layer owns (the writer doesn't)

- **Eligibility, `compute_eligibility`** — pure, tested in isolation: copy/move rollbackable iff nothing overwrote;
  delete never (`permanent_delete`); trash/rename/create-folder/create-file open rollbackable (rechecked at rollback
  time, in the rollback engine); compress rollbackable iff net-new (`archive_overwrite` otherwise); zip-inner edit not
  yet (`zip_edit_unsupported`). `execution_status` is deliberately NOT an input — a failed/canceled op stays
  rollbackable for what it reached.
- **A driver's note beats the rule, `note_not_rollbackable`** — some reasons aren't derivable from the item rows at all,
  so the DRIVER states them and `finalize` takes the note over `compute_eligibility`. Both of today's reasons are
  move-shaped: `directory_merge` and `staged_conflict_resolved` (see §§ "Why a directory merge isn't reversible" and "A
  staged write records where the file will LIVE"). First note wins, and it's op-wide, like every other eligibility
  input: one merged item makes the whole operation unreversible.
- **Completeness, `apply_completeness`** — the per-`row_role` issued-vs-written check. The `WriterJournal` accumulates
  the count of `record_item` calls it ISSUED per role; `finalize` compares them to the writer's durable counts and, on a
  shortfall, downgrades: a missing `rollback_unit` row ⇒ `not_rollbackable(journal_incomplete)` (a lossy journal must
  never claim rollbackability); a missing `search_only` row ⇒ `search_coverage = top_level_only`
  (`search_row_incomplete`). Compared against ISSUED, never the planned `item_count`. The correcting re-finalize fires
  only on a real drop, so it's rare.

The `WriterJournal` also **batches** rows (a per-op buffer flushed at `RECORD_BATCH` or finalize, so a huge op coalesces
into batched writer transactions) and **auto-assigns `seq`** in recording order, so record points never track it.

### Why a reader waits for `write-settled`

**A reader can only trust an op's rows once `write-settled` has fired**, and that is a consequence of the batching
above, not a nicety. An op smaller than `RECORD_BATCH` (512 rows) has its whole tail sitting in the in-memory buffer
until `finalize` flushes it, and every write path emits its terminal event (`write-complete` / `write-cancelled` /
`write-error`) from inside the handler, BEFORE `journal::finalize_op` runs. `finalize_operation` then blocks on the
writer thread's reply, so it doubles as the durability barrier, and only after it returns does the `WriteSettledGuard`
drop and emit `write-settled`. So a frontend reading `get_operation_log_detail` on a terminal event sees an EMPTY page
for any ordinary-sized op. The frontend side of this contract lives in
`apps/desktop/src/lib/file-operations/settled-operations.ts`.

### Per-kind record points and granularity

Each point is where the op already stats the item, so journaling is near-zero marginal cost (no new syscalls):

- **copy** — per-leaf `rollback_unit` rows at `transfer/copy/single_item.rs` (right where each file commits to the
  `CopyTransaction`, carrying the free source `mtime` + the overwrite flag), plus the **created-directory rows** from
  `CopyTransaction::created_dirs`. Files record during the op, dirs after, so dir `seq` > their contents' `seq` the
  rollback engine removes files before dirs. The dir rows land on EVERY terminal path, not only the successful one — a
  canceled copy keeps the directories it created, and without their rows a reversal puts every file back and leaves the
  destination's empty skeleton standing. `copy/mod.rs` routes all its terminal arms through one
  `commit_journaling_created_dirs`, so the next arm added can't forget; `volume/copy.rs` journals once post-loop, before
  it branches on the terminal state.
- **delete** — per-leaf at `delete/walker.rs` and `delete/volume_walker.rs`, one row per file, deliberately (a 1M-file
  delete journals ~1M rows on the order of tens-to-~150 MB — leaf search is the requirement, and retention (§
  "Retention") manages the cost, NOT a row cap). Delete is never rollbackable, so these rows exist purely for "when did
  I delete `dog.jpg`".
- **same-FS move + trash** — the **top-level** `rollback_unit` row (one rename-back / one restore reverses the whole
  subtree) at `transfer/move_op/` / `delete/trash.rs`. A move records where the item LANDED (a conflict can send it
  aside to a fresh `name (N)`), for the unit row and the search-leaf rebase alike; recording the name that was taken
  aims the reversal at a file the operation never touched. A merge is not reversible at all — see § "Why a directory
  merge isn't reversible". Trash also captures the OS `resultingItemURL` (the in-trash location) as the row's dest, so
  the rollback restore knows where to move it back from. The subtree's `search_only` leaves come from the **drive
  index**, not a filesystem walk — see § Search-leaf enumeration.
- **cross-FS move** — per-leaf via `copy_single_item` (it stages a copy), same as copy; the op's `kind` is `move`. The
  rows name the FINAL destination, never the staging path, and a phase-3 conflict makes the op not-rollbackable — see §
  "A staged write records where the file will LIVE". It records **created-directory rows** too, rebased through the same
  `staging_dir → destination` arithmetic phase 4's flush already uses; without them a reversal restores every file and
  leaves the moved folder's empty skeleton at the destination.
- **batch rename** (Ask Cmdr's reviewed plans, `rename/bulk.rs::record_bulk_rename_outcomes`) — one `rollback_unit` row
  per APPLIED row, source → destination, with the size + mtime the review preflight already fingerprinted (no new stat).
  **The mtime crosses a unit boundary here and `journal_snapshot` owns the conversion**: a local fingerprint holds
  NANOseconds (`Metadata::modified` as `u128`), while this column and `verify_snapshot`'s live counterpart
  (`FileEntry::modified_at`) are whole Unix **seconds**. Floor-divide, matching `Duration::as_secs` on the read side,
  and convert with `try_from`, never a saturating `as` cast. Journal nanoseconds and every undo reports drift and
  refuses, silently disabling undo; journal `None` and identity rests on size alone, so a same-size replacement file
  gets renamed back in place of the original. A remote fingerprint already holds seconds and passes through untouched; a
  backend that reports no mtime records `None` and falls back to the size-only check.
- **compress** (`archive_edit`) — spawns directly (not through `start_write_operation`), so `copy_into.rs`'s deferred
  carries its OWN open/finalize bracket. The compress driver supplies the `archive_edit` subkind + a net-new flag
  (probed before the seed overwrites the target) via `ArchiveProvenance` — the journal can't derive them, both compress
  and zip-inner edit share durable `OpKind::ArchiveEdit`, while live progress identifies compress separately. A net-new
  compress records the created archive as its single `rollback_unit` item (with a size/mtime snapshot for the rollback
  drift recheck) and finalizes `rollbackable`; an overwrite of a prior archive is `not_rollbackable`; a plain
  into-archive edit journals its header only (not rollbackable in v1).

### Search-leaf enumeration (`crates/cmdr-operations/src/journal_search.rs`)

For same-FS move + trash, the subtree's descendant leaves are read from the DRIVE INDEX (zero extra filesystem I/O,
through the engine's `OperationIndex::search_leaves`, whose app answer and its gates live in `operations_host/index.rs`)
and recorded as `search_only` rows so "when did I trash `dog.jpg`" hits inside a trashed folder. Two hard ordering
rules:

- **Enumerate BEFORE the OS mutation**, buffered in memory: the index reconciler prunes the subtree the instant it sees
  the trash/rename FSEvent, so a later read would find the rows gone and wrongly stamp `full`.
- **Persist only AFTER the top-level item succeeds**: both loops process per item with partial failure, so a failed item
  must record no leaves (search has no per-item outcome filter — leaves for a never-trashed subtree would return a trash
  that never happened).

`search_coverage = full` is gated on the subtree being PRESENT + CURRENT (`min_subtree_epoch > 0` AND
`== current_epoch`, read via the sanctioned `ReadPool` — never a raw rusqlite dep on the index DB) AND the volume index
`Live` (active + `Fresh`); otherwise it downgrades to `top_level_only` with a typed reason
(`index_absent | index_stale | volume_not_live | capped`). The leaf read is bounded by `SEARCH_LEAF_CAP` (50,000) via
`IndexStore::list_children_on_limited`'s `LIMIT cap + 1`, so a 1M-child folder pays a bounded (~59 ms) synchronous read
before the sub-second rename, not a 1M-row one; over the cap ⇒ top-level row only + `capped` (rollback unaffected — the
top-level row is the undo unit regardless). Numbers + the cap-tuning rationale:
`docs/notes/operation-log-capture-bench.md`.

### Turning recording off (`recording.rs`)

`operationLog.enabled` (default on) is the user's opt-out from recording file activity (CRA Annex I, Part I, (2)(l)).
The registry entry and its Settings row live in `apps/desktop/src/lib/settings/definitions/behavior.ts`; Rust seeds the
gate from `settings.json` in `operation_log_host::start` (`settings::load_operation_log_enabled`, absent ⇒ on) and the
frontend pushes every change through `set_operation_log_enabled` (`OperationLog::set_recording`).

- **The gate sits in the handle, not in a journal.** `OperationLog::journal_open` reads the switch; an op opened while
  off gets an empty `OpJournal`, so every later record / note / finalize through it returns early. The journal stays
  installed, so production and the test journals see one gate, and nothing about the writer or `WriterJournal` changes.
- **Decision/Why: decided once, at open, held to finalize.** An op that opened while on journals every item even if the
  switch flips mid-way; one that opened while off journals nothing even if it flips back on. Gating each record call
  instead would leave a header with only some of its rows, and the completeness check can't catch that: it compares rows
  written against rows ISSUED, and a gated row was never issued. The op would read `rollbackable` while its reversal put
  back only part of what happened. Pinned by `recording/tests.rs`.
- **What keeps working while off**: the writer, retention (it prunes on its own timer through the writer), every read,
  and rollback of operations already in the log. Rollback opens its inverse op through the writer directly, so undoing
  an old entry still records the reversal and moves the original to `rolled_back`; that's the history's own state
  machine, not a new recording.
- **Undo for an unrecorded op is refused, never wrong**: `rollback_operation` finds no row ⇒ `UnknownOperation`. The
  surfaces that offer Undo right after an op (the trash toast and Edit > Undo move to trash, Undo rename, Ask Cmdr's
  rename undo) don't offer it while recording is off (`apps/desktop/src/lib/operation-log/recording.ts`), and the
  history dialog says recording is off.
- **The record path stays cheap while on**: an operation's `OpJournal` already holds its journal, so a record call reads
  no switch and takes no lock of the handle's.

### The bypass boundary

- **`run_instant` ops (rename / mkdir / mkfile)** don't flow through the sink; capture hooks the managed functions
  directly as single-item ops.
- **`paste_clipboard`** (paste-as-file) now runs INSIDE `manager::run_instant` with a `CreateFile` descriptor, so it
  journals a one-item `CreateFile` op for free (the write loop is unchanged, just bracketed by the managed op).
- **The single `move_to_trash_sync` in `rename.rs`** goes through `trash::trash_single_journaled`, journaling a one-item
  trash op that mirrors the batch path (in-trash dest + drive-index search leaves).
- **Volume (SMB / MTP) copy/move/delete + volume `run_instant`** journal through the same seam, but under the REAL
  volume id (not `"root"`). They spawn through their own `volume/copy.rs` / `volume/move.rs` / delete-walker deferreds,
  so each deferred brackets the op with `journal::open_volume_op` / `finalize_op` and the per-item record points call
  the `record_volume_*` siblings of the local helpers. The record points inside the shared `copy_volumes_with_progress`
  / `move_volumes_with_progress` bodies read the `(source_volume_id, dest_volume_id)` off
  `WriteOperationState::journal_volumes` (the deferred sets it) rather than taking new params — those bodies have ~80
  test call sites, and this mirrors how `op_id` already reaches them. `run_instant` (create/rename) unifies local +
  volume via `open_volume_op` with the id (`"root"` for local), so there's no local/volume branch. See § Volume capture
  below.
- **Native drag-out** is explicitly OUT of scope — the destination is another app, outside Cmdr; there's nothing to roll
  back to.
- **Archive move-OUT** (`route_archive_move_out`, a compound extract-then-archive-rewrite) does NOT open an
  operation-log op; its extract phase runs `copy_volumes_with_progress` with no journal target set, so it doesn't
  journal. A copy/move INTO an archive DOES journal (via the compress/edit `ArchiveProvenance` path), but the plain
  into-archive-edit convenience wrapper (`route_archive_copy_into`) defaults its `initiator` to `user` — an
  MCP-initiated into-archive edit records `user`, a minor provenance gap on a rare, already-`not_rollbackable` path.

### Volume capture — carrying the real volume id + honest overwrite

The real volume id is the thing the volume paths need that the local paths don't (overwrite detection is every transfer
path's job, local included — see § "Why a directory merge isn't reversible"):

- **The real volume id, threaded via op state.** `journal.rs` gained `open_volume_op` + `record_volume_leaf` /
  `record_volume_transfer_source` / `record_created_dirs_on` / `record_search_leaf` — the volume-aware siblings of the
  `_local_` helpers, taking explicit source/dest volume ids. The deferreds call `open`/`finalize` directly (they have
  the ids); the per-leaf points inside the shared `*_with_progress` bodies read `WriteOperationState::journal_volumes`
  (`Some((src, dst))`, set by the deferred; `None` in tests / the both-local shortcut ⇒ no journaling). Honesty
  invariant, TDD-guarded: a volume op's rows carry the real id, never `"root"` (a wrong id silently corrupts history —
  `volume_copy_journals_under_the_real_volume_ids_not_root`, `cross_volume_move_journals_per_leaf_move_rows`).
- **How the volume paths surface overwrite.** A copy/move that overwrote isn't rollbackable (deleting the copies can't
  restore the overwritten original). The top-level file→file safe-replace is known at the call site; deep-merge child
  overwrites are counted in `CreatedPaths::overwrote_files` (copy/cross-volume-move) or a `RenameMergeCtx::overwrote`
  flag (same-volume move), and the same-volume resolver records a top-level file overwrite in a shared
  `overwritten_sources` set (it runs in a separate driver callback from the record point). The recorded row's
  `overwrote` bit is the OR of these; `compute_eligibility` reads it op-wide. Per-inner-file granularity isn't tracked
  (op-wide eligibility is all that's consumed). The local paths carry the same bit on their own rows
  (`transfer/move_op/same_fs.rs`'s `item_overwrote`, `copy_single_item`'s `needs_safe_overwrite`).

**Per-source at completion, AND at interruption.** The volume drivers journal one top-level source's leaves when that
source finishes. A directory source stopped mid-stream used to journal nothing, leaving every child it had already
copied on disk and nowhere in the ledger — so a reversal from history couldn't touch them. Both drivers now journal from
their failure arms too (`copy_serial.rs`'s `Err` arm, `copy_concurrent.rs`'s `record_failure`), off the same
`CreatedPaths` they already thread out for the in-flight rollback. The invariant to hold: **every destination file that
survives the operation has a row.** The partial the driver was mid-write on is not one of them — both terminal paths
remove it, so journaling it would name a file that isn't there.

Granularity mirrors local (§ "Per-kind record points and granularity"): cross-volume copy + cross-volume move + volume
delete are per-leaf; a same-volume move is a same-FS-style move (top-level `rollback_unit` row + drive-index
`search_only` leaves, which downgrade to `index_absent` on a volume with no index — verified by the gate in
`enumerate_subtree_for_search`).

### Why a directory merge isn't reversible

A move whose source folder collides with a folder at the destination MERGES, child by child, and journals ONE
`rollback_unit` row whose destination is that pre-existing folder. That folder also holds files this operation never
touched, so reversing the row (a rename-back of the whole folder) would carry them to the source and leave nothing at
the destination. Both merging paths therefore call `journal::note_not_rollbackable(DirectoryMerge)` at the merge branch:
`transfer/move_op/` (same-FS and the cross-FS phase-3 merge) and `transfer/volume/move_same.rs` (same-volume
rename-merge).

**The disqualifying condition is "merged", NOT "overwrote something".** A merge that overwrites nothing has the same
single row and the same failure: pre-existing `B/A` holding a dest-only `x`, source `A/` holding only `y`, the merge
moves `y` in, the emptied source is removed, and the reversal takes `x` away. Flagging only merges whose children
replaced a dest file leaves that live. Pinned by a pair of tests per path (`move_op/move_journal_tests.rs`,
`volume/rename_merge_tests.rs`), one shape each.

**Decision/Why**: the honest flag over per-child rows, for now. Per-child rows (one row per child the merge actually
moved) are the better end state — they'd let the reversal put back exactly what moved and nothing else, and "we merged
into your folder so we can't undo it" is a poor answer for a common operation. They cost a real redesign of the merge
record point on three paths; the flag is honest today. Revisit when merge-undo is worth that.

### A staged write records where the file will LIVE

A cross-FS move copies into `.cmdr-staging-<op_id>/` and renames the tree into place in phase 3, so the paths
`copy_single_item` writes are gone moments later. Journaling them would leave name search pointing into a directory that
no longer exists, and a reversal would read every item as already gone and count a phantom success. So the caller hands
`copy_single_item` a `JournalDestUnder { write_root, final_root }` and the record points rebase
(`transfer/copy/single_item.rs`). Rows land exactly when they always did — only WHAT path they carry changed. A plain
copy passes `None`: it writes where it records.

The rebase is the conflict-free answer, and phase 3 can still resolve a conflict at the real destination after phase 2
journaled. So **any phase-3 conflict resolution marks the operation not-rollbackable**: `Overwrote` when it replaced the
destination (phase 2 can't see that overwrite at all, since nothing collides inside a fresh staging directory),
`DirectoryMerge` for a merge, `StagedConflictResolved` for a rename-aside or a Skip. Without that, a row would name a
path holding a STRANGER's file, and since a move's inverse is a restore-move, a size-and-mtime match (plausible for a
duplicate) would carry that stranger's file off to the source. **Known limit**: those rows still name the naive final
path, so history and name search for such an operation point at a location the file isn't at. Correcting them needs an
amend-a-row writer message; the not-rollbackable flag keeps it from doing harm meanwhile.

### Provenance — initiator threads through every write-start command

Every write-start command now carries an optional `initiator` (default `user`): the local commands (provenance
threading) plus the volume commands (`copy_between_volumes`, `move_between_volumes`, `compress_files`) and the
`run_instant` commands (`create`, `rename`). The FE `mcp-listeners.ts` tags MCP-originated write dispatches `ai_client`
(threaded through the typed command bus alongside `autoConfirm`/`onConflict`, mirroring navigation's `source: 'mcp'`).
The one gap: an into-archive-edit via a volume command defaults to `user` (see the bypass boundary above).

### Row-volume tradeoff

Per-leaf delete/copy rows are the search requirement, so there is **no row cap** on them; the only cap is the
`search_only` leaf enumeration for trash/same-FS-move (search-leaf enumeration, a per-op tunable). Retention (§
"Retention") reclaims the space, not a row cap.

## Rollback — reversing an operation as recheck-then-act inverses

`rollback.rs` is the data-safety-critical engine that undoes a journaled operation. It never runs a bespoke reversal:
each item's inverse is applied through the `Volume` trait (so local and remote reverse uniformly), and the whole inverse
is itself a journaled operation linked back via `rolls_back_op_id`. The write-pipeline glue that spawns it as a
cancelable managed op is `crates/cmdr-operations/src/rollback.rs::dispatch_rollback`.

### Planner here, executor injected

`rollback.rs` **plans**: it pages the journal, verifies each item, decides the inverse act, maps every failure to a
typed skip reason, and keeps the books. It performs nothing. It looks volumes up through an injected `VolumeLookup`
(`rollback/runner.rs`), which the app answers with its volume registry
(`apps/desktop/src-tauri/src/operation_log_host/volumes.rs`). Each decided act goes to an injected `RollbackRunner`
(`rollback/runner.rs`), whose production implementation is `cmdr_operations::rollback::ReversalRunner`. An act that
didn't happen comes back as an `ActRefusal`: the volume's raw `VolumeError`, which the planner maps to a skip, or
`Locked`, which `ReversalRunner` answers for a verified LOCAL item whose removal was refused and whose lock it can't
clear (`schg`). Before that answer it clears the item's Locked flag and ACL and retries the delete once, since a copy
keeps both and either makes `unlink` refuse, then opens the folder holding it (a copied folder's Locked,
`deny delete_child`, or `0500` refuses it too) for one more retry and puts the folder's metadata back; it does this in
the runner, ❌ never in the volume, whose delete is also F8's (`crates/cmdr-operations/src/transfer/DETAILS.md` § "What
a reversal does with that identity").

Why the seam is here: the act a cross-volume restore needs is the staged, retrying, stall-detecting per-file transfer,
and that machinery is private to `cmdr-operations`, behind its `transfer::volume` facade — unreachable from this crate,
and it must stay that way (this subsystem holds no file-moving code). Injection buys the reach without the dependency,
the same move the `spawn` hook in `rollback_operation` makes. The loop stays here; its end-to-end cells drive it with
the real executor, so they and their rig live beside that executor
(`crates/cmdr-operations/src/rollback/planner_tests/`), and only the pure decision helpers are tested here
(`src/rollback/decision_tests.rs`).

The runner carries the loop's three live answers as well as its hands:

- **Stop.** `should_stop()`, not `is_cancelled`. See "What stops a reversal" below.
- **Pause.** `wait_while_paused()` parks at the item boundary, **before** the item is verified. Parking between
  "verified unchanged" and "delete" would let a ten-minute-stale verification authorize a destructive act, so the order
  is a data-safety property, not a style choice. The deferred-directory phase polls the stop too (it once polled
  nothing, so a click during a long sweep kept removing folders).
- **Progress.** `report_progress()` once per FILE item, with totals read from the journal
  (`store::read_rollback_file_totals`, one indexed aggregate) before the first act — so a reversal has no scanning phase
  and its bar means something from the first frame. Directory rows are excluded: that phase sweeps empty leftovers
  rather than moving anything. `ReversalRunner` throttles frames to the operation's progress interval and always sends
  the first one and the one that lands on the total; a cross-volume restore also reports its bytes mid-file, off the
  same callback that carries the stop down to the backend.

The inverse operation's **history header** keeps the original operation's user-facing count unit rather than counting
every row the rollback engine walks. It plans against the original's completed count, falling back to its planned count
for instant and directory-only operations that have no file-progress count, then bounds that number by the rollback
units actually reached. File outcomes consume the file slots first; directory outcomes consume only the remaining slots.
Therefore a copied folder containing three files and its successful reversal both say three items, while an empty-folder
copy still says one. A successfully removed housekeeping directory also cannot disguise a skipped file as another
completed header item. The directory rows remain first-class journal and rollback units; only the summary count uses the
original operation's vocabulary.

An item that fails while a stop is pending is NOT recorded as a skip: nothing was left behind for a reason the skip
column exists to explain, and an unrecorded item is one an idempotent retry simply re-attempts.

### What stops a reversal

`IntentCell::is_cancelled` (`crates/cmdr-operations/src/operation_intent.rs`) answers "the intent isn't `Running`", and
`RollingBack` is one of the values it calls cancelled. That's right for a forward transfer. It is wrong for a reversal
running under an operation that is itself rolling back, where the same value is the instruction to reverse: a reversal
inheriting that reading would bail on its first item, return instantly from its pause gate, and report a clean stop
having reversed nothing.

So a reversal names its reading — `StopMeans::IntentLeavesRunning` or `IntentReachesStopped`
(`crates/cmdr-operations/src/operation_intent.rs`), which `PauseGate::wait_while_paused_until` also takes. Today's
journal-driven rollback is dispatched as its OWN managed operation, opened `Running`, so its runner (`ReversalRunner`)
always uses the first. A cancelled transfer's own wind-down (`transfer/copy/rollback.rs`, `transfer/volume/cleanup.rs`)
runs under a `RollingBack` operation and takes a `WindDown` (`crates/cmdr-operations/src/state/wind_down.rs`), which
reads the second and hands out no intent, so `is_cancelled` is out of its reach.

**Known boundary**: the mid-file layer inside `stream_pipe_file` (retry classification, the between-chunks park) still
reads the transfer's meaning off `state.intent` directly. That's correct for every reversal dispatched today, and it's
what running a reversal under the original operation's own wind-down would have to thread through.

### The two data-safety guards

Every item passes two independent guards before anything is touched; failing either SKIPS the item (never operates on
it), feeding a `partially_rolled_back` result.

**Both guards, and this vocabulary, are shared with the IN-FLIGHT reversals** a cancel runs over a transfer's own ledger
(`crates/cmdr-operations/src/reversal.rs`, and `transfer/DETAILS.md` § "What a reversal does with that identity").
`verify_snapshot`, `SnapshotVerdict`, `SkipReason`, and `SkipTally` have one definition each: a user shouldn't meet two
answers to "did this file change". The in-flight side reports its verdicts on the `write-cancelled` event rather than
storing them, and adds a local-only node-id check of its own. This path's node check is narrower: only a trashed item
carries one (§ "A trashed item must still be the one trashed"). ❌ Don't fork any of them.

1. **Snapshot recheck** (`verify_snapshot`). The item must still match the size/mtime the journal recorded. Every
   recorded field must have a present, equal live value; a recorded field whose live counterpart is absent (an MTP/SMB
   mtime the backend can't prove) is **Unverifiable** — a skip, not a guess. Nothing recorded ⇒ Unverifiable too. A
   recorded field that differs ⇒ **Drift** — skip. This is why a leaf that recorded only `size` (a volume leaf whose
   landed mtime wasn't read back) still verifies on size, while an item whose only field (mtime) is absent live is
   skipped. Directories aren't cheaply verifiable, so a dir's recheck is existence-only, plus the node check when the
   row recorded one (a trash).
2. **Pinned non-destructive restore.** A restore-move (move/trash/rename undo) NEVER overwrites: if the restore target
   is occupied by a DIFFERENT entry it skips (`RestoreTargetOccupied`). The one exception is a **self-collision**
   (`src/rollback/self_collision.rs`): restoring `dog.JPG` → `dog.jpg` on a case-insensitive volume sees the target
   "exist" because it IS the same entry, and the restore renames with `force`. It's decided by identity, **never by
   folding names**: the log can't know a volume's case or normalization rules, and on a case-sensitive one (SFTP to
   Linux, S3, Samba with `case sensitive = yes`, case-sensitive APFS) a folded match renamed over a DIFFERENT file that
   had landed at the old spelling since (#511). In order:
   - **Another volume ⇒ never self** (the `same_volume` gate: a cross-volume restore to the same relative path is a
     genuinely different file).
   - **Paths differing by more than case or normalization ⇒ never self**, even on one inode (a hard link under another
     name is still a name we'd replace).
   - **Both node ids known ⇒ they decide.** `FileEntry::inode` is set only by `LocalPosixVolume`, and only for a file
     with more than one link (`cmdr-local/src/reading.rs`), so this is the rare path. Widening it to every entry was
     ruled out: the hard-link dedup walkers (`cmdr-operations`' `scan.rs`, `scan_walker.rs`, `delete/volume_walker.rs`,
     `scan_preview/volume.rs`) key a seen-set on the bare inode with no device, so every file would join that set
     (memory on a 1M-file scan), and same-numbered inodes on two devices under one walked tree would dedupe as one file.
   - **Spelled identically ⇒ self** (the very path the entry sits at).
   - **Otherwise the listings decide** (`listing_confirms_twin`): for each component whose spelling differs, list its
     parent (sliced from the landed path, so a remote `scheme://` root survives) and compare EXACT names. Only the
     landed spelling listed and the target spelling absent ⇒ the volume folded the target onto the landed entry ⇒ self.
     Both listed ⇒ two entries ⇒ `RestoreTargetOccupied`. Neither, only the target, or a failed listing ⇒ unproven ⇒
     occupied too. The same rule covers NFC/NFD-only differences and a move between case-twin folders. It costs one
     listing per differing component, and only when the target is already occupied.

   Pinned by `rollback/decision_tests.rs` and `crates/cmdr-operations/src/rollback/planner_tests/case_twin_tests.rs`:
   three case-sensitive `InMemoryVolume` cells that must skip and keep the occupant's bytes, and a macOS cell on the
   real (case-insensitive) temp dir that must restore a case-only file and folder rename.

### What counts as already gone

An item a volume answers `NotFound` about (or `exists` false) is `SkipReason::AlreadyGone`, which counts as reversed:
the end state the undo wanted already holds. That's only true for a path the volume still serves. `SftpVolume` and
`WebdavVolume` refuse a path above their root as `NotFound` too, and a remote place's root can be narrowed after the
operation ran (`network/DETAILS.md` § "Editing a connected place"). Reading that refusal as "gone" reported a clean undo
that left every copied file on the server (seen on a real NAS, 2026-09-11).

So `missing_item` checks every side an item touches (both ends of a restore) against `volume.root()`, by whole path
components, and a path outside one is `SkipReason::Failed`: the op lands `partially_rolled_back`. A path inside the root
keeps the idempotent no-op. It fails safe in the one other shape that reaches it, a local share whose active mount root
moved to a second mount: a path under the old mount reads as outside, and the item is reported unreversed rather than
counted. Pinned by `planner_tests/root_reach_tests.rs`, where `RootFenced` (`planner_tests/test_support.rs`) stands in
for the refusing volume, since `InMemoryVolume` serves any path it holds.

**A restore-move whose item is gone asks the OTHER side before calling it done** (`restore_move`). Back at its original
path (an earlier undo, Finder's Put Back) is the end state the undo wanted: `AlreadyGone`, credited. Absent from BOTH
sides (the trash was emptied, or the file was deleted since) is `SkipReason::Vanished`, a real skip, under the same
root-reach rule (`vanished_item` wraps `missing_item`). Crediting it, as this once did, made Edit > Undo move to trash
toast "Put back 3 files." over an emptied trash. Pinned by `trash_rollback_skips_when_trash_was_emptied` and
`trash_rollback_credits_an_item_already_put_back`.

### A copy's original must still be there

Undoing a copy deletes the copy permanently (`Volume::delete`, never the Trash), and "the copy is redundant" only holds
while the original exists. Copy photos off a card, delete them there, then roll the copy back: the undo removed the only
copy left (#518). So a copied file's inverse is `RemoveCopyIfOriginalStays`: after the copy verifies against its
snapshot, `original_stays` stats `unit.source_path` on `unit.source_volume_id` and removes the copy only when the
original is there, a file, at the recorded size.

- **Gone, a directory now, or a different size** ⇒ `SkipReason::OriginalGone`: what was copied exists nowhere else.
- **Can't tell** ⇒ `UnverifiablePrecondition`: the source volume isn't registered (the op-level gate refuses that before
  any item runs, so this is the planner's own floor), the stat errors for any other reason, the backend reports no size,
  or a `NotFound` lands outside the volume's root (a narrowed remote place, § "What counts as already gone").
- **Size only, never mtime.** A local copy records the source's preserved mtime, but a volume copy records the
  destination's own (§ "What a volume leaf's snapshot carries"), which no source matches.
- **An original a later logged operation MOVED counts as gone.** The journal isn't consulted: a copy is only ever
  removed against an original seen in place, and a skip is the safe answer.
- **Order.** The copy is checked first, so a copy already gone stays an idempotent `AlreadyGone` whatever happened to
  its original. The skip is stored like any other, so "Finish rolling back" rechecks it, and an original that came back
  since lets the copy go.
- Only a COPY checks: `create_file` and compress have no original, and a cross-FS move's rows restore rather than
  remove.

Pinned by `crates/cmdr-operations/src/rollback/planner_tests/copy_original_tests.rs`. Copy cells that want their copies
removed seed the originals with `Rig::put_originals`.

### A trashed item must still be the one trashed

The Trash hands a freed name to the next item trashed under it. Trash `~/Work/Invoices`, empty it from the Trash, and a
different `Invoices` trashed later from Finder lands at the same `~/.Trash/Invoices` (verified on macOS 27.0: a folder
trashed via Finder, `rm -rf`'d from the Trash, then a second same-name folder trashed the same way landed at the same
in-Trash path with a new inode and birth time, while the first had kept its inode on the way in; 2026-10-20). A
path-only check moved that stranger back to `~/Work/` and credited a restore (#535).

So the trash records WHICH node landed, and the restore moves back only that node:

- **Recording.** `journal::record_trashed` (`cmdr-operations`) reads one `lstat` of the in-Trash path right after
  `trashItemAtURL` returns it, and stores `NodeIdentity::of` that on the row (`dest_identity`). Both trash paths (the
  batch and `trash_single_journaled`) go through it. Never the source: the Trash, or a File Provider behind it, may land
  a different node than the one trashed, and the in-Trash node is what the undo will find.
- **Restoring.** `restore_move` asks `same_node` (`Volume::node_identity`, one `lstat` on `LocalPosixVolume`) before the
  snapshot check, for files and folders alike: a size and date can match by chance. A different node ⇒ `Drift`, and the
  stranger stays in the Trash. A volume that can't report one ⇒ `UnverifiablePrecondition`; a failed stat ⇒ `Failed`.
  Only the top-level item, never a walk.
- **Inode plus birth time, no device number.** `st_dev` follows the order disks attached, so it can change across a
  reboot or a replug while the node stays put, which would strand every older trash undo on an external drive. The path
  already names the volume. Both fields survive a rename, which is all a trip into and out of the Trash is.
- **Rows from before v4 carry no identity** and keep the old checks (folders: existence; files: size + mtime), so the
  stranger case stays open for them. Refusing them would make every older trash un-undoable. Same for a row whose
  `lstat` failed at record time, or a filesystem with no birth time (`NodeIdentity::of` answers `None`).

Pinned by `crates/cmdr-operations/src/rollback/planner_tests/trash_identity_tests.rs` (the stranger folder and file, the
matching folder, a pre-v4 row, a volume that can't say),
`journal_capture_tests::trashed_item_records_the_node_it_landed_as_in_the_trash` (a real trash on macOS), and
`store::tests::v4_migration_keeps_older_rows_with_no_recorded_identity`.

### Per-kind inverse table

The op kind + item entry-type map to one of four inverse actions (`inverse_action`):

- **copy** → file: `RemoveCopyIfOriginalStays` (delete the copied dest if it still matches the snapshot AND its original
  is still there, § "A copy's original must still be there"); dir: the created dir is `RemoveDirIfEmpty`. A copy of a
  whole tree removes its copied files, then its created dirs.
- **create_file / compress** (`archive_edit`) → `RemoveFileIfUnchanged` (delete the created file / net-new archive only
  if unchanged — a later zip-edit drifts the archive, so it's left untouched).
- **create_folder** → `RemoveDirIfEmpty` (remove only if still empty — a file added since ⇒ keep, partial).
- **move / trash / rename** → `RestoreMove` (move the item back FROM where it landed, `dest`, TO its original, `source`,
  and only the node that landed there when the row recorded it, § "A trashed item must still be the one trashed") —
  **except a directory the operation CREATED**, which is removed like a copy's. `record_created_dirs` writes the created
  path as both source and dest, so `is_created_in_place` (source == dest) is the marker; a cross-FS move creates
  destination folders exactly like a copy does, and restoring one renames it onto itself, a no-op counted as reversed
  that leaves the skeleton behind. A move of an EXISTING directory records source ≠ dest and keeps its restore. The
  executor creates the restore target's parent first: a folder move empties and removes its source tree, and a rename
  into a missing parent fails `ENOENT`, which reads as "already gone" and counts the item as REVERSED — a reversal
  reporting success having restored nothing. Creating a directory only ever adds. Trash's `dest` is the recorded
  in-trash location; a same-volume undo is a `rename`, a cross-volume one goes through
  `transfer::volume::move_file_across_volumes` — the bytes land whole at the target (staged on a `.cmdr-tmp-*` and
  renamed into place) before the source side goes, so a stop mid-file leaves the source untouched and no partial behind.
  A cross-volume restore of a DIRECTORY row is refused: those are recorded per leaf, so one is a journal-shape bug.
- **delete** → refused op-level (a permanent delete can't be restored).

The inverse op's own eligibility is computed like any op: a delete-the-copies undo is `not_rollbackable`, a move/rename
undo is `rollbackable` again, so redo falls out (§ "Future: Cmd+Z").

### Streaming + ordering

Reversal streams the original op's `rollback_unit` rows via `store::read_rollback_units_page` (a `seq DESC` paged
cursor, `before_seq` = the smallest seq of the prior page), so a 1M-item op never materializes its list. `search_only`
leaves are excluded (they're for search, never reversal). Removal happens in two phases matching
`CopyTransaction::rollback`: all **files** first (streamed), then the buffered **created-dir** rows deepest-path-first —
a dir removes only once its contents are gone, and pure `seq DESC` would hit a deep dir before the files it holds. Dirs
are a small fraction of an op (interning shares them), so buffering just the dir rows stays bounded.

Rollback eligibility is Done-only at both boundaries: mutation code journals rollback leaves only for committed rows,
and `read_rollback_units_page` filters `outcome = done`. The executor also rejects any non-Done unit defensively. A
skipped or unsuccessful planned rename must never make an untouched destination eligible for rename-back.

### The E2E pacing hook in the item loop

The file loop pauses for `RollbackRunner::pace_between_items()` before each item (above the pause gate, so a click
landing inside the window is honored for THAT item rather than the next one). The planner owns no knob: the default
answers `None`, and `ReversalRunner` answers from the engine host's `Pacing::rollback_pace()` (the app's answer:
`test_mode::effective_rollback_throttle_ms()`). In production both of that one's sources are unset, so it costs one
atomic load and one `LazyLock` deref per item and changes nothing; under E2E it is what gives a spec a window to press
Cancel on a reversal that would otherwise be over in single-digit milliseconds
(`apps/desktop/test/e2e-playwright/operation-log-rollback.spec.ts`). Its own knob rather than a reuse of the copy
throttle, because a spec has to stage a fast copy and then reverse it slowly. Pinned by
`planner_tests/tests.rs::the_e2e_throttle_hook_paces_the_item_loop`, which measures elapsed time rather than trusting
the helper to exist: whoever moves this loop must carry the three lines across, or every rollback E2E goes back to
racing the engine. The deferred-directory phase is deliberately NOT paced (it removes empty leftovers, so a window there
is dead time for a spec). A Rust test paces a reversal on its own engine: a planner cell in `cmdr-operations` sets the
pace with `host::testing::SettablePacing`, so two paced cells never share a value. The app's `test_mode` answer is one
process-wide value, so the two app-side pins that read it (`operations_host/pacing.rs`, `operations_host/mod.rs`) take
it through `test_mode::pace_rollback_for_test`, which serializes them. Hook conventions: `docs/testing.md` § "E2E
env-var hooks".

### Undoing a job: several operations, newest first

`crates/cmdr-operations/src/rollback.rs::undo_operations` is the frontend-facing entry (IPC:
`commands/operation_log.rs::undo_operations`; the MCP `operations_rollback` tool still dispatches ONE op and polls). It
exists for the Ask Cmdr rename rail, where one run can approve several batches.

- **Order is data safety, not cosmetics** (`rollback/order.rs::undo_order`). Batch one renames `a.txt` → `b.txt`; batch
  three then renames `c.txt` → `a.txt`. Newest-first, batch three vacates `a.txt` before batch one needs it back and
  both restore. Oldest-first, batch one finds `a.txt` occupied by a different entry, the pinned non-destructive restore
  correctly refuses, and that file is never restored — the only trace being a `partially_rolled_back` state. The
  journal's clock is whole seconds, so a job's batches routinely tie; `undo_order` breaks a tie by the reverse of the
  caller's order, and callers therefore pass ids in APPLY order.
- **Sequencing is the driver's, not the queue's.** `dispatch_inverse_reported` hands back a `oneshot` the managed
  inverse resolves with its `RollbackReport`, and the driver awaits each before dispatching the next. Relying on the
  manager's oldest-first lane admission would make the ordering an emergent property of an unrelated module.
- **Refusals are counted, never dropped.** An unknown id, an already-undone op, or a disconnected volume comes back as
  an `OperationUndoOutcome` with a typed `refusal` and zero counts, so the caller's operation count always matches what
  it asked for (invariant 9). Unknown ids carry no start time, so they land last in the report; they reversed nothing,
  so their position changes no outcome.
- **Per-item skip REASONS are persisted, so a report names files, not classes.** The engine writes the `SkipReason` it
  computed into `rollback_skip_reason` alongside `ItemOutcome::Skipped`, both in one `set_item_outcomes` update — so a
  retry that reverses an item CLEARS the reason, and no row ever explains an outcome that no longer holds. An
  `AlreadyGone` item counts as reversed, so it stores no reason at all; the vocabulary lives in `types.rs::SkipReason`,
  shared with the engine rather than duplicated.
- **The report carries a per-reason breakdown, complete but bounded** (`rollback/skips.rs`). `RollbackReport.skips` (and
  each `OperationUndoOutcome.skips`) is one `SkipBreakdown { reason, count, example_name }` per reason, so a 1M-item
  rollback's report is a handful of strings AND every skip is counted — a truncated sample would be a cut to disclose
  under invariant 9, and this way there's no cut. `example_name` is the leaf name at the location the undo FOUND the
  item (`removal_target`), which is the name the file carries right now — the one the user is looking at; the frontend
  does no path arithmetic. The UI merges groups across a job's batches and names the file when a group has one member —
  see `apps/desktop/src/lib/ask-cmdr/DETAILS.md` § Undo after a batch lands.
- **Reading pre-v2 history.** A rollback run before v2 left its items `skipped` with `rollback_skip_reason` NULL, and
  every read surfaces that as `None` / `null` (`OperationItemRow`, `OperationItemView`, so MCP `operations_get` too) —
  "reason not recorded", never a stand-in. The frontend falls back to naming the reason class for exactly this case, and
  still reports the count. Retention and the startup reconcile only ever read `outcome`, so both behave identically
  whether the column is set or NULL. "Finish" does read it: a pre-v2 held-back row is indistinguishable from the
  operation's own skip, so it isn't retried, and the op stays `partially_rolled_back` (see below).

### The `rolling_back` state machine + startup reconcile

`rollback_operation` (the entry) reads the op, gates it (`check_rollbackable`: `UnknownOperation` / `AlreadyRollingBack`
/ `NotRollbackable(reason)` / `VolumeUnavailable{volume_id}` — the cross-volume gate is computed from live mount state,
never stored), then sets `rolling_back` **as late as possible** (right before the spawn) and hands the plan to the
injected `spawn`. The double-rollback guard is automatic: a second request reads `rolling_back` and refuses. On a
**synchronous spawn failure** (a volume dropped between the gate and the spawn, so the inverse never starts) it resets
`rolling_back` back to the state the op held (`rollbackable`, or `partially_rolled_back` for a Finish) in the same call
before returning the typed error, so a retry isn't wedged.

`execute_rollback` resolves the original op at the end (`resolve_final_state`, given the state the op started in):
`RolledBack` (nothing skipped), `PartiallyRolledBack` (any skip, even if nothing could be reversed; Finish retries
them). A run that STOPPED with nothing reversed leaves the op as it found it: `Rollbackable`, or still
`PartiallyRolledBack` when an earlier run reversed some of it. A journal page that fails to read (busy past its timeout,
an undecodable row, an I/O error) is a stop too: the run never saw what lies past it, so it skips the directory sweep
(unread files may sit inside those dirs), finalizes the inverse `Failed`, and resolves like a cancel, keeping what it
reversed. One bad row fails its whole page rather than becoming a per-item skip: a newer build's token can't reach this
build (a downgrade is refused), and a skip would need a new `SkipReason` for a row with no readable path. ❌ A run over
a partial op never resolves `RolledBack` on an empty pass: finding nothing to retry proves nothing came back. It marks
the original's items `rolled_back`/`skipped` and journals the inverse op's own item rows (reversed ⇒ `done`, skipped ⇒
`skipped`), which drive the reconcile.

**Startup reconcile** (`reconcile_rolling_back_on_open`, called by `OperationLog::open` before the journal is reachable)
resolves any op a crash left `rolling_back`: from its unfinalized inverse op's recorded outcomes (any `done` item ⇒
`partially_rolled_back`, else `rollbackable`), or — when **no inverse op row exists** (crashed after setting
`rolling_back` but before the spawn) — straight back to `rollbackable`. Either way a re-issued rollback resumes
idempotently: every per-item inverse is a recheck-then-act, and an already-reversed item reads as `AlreadyGone` (counted
as a no-op success).

That re-entrancy is load-bearing beyond the crash path: the history dialog's **Finish rolling back** button on a
`partially_rolled_back` row is nothing but a second `rollback_operation` call on the SAME op, which the gate admits. The
item loop streams every row that still awaits reversal (`RollbackUnit::awaits_reversal`): `done` rows, plus `skipped`
rows carrying a `rollback_skip_reason` (held back by an earlier run). A `skipped` row with no reason is the operation's
own skip (a bulk rename's blocked row) and never streams. Rows already `rolled_back` aren't re-read; a crash-resumed run
whose outcomes didn't flush re-reads them as `AlreadyGone`. ❌ So don't make `check_rollbackable` refuse
`PartiallyRolledBack`, and ❌ don't let any inverse act without its recheck: either one turns that button into a
double-act. The UI side is `apps/desktop/src/lib/operation-log/DETAILS.md` § "a partly-reversed operation offers to
finish".

### The retention race it closes

The paged cursor spans successive short-lived read connections, not one WAL snapshot, so a concurrent `Prune` could
delete rows between pages and silently under-restore. The fix is NOT a long read transaction (it would block WAL
checkpointing for the whole file-I/O duration) — it's the `rolling_back` state: retention skips any op in `rolling_back`
(and its `rolls_back_op_id` target), so a live rollback's streamed source rows can't vanish mid-stream (see `writer.rs`
`handle_prune`).

### What a volume leaf's snapshot carries

Every leaf of a volume (SMB/MTP) copy records its **size**, top-level and inner alike, so a cross-volume directory
rollback reverses the whole tree rather than skipping its contents. The byte count costs nothing extra: `copy_leaf`
returns what it piped, off a size hint the walker's `list_directory` already paid for, and `CreatedPaths` carries a
`WrittenFile` per destination (`crates/cmdr-operations/src/ledger.rs`) so it rides through to the row (`journal.rs`).

**Each leaf also records the landed file's OWN mtime, read back from the destination** (`journal::LandedTimes` in
`crates/cmdr-operations/src/journal.rs`). Size alone passed a file edited in place at the same length (a VM disk image,
a SQLite database, a re-saved document), and undoing the copy deleted it with the user's edits (#519).

- **The destination's own time, ❌ never the source's.** A destination that doesn't keep the source's date (S3, WebDAV,
  an Android phone over MTP) would differ from a read-time capture on every leaf, flipping each one to `Drift`, which
  reads to the user as "you changed this file" and refuses. The read-back goes through the same `Volume` the reversal
  rechecks with, so both sides see one clock at one precision.
- **Bounded per source, never per file.** The write path returns no landed time (`StreamWritten` carries bytes and
  refusals), so this reads it back: one `get_metadata` for a FILE source, one `list_directory` per destination folder
  for a DIRECTORY source, on every backend. A listed name is only matched against the files the source wrote, never
  joined onto a path. Only a journaling transfer reads anything back.
- **A stopped or failed source reads nothing back** (a stop never waits on the share), so its rows keep the size-only
  check; so does a file the read-back can't place (a listing that failed, a name the listing spells differently), a
  symlink, and a backend that reports no mtime (`None`, which `verify_snapshot` skips over).
- `verify_snapshot` is flat over the row: every recorded field must verify, and at least one must, with no
  inner-versus-top-level branch. Local-FS leaves record the preserved source mtime, which their write path keeps.
- The in-flight reversals (`crates/cmdr-operations/src/reversal.rs`) keep the size-only volume recheck: their ledger
  carries no mtime by design (`crates/cmdr-operations/src/transfer/DETAILS.md` § "What the in-flight ledgers record"),
  and they run seconds after the write, not days.

Pinned by `crates/cmdr-operations/src/journal_capture_volume_tests.rs` (`a_serial_…`, `a_concurrent_…`, the cross-volume
move, and `undoing_a_volume_copy_keeps_a_file_edited_since_at_the_same_length`).

**Remaining gap, one path**: a cross-volume MOVE interrupted mid-directory journals nothing for the children it already
finished. Unlike the copy's, those aren't a completed unit of work — the source sweep runs per top-level source after
its whole subtree lands, so each child exists at BOTH ends. A move row's inverse is a restore, which would find the
source occupied and skip, so the rows could only ever report a partial they can't fix. The honest answer is a per-item
inverse the schema can't express today; the copy path, where an interrupted child IS a finished copy, journals from its
failure arms.

The `InMemoryVolume` fixtures are case- and normalization-sensitive, so they stand in for SFTP to Linux and S3; the
case-insensitive side of the self-collision guard runs on the real macOS temp dir (`case_twin_tests.rs`), which returns
early if that temp dir sits on a case-sensitive volume.

**`InMemoryVolume` is more permissive than the real FS in one way that hides bugs**: its `rename` creates the target's
missing parents, where `LocalPosixVolume` fails with `ENOENT`. A reversal test written against the double can pass on
broken code — `planner_tests/tests.rs::a_restore_recreates_the_folder_the_move_emptied` runs against a real tempdir for
exactly that reason.

### Future: Cmd+Z (designed for, not built)

A later Cmd+Z is
`SELECT op_id FROM operations WHERE initiator='user' AND rollback_state='rollbackable' ORDER BY ended_at DESC LIMIT 1`
then `dispatch_rollback`. The two-axis status + `rolls_back_op_id` linkage make it a query, not a new engine; because a
rollback is itself a journaled user op, "undo the undo" (redo) falls out too. Don't build it; don't preclude it.

## Query API + search

`query.rs` is the read side: short-lived read-only connections, index-served name search, and paged reads for the Debug
panel / alpha dialog / MCP tools. Two kinds of caller open their own short-lived read connection and forward to
`query.rs` (business logic — filtering, paging, dir-path resolution — lives there, never in the callers): the FE IPC
surface is two thin pass-throughs (`commands/operation_log.rs`): `get_recent_operation_log_entries(limit, offset)` and
`get_operation_log_detail(operation_id, item_limit, item_offset)`; the MCP `operations_list` / `operations_get` handlers
(`mcp/executor/operation_log.rs`) do the same off the MCP task.

- **Within one second, "newest first" has no defined order.** `get_recent_operation_log_entries` sorts
  `started_at DESC, op_id DESC`, and `op_id` is a random UUID, so operations that start in the same second come back in
  arbitrary order. That's invisible in normal use and a trap for anything that identifies an operation as "the top row":
  a reversal journals its own inverse operation, which lands in the same second as whatever ran next. Identify an
  operation by DIFFERENCE from the ids you'd already seen
  (`apps/desktop/test/e2e-playwright/operation-log-rollback.spec.ts::stageTransfer` is the worked example) rather than
  by position.
- **Name search is an indexed folded-name lookup, not FTS.** The product headline — "when did I delete `dog.jpg`?" — is
  exact/prefix name equality, so `search_operations` joins `operation_items` to `operations` and matches the indexed
  `source_name_folded` column. The benchmark query
  (`source_name_folded = ? AND kind IN (delete, trash) ORDER BY ended_at DESC`) is served by
  `operation_items_source_name` + the `operations` PK, never a full table scan — pinned by an `EXPLAIN QUERY PLAN` test
  (`query::tests::delete_dog_jpg_is_index_served`). FTS5 stays a clean later add if substring/fuzzy is ever wanted.
- **Search spans every `row_role`, deliberately.** A trashed folder records the top-level `rollback_unit` plus its
  subtree's `search_only` leaves (§ "Per-kind record points and granularity"), so "when did I trash `dog.jpg`" hits even
  when `dog.jpg` sat inside a trashed folder — the asymmetry a uniform-granularity design would leave as a silent miss.
  The one uncovered case (a subtree that couldn't be enumerated) is flagged `search_coverage = top_level_only`, a
  queryable known gap on the row, not a false negative.
- **Names repeat, so they're NOT interned.** Item names duplicate massively across a 1M-file op; a b-tree index handles
  duplicate keys fine, and (unlike dirs) names must stay directly queryable. Prefix match is a folded-range scan on the
  same index (`>= prefix AND < prefix⁺`), never `LIKE` (which wouldn't use the index).
- **Item views resolve interned dirs to full paths.** `OperationRow` is the summary wire type (no `dir_id`s); item rows
  carry interned prefixes, so `get_operation` returns `OperationItemView`s with `source_path`/`dest_path` reconstructed
  — the frontend never sees a `dir_id`.
- **No rendered prose crosses the wire.** Rows carry typed enums (`kind`, `status`, `initiator`, item outcomes) and
  counts; every label and the per-operation summary are composed frontend-side, so they localize per viewer. A
  pre-rendered English `summary` column would be unlocalizable the day it shipped.

## Alpha UI — the "Operation log" dialog

The read API's one user-facing consumer: a menu- and shortcut-triggered soft dialog listing recent operations, newest
first, each expandable to its per-item rows. It lives entirely in the frontend (`apps/desktop/src/lib/operation-log/`)
over the existing read commands, so nothing on this side changes when the dialog does. Its state, paging, item caching,
typed-enum labels, and rollback flow: `apps/desktop/src/lib/operation-log/DETAILS.md`.

**Debugging/demo quality by design** (it may become a sidebar later), so it is deliberately un-gold-plated — but i18n,
style-guide copy, and a11y basics are not optional. It wears the ALPHA badge off the `operation-log` entry in the
repo-root `feature-status.json`.

The wiring with a foot on this side:

- **The native menu item.** Command `log.operationLog` (App scope) needs the `OPERATION_LOG_ID` mappings in
  `menu/command_map.rs` (`menu_id_to_command` + `command_id_to_menu_id`) plus its row in `menu/menu_bar.rs` (**View
  menu**, after the command palette). The other places one command touches, and how a miss fails silently:
  `apps/desktop/src/lib/commands/CLAUDE.md` § Gotchas.
- **Default shortcut ⌘⌥L**, Command-then-Option in the canonical combo vocabulary rather than Apple's display order ⌥⌘
  (`apps/desktop/src/lib/shortcuts/CLAUDE.md`), so it fires through the in-app dispatch and not only via the native
  accelerator. ⌥⌘O is already `file.showInFinder`, so `L` (for "log") is the mnemonic; a collision test in
  `command-registry.test.ts` guards it.
- **The two read commands are the whole backend surface** (§ "Query API + search").
  `#lib/tauri-commands/operation-log.ts` unwraps them for the dialog; the Debug panel calls the raw bindings instead
  (dev-only, bindings-import-exempt).

**Convergence with the agent log.** The dialog is the mutation half of a future unified timeline: when the in-app agent
ships, its decision log (`agent_log`) joins the same user-facing surface, and whether that merged surface is later
_labelled_ "Activity" is a UI-copy call for then. "Operation" stays the entity/row name; "action" stays reserved for the
agent spec's navigation/intent stream. The dialog is intentionally throwaway so that convergence can reshape it (likely
into a sidebar) without sunk cost.

## Retention — prune by age + size, GC dirs, reclaim

Retention runs the writer's `Prune` on startup and on a periodic timer (`retention.rs`, every 6 h), started by
`OperationLog::spawn_retention` on the runtime the host passes. The age/size limits come from a `RetentionLimitsSource`
the host reads fresh from settings each tick (`operation_log_host` over `load_operation_log_retention_limits`), so a
settings change takes effect on the next tick with no restart or applier case. Defaults: **age = forever, size = 3 GB**.
The settings contract: `operationLog.maxAge` (duration ms; `0` = forever; absent ⇒ forever) and `operationLog.maxSize`
(bytes; absent ⇒ 3 GB; `0` = unlimited). The user-facing controls are the "Operation log" settings section
(`OperationLogSection.svelte` + the `operationLog.*` entries in `settings-registry.ts`), which persist exactly these
keys — see `docs/guides/adding-a-new-setting.md` for the registry pattern.

A prune (`handle_prune`) is: **age prune** (delete whole finished ops older than the cutoff) → **size prune** (delete
the oldest whole ops until the DB fits the budget) → **dir GC** → **reclaim**.

- **Prune whole operations only.** Never orphan an item from a kept op; never leave a dangling `rolls_back_op_id` (null
  a surviving op's link to a pruned op BEFORE the delete, or the self-FK rejects it). A rolled-back pair prunes together
  (`rollback_pair_component` pulls in the seed's inverse/original), and any protected partner is excluded with its link
  nulled.
- **Never prune an in-flight rollback's rows.** `protected_ops_fragment` excludes any op in `rolling_back` (the
  original, which a live rollback streams across successive read connections) and its `rolls_back_op_id` target; the
  unfinished inverse is separately excluded by the `ended_at IS NOT NULL` gate. This closes the
  prune-versus-live-rollback race (§ "The retention race it closes") without a long read transaction.
- **Size budget is measured as live bytes.** `live_size_bytes = (page_count - freelist) * page_size` — the size the file
  would have after a full vacuum — so the delete loop makes progress before pages are physically reclaimed (each delete
  grows the freelist). It stops when under budget or nothing prunable remains (everything left protected).
- **Dir GC — the referenced-plus-ancestors closure.** Interning keeps a dir row forever, so pruning ops alone leaves a
  monotonically-growing `dirs` floor. GC iterates leaf-up: delete dirs referenced by no item AND no child dir, repeat
  until stable — exactly the complement of the referenced-dirs-plus-their-ancestors closure, so a referenced dir's whole
  parent chain survives (path reconstruction walks it to the root).
- **Reclaim.** An age-only prune runs one **bounded** `incremental_vacuum` slice (tiered `pick_vacuum_cap`, mirroring
  `indexing/writer/maintenance.rs`) and lets the periodic timer drain the rest over ticks, never stalling the
  lossless-with-backpressure writer. A **size** prune must actually shrink the file to honor the budget, so it drains
  the ENTIRE freelist (`reclaim_fully`, ignoring the cap floor) then `wal_checkpoint(TRUNCATE)` so the truncation
  reaches the physical file (in WAL mode `incremental_vacuum`'s page-count drop otherwise lands only in the WAL).
  Importance sets `auto_vacuum = INCREMENTAL` but never calls `incremental_vacuum`; this DB must, or it grows
  unboundedly. Both slices go through `cmdr_fs::sqlite_util::run_incremental_vacuum` because `incremental_vacuum` frees
  one page per step — see the canonical gotcha in `crates/cmdr-index/src/indexing/writer/DETAILS.md` § "Gotcha —
  row-yielding pragmas".

## The crate boundary

The log is its own crate, with no `tauri` in its tree (`index-crate-isolation` guards it), so
`cargo check -p cmdr-operation-log` verifies it without building the app. Its host is the app's `operation_log_host/`,
which builds the one `OperationLog`, opens it, and passes in everything the log can't know: the data dir, the recording
switch and retention limits from settings, and the runtime retention runs on. The planner's `VolumeLookup` comes from
the write engine, whose `OperationVolumes` is its supertrait.

- **Decision: the log spawns only on what its host passes in.** Retention runs on the `Handle` its host passes in
  (`retention.rs`), the rollback planner's spawn is injected, and the writer is a named `std::thread::Builder` thread.
  Why: the one pool everything shares is the host's call, as in the engine. The crate's `clippy.toml` refuses the
  ambient spawns.

- **Decision: wire types document themselves at the type, not per field.** Why: the `serde` + `specta` types
  (`OperationRow`, `OperationDetail`, `OperationItemView`, `RollbackRefusal`, `RollbackDispatch`, `SkipBreakdown`, the
  `token_enum!` enums) cross into `bindings.ts`, where every `///` lands as JSDoc. They carry `#[allow(missing_docs)]`
  with that reason, so `#![deny(missing_docs)]` holds for everything else without moving the bindings.
- **The `testing` feature** widens exactly what the app's cells reach: `OperationLog::set_journal` (`TestOperationLog`,
  and the transfer bench through `cmdr-operations`' `testing`) and `WriterJournal::writer`. The crate's own recording
  fake and its dropped-row hook stay `cfg(test)`.

### The public surface is capped

`index-crate-isolation` holds the surface to 11 root promises, 6 `OperationLog` methods, seven public modules, and 71
items in them, with no headroom: measured at the extraction, re-measured when the journal API took `OperationId`, and
tightened when the test-only accessors went behind `cfg(test)`
(`scripts/check/checks/index-crate-isolation-ceilings.go`). A helper only this crate's tests call stays `#[cfg(test)]`
(`OperationLogStore::conn` and `schema_version`, `query::coverage_is_complete`), so the count promises nothing to no
caller. By module:

- **Root**: `OperationLog` (the handle), `OpJournal`, `OpenError`, `new_operation_id`, and the seven modules below. The
  handle's methods are `detached`, `open`, `set_recording`, `writer`, `spawn_retention`, and `journal_open`; an
  `OpJournal` records, notes, and finalizes its one operation.
- **`store`**: the DB path, `fold_name`, the error type, the row types, the migration ladder, and the low-level reads
  the IPC commands, the MCP tools, the dump tool, and the planner's app-side cells use.
- **`query`**: the paged history read and indexed name search behind the IPC commands and MCP tools.
- **`writer`** and **`capture`**: the writer thread and the journal the write engine records through.
- **`types`**: the typed classification tokens, which cross IPC, and `OperationId` (with its `as_str`, and a
  `Borrow<str>` so an id-keyed map answers a wire row's text), which the journal API and the write engine's public verbs
  take: `OpenOperation`, the `OperationJournal` seam, and `OpJournal::op_id`.
- **`rollback`**: the planner (`check_rollbackable`, `rollback_operation`, `execute_rollback`, `undo_order`, the
  reconcile), its `RollbackRunner` / `VolumeLookup` seams, and the fail-safe verdicts the engine's in-flight reversals
  share (`verify_snapshot`, `SnapshotVerdict`, `ItemResult`, `SkipTally`).
- **`retention`**: the limits type and the closure shape the host passes.

## The dev bin

`cargo run -p operation-log-dump -- <operation-log.db> [limit]` opens the DB read-only and prints recent operations +
their items, decoding the typed tokens and reconstructing interned dir paths through the SAME library read functions the
app uses (`recent_operations`, `read_operation_items`, `reconstruct_dir_path`) — never a re-implementation. It depends
on this crate, not the app. Because there's no collation, a stock `sqlite3` also opens the file directly; the bin adds
the typed rendering.

## Evidence anchors

- The `dirs` NULL-parent `UNIQUE` gotcha and the IFNULL expression-index fix: verified by
  `store::tests::intern_dir_dedups_and_distinguishes_siblings` and `intern_dir_handles_the_volume_root` (2026-07,
  in-tree tests).
- Migration ladder forward/downgrade/unparseable behavior: `store::tests`
  (`forward_migration_preserves_rows_and_bumps_version`, `downgrade_is_refused_not_destroyed`,
  `unparseable_file_recreates_fresh`).
- A pre-v2 row reading as "reason not recorded" rather than a default:
  `store::tests::a_pre_migration_item_row_reads_as_no_recorded_skip_reason` (writes a row against the v1-only ladder,
  then migrates forward). Per-item reasons round-tripping, and the reversed / already-restored cases recording none:
  `planner_tests::tests::a_skipped_item_persists_the_reason_it_was_skipped` and its two siblings.
