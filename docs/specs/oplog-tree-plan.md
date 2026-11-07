# Operation log: a selectable item tree and partial rollback

Issue [#1033](https://github.com/vdavid/cmdr/issues/1033) is the spec: read it with its comments first (David's
decisions, the Scale acceptance criteria). Design mockup (private): the issue's artifact link. This file is the
just-in-time execution plan over it.

Survey taken 2026-11-07 on `main` at `e1bbec95ad`. Re-grep before each milestone: the code moves under you.

## Loud rules

1. **Data safety first.** Rollback is the one feature here that deletes and moves user files. Every per-item inverse
   stays recheck-then-act (`verify_snapshot`, the occupied-target guard), ❌ never weakened, forked, or skipped for a
   "selected" item. A selection only ever REMOVES rows from the stream; it never adds one `awaits_reversal` wouldn't.
2. **Characterize first.** Milestone 1's pins land before the engine gains a filter, and stay green UNCHANGED through
   every later commit. A pin that has to change its assertion is a behavior change: say so in the commit body and report
   it.
3. **❌ Never edit or renumber a shipped migration.** New schema is a new `Migration` step at the end of
   `crates/cmdr-operation-log/src/store/migrations.rs`, written against the v5 shape.
4. **Backend owns the tree.** Expansion, compaction, windows, rollups, tri-state, and "N of M" are computed in Rust; the
   frontend holds only its hand toggles (expanded rows, selection toggles) and where it's scrolled, exactly like
   `crates/cmdr-operations/src/transfer_tree/`. ❌ No tree model in TS.
5. **Scale is an acceptance criterion, not a nice-to-have.** Two 2M-item ops in the journal: every read is O(expansion +
   window), never O(op). ❌ No full-op scan on any interactive path. The scale test (milestone 4) is the gate.
6. **Business logic lives in `cmdr-operation-log`.** The desktop command
   (`apps/desktop/src-tauri/src/commands/ operation_log.rs`) and the demo's
   (`crates/cmdr-listing-wasm/src/operations_host/commands/history.rs`) are thin pass-throughs. Every new or changed
   command is answered by BOTH hosts; the demo has no threads (`rt.rs` and the inline writer), so nothing new may assume
   one.
7. **Typed end to end.** New tokens live only in `types.rs`; new wire enums are specta types; ❌ never classify by
   string. Every user-facing word comes from a typed value through `operation-log-labels.ts`.
8. **English strings only, each with a good `@key` description**; a translator agent follows later. Count-free
   confirmation (house rule). No "error"/"failed" in copy.
9. **No milestone tags** in code, comments, `reason` strings, test names, or docs.
10. **Foreground commands only.** Gates per commit: `pnpm check --fast && pnpm check svelte-tests-related`, plus
    `pnpm check clippy` and the Rust tests you touched when you touched Rust. ❌ Never a local full `pnpm check`.
11. **Test timing** (David, 2026-11-07): every test this effort adds holds `docs/testing.md`'s two-second bar, except
    the 2M scale test, whose budget is about five seconds measured on a saturated machine, with a comment saying why.
    Over five seconds it's `#[ignore]`d with a how-to-run comment, AND a 20,000-row twin runs normally.
12. **Grab adjacent clear wins.** Each executing agent may fix obvious bugs and add missing tests in the area it touches
    (that's how this effort pays down a lightly tested data-safety feature). List what you grabbed in your report.
    Anything with a tradeoff or a wide surface: report it, don't do it.
13. **Ask yourself "is this solid AND elegant AND documented?" before reporting done.** Update every `CLAUDE.md` /
    `DETAILS.md` your change touches, in the same commit.

## Survey: what's there now

### Backend (`crates/cmdr-operation-log`)

- **Schema v5.** `dirs` (interned, `parent_dir_id` chain to a `''` volume root; identity on the exact name),
  `operations` (WITHOUT ROWID, two-axis state), `operation_items` (per row: `seq`, `entry_type`, `row_role`,
  `source_dir_id` + `source_name(_folded)`, nullable `dest_dir_id` + `dest_name(_folded)`, `size`, `mtime`, `outcome`,
  `rollback_skip_reason`, `dest_inode`/`dest_birth_ns`, `tags_before`/`tags_after`). Item indexes: `(op_id, seq)`,
  `(source_name_folded)`, partial `(dest_name_folded)`. **No index by dir.**
- **Row shapes per kind** (`DETAILS.md` § "Per-kind record points"; verify each in milestone 1):
  - copy and cross-FS move: one `rollback_unit` FILE row per leaf (source = original, dest = landed), plus one DIR row
    per created folder written created-in-place (`source == dest`, both on the DESTINATION side).
  - same-FS move and trash: one top-level `rollback_unit` row per selected item (a folder is ONE `dir` row; its `size`
    is the folder inode's `len()`, meaningless as bytes), plus `search_only` leaves from the drive index with
    `source = original/rel` and `dest = landed/rel`. Over `SEARCH_LEAF_CAP` (50,000), no leaves and the OP is
    `top_level_only` + `capped`. ❗ Coverage is op-level: nothing records WHICH folder was capped.
  - delete: one `rollback_unit` FILE row per leaf, op `not_rollbackable(permanent_delete)`.
  - rename, tag, create_file/create_folder, compress: one row per item.
- **The writer** (`writer.rs`): one thread owns the write connection; `apply_record_items` interns dirs and inserts in
  one transaction per batch; `apply_set_item_outcomes` updates `(op_id, seq)` rows in one transaction.
- **Rollback** (`rollback.rs`): `rollback_operation` gates (`check_rollbackable`), summarizes (`summarize_inverse`, off
  the FIRST unit), sets `rolling_back`, spawns. `execute_rollback` streams `read_rollback_units_page` (`seq DESC`, 512,
  only rows that `awaits_reversal`), files first, then buffered dir rows deepest-first, flushing outcomes per page;
  progress totals from `read_rollback_file_totals` (one aggregate); final state from
  `resolve_final_state(reversed, skipped, stopped, before)`, which Kani proves never says `RolledBack` with skips
  (`rollback/proofs.rs`, `proofs.mutants.json`). Inverse header count: `bookkeeping.rs::inverse_planned_item_count` +
  `RunAcc::totals`.
- **Startup reconcile** (`reconcile_rolling_back_on_open`, `reconcile_one`): resolves a stuck `rolling_back` op from its
  NEWEST inverse op's first 10,000 item rows.
- **Retention** (`writer/prune.rs`): deletes whole ops, then `gc_orphan_dirs` deletes dirs no item row references and no
  dir names as parent.
- **Where the tests are.** The engine's end-to-end cells live beside the real executor:
  `crates/cmdr-operations/src/rollback/planner_tests/` (lifecycle, resume, control, page errors, copy originals, trash
  identity, tags, case twins, root reach, blocked items, undo). Pure helpers: `src/rollback/decision_tests.rs`.
- **The app opens the journal synchronously in `setup()`** (`lib.rs` → `operation_log_host::start` →
  `OperationLog::open` → migrations). A slow migration step is a slow launch.

### Bug found while surveying (fix it in milestone 1)

`reconcile_one` resolves a crash during a SECOND pass wrongly. It reads only the newest inverse op: if that pass crashed
before reversing anything (or before its inverse row opened), the op goes to `Rollbackable` although an earlier pass
already reversed items. It also reads only the first 10,000 inverse rows, so `done` rows past that prefix are missed.
With partial rollback, second passes become the normal case. Fix: decide from the ORIGINAL op's own item outcomes (any
`rolled_back` row, or any `skipped` row with a `rollback_skip_reason`, ⇒ `partially_rolled_back`; else `rollbackable`),
one indexed `EXISTS`. The original's outcomes and the inverse rows flush together in `RunAcc::flush`, so the original is
the truth. Pin it red first.

### Frontend

- `apps/desktop/src/lib/operation-log/OperationLogDialog.svelte`: a 620 px accordion; per-op lazy item fetch capped at
  200 (`ITEM_PAGE`), `.item-path { max-width: 40% }`, a per-row "Done". Roll back per op via `RollbackConfirmDialog`
  (`finishing` prop), `RollbackControls` for a running reversal, relation links (`goToOperation` by DOM id), standing
  notices (`rowStandingNotice`), refusal notices. Paging: `operation-log-trigger.svelte.ts` (`entries.length` is the
  offset).
- `apps/desktop/src/lib/ui/ColumnList.svelte`: semantics `listbox` (focus stays outside; a combobox drives it), `table`,
  `grid` (root takes focus, `aria-activedescendant`, answers cursor keys via `list-cursor.ts`); windowed source with
  `getRow` + `onRangeChange` and placeholder rows; parent-owned cursor; fixed row height; `heldRowOf` teardown guard. No
  tree notion (no level, disclosure, or expansion).
- `apps/desktop/src/lib/file-operations/transfer-tree/TransferTree.svelte`: `role="tree"`, its own slot rendering and
  keyboard (`CURSOR_KEYS`, `expandOrEnter`, `collapseOrLeave`), follow/pin machinery. The backend view
  (`transfer_tree/view.rs`) is the model to copy: request = hand toggles + window, answer = rows + `ancestors` +
  `total_rows`, compaction at flatten time keyed by the deepest folder.
- `apps/desktop/src/lib/utils/shorten-middle-action.ts`: the middle-ellipsis action.
- Dialog-scoped keys are registry commands in `apps/desktop/src/lib/commands/sources/*.ts` with a scope like
  `'Main window/Multi-rename'`, matched with `eventMatchesCommand` (`sources/multi-rename.ts` is the pattern; the panes'
  `selection.toggleAndDown` is `Insert`).
- E2E: `apps/desktop/test/e2e-playwright/operation-log-rollback.spec.ts` (copy, move, drift partial, cancel + finish,
  stale row, typed refusals, progress, pause, cancel inside a big file). Its helpers find rows by `#op-head-<id>`; the
  rewrite changes those selectors.

## Design decisions (answered here; flagged ones need David)

- **Tree axis per kind** (one typed helper, `TreeAxis::of(kind)`): `Dest` for copy, move, create_file, create_folder,
  archive_edit; `Source` for trash, delete, rename, tag. **Why**: the tree shows where the thing the undo acts on lives.
  On the dest axis a copy's and a cross-FS move's file rows AND their created-folder rows sit on one side, so folders
  are real folders and nothing is "housekeeping on the other volume"; a same-FS move's unit and its `search_only` leaves
  both carry a landed dest. Trash stays on source (the Trash's internal names mean nothing to a person; the header says
  "to the Trash"). The row's other end shows as a secondary line only when it differs from `other_root + relative path`
  (a conflict rename `name (2)`), and never for trash.
- **Rollback roles, one typed function**
  (`unit_role(kind, row_role, entry_type, created_in_place, outcome, skip_reason)` →
  `Selectable | Housekeeping | Contents | Settled | NotReversible`):
  - `Selectable`: a `rollback_unit` that `awaits_reversal`, on a rollbackable op. Counts in "N of M".
  - `Housekeeping`: a created-in-place DIR row on a copy or move (the folder a copy made). Not counted, no checkbox of
    its own; it's the folder row, and it's included in a rollback only when its whole subtree is selected (else it can't
    be emptied, and trying would record a misleading `DirNotEmpty` skip).
  - `Contents`: a `search_only` row under a unit. Greyed, shows its unit's state, tooltip "comes back with its folder".
  - `Settled`: `rolled_back`, or the operation's own skip/failure. No checkbox; the status cell speaks.
  - `NotReversible`: any row of a not-rollbackable op. Read-only tree, no checkboxes.
- **Selection encoding: base plus toggles, nearest wins.**
  `RollbackSelection { base: All | None, toggles: Vec<SelectionToggle> }`,
  `SelectionToggle = Folder { dir_id, on } | Item { seq, on }`. A row's decision is its own `Item` toggle, else the
  nearest `Folder` toggle up its axis-dir chain, else `base`. `None` on the wire (the `Option`) means "everything", so
  the default "undo the whole thing" and the existing callers (MCP, `undo_operations`, Finish) send nothing. Capped at
  2,000 toggles (the transfer tree's hand-toggle cap); the backend NORMALIZES (drops toggles shadowed by a later
  ancestor toggle, drops no-ops) and returns the normalized list with every view answer, which the frontend adopts.
  **Why**: O(toggles) on the wire for any op size, keys the backend already hands out, and a nearest-ancestor rule is
  what tri-state checkboxes mean.
- **Summary table: subtree totals per (op, axis dir), maintained by the writer.** Recommended shape (agent may refine,
  document the final one):
  `operation_tree_dirs(op_id, dir_id, parent_dir_id, self_seq, child_dirs, direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, units_rolled_back, units_held_back, units_settled)`,
  `PRIMARY KEY (op_id, dir_id)` WITHOUT ROWID, plus an index on `(op_id, parent_dir_id)` for child listing. Subtree
  totals go on every ancestor up to the volume root (a handful of rows per op), so opening any folder reads one row and
  its children's rows. `files`/`bytes` count FILE rows of both roles (a folder unit's own `size` is an inode length,
  never bytes). A DIR item row gets its own summary row (`self_seq` names it), interning its own path if no child did,
  so a folder appears ONCE in its parent's listing (as a folder) and an empty or capped folder still has a row.
  - **Writer, on insert**: aggregate the batch per axis dir in memory, then one upsert per touched dir and ancestor, in
    the batch's transaction.
  - **Writer, on outcome change** (`apply_set_item_outcomes`): read each row's old outcome + axis dir (indexed by
    `(op_id, seq)`) inside the same transaction, apply the delta along its ancestors. This is the writer's read, not the
    engine's, so the engine still adds no per-row DB read.
  - **Per-reason skip counts are NOT stored per dir**: a row carries its own reason; a folder says "2 skipped". The op
    notice needs only rolled back / held back / still waiting. (Deviation from the issue's "counts per skip reason";
    cheap to add later.)
- **Item indexes** (same migration): `(op_id, source_dir_id, source_name_folded)` and partial
  `(op_id, dest_dir_id, dest_name_folded) WHERE dest_dir_id IS NOT NULL`. They serve "this folder's files, in order,
  from position k" on either axis. ❗ See question 2 on their launch cost.
- **Per-unit subtree coverage** (same migration): nullable `operation_items.subtree_coverage_reason`, written by the
  trash and same-FS-move record points from the per-item `BufferedLeaves` verdict they already hold. Only a unit whose
  own subtree was capped says "over 50,000 files"; an older row (NULL) in a `top_level_only` op gets a hedged line
  ("Cmdr didn't record what's inside this folder"), and an empty folder in a capped op isn't accused of holding 50,000
  files.
- **Backfill: on demand, per op, off every UI thread, cancelable, recorded.** A nullable `operations.tree_summary` token
  (`built`), set at open for every new op (the writer builds as it goes) and NULL for every pre-v6 op. Opening an
  unbuilt op's tree starts its backfill: a read connection streams the op's rows in `seq` chunks, aggregates per dir in
  memory (dirs are few), and hands the result to the writer as one `WriteTreeSummary` message, which applies it only if
  no outcome update touched that op since the backfill's first read (a per-op counter in the writer; a rollback racing
  it restarts the backfill), and stamps `built`. Progress events from the start, the UI shows them past ~10 s; cancel on
  op switch or dialog close. **Why on demand**: respects the user's resources (only ops someone opens pay), and a 2M-row
  op backfills in seconds. Whole-journal background backfill stays a later option.
- **Tree query: the transfer tree's flatten, over SQL.** Request:
  `{ expanded: Vec<RowKey>, selection, window: { offset }, limit ≤ 200 }`. Root = the deepest axis dir that holds the
  whole op with no direct files and one child dir (descend summary rows from the volume root). Each open folder's
  listing size = `child_dirs + direct_files`; locate the window by descending through open folders' sizes
  (O(|expanded|)), then walk forward: child folders from the summary index, ordered by folded name; files from the item
  index, `ORDER BY name_folded LIMIT ? OFFSET ?` within one folder. Compaction at query time (a folder whose only child
  is a folder and that has no files shares its row, keyed by the deepest dir id). Answer: rows (key, label, level, kind,
  expandable, expanded, pos_in_set/set_size, size or rollup, status, role, checkbox state on/off/mixed/none, full path,
  other-end path when it differs), `ancestors` (as the transfer tree's), `total_rows`, `root_path`, `other_root`,
  op-level counts (rolled back / held back / still waiting / selectable), `selected` + `selectable` ("N of M"), the
  normalized toggles, and `summary: Built | Building { done, total } | Unbuilt`. ❗ OFFSET inside one folder is O(k): a
  window deep inside a folder of 1M direct files scans ~k index entries. The scale test measures that case and the agent
  tightens it (a `(name_folded)` keyset on the previous window) only if it misses the budget.
- **"N of M" and tri-state from algebra, not scans.** Build the toggle forest by walking each toggle's dir ancestry (≤
  2,000 toggles × depth). Selected = Σ over the forest with each node's `units_selectable` from its summary row (or 1
  for an item toggle). A folder row is mixed iff some toggle strictly inside it flips a non-zero count. Rollback
  progress totals with a selection come from the same algebra over `unit_files_selectable` / `unit_bytes_selectable` (a
  selection only comes from a tree, so the summary is built; the command refuses a selection on an unbuilt op, typed).
- **Engine filter: a dir-id → decision cache.** `RollbackUnit` gains its axis dir id (+ for a dir unit, its own dir id).
  The cache walks `dirs.parent_dir_id` once per DISTINCT dir (dirs are few) to the nearest toggle; item toggles are a
  `HashMap<seq, bool>`; housekeeping dirs check a precomputed "has an excluded descendant" set. A row filtered out
  counts as `left_out`, is not acted on, not flushed, and not recorded.
- **Final state**: `resolve_final_state` gains `left_out`: `skipped > 0 || left_out > 0 || (resuming && reversed == 0)`
  ⇒ `PartiallyRolledBack` on a run that wasn't stopped. Update the Kani proof ("never `RolledBack` with items skipped,
  unattempted, or left out") and its mutants file.
- **Component: `ColumnList` tree mode, not a fork of `TransferTree`.** Having read both: `ColumnList` already owns the
  hard parts this dialog needs (column widths, windowed source with placeholders, `heldRowOf`, a focused-root grid with
  `aria-activedescendant`, `list-cursor.ts`). `TransferTree`'s keyboard is ~50 lines on top of slots tied to follow/pin.
  So: `semantics: 'treegrid'` (a `grid` plus `aria-level`, `aria-expanded`, `aria-setsize`/`posinset` per row), and a
  `tree` prop `{ levelOf(row), expandedOf(row): boolean | null, onToggle(index, expand) }` that renders indent +
  disclosure in the first column and answers ←/→ (expand-or-enter, collapse-or-parent, with an `onLeaveTree` for ← on a
  top row). Space/Insert/⌘A stay the consumer's (`onKeydown`). The op list also needs a listbox that owns focus: add
  that as a `focusable` listbox (same deferred active-descendant machinery the grid uses). Moving `TransferTree` onto
  this mode is out of scope.
- **Op list paging**: keep the array + offset paging (`operation-log-trigger.svelte.ts`), with "Load more" as a button
  under the list in the Tab order. **Why**: the journal has no cheap total, and a windowed source over offset paging
  buys nothing at 50 per page.
- **Ordering inside a folder**: folders first, then files, each by folded name in index (byte) order. ❗ See question 3.

## Questions for David

1. **The leftover wording after a partial rollback.** The mockup says "two you left out last time". The journal can't
   tell "left out by your selection" from "a cancelled pass never reached it": both are `done` rows still awaiting
   reversal. Recommendation: one honest phrase for both ("two haven't been rolled back yet"), from the summary counts.
   The alternative stores the last pass's scope on the op (one more column), and still can't split a cancelled selective
   pass.
2. **Launch cost of the new indexes.** The migration runs inside `setup()` on launch. Building two indexes over all of
   `operation_items` costs roughly a second or more per million rows (to be measured in milestone 2 on a 4M-row
   journal). Recommendation: pay it once in the ladder (a one-time slower launch for people with huge journals), log it,
   and only revisit if the measurement is bad. Moving `open` off `setup()` is NOT on the table: operations started
   before it opens would go unjournaled.
3. **Order inside a folder.** Byte order of the folded name (index-served, so a folder of a million files windows fast)
   rather than the panes' natural sort (`file2` before `file10`). Recommendation: accept for the log; natural order
   would need a materialized sort per folder.

## Milestones

Dependency graph, and what can run in parallel worktrees:

- **Backend chain**: 1 → 2 → (3 ∥ 4) → 5. After 2 lands, 3 (backfill + its host wiring) and 4 (`tree/` query and
  selection) touch different files and run in two worktrees. 5 needs 4's selection resolver, and only 2's `tree_summary`
  column (not 3) for its "summary not built" refusal.
- **Frontend primitive**: 6 is independent of the whole backend chain. Start it on day one in its own worktree.
- **Then serially**: 7 (needs 4, 5, 6), 8 (needs 7), 9 (needs 8).

Each milestone: one agent, one worktree branch, commits per green step, report ≤350 words incl. "what I grabbed".

### 1. Characterize the rollback engine, fix reconcile

**Scope.** Rust tests only, plus the `reconcile_one` fix. `crates/cmdr-operations/src/rollback/planner_tests/`,
`crates/cmdr-operation-log/src/rollback/decision_tests.rs`, `crates/cmdr-operation-log/src/rollback.rs` (reconcile
only), and row-shape pins beside the journal capture tests.

**Intentions.** Before the engine gains a selection, pin what it does today, so milestone 5's filter is provably
additive. Most of the engine is already well covered (survey above): find the gaps, don't duplicate.

- Pin the ROW SHAPES the tree will rely on, per kind, through the real capture paths: a folder copy (file rows'
  source/dest, created-dir rows created in place on the dest side), a cross-FS move (same), a same-FS move and a trash
  of a folder (one dir unit, `search_only` leaves' source and dest, the unit's `size` is not a byte total), a capped
  trash (`top_level_only`, no leaves), a delete (per-file rows), a batch rename. Where an existing capture test already
  asserts it, point at it in the plan report instead of adding one.
- Pin: drift and occupied-target skips record their reasons and leave the op partial; Finish retries only held-back
  rows; a stopped run keeps what it reversed; a page-read failure skips the dir sweep; the dir sweep runs deepest-first
  and only after every file (a cell where a dir row's `seq` is HIGHER than a file inside it).
- Pin crash reconcile across a SECOND pass: (a) first pass reversed some, second pass crashed before its inverse row
  opened ⇒ `partially_rolled_back` (red today); (b) second pass crashed after reversing nothing ⇒ still partial (red
  today); (c) an inverse whose first 10,000 rows are skips and a later one is done ⇒ partial (red today). Then fix
  `reconcile_one` per § "Bug found while surveying".
- Pin `RollbackReport` / inverse header counts for a folder copy (exists:
  `a_folder_copy_and_its_rollback_report_the_ same_item_count`; extend only if a case is missing).

**Landmines.** `InMemoryVolume::rename` creates missing parents where the real FS fails `ENOENT`: restore-path pins that
matter go on a real tempdir (`a_restore_recreates_the_folder_the_move_emptied` is the pattern). Reconcile runs before
the writer is published: test it through `OperationLog::open` or `reconcile_rolling_back_on_open` directly, the way
`lifecycle_tests.rs` does.

**Test plan.** Every new pin green on current code except the three reconcile cells, which go red, then green with the
fix. Verify a pin bites by breaking the behavior once locally (don't commit that).

**DONE.** Pins merged; reconcile decides from the original's outcomes; `crates/cmdr-operation-log/DETAILS.md` § "The
`rolling_back` state machine + startup reconcile" describes the new rule.

### 2. Schema v6 and a writer-maintained summary

**Scope.** `store/migrations.rs` (new step), `types.rs` (new tokens: `TreeSummaryState`, and reuse
`SearchCoverageReason` for the per-unit column), `writer.rs` (insert and outcome paths), `writer/prune.rs`, the record
points that set the per-unit coverage (`crates/cmdr-operations/src/journal.rs`, `journal_search.rs`, `delete/trash.rs`,
`transfer/move_op/`), `store/mod.rs` reads as needed. A new `src/tree/` module holds the axis and role helpers
(`TreeAxis::of`, `unit_role`), which the writer and later milestones share.

**Intentions.** Every op opened from now on carries an exact summary, kept exact through rollback, so the tree can open
a 2M-item op by reading a handful of rows. Shape per § "Design decisions" (summary table, item indexes,
`subtree_coverage_reason`, `operations.tree_summary`).

**Landmines.**

- The migration is written against v5, in one transaction, and stamps nothing but `schema_version` (the runner does).
  New ops: the writer stamps `tree_summary = built` at `apply_open`.
- The summary update happens in the SAME transaction as the rows it describes (insert batch; outcome batch), or a crash
  leaves them disagreeing. A dropped row (the writer's "drop one row, keep going") must not count in the summary:
  aggregate only rows that inserted.
- `gc_orphan_dirs` must treat `operation_tree_dirs.dir_id` as a live reference, and prune must delete a pruned op's
  summary rows first. A dir interned only for a DIR unit's self row is referenced by nothing else.
- Interning a dir unit's own path: use `intern_dir`, and keep `split_scheme_root` semantics (remote roots).
- Measure the migration's index build on a seeded 4M-row v5 DB (release build) and put the number in the commit body and
  `DETAILS.md` (evidence-anchored). Report it for question 2.
- `retention.rs` size accounting reads the file size; the new table and indexes grow it. Note the per-row overhead.
- The demo journal (`cmdr-listing-wasm`) opens through the same ladder; run its smoke if the crate's checks cover it.

**Test plan.** Migration: a v5 DB with rows migrates, keeps every row, has the indexes (`EXPLAIN QUERY PLAN` for "files
in dir X of op Y from position k"), and old ops read `tree_summary` NULL. Writer: inserting a folder copy, a trash with
leaves, and a delete yields summaries equal to a from-scratch aggregate (write that aggregate as a test helper; the
backfill in milestone 3 reuses the logic, so put the pure aggregation in `src/tree/` and test it once). Outcome changes
(reverse some, skip some with reasons, Finish) keep the summary equal to the aggregate. Prune + GC leave no orphan
summary rows and no dangling dir references.

**DONE.** v6 shipped in the ladder; summary exact under insert, outcome change, prune;
`crates/cmdr-operation-log/ DETAILS.md` § Schema and § "The writer" updated; the index-build measurement recorded.

### 3. Backfill for ops journaled before v6

**Scope.** `crates/cmdr-operation-log/src/tree/backfill.rs` (new), a `WriteTreeSummary` writer message, the per-op
outcome counter in the writer, the host wiring in BOTH hosts (desktop: an `async` command that starts or joins the
backfill and a progress event; demo: the same names in `history.rs`), cancel.

**Intentions.** Opening an old op's tree just works: a spinner past ~200 ms, a real progress bar past ~10 s, cancel on
op switch or dialog close, and never twice for the same op. Subscribe, don't poll: progress arrives as events.

**Landmines.**

- ❌ Never run the aggregation on the writer thread (a 2M-row read there blocks `record_items` and, through
  backpressure, a running copy). Read on a read connection; send the writer only the aggregated rows.
- Race with rollback: the writer applies the result only if that op's outcome counter didn't move since the backfill
  began; otherwise it answers "stale" and the backfill restarts. Also refuse to start on a `rolling_back` op (the
  summary would be built mid-flight); the tree view says so.
- The read connection's `busy_timeout` and chunking: stream by `seq` ranges with a cancel check per chunk, like
  `read_rollback_units_page`.
- Threads: the crate uses `rt.rs`; the demo has none. Shape the job as a resumable chunk loop the host drives (desktop:
  a blocking task; demo: chunks yielded to the event loop), with the logic in the crate.
- Two backfills for one op (the dialog reopened): join the running one.

**Test plan.** Backfilled summary equals the writer-built one for the same rows (the milestone 2 aggregate helper).
Cancel mid-way stamps nothing and a rerun completes. A rollback that lands during a backfill makes it restart, and the
final summary is exact. A `built` op never backfills again. Timing: a 20,000-row backfill well under two seconds.

**DONE.** Old ops open their tree after one backfill; both hosts answer; `DETAILS.md` has a "Tree summary" section
(shape, maintenance, backfill, why on demand).

### 4. The tree view and the selection model

**Scope.** `crates/cmdr-operation-log/src/tree/` (`view.rs`: flatten + window; `selection.rs`: types, normalize,
resolve, counts, tri-state), `query.rs` re-exports if the surface rule wants it (`DETAILS.md` § "The public surface is
capped"), the command `get_operation_log_tree` in both hosts, `bindings.ts` regen, the scale test.

**Intentions.** One request answers one window of ≤ 200 rows with everything the dialog renders, in single-digit ms, for
any op size, statelessly. Per § "Design decisions" (tree query, selection encoding, algebra, roles). The selection
resolver written here is THE resolver milestone 5's engine filter uses: one function, ❌ never a second copy.

**Landmines.**

- Row keys must be stable across requests and unambiguous: `Folder(dir_id)` (deepest dir of a compact chain) and
  `Item(seq)`. A DIR unit is a folder row keyed by its dir, carrying `self_seq` for its checkbox.
- Compaction must not swallow a folder that carries its own checkbox state different from its child (a DIR unit chain: a
  trashed `A` holding only folder `B` is two rows if `A` is a unit, since `A` is what comes back).
- `Contents` rows under a unit: their status and checkbox mirror the unit's, never their own `done` outcome (their
  outcome never changes; rollback doesn't touch `search_only` rows).
- A capped or empty DIR unit: `expandable: false`, with a typed reason (`Capped`, `NotRecorded`, `Empty`) the frontend
  words.
- Cross-volume: every path resolves through `dirs`; a remote root renders `scheme://authority` (`reconstruct_dir_path`).
- Full paths for 200 rows: resolve each distinct dir once per request (the window shares a few dirs).
- The normalized toggle list is part of the answer; the frontend must be able to send it straight back.

**Test plan.** Flatten cells on small fixtures (compaction, ancestors, pos/size, a window crossing open folders, a
folder of files windowed from the middle, DIR-unit rows, contents mirroring, capped/empty units, read-only op).
Selection cells: nearest-wins, normalize drops shadowed and no-op toggles, ⌘A/none as base flips, tri-state on nested
on/off/on chains, "N of M" against a brute-force count over the fixture (property-style over random toggle sets is a
good fit). **The scale test**: seed two 2M-row ops straight into a temp journal in one transaction (realistic shape: one
a 2M-file delete across a deep tree with one folder of ~1M direct files; one a 2M-leaf trash), build their summaries
through the real aggregation, then assert budgets per call (start at 50 ms, tighten from measurements): open the tree,
unfold the biggest folder, window at the END of the 1M-file folder, toggle the 2M folder off and read "N of M", toggle
2,000 scattered items and read again. Time the whole test; apply loud rule 11.

**DONE.** `get_operation_log_tree` answers on desktop and demo; the scale test (and its 20,000-row twin if needed) green
within budget; `DETAILS.md` § "The tree view" (algorithm, keys, compaction, budgets with evidence).

### 5. Partial rollback in the engine

**Scope.** `crates/cmdr-operation-log/src/rollback.rs` (+ `bookkeeping.rs`, `proofs.rs`, `proofs.mutants.json`),
`store/mod.rs` (`RollbackUnit` carries the axis dir id), `crates/cmdr-operations/src/rollback.rs` (dispatch threads the
selection), `rollback_operation` IPC in both hosts (`selection: Option<RollbackSelection>`), MCP `operations_rollback`
and `undo_operations` pass `None`, `RollbackRefusal::NothingSelected` and `SelectionNeedsSummary` (or one variant;
agent's call) with their frontend words.

**Intentions.** Roll back exactly the selected units, with every guard intact, and leave the op honestly
`partiallyRolledBack` when anything selectable stays behind. "Roll back the rest" is the existing re-entry with `None`.

**Landmines.**

- The filter sits where the loop already decides per row, BEFORE pacing, the pause gate, and verification; a filtered
  row costs nothing else. ❌ No per-row DB read: the dir cache plus the item map.
- `left_out` feeds `resolve_final_state` (Kani proof + mutants updated, `pnpm check kani` if available locally, else say
  so).
- Progress totals with a selection come from the summary algebra (milestone 4), never from a pre-scan; with `None` keep
  `read_rollback_file_totals`.
- `summarize_inverse` must summarize the first SELECTED unit, not the first unit.
- The inverse op's header count (`inverse_planned_item_count`, `RunAcc::totals`) is bounded by the selected units, so
  "Rolled back 3 of 400" reads as 3.
- Housekeeping dirs under a mixed subtree are left out, never attempted (see roles).
- `rollback_operation` refuses an empty selection (`N = 0`) and a selection on an op whose summary isn't built, typed,
  before `rolling_back` is set.
- The confirmation's "the selected items" wording needs only "is this a subset", which the frontend knows; nothing
  crosses for it.

**Test plan.** In `planner_tests/` (real executor): a subset of a folder copy leaves the rest on disk and the op
partial; "the rest" then lands `rolledBack`; drift and occupied guards still skip inside a selection; a selection
excluding a file inside a copied folder leaves that folder (no `DirNotEmpty` skip recorded); a trash with two folder
units, one selected; a selection whose units were all already reversed elsewhere (`AlreadyGone`) lands right; crash
reconcile mid-selective-pass (milestone 1's rule holds); progress totals equal the selected files and bytes. Every
milestone 1 pin unchanged.

**DONE.** Engine, both hosts, MCP unchanged in behavior; `DETAILS.md` § "Streaming + ordering" and the state-machine
section describe the selection; the frontend `DETAILS.md` re-entry argument mentions selections.

### 6. `ColumnList` tree mode (parallel from day one)

**Scope.** `apps/desktop/src/lib/ui/ColumnList.svelte`, `column-list-types.ts`, `list-cursor.ts` (if tree keys belong
there), tests beside them, the tier-3 a11y test, Debug > Components
(`routes/dev/components/sections/ ColumnListSection.svelte`), `docs/design-system.md`,
`apps/desktop/src/lib/ui/DETAILS.md` § ColumnList.

**Intentions.** Per § "Design decisions" (component): a `treegrid` semantics with source-owned expansion, indent +
disclosure in the first column, ←/→ tree keys, and a focus-owning listbox for the op list. Every existing consumer
(Search, Selection, rename review, Multi-rename preview) unchanged.

**Landmines.**

- Indent must not feed the width demand of the first column unmeasured: a windowed source shows fallbacks anyway, but an
  array source's `demand` has to include level × indent, or the column clips deep rows.
- The disclosure is a pointer target, not a Tab stop; the row (via active descendant) is the keyboard target.
- `aria-expanded` only on expandable rows; `aria-level` 1-based.
- Cells holding a `Checkbox` primitive: the checkbox mustn't take Tab (one Tab stop per list) and must stay
  `clip: false`.
- `heldRowOf` keeps working for tree rows (a collapse removes many rows at once).
- ❌ Never `TransferTree` changes in this milestone.

**Test plan.** Component tests: levels, disclosure click toggles via the callback, → expands a shut folder and steps
into an open one, ← collapses or goes to the parent, ← on a top row calls `onLeaveTree`, cursor keys unchanged,
focusable listbox announces the cursor row. Tier-3 a11y test for treegrid. Existing ColumnList tests unchanged.

**DONE.** Mode shipped with its Debug row and design-system entry; consumers untouched.

### 7. The master–detail dialog

**Scope.** `apps/desktop/src/lib/operation-log/` (dialog split into the op list, the detail pane, and a tree source
module over `get_operation_log_tree`), `operation-log-labels.ts` (kind-aware status words, skip-reason words,
unexpandable reasons, the second partial notice), `#lib/tauri-commands/operation-log.ts`, `RollbackConfirmDialog`
(`selection: 'all' | 'subset'` wording), `messages/en/operationLog.json` (+ `fileOperations.json` for the confirmation),
component tests.

**Intentions.** The mockup's layout and states: left op list (summary, who, when, one status glyph, colored rollback
state), right detail (header with summary, badges with "Finished" for the execution badge, route line "From X to Y",
standing notice, relation links), the tree filling the height, the action bar (**Roll back selected (N)** + "N of M
selected", plus `RollbackControls` while a reversal runs). ~900 × 640 default, resizable; under ~640 px the split
stacks. Status column speaks only when it matters (Rolled back, Skipped + reason, Didn't finish). Middle ellipsis on
names (`shorten-middle-action`), full path in the tooltip. Read-only tree, no checkboxes, on a not-rollbackable op.
Placeholders for unloaded rows; spinner only past ~200 ms; backfill progress past ~10 s.

**Landmines.**

- Everything reversible starts checked: the default selection is `None` (base All, no toggles), so a plain press sends
  nothing and behaves exactly like today's Roll back / Finish.
- Keep every existing behavior the CLAUDE.md pins: the session-driven `RollbackControls`, `onEnded` re-read, the
  journal-truth badge flip, typed refusals, `rowRollbackAction`'s gate (now on the action bar), relation links (now
  selecting the op in the list, not scrolling an accordion).
- After a reversal ends, re-read the op header AND the tree (outcomes changed); toggles for rows now settled drop out in
  normalization.
- `ModalDialog` resizing and `fillBody`: the two panes must fill, with one scroller each (the tree's own viewport).
- Strings: English only, `@key` descriptions written for translators; delete keys that die (`moreItems`, `itemsError` if
  unused) and their translations (that's removal, not translation).
- E2E selectors: keep `data-op-id` (or similar) stable hooks so milestone 9 can address rows without positions.

**Test plan.** Component tests over a mocked tree source: default all-checked + count, folder uncheck cascades, greyed
contents with their tooltip, read-only op shows no checkboxes, status words per role and reason, the partial notice from
counts, confirmation says "selected" only for a subset, the dispatch carries the normalized toggles, the action bar
reflects `rowRollbackAction`. Update `OperationLogDialog.a11y.test.ts`.

**DONE.** The dialog matches the mockup's three states; docs in `apps/desktop/src/lib/operation-log/` C+D updated
(layout, the tree source, the selection flow); David gets screenshots of the three states for review.

### 8. Keyboard

**Scope.** `apps/desktop/src/lib/commands/sources/operation-log.ts` (new, scope `'Main window/Operation log'`), its
registry wiring, the dialog's keydown, tests.

**Intentions.** The mockup's keyboard map: Tab order op list → tree → action bar → Close; op list ↑/↓/Home/End/PgUp/
PgDn with the cursor as the selection, → or ↩ into the tree; tree ↑/↓/PgUp/PgDn, → / ← from tree mode, ← on a top row
back to the op list, Space toggles (cascading), Insert toggles and moves down, ⌘A / ⌘⇧A all / none, ⌘↩ rolls back the
selection (via the button's `keyHint`), Escape closes. All through `eventMatchesCommand` / `list-cursor.ts`.

**Landmines.** ⌘A in a dialog must not reach the panes' `selection.selectAll` (scope + `whileDialogOpen`); check the
collision test in `command-registry.test.ts`. Insert's panes command is `selection.toggleAndDown`: a dialog-scoped twin,
not a reuse across scopes, unless the registry supports that. Read `apps/desktop/src/lib/commands/CLAUDE.md` § Gotchas
for every place a command touches.

**Test plan.** Keyboard component tests for each row of the map, plus the registry collision tests.

**DONE.** Every action works without a mouse; Settings/Help list the keys.

### 9. End to end, docs, close-out

**Scope.** `operation-log-rollback.spec.ts` (new selectors; a subset rollback against real files; Finish → "Roll back
the rest"), every touched C+D.md, `docs/architecture.md` if the map changes, feature-status if relevant, wipe nothing
yet (the lead wipes this plan after landing).

**Intentions.** Prove a subset rollback reaches the disk through the real engine, and leave the docs describing the
current state.

**Landmines.** The spec's ordering traps (one `afterEach`, throttle cleared first, identify ops by difference). Run it
through `pnpm check desktop-e2e-playwright` (or the single-spec recipe in `test/e2e-playwright/CLAUDE.md`), plus
`desktop-e2e-linux` per the gates.

**Test plan.** New spec: stage a folder copy, open the log, uncheck one file, roll back selected, assert exactly the
rest is gone and the op reads partial with the right notice; then roll back the rest and assert `rolledBack`. Existing
specs adapted to the new selectors, assertions unchanged.

**DONE.** E2E green; the close-out reviews (conformance against the invariants below, docs audit) per the loop.

## Invariants register

1. A selection never adds a row the unfiltered run wouldn't stream, and never skips a guard for a row it streams.
2. `resolve_final_state` never answers `RolledBack` while any selectable unit is skipped, unattempted, or left out
   (Kani-proven).
3. With no selection (`None`), the engine, its reports, its progress totals, and its final states are identical to
   today's (milestone 1 pins unchanged).
4. The summary table equals a from-scratch aggregate of the op's rows after any insert, outcome change, prune, or
   backfill.
5. No interactive read scans a whole op: tree pages, "N of M", tri-state, and rollback totals are O(expansion + window +
   toggles).
6. The engine adds no per-row DB read; the writer's own outcome-delta read happens on the writer thread.
7. Crash reconcile decides from the original op's outcomes; a second-pass crash never reverts a partly reversed op to
   `rollbackable`.
8. Every migration step is new and appended; no shipped step changes.
9. Both hosts (desktop, demo) answer every new or changed command with the crate's logic.
10. The frontend never builds a tree, never words a backend string, and sends back only toggles it was handed.
11. Copy stays count-free in the confirmation, promises no complete reversal, and never says "error" or "failed".
