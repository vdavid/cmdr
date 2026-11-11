/**
 * Undoing a trash: run the rollback engine over the operation and say what came
 * back, honestly.
 *
 * The engine already knows how to reverse a trash — a trash row records the OS's
 * own in-trash location, and its inverse is a restore-move back to the source
 * (`crates/cmdr-operation-log/src/rollback.rs`). This is the surface that offers it the moment it
 * matters, instead of leaving it in the operation log where nobody looks three
 * seconds after a mis-keyed F8.
 *
 * The composition is pure and lives apart from the toast so the honesty rule is
 * testable on its own: anything left behind outranks what came back. A restore
 * that put nine of ten files back must never read as a clean success, because the
 * tenth is still in the trash and only the user can decide that's fine.
 */

import { undoOperations, updateUndoTrashMenu, type UndoReport } from '#lib/tauri-commands/index.js'
import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { joinSentences } from '#lib/intl/sentence-join.js'
import { addToast, dismissToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { lastName } from '#lib/path/grammar.js'

const log = getAppLogger('fileOperations')

/**
 * Dedup id for the undo's own toast. Stable, so hitting Undo twice can't stack
 * two conversations about the same restore.
 */
const TRASH_UNDO_TOAST_ID = 'trash-undo'

/** The dedup id of a trash's own completion toast (Undo / "Go to trash"), so Edit > Undo trash can take it down. */
export function trashCompleteToastId(operationId: string): string {
  return `trash-complete-${operationId}`
}

/**
 * What Edit > Undo move to trash would put back, as its label says it: the one item by
 * name, several by count, or (when the completion can't tell which one item went) neither.
 * Wire-identical to Rust's `menu::TrashUndoTarget`, which renders the menu's words from it.
 */
export type TrashUndoTarget = { kind: 'one'; name: string } | { kind: 'many'; countText: string } | { kind: 'unknown' }

/** What a finished trash moved, as its completion knows it. */
export interface TrashedItems {
  /** What the trash was asked to move. */
  sourcePaths: string[]
  /** How many top-level items actually went (`filesProcessed`): fewer when the OS refused some. */
  itemCount: number
}

/**
 * The newest trash this session that nobody has undone yet: what Edit > Undo move to trash
 * (`edit.undoTrash`) reverses. The toast's Undo button is mouse-first and gone after ten
 * seconds; this keeps the same action a menu item and a palette command away for as long
 * as it's the last thing trashed. Session-only on purpose: an "Undo" that reached back to
 * a trash from yesterday would surprise more than it helps, and the operation log already
 * rolls back any trash in the journal.
 *
 * Every change is pushed to the menu bar (greyed out and plainly worded while this is
 * `null`); the palette reads it when it opens.
 */
let pending: { operationId: string; items: TrashedItems } | null = null

function setPending(next: typeof pending): void {
  pending = next
  pushToMenuBar()
}

/** Fire-and-forget: a menu label that didn't land is worth a log line, never a throw into a trash's completion. */
function pushToMenuBar(): void {
  void updateUndoTrashMenu(trashUndoTarget()).catch((error: unknown) => {
    log.warn('Could not update the Undo move to trash menu item: {error}', { error })
  })
}

/** Records a finished trash as the one Edit > Undo move to trash reverses, replacing any older one. */
export function rememberTrashForUndo(operationId: string, items: TrashedItems): void {
  setPending({ operationId, items })
}

/** What Edit > Undo move to trash would put back right now, or `null` when there's nothing to undo. */
export function trashUndoTarget(): TrashUndoTarget | null {
  if (pending === null) return null
  const { sourcePaths, itemCount } = pending.items
  if (itemCount > 1) return { kind: 'many', countText: formatNumber(itemCount) }
  // One item went. Its name is only certain when it was the only one asked for: the
  // completion counts what the OS refused, but doesn't say which.
  if (sourcePaths.length === 1) {
    const path = sourcePaths[0]
    return { kind: 'one', name: lastName(path) }
  }
  return { kind: 'unknown' }
}

/** Whether there's a trash to undo: the palette's `trashUndoAvailable` condition. */
export function isTrashUndoAvailable(): boolean {
  return pending !== null
}

/**
 * Push the current state at the menu bar again. For a language change, which rebuilds the
 * bar with the item disabled and plainly worded (`DualPaneExplorer`'s `menu-bar-rebuilt`).
 */
export function resyncTrashUndoMenu(): void {
  pushToMenuBar()
}

/**
 * What the command palette calls `edit.undoTrash` right now: the menu item's words. Falls back
 * to the static name, which is also what Settings > Shortcuts and the help window show.
 */
export function trashUndoCommandLabel(): string {
  const target = trashUndoTarget()
  switch (target?.kind) {
    case 'one':
      return tString('commands.editUndoTrash.one', { name: target.name })
    case 'many':
      return tString('commands.editUndoTrash.many', {
        countText: target.countText,
        count: pending?.items.itemCount ?? 0,
      })
    default:
      return tString('commands.editUndoTrash.label')
  }
}

/** What a finished trash undo amounts to, before it becomes a sentence. */
export type TrashUndoOutcome =
  /** Every item is back where it was. */
  | { status: 'restored'; restored: number }
  /**
   * Not everything came back: `skipped` stayed in the trash (Cmdr never overwrites while
   * undoing), and `gone` were no longer there to put back.
   */
  | { status: 'partial'; restored: number; skipped: number; gone: number }
  /** Nothing came back because the trash no longer holds any of it (emptied, or deleted since). */
  | { status: 'gone'; gone: number }
  /** Nothing was reversed and nothing was even attempted. */
  | { status: 'unavailable' }

/**
 * Read a finished undo as an outcome.
 *
 * A refused operation carries no per-item numbers at all (the whole thing was
 * turned down: already undone, or a volume it needs is gone), so it can't be
 * folded into `skipped` without understating what was missed. It lands in
 * `partial` when anything came back and `unavailable` when nothing did.
 *
 * An item the trash no longer holds comes back as a `vanished` skip, and is told
 * apart from one that stayed in the trash: "stayed in the trash" would send the
 * user looking for something that isn't there.
 */
export function trashUndoOutcome(report: UndoReport): TrashUndoOutcome {
  const refused = report.operations.some((operation) => operation.refusal !== null)
  if (report.restored === 0 && report.skipped === 0) return { status: 'unavailable' }
  const gone = report.operations
    .flatMap((operation) => operation.skips)
    .filter((group) => group.reason === 'vanished')
    .reduce((sum, group) => sum + group.count, 0)
  const skipped = report.skipped - gone
  if (report.restored === 0 && skipped === 0 && !refused) return { status: 'gone', gone }
  if (report.skipped > 0 || refused) {
    return { status: 'partial', restored: report.restored, skipped, gone }
  }
  return { status: 'restored', restored: report.restored }
}

/** The one line the toast shows once the undo settles. */
export function trashUndoMessage(outcome: TrashUndoOutcome): string {
  switch (outcome.status) {
    case 'restored':
      return tString('fileOperations.trash.undone', {
        countText: formatNumber(outcome.restored),
        count: outcome.restored,
      })
    case 'partial': {
      // Lead with what came back; "0 items stayed in the trash" would be noise.
      const head =
        outcome.skipped > 0 || outcome.gone === 0
          ? tString('fileOperations.trash.undonePartial', {
              restoredText: formatNumber(outcome.restored),
              restored: outcome.restored,
              skippedText: formatNumber(outcome.skipped),
              skipped: outcome.skipped,
            })
          : tString('fileOperations.trash.undone', {
              countText: formatNumber(outcome.restored),
              count: outcome.restored,
            })
      if (outcome.gone === 0) return head
      const goneLine = tString('fileOperations.trash.undoPartlyGone', {
        countText: formatNumber(outcome.gone),
        count: outcome.gone,
      })
      return joinSentences([head, goneLine])
    }
    case 'gone':
      return tString('fileOperations.trash.undoGone', { count: outcome.gone })
    case 'unavailable':
      return tString('fileOperations.trash.undoUnavailable')
  }
}

/** Only a complete restore is a success; the toast colours itself from this. */
export function trashUndoLevel(outcome: TrashUndoOutcome): 'success' | 'info' {
  return outcome.status === 'restored' ? 'success' : 'info'
}

/**
 * Run the undo and report it, start to finish.
 *
 * A restore is a queued operation, so it waits out anything already working the
 * same volume and can take a while. That's why it gets a PERSISTENT progress
 * toast rather than a transient one: a restore that outlives a 7-second timeout
 * must not leave the user with no sign that anything is happening. `addToast`
 * replaces content and level in place but never dismissal, so the result arrives
 * as a fresh transient toast rather than a replacement.
 */
export async function runTrashUndo(operationId: string): Promise<void> {
  // Whatever the outcome, this trash has had its undo: a second one could only be refused,
  // and whatever it couldn't put back (gone from the trash, or its spot taken) won't come
  // back on a retry either. So Edit > Undo move to trash greys out the moment it starts.
  if (pending?.operationId === operationId) setPending(null)
  addToast(tString('fileOperations.trash.undoing'), {
    id: TRASH_UNDO_TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
  })

  try {
    const report = await undoOperations([operationId])
    const outcome = trashUndoOutcome(report)
    log.info('Trash undo for {operationId} finished: {status}', { operationId, status: outcome.status })
    dismissToast(TRASH_UNDO_TOAST_ID)
    addToast(trashUndoMessage(outcome), { level: trashUndoLevel(outcome), timeoutMs: 7000 })
  } catch (error) {
    log.warn('Trash undo for {operationId} did not run: {error}', { operationId, error })
    dismissToast(TRASH_UNDO_TOAST_ID)
    addToast(tString('fileOperations.trash.undoUnavailable'), { level: 'info', timeoutMs: 7000 })
  }
}

/**
 * Edit > Undo move to trash: undo the newest trash not undone yet, taking its completion
 * toast down so its Undo can't run a second time. With nothing to undo it does nothing: the
 * menu item is greyed out and the palette row hidden then, so only a stale click or a
 * rebound key gets here.
 */
export async function undoLastTrash(): Promise<void> {
  if (pending === null) return
  const { operationId } = pending
  dismissToast(trashCompleteToastId(operationId))
  await runTrashUndo(operationId)
}

/** Test-only reset back to "nothing to undo", so one spec can't leak a trash into the next. */
export function _resetForTesting(): void {
  pending = null
}
