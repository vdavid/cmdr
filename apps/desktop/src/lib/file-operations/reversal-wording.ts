/**
 * What a reversal will DO to the user's files, and every surface's wording for it.
 *
 * One vocabulary feeds every moment that must never contradict another: the question
 * asked before a rollback starts (`RollbackConfirmDialog`), whether the operation is
 * still running or long finished, and the running bar named two seconds later (the
 * queue row, the corner chip, the progress dialog). They read the same
 * `RollbackConfirmVariant`, so a copy edit can't leave one promising a restore while
 * the other announces a delete.
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
  /** A copy still running: stop it and delete what it has written. */
  | 'stopAndDelete'
  /** A move still running: stop it and carry back what it has moved so far. */
  | 'stopAndMoveBack'
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
 * Which reversal a STILL-RUNNING operation earns, for the question in front of the
 * Rollback button on a live transfer.
 *
 * Same fact as {@link rollbackConfirmVariant} — undoing a copy takes files away,
 * undoing a move carries them home — asked at the other moment: the operation is
 * mid-flight, so the wording says "so far" and owns the overwrite it can't undo.
 * The two agree on which reversals remove files, pinned by the tests, because
 * that is a property of the operation and not of when the button was pressed.
 *
 * `rename` maps to the deleting arm it can never reach: a rename is one syscall,
 * so nothing of it is ever in flight to stop. The arm exists so a new `OpKind`
 * is a compile error here rather than a blank body in front of a user.
 */
export function inFlightRollbackVariant(kind: OpKind): RollbackConfirmVariant {
  switch (kind) {
    case 'move':
    case 'trash':
      return 'stopAndMoveBack'
    case 'copy':
    case 'createFolder':
    case 'createFile':
    case 'archiveEdit':
    case 'delete':
    case 'rename':
      return 'stopAndDelete'
  }
}

/**
 * What a running reversal is called where an operation gets NAMED without room for a
 * count: the queue row's label and the corner chip's action word. Both sit beside a
 * readout or a tooltip that already carries the numbers, so these stay count-free, the
 * way `queue.row.label`'s "Copying" does.
 *
 * The two in-flight variants can't reach here — the backend sets `reverses` only on an
 * operation that IS the reversal of a finished one — but they're mapped so a new variant
 * is a compile error rather than a blank label.
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
    // reversal of a finished one, and the two in-flight variants describe one that
    // is still running. Mapped to the in-flight title so a new variant is a compile
    // error here rather than a blank label, and so these arms cost no dead string
    // in ten locales.
    case 'stopAndDelete':
    case 'stopAndMoveBack':
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
    case 'stopAndMoveBack':
      return 'fileOperations.transferProgress.titleRollingBack'
  }
}
