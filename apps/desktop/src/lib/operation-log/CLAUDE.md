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
- `rollback-refusal.ts`: the `TypedFailure` that carries a `RollbackRefusal` from the command wrapper to the row.

## Must-knows

- **Every string comes from a typed enum through `operation-log-labels.ts`, ❌ never a display string the backend
  rendered.** Exhaustive switches, so a new `OpKind` or `RollbackState` is a compile error rather than a blank cell.
- **Roll back dispatches and lets go. ❌ Don't grow a progress dialog here.** The user is reading history, not watching
  a transfer; the status corner and the queue window own the reversal from the moment the command returns.
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
- **A `notRollbackable` row explains itself on sight** (`notRollbackableNotice` off `notRollbackableReason`): it never
  offers the button whose refusal would otherwise carry the reason. Pinned by the component, a11y, and E2E suites.
- **`entries.length` is the paging offset**, one source of truth, so an append can't desync from what's shown.

Flows, the routing decision, the copy contract, and the caching rules: `DETAILS.md`.
