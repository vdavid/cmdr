/**
 * What a reversal will DO to the user's files, and every surface's wording for it.
 *
 * One classifier feeds two moments that must never contradict each other: the question
 * asked before a rollback starts (`RollbackConfirmDialog`), and the running bar named
 * two seconds later (the queue row, the corner chip, the progress dialog). They read
 * the same `RollbackConfirmVariant`, so a copy edit can't leave one promising a restore
 * while the other announces a delete.
 *
 * Pure, no I/O: it returns catalog KEYS, so the callers resolve them with their own
 * placeholders.
 */

import type { OpKind } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * What rolling back will DO to the user's files.
 *
 * The three `undo*` values mirror the backend's `inverse_kind`
 * (`src-tauri/src/operation_log/rollback.rs`), the one place that decides whether an
 * inverse deletes, moves, or renames.
 */
export type RollbackConfirmVariant =
  /** A copy or move still running: stop it and delete what it has written. */
  | 'stopAndDelete'
  /** Undoing a finished copy, new file/folder, or compress: delete what it made. */
  | 'undoByDeleting'
  /** Undoing a finished move or trash: the files travel back, nothing is deleted. */
  | 'undoByMovingBack'
  /** Undoing a finished rename: the names change back, nothing moves. */
  | 'undoByRenamingBack'

/**
 * Which reversal an operation of this kind earns, because what a rollback DOES depends
 * on what was done: undoing a copy deletes, undoing a move carries the files home,
 * undoing a rename only changes names back.
 *
 * Mirrors the backend's `inverse_kind` (`operation_log/rollback.rs`) arm for arm,
 * including its `delete → delete` arm: a permanent delete is never rollbackable, so the
 * button never appears on one, and the arm exists so a NEW `OpKind` is a compile error
 * here rather than a confidently wrong sentence in front of a user.
 */
export function rollbackConfirmVariant(kind: OpKind): RollbackConfirmVariant {
  switch (kind) {
    case 'copy':
    case 'createFolder':
    case 'createFile':
    case 'archiveEdit':
    case 'delete':
      return 'undoByDeleting'
    case 'move':
    case 'trash':
      return 'undoByMovingBack'
    case 'rename':
      return 'undoByRenamingBack'
  }
}

/**
 * What a running reversal is called where an operation gets NAMED without room for a
 * count: the queue row's label and the corner chip's action word. Both sit beside a
 * readout or a tooltip that already carries the numbers, so these stay count-free, the
 * way `queue.row.label`'s "Copying" does.
 *
 * `stopAndDelete` can't reach here — the backend sets `reverses` only on an operation
 * that IS the reversal of a finished one — but it's mapped so a new variant is a compile
 * error rather than a blank label.
 */
export function reversalLabelKey(variant: RollbackConfirmVariant): MessageKey {
  switch (variant) {
    case 'undoByMovingBack':
      return 'queue.row.reversalMovingBack'
    case 'undoByDeleting':
      return 'queue.row.reversalDeleting'
    case 'undoByRenamingBack':
      return 'queue.row.reversalRenamingBack'
    // Unreachable: the backend sets `reverses` only on an operation that IS the
    // reversal of a finished one. Mapped to the in-flight title so a new variant
    // is a compile error here rather than a blank label, and so this arm costs no
    // dead string in ten locales.
    case 'stopAndDelete':
      return 'fileOperations.transferProgress.titleRollingBack'
  }
}

/**
 * The progress dialog's title for the same reversal, where there IS room for the scope:
 * "Putting 1,240 files back". Each key takes `count` (the plural selector) and
 * `countText` (the same number, already grouped for the locale), and each has a `=0` arm
 * for the frames before the backend's journal count lands.
 */
export function reversalTitleKey(variant: RollbackConfirmVariant): MessageKey {
  switch (variant) {
    case 'undoByMovingBack':
      return 'fileOperations.transferProgress.titleReversalMovingBack'
    case 'undoByDeleting':
      return 'fileOperations.transferProgress.titleReversalDeleting'
    case 'undoByRenamingBack':
      return 'fileOperations.transferProgress.titleReversalRenamingBack'
    case 'stopAndDelete':
      return 'fileOperations.transferProgress.titleRollingBack'
  }
}
