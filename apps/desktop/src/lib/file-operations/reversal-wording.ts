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

import type { LifecycleStatus, OpKind, RollbackSupport, WriteOperationPhase } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/**
 * What rolling back will DO to the user's files.
 *
 * The four `undo*` values mirror the backend's `inverse_kind`
 * (`crates/cmdr-operation-log/src/rollback.rs`), the one place that decides whether an
 * inverse deletes, moves, renames, or restores tags.
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
  /** Undoing a finished Finder-tag change: each file's prior tags come back, nothing moves. */
  | 'undoByRestoringTags'

/**
 * Which reversal an operation of this kind earns, because what a rollback DOES depends
 * on what was done: undoing a copy deletes, undoing a move carries the files home,
 * undoing a rename only changes names back.
 *
 * Mirrors the backend's `inverse_kind` (`crates/cmdr-operation-log/src/rollback.rs`) arm for arm,
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
    case 'tag':
      return 'undoByRestoringTags'
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
 * `rename` and `tag` map to the deleting arm they can never reach: a rename is one
 * syscall and a tag write one attribute per file, so nothing of either is ever in
 * flight to stop. The arm exists so a new `OpKind`
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
    case 'tag':
      return 'stopAndDelete'
  }
}

/**
 * Why Rollback is off for an operation whose STRATEGY can't reverse, as the disabled
 * button's tooltip: the reason in the person's terms, then what the Cancel beside it
 * does instead. `null` when the operation can roll back.
 *
 * Only copy and move show a Rollback button, so four reasons reach a person; the rest
 * share one plain sentence. They're mapped so a new `RollbackSupport` variant is a
 * compile error here rather than a vague tooltip in front of a user.
 */
export function rollbackBlockedTooltipKey(support: RollbackSupport): MessageKey | null {
  switch (support) {
    case 'supported':
      return null
    case 'crossVolumeMove':
      return 'fileOperations.transferProgress.rollbackUnavailableCrossVolumeTooltip'
    case 'sameVolumeMove':
      return 'fileOperations.transferProgress.rollbackUnavailableSameVolumeTooltip'
    case 'archiveRewrite':
      return 'fileOperations.transferProgress.rollbackUnavailableArchiveRewriteTooltip'
    case 'archiveMoveOut':
      return 'fileOperations.transferProgress.rollbackUnavailableArchiveMoveOutTooltip'
    case 'nothingToPutBack':
    case 'instantChange':
    case 'isReversal':
    case 'settled':
      return 'fileOperations.transferProgress.rollbackUnavailableOtherTooltip'
  }
}

/**
 * Whether pressing Rollback RIGHT NOW could still reverse anything, which the
 * operation's `rollbackSupport` can't answer on its own: that verdict is a property of
 * the STRATEGY, decided once at spawn, and this is the other half of the question.
 *
 * One case needs it, and it's the last stage of a move between filesystems. That move
 * copies into a staging folder, renames the tree into its final place, and only then
 * removes the originals — and the engine commits its transaction before that last
 * stage begins, so a Rollback pressed during the source sweep carries nothing home.
 * It stops the sweep, exactly like the Cancel beside it: the moved files stay at the
 * destination and the originals the sweep hasn't reached stay where they are, which
 * is what `fileOperations.cancelRollback.moveAlreadyLanded` reports afterwards. So
 * every surface that offers Rollback asks this too, or it offers a journey home the
 * engine can no longer make.
 *
 * The `deleting` phase is the signal because it IS the sweep (`cross_fs.rs`'s
 * `delete_sources_after_move` opens it with an unthrottled 0-of-total tick, the same
 * one that retitles the dialog "Removing the originals…"). Only for a move: a delete
 * wears the phase too, and a copy's reversal deletes what it wrote whatever phase it
 * is in.
 */
export function reversalWindowClosed(kind: OpKind, phase: WriteOperationPhase | null): boolean {
  return kind === 'move' && phase === 'deleting'
}

/** Why a live operation can't roll back right now. */
export type LiveRollbackBlock =
  /** It's queued or over: nothing is running to stop. */
  | 'notRunning'
  /** Its strategy can't reverse. */
  | 'cantReverse'
  /** It's already undoing what it wrote. */
  | 'alreadyRollingBack'
  /** It's still counting, so it has written nothing to put back. */
  | 'nothingWrittenYet'
  /** {@link reversalWindowClosed}: everything has already landed. */
  | 'reversalWindowClosed'

/**
 * Whether a live operation can roll back right now: the queue row's button and a tool's question
 * read the same verdict, so one can't offer what the other turns down.
 *
 * The status is the snapshot's: a parked operation emits no more ticks, so its last phase describes
 * a transfer that has stopped.
 */
export function liveRollbackBlock(facts: {
  status: LifecycleStatus
  rollbackSupport: RollbackSupport
  phase: WriteOperationPhase | null
  kind: OpKind
}): LiveRollbackBlock | null {
  if (facts.status !== 'running' && facts.status !== 'paused') return 'notRunning'
  if (facts.rollbackSupport !== 'supported') return 'cantReverse'
  if (facts.phase === 'rolling_back') return 'alreadyRollingBack'
  if (facts.phase === 'scanning') return 'nothingWrittenYet'
  if (reversalWindowClosed(facts.kind, facts.phase)) return 'reversalWindowClosed'
  return null
}

/**
 * The tooltip on a LIVE Rollback button, which exists to say how this click differs
 * from the plain Cancel beside it — so it has to name what the reversal does to the
 * files, the same split the confirmation two clicks later makes: a copy's undo
 * deletes what it wrote, a move's carries it back.
 *
 * The three `undo*` variants can't reach here (this is the button on a RUNNING
 * transfer), and they map to the sibling that shares their direction so a new variant
 * is a compile error rather than a copy's wording over a move.
 */
export function inFlightRollbackTooltipKey(variant: RollbackConfirmVariant): MessageKey {
  switch (variant) {
    case 'stopAndMoveBack':
    case 'undoByMovingBack':
      return 'fileOperations.transferProgress.rollbackTooltipStopAndMoveBack'
    case 'stopAndDelete':
    case 'undoByDeleting':
    case 'undoByRenamingBack':
    case 'undoByRestoringTags':
      return 'fileOperations.transferProgress.rollbackTooltip'
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
    case 'undoByRestoringTags':
      return 'queue.row.reversalRestoringTags'
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
    case 'undoByRestoringTags':
      return 'fileOperations.transferProgress.titleReversalRestoringTags'
    case 'stopAndDelete':
    case 'stopAndMoveBack':
      return 'fileOperations.transferProgress.titleRollingBack'
  }
}
