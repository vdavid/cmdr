/**
 * What the reversal after a cancel actually managed, turned into the toast a
 * person reads a second later.
 *
 * The reversal leaves things behind on purpose: it refuses to remove or move
 * back anything it can't match against what the transfer wrote
 * (`crates/cmdr-operations/src/transfer/DETAILS.md` § "What a
 * reversal does with that identity"). The bar drains to zero either way, so
 * zero means "this reversal is finished", never "everything came off the disk" —
 * this readout is what says which of the two happened.
 *
 * Pure and separate from the raising, so the honesty rule is testable on its
 * own: a reversal that left anything behind must never render as a clean
 * success, and it must name the reason rather than a reason class.
 */

import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { addToast } from '#lib/ui/toast/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { NonErrorToastLevel } from '#lib/ui/toast/index.js'
import type {
  CancelRollback,
  RecoveredOriginal,
  SkipBreakdown,
  SkipReason,
  StagedLeftoverFate,
  StagedLeftovers,
  WriteOperationType,
} from '#lib/ipc/bindings.js'
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
  /** One line per original that had to keep a recovered sibling name. */
  recovered: string[]
  /** One line per fate of Cmdr's own unfinished writes that stayed at the
   *  destination. Empty whenever the cleanup cleared them, which is the
   *  ordinary ending. */
  staged: string[]
  level: NonErrorToastLevel
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
  locked: {
    named: 'fileOperations.cancelRollback.reason.locked.named',
    counted: 'fileOperations.cancelRollback.reason.locked.counted',
  },
  alreadyGone: null,
  // Only the journal's restore-move (an undo of a FINISHED move, trash, or rename) reports it;
  // an in-flight reversal reads a missing item as `alreadyGone`.
  vanished: null,
  // Only the journal's undo of a FINISHED copy checks the copy's original; a cancel's
  // reversal removes copies whose originals it was reading moments ago.
  originalGone: null,
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
 * "look at this" and may be worth retrying once the drive is back. `locked` is
 * turned down too: a lock only an administrator can lift.
 */
function levelFor(skips: SkipBreakdown[]): NonErrorToastLevel {
  return skips.some((group) => group.reason === 'failed' || group.reason === 'locked') ? 'warn' : 'info'
}

/**
 * Per fate of Cmdr's own unfinished writes, the line that NAMES the one it
 * applies to and the line that COUNTS several, for the same reason as
 * `REASON_KEYS`. A `Record`, so a new fate is a compile error here rather than
 * a silently dropped line.
 */
const STAGED_KEYS: Record<StagedLeftoverFate, { named: MessageKey; counted: MessageKey }> = {
  refused: {
    named: 'fileOperations.cancelRollback.stagedLeftover.named',
    counted: 'fileOperations.cancelRollback.stagedLeftover.counted',
  },
  removedWhenBack: {
    named: 'fileOperations.cancelRollback.stagedLeftover.waiting.named',
    counted: 'fileOperations.cancelRollback.stagedLeftover.waiting.counted',
  },
  landsWhenBack: {
    named: 'fileOperations.cancelRollback.stagedLeftover.landing.named',
    counted: 'fileOperations.cancelRollback.stagedLeftover.landing.counted',
  },
  rescued: {
    named: 'fileOperations.cancelRollback.stagedLeftover.rescued.named',
    counted: 'fileOperations.cancelRollback.stagedLeftover.rescued.counted',
  },
}

/**
 * The line about one fate of Cmdr's OWN unfinished writes at the destination.
 *
 * Deliberately outside `reasons`: those lines sit under "Cmdr skips anything it
 * isn't sure about", and nothing chose to keep these. The fate decides the
 * words: scratch the destination refused, scratch waiting for a server to come
 * back, or a COMPLETE file waiting for its name or saved under a ` (recovered)`
 * one, which is the user's data and ❌ never worded as something to remove.
 * Naming it when there is one is the
 * same call the reason lines make: that name is what the user would go looking
 * for at the destination.
 */
function stagedLine(staged: StagedLeftovers): string {
  const keys = STAGED_KEYS[staged.fate]
  if (staged.count === 1) {
    return tString(keys.named, { name: staged.exampleName, volumeName: staged.volumeName })
  }
  return tString(keys.counted, {
    countText: formatNumber(staged.count),
    count: staged.count,
    volumeName: staged.volumeName,
  })
}

/** Name both paths when an original could not take its old name back. */
function recoveredLine(recovered: RecoveredOriginal): string {
  return tString('fileOperations.cancelRollback.recoveredOriginal', {
    path: recovered.path,
    keptAt: recovered.keptAt,
  })
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

/**
 * The line for a cross-filesystem move stopped on its last step, while it was
 * deleting the originals of files that had all already arrived.
 *
 * The only headline here that reports NO reversal, because none ran and none
 * can: carrying the files home would be a second full transfer across the
 * drives, which Cmdr doesn't do. So it claims nothing about an undo. It says
 * where the files are, that they're staying there, and how many originals are
 * still sitting in the folder the move started from — the ones already deleted
 * are gone, and this line must never suggest otherwise.
 */
function landedMoveHeadline(originalsLeft: number): string {
  return tString('fileOperations.cancelRollback.moveAlreadyLanded', {
    countText: formatNumber(originalsLeft),
    count: originalsLeft,
  })
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
 *
 * **Both silences break the moment `stagedLeftovers` arrives.** `outcome`
 * answers for the LEDGER alone, so a reversal really can walk it to the last
 * entry while gigabytes of Cmdr's own scratch sit at the destination, and a
 * user who chose Rollback asked for that destination to be cleared. So a
 * leftover always speaks, it always takes the wording that claims nothing about
 * completeness (never `cleanHeadline`, which says "the items"), and it is never
 * `success`.
 *
 * **`originalsStillInPlace` breaks the first silence for the same reason**, and
 * is the one shape where `notRolledBack` has real news. A cross-filesystem move
 * stopped on its last step has every file whole at the destination and some
 * originals already deleted, and no reversal is coming for them. Silence there
 * would leave a user who pressed Rollback believing nothing had happened.
 */
/**
 * The `notRolledBack` arm: no reversal ran, so there is no count of undone
 * items and nothing to word a verb around. Two things can still speak, and
 * neither of them is `outcome` — see `readCancelRollback` for why silence is
 * the right default and what breaks it.
 */
function readNothingReversed(
  originalsStillInPlace: CancelRollback['originalsStillInPlace'],
  staged: string[],
  recovered: string[],
): CancelRollbackReadout | null {
  const landed = originalsStillInPlace === null ? null : landedMoveHeadline(originalsStillInPlace.count)
  if (landed === null && staged.length === 0 && recovered.length === 0) return null
  return {
    headline: landed,
    leftBehind: null,
    reasons: [],
    recovered,
    staged,
    // Nothing went wrong on the landed-move path: the files are whole at the
    // destination and the line only says so. Cmdr's own scratch outliving the
    // sweep is the one thing here worth a colour.
    level: staged.length === 0 ? 'info' : 'warn',
  }
}

function readCompletedReversal(
  operationType: WriteOperationType,
  reversed: number,
  staged: string[],
  recovered: string[],
): CancelRollbackReadout | null {
  if (reversed === 0 && staged.length === 0 && recovered.length === 0) return null
  if (staged.length === 0 && recovered.length === 0) {
    return {
      headline: cleanHeadline(operationType, reversed),
      leftBehind: null,
      reasons: [],
      recovered: [],
      staged: [],
      level: 'success',
    }
  }
  return {
    headline: reversed === 0 ? null : partialHeadline(operationType, reversed),
    leftBehind: null,
    reasons: [],
    recovered,
    staged,
    level: staged.length === 0 ? 'info' : 'warn',
  }
}

export function readCancelRollback(
  rollback: CancelRollback,
  operationType: WriteOperationType,
): CancelRollbackReadout | null {
  const { outcome, reversed, skips, stagedLeftovers, originalsStillInPlace } = rollback
  const staged = stagedLeftovers.map(stagedLine)
  const recovered = rollback.recovered.map(recoveredLine)
  if (outcome === 'notRolledBack') return readNothingReversed(originalsStillInPlace, staged, recovered)
  if (outcome === 'rolledBack') return readCompletedReversal(operationType, reversed, staged, recovered)

  const reasons = skips.map(reasonLine).filter((line): line is string => line !== null)
  if (reasons.length === 0) {
    return {
      headline: stoppedHeadline(operationType, reversed),
      leftBehind: null,
      reasons: [],
      recovered,
      staged,
      level: staged.length === 0 ? 'info' : 'warn',
    }
  }
  return {
    headline: reversed === 0 ? null : partialHeadline(operationType, reversed),
    leftBehind: tString('fileOperations.cancelRollback.leftBehind'),
    reasons,
    recovered,
    staged,
    level: staged.length === 0 ? levelFor(skips) : 'warn',
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
    timeoutMs:
      readout.reasons.length > 0 || readout.recovered.length > 0 || readout.staged !== null
        ? REASONS_TIMEOUT_MS
        : SUMMARY_TIMEOUT_MS,
    props: { readout },
  })
}
