/**
 * What the reversal after a cancel actually managed, turned into the toast a
 * person reads a second later.
 *
 * The reversal leaves things behind on purpose: it refuses to remove or move
 * back anything it can't match against what the transfer wrote
 * (`src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "What a
 * reversal does with that identity"). The bar drains to zero either way, so
 * zero means "this reversal is finished", never "everything came off the disk" —
 * this readout is what says which of the two happened.
 *
 * Pure and separate from the raising, so the honesty rule is testable on its
 * own: a reversal that left anything behind must never render as a clean
 * success, and it must name the reason rather than a reason class.
 */

import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { tString } from '$lib/intl/messages.svelte'
import { addToast } from '$lib/ui/toast'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { ToastLevel } from '$lib/ui/toast'
import type { CancelRollback, SkipBreakdown, SkipReason, WriteOperationType } from '$lib/ipc/bindings'
import CancelRollbackToastContent from './CancelRollbackToastContent.svelte'

/** What the toast says, already localized, in the order the lines are read. */
export interface CancelRollbackReadout {
  /** What the reversal managed. `null` when it managed nothing, so the toast
   *  opens on the explanation rather than on "removed 0 items". */
  headline: string | null
  /** Sets the expectation before the reasons. `null` unless something stayed. */
  leftBehind: string | null
  /** One line per typed reason, in the order the backend grouped them. */
  reasons: string[]
  level: ToastLevel
}

/**
 * What a reversal DID to the files, which decides every verb in the toast.
 *
 * Only a same-volume move carries items home; every other in-flight reversal
 * deletes what the transfer wrote. A cross-volume move can't be reversed at all
 * and reports `notRolledBack`, so it never reaches here.
 *
 * Read off the EVENT's type, never a view's config: a dialog that ADOPTED a
 * running operation was handed no birth context, so its config's operation type
 * is inert there.
 */
function movesItemsBack(operationType: WriteOperationType): boolean {
  return operationType === 'move'
}

/**
 * Per reason a reversal can leave an item alone: the line that NAMES the one
 * item it applies to, and the line that COUNTS them when it applies to several.
 * Two keys rather than one ICU plural, because "name it" vs "count them" is a
 * display decision, not a plural category — a locale with only `other` (Chinese,
 * Vietnamese) could not express both.
 *
 * `alreadyGone` is `null`: the desired end state already held, so the backend
 * counts it as reversed and it never arrives as a skip. Mapped anyway, so a new
 * `SkipReason` is a compile error here rather than a silently dropped line.
 */
const REASON_KEYS: Record<SkipReason, { named: MessageKey; counted: MessageKey } | null> = {
  drift: {
    named: 'fileOperations.cancelRollback.reason.drift.named',
    counted: 'fileOperations.cancelRollback.reason.drift.counted',
  },
  unverifiablePrecondition: {
    named: 'fileOperations.cancelRollback.reason.unverifiable.named',
    counted: 'fileOperations.cancelRollback.reason.unverifiable.counted',
  },
  restoreTargetOccupied: {
    named: 'fileOperations.cancelRollback.reason.spotTaken.named',
    counted: 'fileOperations.cancelRollback.reason.spotTaken.counted',
  },
  dirNotEmpty: {
    named: 'fileOperations.cancelRollback.reason.folderNotEmpty.named',
    counted: 'fileOperations.cancelRollback.reason.folderNotEmpty.counted',
  },
  failed: {
    named: 'fileOperations.cancelRollback.reason.failed.named',
    counted: 'fileOperations.cancelRollback.reason.failed.counted',
  },
  alreadyGone: null,
}

/** What happened to the items one reason applies to: named when it's the only
 *  one, counted when there are several. `null` when the reason has no line. */
function reasonLine(group: SkipBreakdown): string | null {
  const keys = REASON_KEYS[group.reason]
  if (keys === null) return null
  if (group.count === 1) return tString(keys.named, { name: group.exampleName })
  return tString(keys.counted, { countText: formatNumber(group.count), count: group.count })
}

/**
 * A leftover the DRIVE refused is not the same news as one Cmdr chose to keep.
 *
 * Every other reason is Cmdr protecting something, which is `info`; `failed`
 * means the undo asked and was turned down, which is worth a colour that says
 * "look at this" and may be worth retrying once the drive is back.
 */
function levelFor(skips: SkipBreakdown[]): ToastLevel {
  return skips.some((group) => group.reason === 'failed') ? 'warn' : 'info'
}

/** The count line for a reversal that finished, with nothing left behind. */
function cleanHeadline(operationType: WriteOperationType, reversed: number): string {
  const key: MessageKey = movesItemsBack(operationType)
    ? 'fileOperations.cancelRollback.doneMovingBack'
    : 'fileOperations.cancelRollback.doneDeleting'
  return tString(key, { countText: formatNumber(reversed), count: reversed })
}

/** The count line for a reversal that left something behind: the same numbers,
 *  worded so it claims nothing about what it didn't reach. */
function partialHeadline(operationType: WriteOperationType, reversed: number): string {
  const key: MessageKey = movesItemsBack(operationType)
    ? 'fileOperations.cancelRollback.someMovedBack'
    : 'fileOperations.cancelRollback.someDeleted'
  return tString(key, { countText: formatNumber(reversed), count: reversed })
}

/** The line for a reversal the user stopped partway, which says so rather than
 *  leaving the leftovers looking like Cmdr's decision. */
function stoppedHeadline(operationType: WriteOperationType, reversed: number): string {
  const key: MessageKey = movesItemsBack(operationType)
    ? 'fileOperations.cancelRollback.stoppedMovingBack'
    : 'fileOperations.cancelRollback.stoppedDeleting'
  return tString(key, { countText: formatNumber(reversed), count: reversed })
}

/**
 * Read a finished reversal as the lines to show, or `null` when it has nothing
 * to say.
 *
 * Two silences, both deliberate. `notRolledBack` means everything the transfer
 * wrote is still where it landed, which is what a plain Cancel asks for and
 * what stopping a reversal before its first item leaves — announcing it would
 * be announcing that nothing happened. A clean reversal of an EMPTY ledger is
 * the same: the transfer hadn't written anything yet, so there is no undo to
 * report.
 *
 * A reversal the user stopped partway is told apart by its empty `skips`: a
 * full pass that skipped nothing lands `rolledBack`, so `partiallyRolledBack`
 * with no groups can only be a stop. When a stop DOES carry groups, the partial
 * wording covers it, because every line it prints is true either way and none
 * of them claims the ledger was walked to the end.
 */
export function readCancelRollback(
  rollback: CancelRollback,
  operationType: WriteOperationType,
): CancelRollbackReadout | null {
  const { outcome, reversed, skips } = rollback
  if (outcome === 'notRolledBack') return null
  if (outcome === 'rolledBack') {
    if (reversed === 0) return null
    return { headline: cleanHeadline(operationType, reversed), leftBehind: null, reasons: [], level: 'success' }
  }

  const reasons = skips.map(reasonLine).filter((line): line is string => line !== null)
  if (reasons.length === 0) {
    return { headline: stoppedHeadline(operationType, reversed), leftBehind: null, reasons: [], level: 'info' }
  }
  return {
    headline: reversed === 0 ? null : partialHeadline(operationType, reversed),
    leftBehind: tString('fileOperations.cancelRollback.leftBehind'),
    reasons,
    level: levelFor(skips),
  }
}

/** A plain line reads for about as long as the app's other result toasts. */
const SUMMARY_TIMEOUT_MS = 7000
/** A list of reasons is more to read, and it's the case a user most needs to
 *  finish reading, so it holds for longer. Hovering pauses the clock. */
const REASONS_TIMEOUT_MS = 12000

/** Dedup id, so a second cancelled transfer replaces the last one's summary
 *  rather than stacking a second conversation about undone files. */
const CANCEL_ROLLBACK_TOAST_ID = 'cancel-rollback'

/** Say what the reversal after a cancel managed, when there's anything to say. */
export function raiseCancelRollbackToast(rollback: CancelRollback, operationType: WriteOperationType): void {
  const readout = readCancelRollback(rollback, operationType)
  if (readout === null) return
  addToast(CancelRollbackToastContent, {
    id: CANCEL_ROLLBACK_TOAST_ID,
    level: readout.level,
    timeoutMs: readout.reasons.length > 0 ? REASONS_TIMEOUT_MS : SUMMARY_TIMEOUT_MS,
    props: { readout },
  })
}
