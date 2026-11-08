# Operation log (frontend)

The alpha "Operation log" dialog (View > Operation log, ⌥⌘L), master–detail: recent file operations on the left, the
picked one's header, notice, and item tree on the right, with a checkbox on everything a rollback can still take, and an
action bar that rolls back what's checked. Reads the journal through `#lib/tauri-commands/operation-log.ts`; the durable
journal and the tree itself are `crates/cmdr-operation-log/CLAUDE.md`.

## Module map

- `operation-log-trigger.svelte.ts`: `operationLogState` plus open / close / page-more, and `markOperationRollingBack`.
- `OperationLogDialog.svelte`: the shell. Owns the list cursor, the tree source, the action bar, and the pending
  rollback question (stacks `#lib/file-operations/RollbackConfirmDialog.svelte`). Panes: `OperationList.svelte`,
  `OperationDetail.svelte`, `OperationTree.svelte`.
- `operation-tree-source.svelte.ts`: the tree's hand toggles, cursor, window, and backfill over
  `get_operation_log_tree`.
- `operation-log-labels.ts` / `operation-tree-labels.ts`: the pure typed-enum → UI mappings.
- `RollbackControls.svelte`: Pause / Resume and Cancel for a running reversal. `rollback-refusal.ts`: the typed refusal.

## Must-knows

- **Every string comes from a typed enum through the two label modules, ❌ never a display string the backend
  rendered.** Exhaustive switches, so a new variant is a compile error rather than a blank cell.
- **❌ No tree model in TS.** The backend owns expansion, compaction, tri-state, and "N of M"; the source holds only
  hand toggles and sends the selection back exactly as the last answer normalized it.
- **Answers are generation-tagged**: only an answer asked at the latest edit is adopted, or a fast second toggle is
  wiped. The tree reads only after the list cursor rests (`SETTLE_MS`).
- **Untouched means everything**: the default selection is `null`, and a press sends `null` unless the person narrowed
  it, so a plain press behaves exactly like a whole-operation rollback.
- **The action bar offers a press on exactly the states `check_rollbackable` admits** (`rollbackAction` over
  `rowRollbackAction`): `rollbackable` says "Roll back selected (N)" (or "Roll back" at 0 of 0 or with no tree), and
  `partiallyRolledBack` says "Roll back the rest" until narrowed. ❌ Never widen it, ❌ never word the rest as a fresh
  rollback, and ❌ never a dead end at 0 of 0. Why the re-entry is safe: `DETAILS.md`.
- **Roll back dispatches and lets go. ❌ Don't grow a progress dialog here.** The status corner and the queue own the
  reversal; `RollbackControls` are the queue's own commands.
- **The controls command the REVERSAL, on `inverseOpId`, ❌ never the operation's `opId`.** Which control shows follows
  the SESSION; its end (`onEnded`) re-reads the header and the tree. ❌ No polling.
- **The badge flip to "Rolling back" is journal truth**: the backend writes `rolling_back` before the dispatch returns.
- **The confirmation's `variant` must match what the inverse DOES** (`rollbackConfirmVariant`), and it says "selected"
  only for a subset. ❌ No copy may promise a complete reversal.
- **A refusal is typed.** Catch it with `asRollbackRefusal`, word it with `rollbackRefusalNotice`.
- **`entries.length` is the paging offset**, one source of truth.
- **Keys are registry commands** (`Main window/Operation log`) read by `eventMatchesCommand`. ⌘A / ⌘⇧A fire as the
  native menu's Select all / Deselect all first, so the dialog's menu claims carry them; ❌ never keydown alone.

Layout, the tree source, the selection flow, and the copy contract: `DETAILS.md`.
