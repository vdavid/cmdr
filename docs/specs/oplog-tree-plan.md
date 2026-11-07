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
   window), never O(op). ❌ No full-op scan on any interactive path. The always-on gate is STRUCTURAL (query plans and
   VM-step growth, milestone 4), because Rust tests build SQLite at `-O0` and a 20,000-row twin hides an O(op) read
   inside any ms budget; the timed 2M test is the release-build budget.
6. **Business logic lives in `cmdr-operation-log`.** The desktop command
   (`apps/desktop/src-tauri/src/commands/operation_log.rs`) and the demo's
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
    Over five seconds it's `#[ignore]`d with a how-to-run comment, AND a 20,000-row twin runs normally. Expect the 2M
    test to be ignored in the dev profile; its comment names the exact
    `cargo nextest run --release --run-ignored ignored-only -E 'test(...)'` line and says its ms budgets are release
    numbers. An E2E test that runs past two seconds is split, since loosening `e2e-duration-allowlist.json` needs
    David's OK.
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

### Bugs found while surveying

**Reconcile (fix it in milestone 1).** `reconcile_one` reads only the NEWEST inverse op, and only its first 10,000 rows.
Two cases resolve a partly reversed op to `rollbackable`: (b) a second pass opened its inverse row and crashed before
reversing anything (the newest inverse has no `done` row, though an earlier pass's does), and (c) an inverse whose first
10,000 rows are skips with a `done` row past them. A second pass that crashed BEFORE its inverse row opened is fine
today: the newest inverse is then the first pass's. With partial rollback, second passes become the normal case.

- **The original alone isn't the truth either**: `RunAcc::flush` (`rollback/bookkeeping.rs`) sends two writer messages,
  `record_items` (the inverse's rows) and THEN `set_item_outcomes` (the original's), so a crash between them leaves
  `done` inverse rows over untouched original rows.
- **Fix**: partial if ANY of: the original's summary root shows something rolled back or held back (built ops); an
  `EXISTS` over the original's rows for `rolled_back` or a stored `rollback_skip_reason` (unbuilt ops; not index-served,
  so it scans that op, which only happens for an op a crash left stuck); or ANY inverse of this original (found through
  `operations_rolls_back`, not only the newest) has a `done` row (an `EXISTS` that stops at the first hit). Else
  `rollbackable`.
- **Existing pins change, on purpose**: `lifecycle_tests.rs::seed_rolling_back` seeds only inverse rows; rewrite it to
  seed the original's outcomes too. Its "inverse all skipped ⇒ rollbackable" cell contradicts `resolve_final_state` (any
  held-back skip ⇒ partial) and the new rule: it flips to partial, and the commit body says why (loud rule 2).

**Redo of a move, trash, or rename skips every row (for David; NOT fixed in this effort).** `inverse_item_row`
(`rollback/bookkeeping.rs`) records the inverse's rows with no dest, yet `inverse_kind(Move | Trash) = Move` and
`Rename = Rename` open `rollbackable` (`capture.rs::compute_eligibility`), and `DETAILS.md` says "redo falls out".
Rolling back such an inverse reaches `restore_move`, which answers `Skipped(Failed)` for every dest-less row. It fails
safe (nothing moves) but the inverse lands partial with a "Failed" skip per row, and no planner test covers a move redo.
The fix (record the original's source as the inverse row's dest) turns on a redo that has never worked, so it wants
David's go and its own issue and tests. For this effort: `TreeAxis` is op-aware (an op with `rolls_back_op_id` reads on
the Source axis), so every inverse op still has a tree.

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

## Design decisions

- **Tree axis per op** (one typed helper, `TreeAxis::of(kind, is_inverse)`, an exhaustive `match` over `OpKind` with no
  `_` arm): `Dest` for copy, move, create_file, create_folder, archive_edit (all three subkinds); `Source` for trash,
  delete, rename, tag, and for EVERY inverse op (its rows carry no dest; see § "Bugs found while surveying"). **Why**:
  the tree shows where the thing the undo acts on lives. On the dest axis a copy's and a cross-FS move's file rows AND
  their created-folder rows sit on one side, so folders are real folders and nothing is "housekeeping on the other
  volume"; a same-FS move's unit and its `search_only` leaves both carry a landed dest. Trash stays on source (the
  Trash's internal names mean nothing to a person; the header says "to the Trash"). The row's other end shows as a
  secondary line only when it differs from `other_root + relative path` (a conflict rename `name (2)`), and never for
  trash.
- **Rollback roles, one typed function**
  (`unit_role(kind, row_role, entry_type, created_in_place, outcome, skip_reason)` →
  `Selectable | Housekeeping | Contents | Settled | NotReversible`):
  - `Selectable`: a `rollback_unit` that `awaits_reversal`. Counts in "N of M".
  - `Housekeeping`: a created-in-place DIR row on a copy or move (the folder a copy made). Not counted, no checkbox of
    its own; it's the folder row, and it's included in a rollback only when its whole subtree is selected (else it can't
    be emptied, and trying would record a misleading `DirNotEmpty` skip).
  - `Contents`: a `search_only` row under a unit. Greyed, shows its unit's state, tooltip "comes back with its folder".
  - `Settled`: `rolled_back`, or the operation's own skip/failure. No checkbox; the status cell speaks.
  - **Op-level rollbackability is applied at QUERY time, never stored.** A not-rollbackable op (or one `rolling_back`)
    renders read-only, no checkboxes. Eligibility is written at `apply_finalize`, AFTER the rows, so a summary column
    that depended on it would be wrong for every op that finalizes `not_rollbackable`. Rule for the summary: every
    column is a pure function of row fields (`row_role`, `entry_type`, `outcome`, `rollback_skip_reason`,
    created-in-place).
  - **Nothing selectable, rows still waiting**: a copied folder held back by `DirNotEmpty` is `Housekeeping`, so once
    every file under it is settled the tree reads "0 of 0". Then the action bar offers "Roll back the rest" and sends
    `None`, which streams the dir and removes it if it's empty now. ❌ Never a dead end.
- **Selection encoding: base plus toggles, nearest wins.**
  `RollbackSelection { base: All | None, toggles: Vec<SelectionToggle> }`,
  `SelectionToggle = Folder { dir_id, on } | Item { seq, on }`. A row's decision is its own `Item` toggle, else the
  nearest `Folder` toggle up its chain, else `base`. **A DIR unit's chain starts at its OWN dir** (unchecking a trashed
  folder's row is `Folder { its dir, off }`), every other row's at its axis dir. `None` on the wire (the `Option`) means
  "everything", so the default "undo the whole thing" and the existing callers (MCP, `undo_operations`, Finish) send
  nothing. Capped at 2,000 toggles (the transfer tree's hand-toggle cap); past it the view answers a typed
  `toggles_capped` and refuses the toggle, and the dialog says so ("pick a folder instead").
  - **Normalization**, in the crate, run by the view AND re-run by the engine (❌ never trust the wire): drop toggles
    shadowed by a later ancestor toggle, drop no-ops, drop `Folder` toggles strictly inside a DIR unit (its Contents
    have no checkbox), drop `Item` toggles on `search_only` or Housekeeping seqs, and drop dir ids with no summary row
    for THIS op (`dirs` is interned globally, so a foreign id could flip decisions). An `Item` toggle naming a DIR
    unit's own seq becomes `Folder { its self_dir_id }`, so the item map and the folder chain can't disagree. The view
    returns the normalized list.
  - **Why**: O(toggles) on the wire for any op size, keys the backend already hands out, and a nearest-ancestor rule is
    what tri-state checkboxes mean.
- **Summary table: subtree totals per (op, axis dir), maintained by the writer.** Recommended shape (agent may refine,
  document the final one):
  `operation_tree_dirs(op_id, dir_id, parent_dir_id, name_folded, self_seq, child_dirs, direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, units_rolled_back, units_held_back, units_settled)`,
  `PRIMARY KEY (op_id, dir_id)` WITHOUT ROWID, plus an index on `(op_id, parent_dir_id, name_folded)`, so a folder's
  child folders come out in order from the index (a folder of 100,000 subfolders never sorts per window). Subtree totals
  go on every ancestor up to the volume root (a handful of rows per op), so opening any folder reads one row and its
  children's rows. `files`/`bytes` count FILE rows of both roles (a folder unit's own `size` is an inode length, never
  bytes). A DIR item row gets its own summary row (`self_seq` names it), so a folder appears ONCE in its parent's
  listing (as a folder) and an empty or capped folder still has a row. Its own dir id is a new nullable
  `operation_items.self_dir_id`, interned by the writer at insert (so neither the engine nor the backfill resolves a dir
  by name, and a read connection never has to intern); for pre-v6 DIR rows the writer fills it while applying a backfill
  (they're few: one per trashed or moved folder, one per created folder). **A DIR unit counts in its OWN dir's subtree
  totals** (and so in every ancestor's), matching the selection rule that decides it by its own folder's toggle:
  `Folder { D, off }` on a trashed folder D subtracts exactly 1 from N.
  - **A summary write that fails never drops item rows**: inside the same transaction it sets that op's `tree_summary`
    back to NULL, and the next open backfills it exact. (`apply_record_items` drops a failing row with no savepoint, and
    `apply_set_item_outcomes` aborts its batch on any error; the summary must not drift silently in either.)
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
  from position k" on either axis. They're built in the ladder at launch (decided, see § "Decided"), measured in
  milestone 2 for time AND file-size growth.
- **A folder that can't unfold, worded from the op-level coverage** (no new per-unit column: cut as scope the issue
  doesn't need). A DIR unit with no recorded contents on a `top_level_only` op says "Cmdr didn't record what's inside
  this folder", and when the op's reason is `capped` it adds that the operation held a folder of over 50,000 files.
  Coverage is op-level, so the line speaks about the operation, never claims THIS folder held 50,000 files (it might be
  an empty one beside the big one). On a `full` op such a unit is an empty folder and says so.
- **Backfill: on demand, per op, off every UI thread, cancelable, recorded.** A nullable `operations.tree_summary` token
  (`built`), set at open for every new op (the writer builds as it goes) and NULL for every pre-v6 op. Opening an
  unbuilt op's tree starts its backfill: a read connection streams the op's rows in `seq` chunks, aggregates per dir in
  memory (dirs are few), and hands the result to the writer as one `WriteTreeSummary` message, which stamps `built`.
  - **Staleness by a stored generation**: a new `operations.outcome_generation`, bumped inside every outcome
    transaction. The backfill reads it before its first chunk; the writer applies the result only if it's unchanged,
    else answers `Stale` and the backfill restarts. (Pre-v6 ops are all finished, so no inserts race it; only outcome
    updates can.) A writer-side atomic would be bumped before commit and could pass a stale read.
  - **The writer applies only to an op that still exists and isn't `built`**, else answers `Gone` / `AlreadyBuilt`:
    retention can prune the op mid-backfill, and orphan summary rows would keep dirs alive past `gc_orphan_dirs`.
  - Progress events from the start, the UI shows them past ~10 s; cancel on op switch or dialog close; a backfill starts
    only for an op the op-list cursor SETTLED on. **Why on demand**: respects the user's resources (only ops someone
    opens pay), and a 2M-row op backfills in seconds. Whole-journal background backfill stays a later option.
- **Tree query: the transfer tree's flatten, over SQL.** Request:
  `{ expanded: Vec<RowKey>, selection, cursor: Option<RowKey>, window: { offset }, limit ≤ 200 }`. The frontend holds
  its cursor as a `RowKey`; the answer returns `cursor_index` (that row's flat index, or its nearest visible ancestor's
  when it collapsed away), so an expand above the cursor or a post-rollback re-read keeps the cursor on its row (the
  transfer tree's `focus_row` answers the same). Root = the deepest axis dir that holds the whole op with no direct
  files and one child dir (descend summary rows from the volume root); an op whose rows sit under several volume roots
  is a FOREST (several top rows, no shared root path). Each open folder's listing size = `child_dirs + direct_files`;
  locate the window by descending through open folders' sizes (O(|expanded|)), then walk forward: child folders from the
  summary index, ordered by folded name; files from the item index, `ORDER BY name_folded LIMIT ? OFFSET ?` within one
  folder. Compaction at query time (a folder whose only child is a folder and that has no files shares its row, keyed by
  the deepest dir id). Answer: rows (key, label, level, kind, expandable, expanded, pos_in_set/set_size, size or rollup,
  status, role, checkbox state on/off/mixed/none, full path, other-end path when it differs), `ancestors` (as the
  transfer tree's), `total_rows`, `root_path`, `other_root`, op-level counts (rolled back / held back / still waiting /
  selectable), `selected` + `selectable` ("N of M"), the normalized toggles, and
  `summary: Built | Building { done, total } | Unbuilt`. OFFSET inside one folder is O(k): past a folder's midpoint,
  query `ORDER BY name_folded DESC` with offset `size - k - limit` and reverse, which caps the walk at size/2 (child
  folders the same way). The worst case is then the MIDDLE of a big folder, and that's what the scale test budgets.
- **"N of M" and tri-state from algebra, not scans.** Build the toggle forest by walking each toggle's dir ancestry (≤
  2,000 toggles × depth). Selected = Σ over the forest with each node's `units_selectable` from its summary row (or 1
  for an item toggle). A folder row is mixed iff some toggle strictly inside it flips a non-zero count. Rollback
  progress totals with a selection come from the same algebra over `unit_files_selectable` / `unit_bytes_selectable`,
  plus one point read per `Item` toggle (≤ 2,000) counting only FILE units with `MAX(size, 0)`, exactly as
  `read_rollback_file_totals` and the loop's crediting do. A selection only comes from a tree, so the summary is built;
  the command refuses a selection on an unbuilt op, typed.
- **Engine filter: a dir-id → decision cache, on the RAW row.** The decision runs on the `OperationItemRow` (which
  already carries `source_dir_id` / `dest_dir_id`, plus the new `self_dir_id`) BEFORE `map_rollback_unit`, so a left-out
  row costs one cache lookup and never pays `resolve_dir`'s path reconstruction. The cache walks `dirs.parent_dir_id`
  once per DISTINCT dir (one read per distinct dir, never per row) to the nearest toggle; item toggles are a
  `HashMap<seq, bool>`; housekeeping dirs check a precomputed "has an excluded descendant" set. A row filtered out
  counts as `left_out`, is not acted on, not flushed, and not recorded. Three files out of 2M still stream every row
  past the filter; the scale test measures that skip pass, and only if it's slow (over a second or so, release) does the
  SQL stream narrow to the `on` toggles' subtrees through the item indexes.
- **Inverse summary from the selection, not by streaming.** `summarize_inverse` reads one row today; "the first SELECTED
  unit" found by streaming is O(op) on the synchronous dispatch path. Derive it: an `on` `Item` toggle → that row (one
  point read); else the deepest `on` `Folder` toggle → its summary row names a unit under it; base `All` with no `on`
  toggle keeps today's newest-unit read.
- **Final state**: `resolve_final_state` gains `left_out`: `skipped > 0 || left_out > 0 || (resuming && reversed == 0)`
  ⇒ `PartiallyRolledBack` on a run that wasn't stopped. `rollback/proofs.rs` takes `left_out` as a `kani::any()` input
  and asserts `RolledBack ⇒ left_out == 0`; `proofs.mutants.json` gains a mutant that drops the `left_out` term.
- **Component: a thin `treegrid` in `ColumnList`, tree logic beside it.** Having read both: `ColumnList` already owns
  the hard parts this dialog needs (column widths, windowed source with placeholders, `heldRowOf`, a focused-root grid
  with `aria-activedescendant`, `list-cursor.ts`), and its grid already hands every non-cursor key to the consumer's
  `onKeydown`. So `ColumnList` gains only `semantics: 'treegrid'`: the role, plus
  `rowAria(row) → { level, expanded, setSize, posInSet }` on each row, and only for WINDOWED sources (no array-source
  width demand to grow). The consumer's first-column snippet draws indent + disclosure. ←/→ live in a pure
  `tree-cursor.ts` beside `list-cursor.ts` (expand-or-enter, collapse-or-parent through the answer's `ancestors`,
  leave-the-tree on a top row), called from `onKeydown`. `TransferTree` can adopt `tree-cursor.ts` later; out of scope
  here.
- **The op list is a `grid`** (one column of cells), which already owns focus and its active descendant. **Why**: a
  `listbox` puts its role on the viewport and gives options `tabindex=-1`, so a click focuses an option; a second focus
  model for one list isn't worth it. (The issue says listbox; grid is the built equivalent.)
- **Op list paging**: keep the array + offset paging (`operation-log-trigger.svelte.ts`), with "Load more" as a button
  under the list in the Tab order. **Why**: the journal has no cheap total, and a windowed source over offset paging
  buys nothing at 50 per page.
- **Ordering inside a folder**: folders first, then files, each by folded name in index (byte) order.
- **The tree source: one request in flight, generation-tagged** (reuse `createCoalesced` from
  `#lib/file-operations/transfer-tree/transfer-tree-view.svelte.ts`). Each local edit bumps a generation; an answer's
  normalized toggles are adopted only when its generation is the latest, and the pressed row's checkbox renders
  optimistically. **Why**: Space, Space fast (or Insert on key repeat) otherwise lets answer 1 wipe toggle 2.
- **The op-list cursor settles before it loads.** The header renders from the row at once; the tree read (and any
  backfill) waits ~150 ms for the cursor to settle, so holding ↓ through 50 ops costs one tree read, not 50 plus 50
  started-and-cancelled backfills.
- **Native-menu keys reach the dialog by dispatch, not keydown.** ⌘A / ⌘⇧A fire as the menu's `selection.selectAll` /
  `selection.deselectAll` before any webview keydown, and the dialog gate refuses both over a dialog
  (`routes/(main)/dialog-command-gate.ts`). Route them to the open Operation log's tree when it holds focus, in the
  shape of `handleTextRegionShortcut`. Space, Insert, and ⌘↩ aren't bar accelerators and work as dialog-scoped registry
  commands.

## Decided (lead, 2026-11-07, on the planner's recommendations)

1. **One leftover phrase.** The journal can't tell "left out by your selection" from "a cancelled pass never reached it"
   (both are `done` rows still awaiting reversal), so the partial notice uses one honest phrase for both ("two haven't
   been rolled back yet"), from the summary counts, not the mockup's "you left out last time".
2. **Pay the index build at launch**, in the ladder, measured in milestone 2 (launch time AND file growth on a 4M-row
   journal). Moving `open` off `setup()` isn't an option: operations started before it opens would go unjournaled. If
   the measurement is bad, milestone 2 stops and the lead takes it to David (fallback on the table: build the two
   indexes after open through the writer, with the tree answering `Building` until they exist).
3. **Byte order of the folded name inside a folder** (index-served), not the panes' natural sort.

## Milestones

Dependency graph, and what can run in parallel worktrees:

- **Backend chain**: 1 → 2 → (3 ∥ 4) → 5. After 2 lands, 3 (backfill + its host wiring) and 4 (`tree/` query and
  selection) touch different files and run in two worktrees. They meet at a seam milestone 2 defines: a
  `TreeSummaryStatus` read plus a small backfill-registry type with `status(op_id)`, so 4 answers `Building` without 3's
  internals and 3 fills it in. 5 needs 4's selection resolver, and only 2's `tree_summary` column (not 3) for its
  "summary not built" refusal.
- **Frontend primitive**: 6 is independent of the whole backend chain. Start it on day one in its own worktree.
- **Then serially**: 7 (needs 3, 4, 5, 6), 8 (needs 7), 9 (needs 8).

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
  trash (`top_level_only`, no leaves), a delete (per-file rows), a batch rename, a net-new compress. Where an existing
  capture test already asserts it, point at it in the report instead of adding one.
- Pin: drift and occupied-target skips record their reasons and leave the op partial; Finish retries only held-back
  rows; a stopped run keeps what it reversed; a page-read failure skips the dir sweep; the dir sweep runs deepest-first
  and only after every file (a cell where a dir row's `seq` is HIGHER than a file inside it).
- Pin crash reconcile, one cell per clause of the new rule, each named as that clause's guard (delete the clause and
  exactly its cell goes red):
  - **Original has a `rolled_back` row** ⇒ partial: (b) a first pass reversed some, a second pass opened its inverse
    (seeded at `started_at` 3, the first at 2, so "newest" isn't an id tiebreak) and crashed having reversed nothing.
    Red today.
  - **Original has a stored skip reason** ⇒ partial: a first pass that only held items back, then a second pass crashed
    as in (b). Red today.
  - **Any inverse has a `done` row** ⇒ partial: (d) a crash between the two `RunAcc::flush` messages (inverse rows
    `done`, original untouched). Green before AND after the fix; its job is guarding this clause. Plus (c): an inverse
    whose first 10,000 rows are skips and a later one is `done` (red today, through the prefix cap).
  - **None of the above** ⇒ `rollbackable`, and (a) a second pass that crashed before its inverse row opened stays
    partial (green today). Rewrite `seed_rolling_back` to seed original outcomes too, flip the "inverse all skipped ⇒
    rollbackable" cell to partial with the why in the commit body, then fix `reconcile_one` per § "Bugs found while
    surveying". Without the summary (it lands in milestone 2), the rule here is the two `EXISTS` clauses; milestone 2
    adds the summary-root shortcut and its own cell.
- Pin `RollbackReport` / inverse header counts for a folder copy (exists:
  `a_folder_copy_and_its_rollback_report_the_same_item_count`; extend only if a case is missing).
- Pin today's redo behavior for a move (rolling back its inverse skips every row as `Failed`, nothing moves), so the
  separate redo fix, when David says go, flips a known pin rather than discovering it.

**Landmines.** `InMemoryVolume::rename` creates missing parents where the real FS fails `ENOENT`: restore-path pins that
matter go on a real tempdir (`a_restore_recreates_the_folder_the_move_emptied` is the pattern). Reconcile runs before
the writer is published: test it through `OperationLog::open` or `reconcile_rolling_back_on_open` directly, the way
`lifecycle_tests.rs` does.

**Test plan.** Every new pin green on current code except the reconcile cells marked red above, which go red, then green
with the fix. Verify a pin bites by breaking the behavior once locally (don't commit that).

**DONE.** Pins merged; reconcile decides from the original's outcomes; `crates/cmdr-operation-log/DETAILS.md` § "The
`rolling_back` state machine + startup reconcile" describes the new rule.

### 2. Schema v6 and a writer-maintained summary

**Scope.** `store/migrations.rs` (new step), `types.rs` (new token: `TreeSummaryState`), `writer.rs` (insert and outcome
paths), `writer/prune.rs`, `store/mod.rs` reads as needed. A new `src/tree/` module holds the axis and role helpers
(`TreeAxis::of`, `unit_role`), which the writer and later milestones share, and the seam milestones 3 and 4 meet at
(`TreeSummaryStatus`, the backfill-registry type with `status(op_id)`; 3 fills in its body). The 4M-row seeding for the
measurement lives in an `#[ignore]`d bench in the crate, in the pattern of `docs/notes/operation-log-capture-bench.md`.

**Intentions.** Every op opened from now on carries an exact summary, kept exact through rollback, so the tree can open
a 2M-item op by reading a handful of rows. Shape per § "Design decisions" (summary table with row-pure columns and
`name_folded`, item indexes, `self_dir_id`, `operations.tree_summary`, `operations.outcome_generation`). Add the
summary-root shortcut to milestone 1's reconcile rule.

**Landmines.**

- The migration is written against v5, in one transaction, and stamps nothing but `schema_version` (the runner does).
  New ops: the writer stamps `tree_summary = built` at `apply_open`.
- The summary update happens in the SAME transaction as the rows it describes (insert batch; outcome batch), or a crash
  leaves them disagreeing. A dropped row (the writer's "drop one row, keep going") must not count in the summary:
  aggregate only rows that inserted.
- `gc_orphan_dirs` must treat `operation_tree_dirs.dir_id` as a live reference, and prune must delete a pruned op's
  summary rows first. A dir interned only for a DIR unit's self row is referenced by nothing else.
- Interning a dir unit's own path: use `intern_dir`, and keep `split_scheme_root` semantics (remote roots).
- A summary write that fails resets the op's `tree_summary` to NULL in the same transaction; ❌ never drop or roll back
  item rows for it.
- Measure the migration on a seeded 4M-row v5 DB (release build): launch time, WAL growth during the step, and the
  file's permanent growth. Put the numbers in the commit body and `DETAILS.md` (evidence-anchored). If launch time is
  bad, stop and report (§ "Decided" 2).
- `retention.rs` prunes by file size, so the new indexes and table mean a big journal keeps somewhat less history after
  the upgrade. Note the per-row overhead and the effect in `DETAILS.md` § Retention.
- The demo journal (`cmdr-listing-wasm`) opens through the same ladder; run its smoke if the crate's checks cover it.

**Test plan.** Migration: a v5 DB with rows migrates, keeps every row, has the indexes (`EXPLAIN QUERY PLAN` for "files
in dir X of op Y from position k"), and old ops read `tree_summary` NULL. **Golden summaries, written by hand**: three
or four tiny fixtures (a two-file folder copy, a trash of one folder with two leaves, a capped trash, a cross-FS move
with a held-back folder) with their expected summary rows literally in the test, since a wrong role or axis would make
any code-vs-code comparison agree with itself. Then: inserting a folder copy, a trash with leaves, and a delete yields
summaries equal to a from-scratch aggregate (the backfill in milestone 3 reuses that aggregation, so it lives in
`src/tree/`). Outcome changes (reverse some, skip some with reasons, Finish) keep the summary equal to the aggregate.
Prune + GC leave no orphan summary rows and no dangling dir references. A finalize that flips the op to
`not_rollbackable` leaves the summary unchanged (row-pure columns). A summary-write failure, injected by a test-made
`CREATE TRIGGER … BEFORE INSERT ON operation_tree_dirs BEGIN SELECT RAISE(ABORT, 'x'); END` (no production fault seam),
leaves the op unbuilt and every item row in place. A reconcile cell for the summary-root shortcut. A DIR row's
`self_dir_id` resolves to its own path, local and scheme-rooted.

**DONE.** v6 shipped in the ladder; summary exact under insert, outcome change, prune;
`crates/cmdr-operation-log/DETAILS.md` § Schema and § "The writer" updated; the migration measurements recorded.

### 3. Backfill for ops journaled before v6

**Scope.** `crates/cmdr-operation-log/src/tree/backfill.rs` (new), a `WriteTreeSummary` writer message (which also
interns and fills `self_dir_id` for the op's pre-v6 DIR rows), the host wiring in BOTH hosts (desktop: an `async`
command that starts or joins the backfill and a progress event; demo: the same names in `history.rs`), cancel.

**Intentions.** Opening an old op's tree just works: a spinner past ~200 ms, a real progress bar past ~10 s, cancel on
op switch or dialog close, and never twice for the same op. Subscribe, don't poll: progress arrives as events.

**Landmines.**

- ❌ Never run the aggregation on the writer thread (a 2M-row read there blocks `record_items` and, through
  backpressure, a running copy). Read on a read connection; send the writer only the aggregated rows.
- Race with rollback: the writer applies the result only if `outcome_generation` is what the backfill read before its
  first chunk; otherwise `Stale`, and the backfill restarts. Also refuse to start on a `rolling_back` op; the tree view
  says so.
- Race with retention: the writer applies only to an op that exists and isn't `built` (`Gone` / `AlreadyBuilt`).
- The read connection's `busy_timeout` and chunking: stream by `seq` ranges with a cancel check per chunk, like
  `read_rollback_units_page`.
- Threads: the crate uses `rt.rs`; the demo has none. Shape the job as a resumable chunk loop the host drives (desktop:
  a blocking task; demo: chunks yielded to the event loop), with the logic in the crate.
- Two backfills for one op (the dialog reopened): join the running one.

**Test plan.** Backfilled summary equals the writer-built one for the same rows (the milestone 2 aggregate helper).
Cancel mid-way stamps nothing and a rerun completes. A rollback that lands during a backfill makes it restart, and the
final summary is exact. A prune during a backfill leaves no summary rows and no live dirs behind. A `built` op never
backfills again. Pre-v6 DIR rows get their `self_dir_id`. Timing: a 20,000-row backfill well under two seconds.

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
- An inverse op reads on the Source axis (`TreeAxis::of`); a move's inverse must still show a tree.
- Demo arg structs: give every new one (and the changed `RollbackArgs`, milestone 5) `#[serde(deny_unknown_fields)]`, so
  a field the demo hasn't learned fails loudly instead of being dropped.

**Test plan.** Flatten cells on small fixtures (compaction, ancestors, pos/size, a window crossing open folders, a
folder of files windowed from the middle, DIR-unit rows, contents mirroring, capped/empty units, read-only op, a forest,
an inverse op). Selection cells: nearest-wins, a DIR unit decided by its own folder toggle, every normalize rule
(shadowed, no-op, inside a DIR unit, Contents/Housekeeping item toggles, a foreign dir id), the 2,000 cap answers
`toggles_capped`, ⌘A/none as base flips, tri-state on nested on/off/on chains, `Folder { D, off }` on a trashed folder D
subtracts exactly 1 from N, an `Item` toggle on a DIR unit's seq becomes its folder toggle, the cursor key comes back as
the right `cursor_index` (and as its ancestor's after a collapse), and "N of M" against a brute-force count over the
fixture. The brute force is an independent per-row walk over path prefixes that shares NO code with the resolver;
property-style over random toggle sets is a good fit.

**The structural gate (always on, under two seconds).** For every interactive query (tree open, unfold, window from the
start, middle, and end of a folder, "N of M", the engine's per-row decision): its `EXPLAIN QUERY PLAN` shows no
`SCAN operation_items` or `SCAN operation_tree_dirs` (precedent: `query/tests.rs::delete_dog_jpg_is_index_served`), and
its SQLite VM step count (rusqlite's `progress_handler`) on the same fixture at 20,000 and 200,000 rows stays within a
small constant factor (an O(op) read grows 10×). This is what bites in the dev profile.

**The timed scale test (release budgets).** Seed two 2M-row ops straight into a temp journal in one transaction and
build their summaries through the real aggregation: a 2M-file delete across a deep tree with one folder of ~1M direct
files and one folder of 100,000 child folders (tree open, unfold, the middle of the 1M-file folder, the middle of the
100,000-folder listing); and a 2M-file folder copy, whose `units_selectable` really is 2M and whose created folders are
Housekeeping (toggle the 2M folder off and read "N of M", toggle 2,000 scattered items and read again, and the engine's
skip pass over 2M rows for a three-file selection). Budgets per call start at 50 ms and tighten from measurements. Time
the whole test; apply loud rule 11.

**DONE.** `get_operation_log_tree` answers on desktop and demo; the structural gate green; the timed test green in
release within budget (and its 20,000-row twin running normally); `DETAILS.md` § "The tree view" (algorithm, keys,
compaction, budgets with evidence).

### 5. Partial rollback in the engine

**Scope.** `crates/cmdr-operation-log/src/rollback.rs` (+ `bookkeeping.rs`, `proofs.rs`, `proofs.mutants.json`),
`store/mod.rs` (`RollbackUnit` carries the axis dir id), `crates/cmdr-operations/src/rollback.rs` (dispatch threads the
selection), `rollback_operation` IPC in both hosts (`selection: Option<RollbackSelection>`), MCP `operations_rollback`
and `undo_operations` pass `None`. The selection rides on `InversePlan` (where `execute_rollback` reads it), entered
through a new `rollback_operation_with_selection`; `rollback_operation` stays as its `None` wrapper, so its ~20 existing
callers and tests (app backend suites, `cmdr-operations` test modules) stay byte-identical (loud rule 2).
`RollbackRefusal::NothingSelected` and `SelectionNeedsSummary` (or one variant; agent's call) with their frontend words.

**Intentions.** Roll back exactly the selected units, with every guard intact, and leave the op honestly
`partiallyRolledBack` when anything selectable stays behind. "Roll back the rest" is the existing re-entry with `None`.

**Landmines.**

- The filter sits where the loop already decides per row, BEFORE pacing, the pause gate, and verification; a filtered
  row costs nothing else. ❌ No per-row DB read: the dir cache (one read per distinct dir) plus the item map. The engine
  re-normalizes the selection it's handed.
- A DIR unit is decided from its `self_dir_id` (unchecking a trashed folder's own row must keep it in the Trash).
- The filter decides on the raw `OperationItemRow` BEFORE `map_rollback_unit` (§ "Design decisions").
- `left_out` feeds `resolve_final_state` (Kani proof + mutants per § "Design decisions"). Run `pnpm check kani` locally
  if the toolchain is there, else say so; `desktop-rust-property-mutants` checks the new mutant and is left to CI.
- Progress totals with a selection come from the summary algebra plus item-toggle point reads (milestone 4), never from
  a pre-scan; with `None` keep `read_rollback_file_totals`.
- `summarize_inverse` derives its unit from the selection (§ "Design decisions"), ❌ never by streaming.
- The demo's `RollbackArgs` gets `selection` AND `#[serde(deny_unknown_fields)]`; a demo cell rolls back a subset and
  asserts the rest stayed. Without it, getcmdr.com/demo would reverse everything while the UI says "selected".
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
units, one selected; unchecking a trashed folder's OWN row keeps it in the Trash and leaves the op partial; a selection
whose units were all already put back by hand (`AlreadyGone`, counted as reversed) lands `rolledBack` when nothing else
awaits and `partiallyRolledBack` when other units were left out (two cells); crash reconcile mid-selective-pass
(milestone 1's rule holds); progress totals equal the frames the loop credits (files only, `MAX(size, 0)`), item toggles
included; the 0-of-0 case (copy a folder, add a file into it, roll back so the folder is held back, remove the file,
roll back with `None`) lands `rolledBack`; a wire selection with a foreign dir id or a Contents seq is normalized away.
Every milestone 1 pin unchanged.

**DONE.** Engine, both hosts, MCP unchanged in behavior; `DETAILS.md` § "Streaming + ordering" and the state-machine
section describe the selection; the frontend `DETAILS.md` re-entry argument mentions selections.

### 6. `ColumnList` tree mode (parallel from day one)

**Scope.** `apps/desktop/src/lib/ui/ColumnList.svelte`, `column-list-types.ts`, `column-list-layout.ts` (if the height
cap below bites), a new pure `tree-cursor.ts` beside `list-cursor.ts`, tests beside them, the tier-3 a11y test, Debug >
Components (`routes/dev/components/sections/ColumnListSection.svelte`), `docs/design-system.md`,
`apps/desktop/src/lib/ui/DETAILS.md` § ColumnList.

**Intentions.** Per § "Design decisions" (component): `ColumnList` gains only `semantics: 'treegrid'` with `rowAria`,
windowed sources only; the indent and disclosure are the consumer's snippet; ←/→ are `tree-cursor.ts`. Every existing
consumer (Search, Selection, rename review, Multi-rename preview) unchanged.

**Landmines.**

- **The browser's height cap.** Spacers are `range.start × rowHeightPx`; 2M rows × ~24 px ≈ 48M px, and WebKit's layout
  units cap somewhere around 33.5M px (unverified on WKWebView). Measure first: a windowed `ColumnList` with
  `count = 2_000_000` in the dev app, press End, check the last row is reachable and `scrollIndexIntoView` lands. If it
  clamps, add a scaled scroll mapping (virtual position ↔ capped scroll height) in `column-list-layout.ts` with unit
  tests, and add this browser-side case to the scale acceptance. File an issue if the panes' `virtual-scroll.ts` shares
  the shape (unrelated to this card).
- **A checkbox in a treegrid cell must not take focus.** Ark's hidden input takes focus on click, which drops the grid's
  `aria-activedescendant`, and a focused native checkbox also toggles on Space, so the dialog's Space would flip it
  twice. `tabindex={-1}` plus `preventDefault` on pointerdown; the click moves the cursor and toggles through the same
  path as Space. No existing consumer has a non-tabbable checkbox in a grid; this is the first.
- **A cursor on a placeholder row**: `activeDescendantId` already answers `undefined` for an unloaded row. What needs
  proving is that the deferred active descendant NAMES the row once it loads (PageDown or End into unloaded rows, then
  the window lands), so VoiceOver reads it.
- **`aria-selected` follows the cursor in a grid** (`ColumnList` sets it from `isUnderCursor`). In a treegrid whose rows
  carry checkboxes, a screen reader would hear "selected" for the cursor row whatever its checkbox says, so `treegrid`
  drops `aria-selected`; the active descendant marks the cursor, the checkbox its state.
- The disclosure is a pointer target, not a Tab stop; the row (via active descendant) is the keyboard target.
- `aria-expanded` only on expandable rows; `aria-level` 1-based.
- `heldRowOf` keeps working for tree rows (a collapse removes many rows at once).
- ❌ Never `TransferTree` changes in this milestone.

**Test plan.** Component tests: `rowAria` lands on each row, `tree-cursor.ts` (→ expands a shut folder and steps into an
open one, ← collapses or goes to the parent, ← on a top row leaves the tree), cursor keys unchanged, a checkbox click
keeps focus on the grid (the "Space then toggles exactly once more" cell is milestone 8's, since Space is the dialog's),
the loaded-row announcement, no `aria-selected` on treegrid rows. Tier-3 a11y test for treegrid. Existing ColumnList
tests unchanged.

**DONE.** Mode shipped with its Debug row and design-system entry; the 2M-row reach measured (and fixed if needed);
consumers untouched.

### 7. The master–detail dialog

**Scope.** `apps/desktop/src/lib/operation-log/` (dialog split into the op list, the detail pane, and a tree source
module over `get_operation_log_tree`), `operation-log-labels.ts` (kind-aware status words, skip-reason words,
unexpandable reasons, the second partial notice), `#lib/tauri-commands/operation-log.ts`, `RollbackConfirmDialog`
(`selection: 'all' | 'subset'` wording), `messages/en/operationLog.json` (+ `fileOperations.json` for the confirmation),
component tests.

**Intentions.** The mockup's layout and states: left op list (a `grid`: summary, who, when, one status glyph, colored
rollback state), right detail (header with summary, badges with "Finished" for the execution badge, route line "From X
to Y", standing notice, relation links), the tree filling the height with a tri-state "select all" checkbox in its
header (a `column.header` snippet on the same base flip as ⌘A / ⌘⇧A, with an accessible name), the action bar (**Roll
back selected (N)** + "N of M selected", "Roll back the rest" when nothing selectable remains but rows still wait, plus
`RollbackControls` while a reversal runs). ~900 × 640 default, resizable; under ~640 px the split stacks. Status column
speaks only when it matters (Rolled back, Skipped + reason, Didn't finish). Middle ellipsis on names
(`shorten-middle-action`), full path in the tooltip. Read-only tree, no checkboxes, on a not-rollbackable op AND while
the op is `rolling_back` (toggles frozen). Placeholders for unloaded rows; spinner only past ~200 ms; backfill progress
past ~10 s. The tree source is coalesced and generation-tagged, and the op cursor settles ~150 ms before the tree loads
(§ "Design decisions"). Counts ("N of M", "Roll back selected (N)", folder rollups like "12 files, 3 rolled back") go
through the house integer formatter (`formatInteger`, thousands separators) with ICU plurals.

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
- The toggle cap answers `toggles_capped`: word it ("That's as many separate picks as Cmdr can hold. Pick a folder
  instead."), and keep the previous selection.

**Test plan.** Component tests over a mocked tree source: default all-checked + count, folder uncheck cascades, greyed
contents with their tooltip, read-only op (and a `rolling_back` one) shows no live checkboxes, the header select-all
tri-state, status words per role and reason, the partial notice from counts, confirmation says "selected" only for a
subset, the dispatch carries the normalized toggles, the action bar reflects `rowRollbackAction` and the 0-of-0 "the
rest" case, two toggles before the first answer lands both survive, ↓ ten times fast ⇒ one tree request and at most one
backfill start, expanding a folder above the cursor keeps the cursor on its row, the cap's words, thousands separators.
Update `OperationLogDialog.a11y.test.ts`.

**DONE.** The dialog matches the mockup's three states; docs in `apps/desktop/src/lib/operation-log/` C+D updated
(layout, the tree source, the selection flow); David gets screenshots of the three states for review.

### 8. Keyboard

**Scope.** `apps/desktop/src/lib/commands/sources/operation-log.ts` (new, scope `'Main window/Operation log'`), its
registry wiring, the dialog's keydown, the dispatch-side route for ⌘A / ⌘⇧A (`routes/(main)/`), tests.

**Intentions.** The mockup's keyboard map: op list ↑/↓/Home/End/PgUp/PgDn with the cursor as the selection, → or ↩ into
the tree; tree ↑/↓/PgUp/PgDn, → / ← through `tree-cursor.ts`, ← on a top row back to the op list, Space toggles
(cascading), Insert toggles and moves down, ⌘A / ⌘⇧A all / none, ⌘↩ rolls back the selection (via the button's
`keyHint`), Escape closes. All through `eventMatchesCommand` / `list-cursor.ts` / `tree-cursor.ts`. The real Tab order
follows the DOM: op list → Load more → the header's relation links → the tree → the action bar → Close. State it in the
docs and pin it.

**Landmines.**

- ⌘A / ⌘⇧A arrive as the native menu's `selection.selectAll` / `selection.deselectAll`, which the dialog gate refuses
  over any dialog: they never reach the dialog's keydown. Route them to the Operation log's tree when it holds focus (§
  "Design decisions"); a component keydown test passes while the real app fails, so test through the dispatch path (or
  an E2E through the menu).
- Insert's panes command is `selection.toggleAndDown`: a dialog-scoped twin, not a reuse across scopes, unless the
  registry supports that. Read `apps/desktop/src/lib/commands/CLAUDE.md` § Gotchas for every place a command touches.

**Test plan.** Keyboard component tests for each row of the map, a checkbox click followed by Space toggles exactly once
more, the dispatch-path test for ⌘A / ⌘⇧A, the Tab order in `OperationLogDialog.a11y.test.ts`, plus the registry
collision tests.

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

**Test plan.** Two new tests sharing one staging helper, each under the E2E two-second bar (loud rule 11): (1) stage a
folder copy, open the log, uncheck one file, roll back selected, assert exactly the rest is gone and the op reads
partial with the right notice; (2) stage the same and roll back a subset through the engine command directly, then press
"Roll back the rest" in the dialog and assert `rolledBack`. Existing specs adapted to the new selectors, assertions
unchanged.

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
6. The engine adds no per-ROW DB read (one per distinct dir, for the decision cache); the writer's own outcome-delta
   read happens on the writer thread.
7. Crash reconcile reads the original op's outcomes, its summary, AND every inverse's rows; a crash at any point of any
   pass never reverts a partly reversed op to `rollbackable`.
8. Every migration step is new and appended; no shipped step changes.
9. Both hosts (desktop, demo) answer every new or changed command with the crate's logic.
10. The frontend never builds a tree, never words a backend string, and sends back only toggles it was handed.
11. Copy stays count-free in the confirmation, promises no complete reversal, and never says "error" or "failed".
12. Summary columns are pure functions of row fields; op-level rollbackability applies at query time. A summary write
    failure marks the op unbuilt, never drops item rows.
13. A DIR unit is decided by its own folder's toggle; selections are normalized by the crate on every read AND by the
    engine, never trusted from the wire.
14. The demo refuses unknown command fields, so it can never silently ignore a selection.
15. No interactive state is a dead end: when nothing is selectable but rows still await reversal, "Roll back the rest"
    is offered.
