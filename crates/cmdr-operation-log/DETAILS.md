# Operation log — details

Depth for the `cmdr-operation-log` crate. Read this before any non-trivial work here: editing, planning, reorganizing,
or advising. Must-knows and the module map: `CLAUDE.md`. This doc captures what shipped and the durable rationale a
future agent needs on hand, including every decision behind it. App paths named here (`commands/`, `mcp/`,
`operation_log_host/`) are under `apps/desktop/src-tauri/src/`; the write engine that journals into the log is
`crates/cmdr-operations/`.

**Clocks and timers go through `crate::rt`** (`src/rt.rs`, over `cmdr_fs::rt`): the journal's wall clock (`now_secs`),
the retention tick, and the rollback pacing. `clippy.toml` refuses the direct calls, because the log also runs in the
browser demo, where `std`'s clock and tokio's timer trap; natively each is the same call.

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
`operations_get` / `operations_rollback` — see `apps/desktop/src-tauri/src/tools/actions/DETAILS.md`). The UI (the Debug
panel and the alpha dialog) builds on the read side.

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

Four tables plus `meta` (the migration anchor). Full DDL: `store/migrations.rs` (`migrate_v1_initial`, then each
appended step). The shape and its non-obvious choices (the fourth, `operation_tree_dirs`, has its own § "Tree summary"):

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
  the alpha dialog. Two v6 columns serve the item tree (§ "Tree summary"): the nullable `tree_summary`
  (`TreeSummaryState`, `built` or NULL) and `outcome_generation`.
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
  integer; only a trash writes it (§ "A trashed item must still be the one trashed"). The nullable `tags_before` +
  `tags_after` BLOB pair (v5) is a tag write's Finder-tags value around the write, both set or both NULL, only on a
  `tag` op's rows (§ "A tag write restores the exact prior value"). Directories the op created are **first-class `dir`
  rows** sequenced after their contents, so a `seq DESC` rollback removes files before the dirs that held them. Names
  are indexed folded (search) and, unlike dirs, **not interned** — a b-tree handles the massive duplication across a
  large op fine, and names must stay directly queryable. The nullable `self_dir_id` (v6) is a DIR row's own folder on
  the op's tree axis (§ "Tree summary"). Indexes: `(op_id, seq)`, `(source_name_folded)`, partial `(dest_name_folded)`,
  and `(op_id, source_dir_id, source_name_folded)` plus partial `(op_id, dest_dir_id, dest_name_folded)`, which serve
  "this folder's files of this op, in name order" and which the writer builds after open, outside the ladder (§ "Tree
  summary").

## The writer (`writer.rs`)

One dedicated OS thread owns the single write connection; the cloneable `OperationLogWriter` handle is the only way in.
Message surface: `OpenOperation` (insert the header, `Running`, its tree summary `built`), `RecordItems` (batched insert
of a slice in one transaction, interning dirs + folding names), `FinalizeOperation` (write terminal state, return
per-`row_role` durable counts), `SetRollbackState` and `SetItemOutcomes` (the rollback engine's two writes, each a
barrier), `FillSelfDirs` and `WriteTreeSummary` (a tree backfill's two writes, each a barrier,
`src/writer/backfill.rs`), `BuildTreeIndexes`, `Prune` (retention), `Flush` (barrier), `Shutdown`. `RecordItems` and
`SetItemOutcomes` carry the op's tree-summary delta in their own transaction (`src/writer/tree_summary.rs`, § "Tree
summary").

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
  delete never (`permanent_delete`); trash/rename/create-folder/create-file/tag open rollbackable (rechecked at rollback
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

Each point is where the op already stats the item, so journaling is near-zero marginal cost (no new syscalls). The row
SHAPES below (which side each row's source and dest sit on, created-in-place folder rows, a folder unit's `size` being
its inode length) are pinned through the real capture paths, resolved to full paths, in
`crates/cmdr-operations/src/journal_row_shape_tests.rs`. Three facts a reader of rows trips on:

- **Dest presence**: every `rollback_unit` row carries a dest except a delete's (source only). A `search_only` leaf
  carries one only when its top-level item's landing is known (a trash the OS reported no in-Trash path for records them
  source-only).
- **Only a batch rename writes non-`done` rows of its own** (`skipped` / `failed`, no `rollback_skip_reason`), each
  naming the destination it was MEANT to reach, so a dest never proves a row landed. Copy, move, delete, and trash
  journal only what landed: a file skipped on a conflict, or a folder that already existed, has no row.
- **`search_only` rows include `dir` rows**: the drive index enumerates every descendant, folders included
  (`apps/desktop/src-tauri/src/operations_host/index.rs`, `enumerate_from_id`).

The per-kind record points:

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
- **tag** — not an engine op: the app's tag writers collect what they changed and journal it as one `tag` op through
  `OperationLog::journal_open` once the action ends (§ "A tag write restores the exact prior value").
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
(threaded through the typed command bus alongside `onConflict`, mirroring navigation's `source: 'mcp'`). The one gap: an
into-archive-edit via a volume command defaults to `user` (see the bypass boundary above).

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
(`src/rollback/decision_tests.rs`). Three of them are also PROVEN for every input with Kani (`src/rollback/proofs.rs`,
`pnpm check kani`): `verify_snapshot` matches exactly when every recorded field is present and unchanged (over every
`u64` and `i64`, assuming a field holds its live value cast `as i64`, as every journal writer records it),
`resolve_final_state` never calls a run rolled back with items skipped or unattempted and never loses a partial op, and
`item_action` removes created folders, checks a copy's original, and agrees with `inverse_kind`. Each has failing
mutants on record beside it (`proofs.mutants.json`). The token enums derive `kani::Arbitrary` under `cfg(kani)` only.

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

### A tag write restores the exact prior value

Finder color tags live in one xattr per file (`cmdr_fs::entry::FINDER_TAGS_XATTR`), and a `set`, `toggle`, or `clear`
across a selection can wipe tags people organize with. So every tag write journals as a `tag` op, one `rollback_unit`
row per file it changed, carrying the raw value before and after (`TagsChange`, columns `tags_before` + `tags_after`,
schema v5). Who writes them: `apps/desktop/src-tauri/src/file_system/tags_journal.rs`, for the keyboard command, the
context-menu circles (`user`), and the `tag` tool (`agent` or `ai_client`, by caller).

- **Opaque bytes, never a decoded tag list.** The plist codec is the app's; the log and the engine only compare and
  write bytes. That also makes the restore exact: every tag, colorless ones and their order included, and even a value
  Cmdr can't decode (a `clear` overwrites one, and its undo puts it back).
- **A file with no attribute records the empty-array plist as its before**, never "absent": the restore can't remove the
  attribute, since a QNAP answers success to a remove and keeps it (`crates/cmdr-smb/DETAILS.md` § "Extended
  attributes"). The empty array reads as no tags everywhere.
- **The recheck is exact-bytes equality with `after`.** Anything else is `Drift` and the file keeps its tags: someone
  retagged it since (Finder, another Mac on a share, a later Cmdr write), or another writer encoded the same tags
  differently, which fails safe. A file reading `before` already is credited as reversed (`AlreadyGone`, the end state
  holds: an earlier pass, a crash resume). A file gone is `Vanished`, under the same root-reach rule as a restore. One
  read per file (`read_xattr_many`, `Verified`), then `InverseAct::WriteXattr`.
- **The runner tells the panes**: a tag lands with no new size (and on a QNAP no new date), so `ReversalRunner` asks
  `ListingHost::row_xattrs_may_have_changed` after each write, the same door the share watcher uses. The reversal runs
  as its own `WriteOperationType::Tag`, so the queue, the quit prompt, and the corner chip name it as a tag change.
- **One user action is one op.** A `toggle` of two colors makes a pass per color; the app merges them per file (first
  before, last after) and drops a file that ended where it started. A file whose value can't be read isn't written at
  all (the app's typed skip), so there's never a change with no before. A write that failed partway still journals the
  files before it, under a `failed` op.

Pinned by `crates/cmdr-operations/src/rollback/planner_tests/tag_tests.rs` (restore, retagged since, gone, already back,
the undo refused a rollback of its own, a row with no values) and `tags_journal_test.rs` beside the app's writer (exact
bytes off a real disk, the initiator).

### Per-kind inverse table

The op kind + item entry-type map to one of four inverse actions (`inverse_action`); `item_action` adds the
created-in-place override below and is the ONE per-item decision: the reversal acts on it and the dispatch summary
(`summarize_inverse`) names it, so the queue row can't say "move back" for a folder the reversal removes:

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
- **tag** → `RestoreTags` (write the file's prior Finder-tags value back, only while it still holds the value the write
  left, § "A tag write restores the exact prior value"). Its inverse is a `tag` op too, rows swapped.
- **delete** → refused op-level (a permanent delete can't be restored).

### No rollback of a rollback

Undoing an operation leaves the ORIGINAL `rolled_back` or `partially_rolled_back`, and that's the whole story (David,
2026-11-08, #1033): no redo. `check_rollbackable` refuses any op with `rolls_back_op_id` set,
`RollbackRefusal::IsARollback`, BEFORE every other check, so the dialog, MCP's `operations_rollback`, and
`undo_operations` all hit the same guard, and an inverse that stored `rollbackable` (its eligibility is computed like
any op's at finalize, so a move or rename undo says so) stays harmless. `query::recent_operations` and
`query::search_operations` leave inverses out (`rolls_back_op_id IS NULL`), so neither the dialog, the MCP list, nor
name search shows one; `get_operation` still reads one by id.

- **Why**: an inverse's rows carry no dest (`bookkeeping::inverse_item_row`), so rolling back a move, trash, or rename
  undo held every row back as `Failed`. A redo that never worked was a dead button, and making it work is a feature
  nobody asked for.
- **The inverse's rows stay**, internally: the startup reconcile reads every inverse's `done` rows (§ "The
  `rolling_back` state machine"), `OperationRow.inverse_op_id` (`store::fill_inverse_op_ids`) is how the dialog's
  `RollbackControls` find the running reversal, and retention guards a rolling-back op's inverse target.
- Pinned by `crates/cmdr-operations/src/rollback/planner_tests/redo_tests.rs` (move, rename, copy, and a stored
  `rollbackable` refused first), `tag_tests.rs::the_undo_of_a_tag_write_is_never_rolled_back`, the MCP cell
  `tools/actions/operation_log/tests.rs::what_the_journal_refuses_is_typed_with_no_dialog`, and
  `query/tests.rs::neither_the_feed_nor_a_search_lists_a_rollback`.

### Streaming + ordering

Reversal streams the original op's `rollback_unit` rows via `store::read_rollback_units_page` (a `seq DESC` paged
cursor, `before_seq` = the smallest seq of the prior page), so a 1M-item op never materializes its list. `search_only`
leaves are excluded (they're for search, never reversal). Removal happens in two phases matching
`CopyTransaction::rollback`: all **files** first (streamed), then the buffered **created-dir** rows deepest-path-first —
a dir removes only once its contents are gone, and pure `seq DESC` would hit a deep dir before the files it holds. Dirs
are a small fraction of an op (interning shares them), so buffering just the dir rows stays bounded. The sweep waits for
the LAST page, never per page. Pinned by `crates/cmdr-operations/src/rollback/planner_tests/dir_sweep_tests.rs` (folder
rows streamed before every file, shallow first, a file inside on the second page).

A trashed or same-FS-moved folder is ONE DIR `rollback_unit`, so it's reversed in the sweep too, and a big trash can put
100,000 of them there. So the sweep owes each item what the file loop does: it parks on a pause BEFORE the item is
verified, polls the stop, and credits the bar. **The bar counts every `UnitRole::Selectable` row** (each file, and each
folder the op moved, trashed, or made on its own; `rollback.rs::credits_progress`), bytes from files only (a folder's
`size` is an inode length). A folder a copy or move made around its files is housekeeping the sweep removes, never on
the bar. The totals come off `store::read_rollback_progress_totals`, which counts the same rows, or off the selection's
algebra (below). Pinned by `crates/cmdr-operations/src/rollback/planner_tests/selection_tests.rs`
(`a_trash_of_folders_moves_the_bar_once_per_folder`,
`the_folder_sweep_parks_before_each_folder_and_skips_the_left_out_one`).

**Partial rollback: a selection filters the stream** (`execute_rollback_with_selection`,
`rollback_operation_with_selection`; issue #1033). The item tree's `RollbackSelection` rides on `InversePlan.selection`
(`None` takes every row: the default, "Roll back the rest", MCP, and undo). The gate (`tree/rollback_filter.rs::narrow`)
normalizes it with THE resolver (`tree/selection.rs`, § "The selection"), ❌ never trusting the wire, and refuses it
typed (`RollbackRefusal::Selection(SelectionRefusal)`) before `rolling_back` is set: `summaryNotBuilt` (the decision
reads the summary), `tooManyToggles` (normalizing drops the NEWEST toggles past 2,000, and a dropped `off` would reverse
a row the person unchecked), `nothingSelected` (`N = 0`). A selection that normalizes to everything IS `None`, so its
run, report, and bar are an unfiltered run's. The engine normalizes again before it streams.

- **Decided on the RAW row, before any path resolves** (`store::read_rollback_rows_page`, then `store::rollback_unit_of`
  only for kept rows): a left-out row costs one `HashMap` lookup plus one summary read per distinct folder for the run,
  is never verified, acted on, flushed, or recorded, and counts as `left_out`. A selection only removes rows from the
  stream; every row it keeps goes through every guard.
- **Paging stays on the raw rows**: `before` is the page's smallest raw `seq`, never a kept row's, or a page that keeps
  nothing would read again forever. A page whose summary lookup fails stops the run like a page it couldn't read (❌
  never a guessed decision).
- **The stop is asked per page too**: a narrow selection over a big op streams page after page it keeps nothing of.
- **A folder unit is decided by its own folder** (`self_dir_id`, its axis side: unchecking a trashed folder keeps it in
  the Trash; a same-FS move's is the landed folder). A created folder (no checkbox) goes exactly when everything
  selectable under it does (`Selection::fully_chosen`), so a left-out file keeps its folders and no misleading
  `DirNotEmpty` lands on them, and a folder whose every file was checked one by one goes with them (an empty created
  folder among them follows the nearest counted folder above it). Safe either way: the sweep only removes an empty
  folder. Pinned by `selection_tests.rs::a_created_folder_goes_when_*`.
- **The bar**: `chosen()` units and bytes off the summary (`RollbackFilter::progress_totals`), the same rows the loop
  credits. **The queue row's name** comes from one row the selection takes (`Selection::sample_seq`: an `on` item, else
  a row under the deepest `on` folder), one point read, ❌ never a stream.
- **The skip pass, measured** (release, Apple Silicon laptop, 2026-11-08, `tree/scale_tests.rs`): the engine's page read
  streams a 2M-row copy in 2.62 s; with a three-file selection the pass takes 2.83 s. The decision adds 8%; the stream
  is the cost, and an unfiltered rollback pays it too, spread over its acts. The stream is NOT narrowed to the `on`
  subtrees (through the item indexes): that's a second streaming order in the data-safety core, worth it only if a
  narrow selection's wait on a multi-million-row op proves a real problem.
- Left-out rows keep the op `partially_rolled_back` (§ below), and the inverse's header counts only what the run
  attempted (`RunAcc::totals`), so "3 of 400" reads 3. Pinned by the same `selection_tests.rs`, each cell shown red on a
  mutant of the clause it guards; a demo cell (`cmdr-listing-wasm`'s `commands_test.rs`) pins that the demo's
  `rollback_operation` takes the selection and refuses an unknown field.

Rollback eligibility is narrow at both boundaries: mutation code journals rollback leaves only for committed rows, and
`read_rollback_units_page` streams only rows that await reversal (`done`, or `skipped` with a stored
`rollback_skip_reason`, § "The `rolling_back` state machine"). `restore_move` also refuses a unit that doesn't await
reversal. A row the operation itself skipped (no reason) must never make an untouched destination eligible for
rename-back, and a `rolled_back` row never streams again: Finish can't touch a file someone later put at a reversed
copy's path (`planner_tests/resume_tests.rs::finishing_never_touches_a_row_an_earlier_pass_reversed`).

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
`commands/operation_log.rs::undo_operations`; a yes to the MCP `operations_rollback` tool's question dispatches ONE op,
and its caller polls). It exists for the Ask Cmdr rename rail, where one run can approve several batches.

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

`rollback_operation` (the entry) reads the op, gates it (`check_rollbackable`: `UnknownOperation` / `IsARollback`
(first, § "No rollback of a rollback") / `AlreadyRollingBack` / `NotRollbackable(reason)` /
`VolumeUnavailable{volume_id}` — the cross-volume gate is computed from live mount state, never stored), then sets
`rolling_back` **as late as possible** (right before the spawn) and hands the plan to the injected `spawn`. The
double-rollback guard is automatic: a second request reads `rolling_back` and refuses. On a **synchronous spawn
failure** (a volume dropped between the gate and the spawn, so the inverse never starts) it resets `rolling_back` back
to the state the op held (`rollbackable`, or `partially_rolled_back` for a Finish) in the same call before returning the
typed error, so a retry isn't wedged.

`rollback_operation` is `rollback_operation_with_selection` with `None`; a selection is refused typed here too, before
`rolling_back` is set (§ "Streaming + ordering").

`execute_rollback` resolves the original op at the end (`resolve_final_state`, given the state the op started in and the
run's `RunTally { reversed, skipped, left_out }`): `RolledBack` (nothing skipped, nothing left out by a selection),
`PartiallyRolledBack` (any skip, even if nothing could be reversed, or any row the selection left out; "Roll back the
rest" retries them). Kani proves `RolledBack` never comes with a skip or a left-out row (`rollback/proofs.rs`). A run
that STOPPED with nothing reversed leaves the op as it found it: `Rollbackable`, or still `PartiallyRolledBack` when an
earlier run reversed some of it. A journal page that fails to read (busy past its timeout, an undecodable row, an I/O
error) is a stop too: the run never saw what lies past it, so it skips the directory sweep (unread files may sit inside
those dirs), finalizes the inverse `Failed`, and resolves like a cancel, keeping what it reversed. One bad row fails its
whole page rather than becoming a per-item skip: a newer build's token can't reach this build (a downgrade is refused),
and a skip would need a new `SkipReason` for a row with no readable path. ❌ A run over a partial op never resolves
`RolledBack` on an empty pass: finding nothing to retry proves nothing came back. It marks the original's items
`rolled_back`/`skipped` and journals the inverse op's own item rows (reversed ⇒ `done`, skipped ⇒ `skipped`), which
drive the reconcile.

**Startup reconcile** (`reconcile_rolling_back_on_open`, called by `OperationLog::open` before the journal is reachable)
resolves any op a crash left `rolling_back` (`reconcile_one`). It's `partially_rolled_back` if ANY of: the original has
a `rolled_back` row; the original has a stored `rollback_skip_reason` (an earlier pass held it back); or ANY inverse of
it (every op whose `rolls_back_op_id` names it) has a `done` row. Else `rollbackable`, which covers a crash after
setting `rolling_back` but before the inverse op opened. For an op whose tree summary is built, the original's two
clauses are read off its volume-root summary rows (`units_rolled_back + units_held_back`, kept exact in the outcome
transactions themselves, § "Tree summary"); an unbuilt op pays the `EXISTS` over its rows (`has_rollback_outcome`), and
the inverse clause is always an `EXISTS` (`any_inverse_reversed`). Each stops at the first hit; a read that fails
resolves partial. Why every source:

- **The original alone misses a crash mid-flush**: `RunAcc::flush` sends the inverse's rows (`record_items`) and THEN
  the original's outcomes (`set_item_outcomes`), so a crash between them leaves `done` inverse rows over untouched
  original rows.
- **The newest inverse alone misses a crashed second pass**: Finish (and partial rollback) open a fresh inverse per
  pass, and one that crashed before reversing anything holds no `done` row while an earlier pass's inverse does.
- **An inverse alone misses a dropped row**: the writer drops a row it can't insert and `flush` still sends the
  original's outcomes.
- **No prefix cap**: an inverse whose first 10,000 rows are skips can hold its `done` row past them.

Pinned one cell per clause (`cmdr-operations`' `planner_tests/lifecycle_tests.rs`, `reconcile_guard_*`, which seed an
unbuilt op so the row clauses decide; the summary clause's cell is `src/tree/maintenance_tests.rs`). Either way a
re-issued rollback resumes idempotently: every per-item inverse is a recheck-then-act, and an already-reversed item
reads as `AlreadyGone` (counted as a no-op success).

That re-entrancy is load-bearing beyond the crash path: the history dialog's **Finish rolling back** button on a
`partially_rolled_back` row is nothing but a second `rollback_operation` call on the SAME op, which the gate admits. The
item loop streams every row that still awaits reversal (`RollbackUnit::awaits_reversal`): `done` rows, plus `skipped`
rows carrying a `rollback_skip_reason` (held back by an earlier run). A `skipped` row with no reason is the operation's
own skip (a bulk rename's blocked row) and never streams. Rows already `rolled_back` aren't re-read; a crash-resumed run
whose outcomes didn't flush re-reads them as `AlreadyGone`. ❌ So don't make `check_rollbackable` refuse
`PartiallyRolledBack`, and ❌ don't let any inverse act without its recheck: either one turns that button into a
double-act. The UI side is `apps/desktop/src/lib/operation-log/DETAILS.md` § "a partly-reversed operation offers "Roll
back the rest"".

### Operations a crash interrupted

At open no engine runs yet, so every op still `queued` or `running` is one a crash interrupted. Left alone it read
"still running" forever: no `ended_at`, so retention by age never pruned it, its tree backfill refused it as running,
and it sat at the `not_rollbackable` it opened with (NULL reason). `interrupted.rs::end_interrupted_on_open`, called by
`OperationLog::open` AFTER the reconcile (which reads a stale inverse's rows) and before the journal is published (so
nothing this launch journals can look stale), ends each one in one writer statement (`end_interrupted`, a no-op once the
op ended):

- `failed` ("Didn't finish"), `ended_at = started_at`: the only time the journal knows. It never claims a duration it
  didn't see, and never jumps to "now" to outlive its retention.
- Its rollback state as finalize would have decided it, failing safe where the deciding facts were never written:
  `delete` ⇒ `not_rollbackable(permanent_delete)`; a copy with any `overwrote` row ⇒ `not_rollbackable(overwrote)` (an
  `EXISTS`, paid once per interrupted copy); `move` and `archive_edit` ⇒ `not_rollbackable(journal_incomplete)`, since a
  directory merge, a staged conflict resolved at the destination, or a compress over an existing archive is a driver
  note finalize never got, and guessing could carry untouched files away; every other kind (a copy without overwrites,
  trash, rename, create, tag) ⇒ `rollbackable`, since every inverse rechecks before it acts and the rows the crash lost
  simply aren't reversed.
- An interrupted INVERSE ends the same way but keeps its stored state: it's never rolled back, and its original's state
  is the reconcile's call.
- Search coverage becomes `top_level_only(interrupted)`: rows the op never got to write are missing, and a coverage note
  it held never reached finalize, so it can't honestly claim `full`. The tree words a folder unit with no recorded
  contents as `NotRecorded` off the same coverage (`tree/layout.rs`).
- `operations` has no status index: the step scans it once per launch, as `ops_in_rolling_back` does (the table is small
  next to its items).

Pinned by `src/interrupted/tests.rs` (every kind, an interrupted inverse, the open ordering, a finalize that landed
first, aging from the start),
`tree/backfill_tests.rs::an_op_a_crash_left_running_backfills_once_the_next_launch_ends_it`, and, end to end through the
gate and the executor, `cmdr-operations`'
`planner_tests/lifecycle_tests.rs::a_copy_a_crash_interrupted_rolls_back_what_it_recorded`.

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

**Each leaf also records the landed file's OWN mtime** (`journal::LandedTimes` and `record_volume_transfer_source` in
`crates/cmdr-operations/src/journal.rs`). Size alone passed a file edited in place at the same length (a VM disk image,
a SQLite database, a re-saved document), and undoing the copy deleted it with the user's edits (#519).

- **The destination's own time, ❌ never the source's.** A destination that doesn't keep the source's date (WebDAV on
  Apache, an Android phone over MTP) would differ from a read-time capture on every leaf, flipping each one to `Drift`,
  which reads to the user as "you changed this file" and refuses.
- **❗ The contract: what a row records is what the undo's recheck reads**, `Volume::get_metadata`. Two sources keep it.
  The write's own report comes first: a backend that knows the time `get_metadata` will answer returns it on
  `StreamWritten::landed_at` (`LandedModified::Known`), it rides the leaf's `WrittenFile::landed_modified`, and nothing
  is read back. Otherwise the time is read back: one `get_metadata` for a FILE source, and for a DIRECTORY source one
  `list_directory` per destination folder holding a file whose write reported nothing. A listing agreeing with
  `get_metadata` is the backend's half of the contract, held by
  `cmdr_fs::volume::conformance::assert_landed_time_agrees_with_get_metadata` on every backend.
- **Why the write reports, on S3 (#651).** A listing's `LastModified` is the upload time; a HEAD answers the source's
  own date the copy stored in `x-amz-meta-mtime`. Reading a folder copy's times from a listing made every file `Drift`
  whose source wasn't saved seconds before the upload, so the undo reversed nothing (failing safe: it deleted nothing
  either). A HEAD per file would agree but bills a request each, so the upload reports the date it sent instead
  (`crates/cmdr-s3/DETAILS.md` § "Dates"), and an S3 folder copy reads nothing back. Every other backend's listing and
  `get_metadata` come off the same server answer (verified 2026-10-21 by the conformance cell on local, SMB, SFTP,
  WebDAV on Nextcloud, MTP's virtual device, ADB's fake, and S3 on VersityGW and Garage), so they report `Unknown`.
- **A report holds only for the name the write went to.** A staged write's time is its temp's, and the landing rename
  isn't promised to keep it, so the engine keeps a report only when the write's target was the final name
  (`stream_pipe::landed_at_final_name`); a landing settled after the fact (a lost reply, a cut-off rename) reports
  `Unknown` too.
- **Bounded per source, never per file.** A listed name is only matched against the files the source wrote, never joined
  onto a path. Only a journaling transfer reads anything back.
- **A stopped or failed source reads nothing back** (a stop never waits on the share), so its rows keep what the writes
  reported, else the size-only check; so does a file the read-back can't place (a listing that failed, a name the
  listing spells differently), a symlink, and a backend that reports no mtime (`None`, which `verify_snapshot` skips
  over).
- `verify_snapshot` is flat over the row: every recorded field must verify, and at least one must, with no
  inner-versus-top-level branch. Local-FS leaves record the preserved source mtime, which their write path keeps.
- The in-flight reversals (`crates/cmdr-operations/src/reversal.rs`) keep the size-only volume recheck: their ledger
  carries no mtime by design (`crates/cmdr-operations/src/transfer/DETAILS.md` § "What the in-flight ledgers record"),
  and they run seconds after the write, not days.

Pinned by `crates/cmdr-operations/src/journal_capture_volume_tests.rs` (`a_serial_…`, `a_concurrent_…`, the cross-volume
move, `undoing_a_volume_copy_keeps_a_file_edited_since_at_the_same_length`, and the S3-shaped double's
`…_store_listing_upload_times_…` and `an_interrupted_folder_copy_records_the_landed_times_its_writes_reported`), and end
to end by `s3_engine_integration_test.rs::s3_integration_a_finished_copy_onto_a_bucket_rolls_back_*`, whose tree is
dated days back so the listing and the HEAD can't coincide.

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
`SELECT op_id FROM operations WHERE initiator='user' AND rollback_state='rollbackable' AND rolls_back_op_id IS NULL ORDER BY ended_at DESC LIMIT 1`
then `dispatch_rollback`. The two-axis status + `rolls_back_op_id` linkage make it a query, not a new engine. There's no
redo (§ "No rollback of a rollback"). Don't build it; don't preclude it.

## Query API + search

`query.rs` is the read side: short-lived read-only connections, index-served name search, and paged reads for the Debug
panel / alpha dialog / MCP tools. Two kinds of caller open their own short-lived read connection and forward to
`query.rs` (business logic — filtering, paging, dir-path resolution — lives there, never in the callers): the FE IPC
surface is three thin pass-throughs (`commands/operation_log.rs`): `get_recent_operation_log_entries(limit, offset)`,
`get_operation_log_detail(operation_id, item_limit, item_offset)`, and `get_operation_log_tree(operation_id, request)`
(§ "The tree view"); the MCP `operations_list` / `operations_get` handlers (`tools/actions/operation_log.rs`) do the
same off the MCP task.

- **A rollback is never listed as its own operation**: the feed and search take `rolls_back_op_id IS NULL`, and the
  original carries the state and the running reversal's id (§ "No rollback of a rollback").
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

## Tree summary (`src/tree/`, `src/writer/tree_summary.rs`)

The Operation log's item tree (#1033) opens any op, 2M items included, by reading a handful of rows: per (op, folder)
subtree totals in `operation_tree_dirs`, kept exact by the writer.

- **The axis** (`TreeAxis::of(kind, is_inverse)`, an exhaustive match): `Dest` for copy, move, create, archive edit;
  `Source` for trash, delete, rename, tag, and every inverse (its rows carry no dest). A row missing its axis side falls
  back to the other one (`TreeAxis::side`), so the tree may be a forest, one root per volume, and no row is ever left
  out. An inverse whose original retention pruned loses its `rolls_back_op_id`, reads as a plain `Delete` / `Move`, and
  still lands on its source side, since its rows have no dest.
- **Roles** (`unit_role`, row-pure): `Contents` (`search_only`), `Settled` (rolled back, or the op's own skip or
  failure), `NotReversible` (no inverse: a delete's), `Housekeeping` (a created-in-place DIR row of a copy or move, the
  folder a transfer made), else `Selectable`. Op-level rollbackability is applied by readers at query time, ❌ never
  stored: eligibility is written at finalize, after the rows.
- **One row per folder, ancestors included.** `(op_id, dir_id)` WITHOUT ROWID, plus
  `(op_id, parent_dir_id, name_folded)` so a folder's child folders come off the index in order. Columns:
  `parent_dir_id`, `name_folded`, `self_seq`, `child_dirs` (child rows in this op), `direct_files` (FILE rows directly
  inside), and the subtree totals `files` / `bytes` (FILE rows of both roles, `MAX(size, 0)`), `units_selectable`,
  `unit_files_selectable` / `unit_bytes_selectable` (the reversal's progress totals, as `read_rollback_file_totals`
  counts them), `units_rolled_back`, `units_held_back`, `units_unfinished` (all over `rollback_unit` rows, housekeeping
  included), and `housekeeping_waiting` (why "Roll back the rest" still has work at 0 of 0). Per-skip-reason counts are
  NOT stored per folder; a row carries its own.
- **A DIR row is its own folder.** `operation_items.self_dir_id` is the folder at the row's AXIS-side path, interned by
  the writer at insert (`store::intern_child_dir`, the child of the already interned parent, so a `scheme://` root
  survives). The row counts in that folder's subtree, never its parent's, so unchecking a trashed folder subtracts
  exactly its unit, and its summary row carries `self_seq` (the lowest seq when several DIR rows share a folder). ❗ A
  same-FS move's unit owns its LANDED folder; a source-side id would never match the folder the tree hands out.
- **Maintained in the rows' own transaction.** `RecordItems` adds exactly the rows that inserted (a dropped row never
  counts); `SetItemOutcomes` reads each row's old fields by `(op_id, seq)` on the writer, takes the old contribution
  away, adds the new, and bumps `outcome_generation`. Each delta walks the dir chain once per distinct folder. A new
  folder row adds one to its parent's `child_dirs`.
- **A summary write that fails never costs a row.** The delta rolls back to its savepoint and the op's `tree_summary`
  goes NULL in the same transaction; the writer then maintains nothing for it until it's rebuilt. So does a DIR row
  whose own folder didn't intern (the row still inserts, with no `self_dir_id`). Pinned by
  `src/tree/maintenance_tests.rs` with a test-made `RAISE(ABORT)` trigger (no production fault seam).
- **Exact means equal to `summary::aggregate_op`** (test-only), the from-scratch aggregate over the op's rows in `seq`
  chunks, made of the same `read_facts_after` and `TreeDelta::into_rows` a backfill steps through. Golden fixtures with
  hand-written expected rows: `src/tree/tests.rs`.
- **Unbuilt ops and the backfill seam** (`src/tree/status.rs`): every op journaled before v6 reads `tree_summary` NULL.
  `read_tree_summary_status` answers `IndexesBuilding` (below), `Built`, `Unbuilt(OperationRunning | RollingBack)` (a
  backfill can't start: inserts don't bump the generation, and a rollback would make it restart on every batch),
  `Building(progress)` from the `BackfillRegistry`, or `Unbuilt(NotBuilt)`.
- **The backfill** (`src/tree/backfill.rs`, writer half `src/writer/backfill.rs`): on demand, the first time someone
  opens an unbuilt op's tree. Decision/Why on demand: only ops someone opens pay, and a whole-journal background pass
  stays a later option.
  - **A chunk loop the host drives.** `TreeBackfill::start(&log, op_id)` answers `TreeBackfillStart` (`Started` with the
    job, `Joined`, `Built`, `Unbuilt(reason)`, `UnknownOperation`); each `step()` reads one `seq` chunk (4,096 rows) on
    the job's own read connection into an in-memory per-folder delta and answers `Progress { done, total }`, until
    `Finished(TreeBackfillEnd)`. The desktop drives it on the blocking pool, the demo between turns of the page's event
    loop (no thread); both emit every step as `operation-log-tree-backfill` (`TreeBackfillEvent`). ❌ Never aggregate on
    the writer thread: a 2M-row read there blocks `record_items` and, through backpressure, a running copy.
  - **Pre-v6 DIR rows get their own folder on the writer.** A read connection can't intern, so a chunk holding DIR rows
    with no `self_dir_id` sends their seqs as `FillSelfDirs` (same axis side as an insert; skipped for a built op, whose
    summary placed them in their parent), then reads the chunk again. A cancelled run keeps what it filled; it's
    harmless and the rerun skips it.
  - **The result applies only if nothing moved.** `WriteTreeSummary` replaces the op's summary rows and stamps `built`
    in one transaction, only if the op exists (`Gone`: retention pruned it, so no summary row keeps its dirs alive),
    isn't built (`AlreadyBuilt`), and its `outcome_generation` still equals the value read before the first chunk
    (`Stale`: a rollback landed outcomes, so the job reads again, or stops with `Unbuilt(RollingBack)` if one is
    running). A prune that also took the op's folders mid-read ends `Gone` too, never `Unreadable`.
  - **One per op, cancelable.** `BackfillRegistry` (one per `OperationLog`, `tree_backfills()`) claims the op at start;
    a second start joins, and takes back a cancel the closing dialog asked for, so reopening it keeps the work. A cancel
    stops the job at its next step, forgetting the claim in the same lock that saw it; a finished or dropped job frees
    the op.
  - **It doesn't wait for the folder indexes**: the job reads by `(op_id, seq)` and its write queues behind
    `BuildTreeIndexes` on the writer, so `start` reads the op's own state rather than `read_tree_summary_status`, which
    puts `IndexesBuilding` first.
  - Pinned by `src/tree/backfill_tests.rs` (equal to the writer-built rows for a copy, a trash, and a same-FS move;
    axis-side own folders; cancel, join, drop, stale restart, a rollback starting, a prune, a summary broken mid-copy,
    the indexes still building, a 20,000-row run inside the two-second bar) and the demo's
    `an_old_op_backfills_its_tree_summary_and_says_so`.
- **The reconcile reads it.** A built op answers the "original settled a row" clause from its volume-root rows instead
  of an `EXISTS` over the op (§ "The `rolling_back` state machine + startup reconcile").
- **Prune and GC**: a pruned op's summary rows go first (they reference the op and its dirs), and `gc_orphan_dirs` keeps
  any dir a summary row or a `self_dir_id` names: a DIR unit's own folder is often referenced by nothing else.
- **The two item indexes are built by the writer after open, never in the ladder** (`src/store/tree_indexes.rs`).
  Decision/Why: `OperationLog::open` runs inside `setup()` on the app's main thread, before the event loop runs and the
  window can show, and these indexes are the only part of v6 that scales with the journal. So `open` queues
  `BuildTreeIndexes` on the writer right after the reconcile (whose writes block on their reply), and operations journal
  behind it through the channel's backpressure. Until both exist, `read_tree_summary_status` answers `IndexesBuilding`;
  once the build settles, the writer tells the log's `BackfillRegistry` (`settle_indexes`; `when_indexes_settled`
  answers at once if it already has, else on the writer thread after the build), and each host turns that into one
  `operation-log-tree-indexes-settled` event, so the tree reads again on it rather than polling. A build that couldn't
  finish (a full disk, a name in the way) settles too, and the tree answers `IndexesUnavailable` for the rest of the
  launch; every open queues the build again (pinned by
  `a_folder_index_build_that_cant_finish_says_so_and_runs_again_next_launch`). `IF NOT EXISTS` makes later launches a
  schema lookup. The build logs its duration at info, as every ladder step does.
- **Upgrade cost** (`src/store/migration_bench.rs`, release, a seeded 4M-row v5 journal of 1,053 MB, Apple Silicon,
  2026-11-07): the migrating open on the main thread takes 1.2 ms; the writer's index build takes 4.5 s, once; the WAL
  reaches 386 MB during it; the file grows 384 MB for good, about 96 bytes or 96 MB per million rows (half the seeded
  rows carry a dest). The `ALTER`s and the empty table are free. A journal at the 3 GB retention default holds about 11M
  rows, so its first launch after the upgrade journals about 13 s late. Rerun command and scaling:
  `docs/notes/operation-log-capture-bench.md` § 5. Measured before the indexes went files-only (below), so it's an upper
  bound: DIR rows are no longer in them.
- **The item indexes hold FILE rows only** (`WHERE entry_type = 'file'`). Decision/Why: a folder's DIR rows list as its
  child folders off the summary, so the file listing filters them out, and a filter the index can't answer reads the
  table for every row an `OFFSET` skips. The middle of a 1M-file folder took 800 ms that way and takes 10 ms index-only
  (release, Apple Silicon, `src/tree/scale_tests.rs`, 2026-11-07).

## The tree view (`src/tree/view.rs`, `layout.rs`, `read.rs`, `selection.rs`)

`query::operation_tree(conn, op_id, request, registry)` answers one window (≤ 200 rows) of an op's item tree,
statelessly: the request carries the frontend's hand toggles (expanded folder keys, the selection, the cursor key) and
an offset; the answer carries every row with everything it renders, the open rows above the window (`ancestors`), the
cursor's index, the op's tally, and the selection normalized. Both hosts answer it as `get_operation_log_tree` (the demo
with `deny_unknown_fields` on its args and on the request). Anything but a built summary (`IndexesBuilding`,
`Building { progress }` from the host's `BackfillRegistry`, `Unbuilt { reason }`) comes back with no rows.

- **Keys**: `Folder { dir_id }` (the deepest folder of a compacted chain) and `Item { seq }` (a file row). Stable across
  requests: compaction depends only on inserts, which a finished op no longer gets. A folder unit is a folder row keyed
  by its own dir; its checkbox is that folder's toggle.
- **The top**: each volume root descends while a folder holds no files and one child folder, stopping ABOVE any folder
  an item row names (`self_seq`), so a trash of one folder shows that folder as the one top row with its checkbox. One
  volume root: its descended folder is `root_path` and its listing is the top. Several: a forest, one top row per
  volume, labelled with its full path, and no `root_path`.
- **A listing**: child folders first (off the summary's child index), then files (off the item index of the op's axis),
  each by folded name in byte order, ties by id. A dest-axis folder also lists the rows that fell back to their source
  side: its file query is a `UNION ALL` of the two index ranges that SQLite merges in order, never sorts.
- **Compaction**: a folder with no files and one child folder shares the child's row (`a/b/c`), unless either is a
  folder an item row names: a trashed `A` holding only folder `B` is two rows, since `A` is what comes back.
- **The window**: rows under an open folder = its listing + its open children's rows, summed over the expansion only;
  the first row is found by descending through those sums (O(expansion)), then rows are read forward from the listings,
  each at most `limit` at a time. An expanded key under a closed folder stays a hand toggle but counts nothing.
- **Inside one folder, an `OFFSET` is O(k)**, so past a folder's midpoint the read runs from the end (`plan_range`),
  capping the walk at half the folder. Child folders read their offset's key index-only, then keyset from it: an
  `OFFSET` over the WITHOUT ROWID summary seeks the table for every skipped row (50 ms at the middle of 100,000 child
  folders, 2 ms keyset). The cursor's position counts from the nearer end the same way.
- **Rows**: label, level, `pos_in_set` / `set_size`, `role` (`TreeRowRole`, from `unit_role`), outcome and skip reason,
  checkbox (on, off, mixed, none), a file's size, a folder's `TreeTally`, full path, and `other_end` only when the row's
  other side isn't `other_root` + its relative path (a conflict rename), never for a trash. `other_root` is read off the
  first of the op's first 64 rows with both sides. A `contents` row shows its folder unit's outcome and checkbox, never
  its own `done`. A folder with nothing listed can't unfold and says why: a folder unit on a `top_level_only` op is
  `capped` (the op held a folder of over 50,000 files: op-level, never "this folder") or `notRecorded`; anything else
  was `empty`.
- **Read-only**: an op that isn't `rollbackable` or `partiallyRolledBack` (so also one `rolling_back`) has no
  checkboxes, applied here at query time.

### The selection (`src/tree/selection.rs`)

`RollbackSelection { base: Everything | Nothing, toggles: [Folder { dir_id, on } | Item { seq, on }] }`, `null` meaning
everything. A row's decision is its own item toggle, else the nearest folder toggle up its folder chain (a folder unit's
chain starts at its own dir, any other row's at its axis folder), else the base. `Selection` is THE resolver: the view's
counts and checkboxes and the engine's per-row decision (`includes`) both go through it.

- **Normalized on every read, never trusted from the wire**, in order: a toggle naming no summary row or no `Selectable`
  row of THIS op goes (a foreign dir id, since `dirs` is global; a `search_only`, housekeeping, settled, or unknown
  seq); an item toggle on a folder unit becomes its folder's toggle; a folder toggle strictly inside a folder unit goes
  (contents have no checkbox); a later folder toggle covers every earlier toggle inside or on it, and a later toggle of
  an item replaces an earlier one, so "nearest wins" equals "latest wins"; a toggle equal to what it inherits goes, as
  does a folder toggle over no selectable unit; past 2,000 the newest go and the answer says `toggles_capped`. The
  answer carries the normalized selection, which the frontend sends straight back.
- **Counted by algebra, never by a scan**: the toggles' folders and their ancestors form a small forest; each folder in
  it gets its selected (units, files, bytes) from its children's and its own item toggles, every folder outside it is
  all-or-nothing by its decision, and the op's totals are its volume roots'. Units and bytes are a selective reversal's
  progress totals, the rows `read_rollback_progress_totals` counts for an unfiltered one. Tri-state: `none` at zero
  selectable, else on, off, or mixed by selected against selectable.
- **The engine's decision** (`includes`): one HashMap lookup per row plus one summary read per distinct folder (cached).
  A housekeeping folder goes along only when its decision is on AND everything selectable under it is selected, since a
  folder still holding a file can't be emptied.
- Pinned by `src/tree/selection_tests.rs`, including a brute force over paths that shares no code with the resolver (200
  seeded random selections, "N of M" and every row's decision).

### Scale (`src/tree/scale_tests.rs`)

- **Structural gate, always on**: every statement the tree reads with runs under `EXPLAIN QUERY PLAN` with no table scan
  and no temp b-tree, and every interactive read (open, unfold, the start, middle, and end of a big folder, the middle
  of a folder of folders, the cursor, three "N of M" shapes, the engine's decision for 2,000 rows) keeps its SQLite VM
  steps flat between a 3,000-row and a 30,000-row journal whose opened folders stay the same size. Why steps, not ms:
  test builds run SQLite at `-O0`, and an O(op) read through an index `SEARCH` passes any plan check (a full count over
  the op's rows went 10,792 → 91,679 steps and failed it).
- **Timed, release** (`two_two_million_row_ops_answer_inside_their_budgets`, ignored; its doc comment has the command):
  a 2M-file delete (one folder of 1M files, one of 100,000 child folders) and a 2M-file folder copy, seeded in about 40
  s. Measured (Apple Silicon, 2026-11-07): open 0.2 ms, unfold 0.6, the middle of the 1M-file folder 15, its end 0.4,
  the middle of 100,000 child folders 2, the cursor in the 1M-file folder 29, "N of M" with the copied folder off 0.3,
  with three files on 0.3, with 2,000 items off 23. The engine's skip pass (its own page read and filter) over 2M rows
  for a three-file selection took 2.83 s against 2.62 s to stream the rows alone (2026-11-08): the decision is cheap,
  the stream is today's cost of any rollback (§ "Streaming + ordering"). Budgets are about three times those; the twin
  at 20,000 rows runs every time without budgets.

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
  `crates/cmdr-menu/src/ids.rs` (`menu_id_to_command` + `command_id_to_menu_id`) plus its row in
  `crates/cmdr-menu/src/bar.rs` (**View menu**, after the command palette). The other places one command touches, and
  how a miss fails silently: `apps/desktop/src/lib/commands/CLAUDE.md` § Gotchas.
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
- **The tree's indexes cost history.** The two v6 item indexes add about 96 bytes per item row (§ "Tree summary"), so
  under the same byte budget a big journal keeps roughly a quarter fewer rows than before v6. The summary table itself
  is a few rows per folder per op, noise beside that.
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

`index-crate-isolation` holds the surface to 11 root promises, 7 `OperationLog` methods, seven public modules, and 95
items in them, with no headroom: measured at the extraction, re-measured when the journal API took `OperationId`,
tightened when the test-only accessors went behind `cfg(test)`, raised for the item tree's backfill (`tree_backfills`,
and in `query` the backfill job, its four wire types, `BackfillRegistry`, `BackfillProgress`, and `UnbuiltReason`; start
and cancel live on `TreeBackfill` and the registry, not the handle), and raised for the item tree's window (§ "The tree
view"), whose request, answer, and selection vocabulary both hosts pass through, and raised for partial rollback
(`rollback_operation_with_selection`, `execute_rollback_with_selection`, `SelectionRefusal`; § "Streaming + ordering")
(`scripts/check/checks/index-crate-isolation-ceilings.go`). A helper only this crate's tests call stays `#[cfg(test)]`
(`OperationLogStore::conn` and `schema_version`, `query::coverage_is_complete`), so the count promises nothing to no
caller. By module:

- **Root**: `OperationLog` (the handle), `OpJournal`, `OpenError`, `new_operation_id`, and the seven modules below. The
  handle's methods are `detached`, `open`, `set_recording`, `writer`, `spawn_retention`, `journal_open`, and
  `tree_backfills`; an `OpJournal` records, notes, and finalizes its one operation.
- **`store`**: the DB path, `fold_name`, the error type, the row types, the migration ladder, and the low-level reads
  the IPC commands, the MCP tools, the dump tool, and the planner's app-side cells use.
- **`query`**: the paged history read and indexed name search behind the IPC commands and MCP tools, the item tree's
  backfill job and the registry the hosts drive (§ "Tree summary"), and its window (`operation_tree`) with its wire
  types (§ "The tree view").
- **`writer`** and **`capture`**: the writer thread and the journal the write engine records through.
- **`types`**: the typed classification tokens, which cross IPC, and `OperationId` (with its `as_str`, and a
  `Borrow<str>` so an id-keyed map answers a wire row's text), which the journal API and the write engine's public verbs
  take: `OpenOperation`, the `OperationJournal` seam, and `OpJournal::op_id`.
- **`rollback`**: the planner (`check_rollbackable`, `rollback_operation` and `execute_rollback` with their
  `_with_selection` twins and `SelectionRefusal`, `undo_order`, the reconcile), its `RollbackRunner` / `VolumeLookup`
  seams, and the fail-safe verdicts the engine's in-flight reversals share (`verify_snapshot`, `SnapshotVerdict`,
  `ItemResult`, `SkipTally`).
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
