# Operation log (frontend)

The alpha "Operation log" dialog (View > Operation log, ⌥⌘L): your file operations newest first, each expandable to its
per-item rows, and each reversible one carrying a Roll back button. Reads the journal through
`$lib/tauri-commands/operation-log.ts`; the durable journal itself is
`apps/desktop/src-tauri/src/operation_log/CLAUDE.md`.

## Module map

- `operation-log-trigger.svelte.ts`: `operationLogState` plus open / close / page-more, and `markOperationRollingBack`.
- `OperationLogDialog.svelte`: the soft dialog. Owns the pending rollback question and stacks
  `$lib/file-operations/RollbackConfirmDialog.svelte` over itself.
- `operation-log-labels.ts`: the pure typed-enum → UI mappings (labels, the refusal notice). The confirmation variant is
  `$lib/file-operations/reversal-wording.ts`, which words the RUNNING reversal off the same map.
- `RollbackControls.svelte`: Pause / Resume and Cancel on a row whose reversal is running.
- `rollback-refusal.ts`: the `TypedFailure` that carries a `RollbackRefusal` from the command wrapper to the row.

## Must-knows

- **Every string comes from a typed enum through `operation-log-labels.ts`, ❌ never a display string the backend
  rendered.** Exhaustive switches, so a new `OpKind` or `RollbackState` is a compile error rather than a blank cell.
- **Roll back dispatches and lets go. ❌ Don't grow a progress dialog here.** The user is reading history, not watching
  a transfer; the status corner and the queue window own the reversal from the moment the command returns. The row's
  Pause / Cancel are the one exception, and they're the queue's own commands, ❌ never a second path to the IPC.
- **A rolling-back row commands the REVERSAL, on `inverseOpId`, ❌ never its own `opId`** (a finished operation). The id
  is journal truth on every read AND on the dispatch's answer, so a reversal someone else started gets the same buttons.
  Which control shows follows the SESSION, ❌ not the read-on-open journal row. A reversal emits NO terminal event, so
  liveness also reads `session.leftRegistry`, or the buttons outlive it and every press reaches nothing.
- **The badge flip to "Rolling back" is journal truth, not optimism.** The backend gate writes `rolling_back`
  synchronously before the dispatch returns, so `markOperationRollingBack` repeats what the journal already says. ❌
  Don't turn it into a re-read.
- **The confirmation's `variant` must match what the inverse DOES.** `rollbackConfirmVariant` mirrors the backend's
  `inverse_kind`; wording a move's reversal as a delete would scare people off an operation that takes nothing away. The
  queue row, corner chip, and progress dialog name the running reversal off that SAME variant, so they can't contradict
  the answer the user just gave.
- **A rollback from history can come out PARTIAL**: it skips anything it can't verify against the snapshot the journal
  recorded. ❌ No copy here may promise a complete reversal.
- **A refusal is typed.** Catch it with `asRollbackRefusal`, word it with `rollbackRefusalNotice`; ❌ never render the
  wire value, and never let a refused press look like nothing happened.
- **A row offers a button on exactly the states `check_rollbackable` admits** (`rowRollbackAction`): `rollbackable` says
  "Roll back", `partiallyRolledBack` says "Finish rolling back". ❌ Never widen it past that gate, and ❌ never word a
  finish as a fresh rollback. Finishing is safe because every per-item inverse rechecks then acts, so an item the first
  pass reversed reads as gone and is credited without touching a thing; `DETAILS.md` has the full argument.
- **Both states whose badge leaves a person guessing explain themselves on sight** (`rowStandingNotice`, exhaustive over
  `RollbackState`): a `notRollbackable` row carries its stored reason, a partly-reversed one says what became of the
  files. Pinned by the component, a11y, and E2E suites.
- **`entries.length` is the paging offset**, one source of truth, so an append can't desync from what's shown.

Flows, the routing decision, the copy contract, and the caching rules: `DETAILS.md`.
