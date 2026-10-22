/**
 * Composes the toast shown when a copy/move/compress/trash/delete operation completes.
 *
 * This is the i18n pilot (the hardest existing multi-variable case): every
 * wording lives in `transfer.*` catalog keys resolved through `t()` and ICU
 * `select` + `plural`, NOT hardcoded English. The branch SHAPE matches the
 * historic composer exactly; `transfer-complete-toast.test.ts` is the parity
 * net asserting the rendered en output is byte-identical to the pre-i18n copy.
 *
 * Copy and move report what the user SELECTED at the top level, split by type:
 * "Moved 1 file and 3 folders". Interior counts never surface — moving one
 * folder with thousands of files inside still reads as one folder. When the user
 * skipped clashing files, the skipped count appears as a suffix (skips are
 * always file-level because folders always merge). When the per-type split is
 * unknown (a top-level kind probe came back partial), it falls back to the
 * file-count wording. Trash and delete keep the historic short wording.
 *
 * Two restructurings ICU needs (raw counts alone can't express the wording):
 *  - The "N files and M folders" join omits a zero part. ICU plural branches are
 *    independent and can't see each other's emptiness, so the caller passes a
 *    `kind` discriminator (`both` | `filesOnly` | `foldersOnly`) and the message
 *    `select`s on it.
 *  - Counts are embedded as `#lib/intl`-preformatted STRINGS (`formatNumber`)
 *    passed as `*Text` params, keeping number formatting single-sourced in
 *    `#lib/intl`. The raw integer is passed alongside ONLY to drive ICU plural
 *    selection (noun + was/were), never for display.
 */

import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import { formatDisjunctionList } from '#lib/intl/list-format.js'
import { joinSentences } from '#lib/intl/sentence-join.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type {
  AppearedDuringMove,
  DurabilityUnconfirmed,
  LockedKept,
  LostMetadataClass,
  MetadataLeftBehind,
  TopLevelSkipped,
  TrashRefusedItems,
} from '#lib/ipc/bindings.js'
import type { TransferOperationType } from '#lib/file-explorer/types.js'
import type { NonErrorToastLevel } from '#lib/ui/toast/index.js'
import { trashRefusedCountSentence } from './transfer-error-messages'

export interface TransferCompleteToastInput {
  operationType: TransferOperationType
  /** Every source file the operation considered (transferred + skipped). Mirrors the BE's `filesProcessed`. */
  filesProcessed: number
  /** Subset of `filesProcessed` that the user (or upfront policy) chose to skip via conflict resolution. */
  filesSkipped: number
  /** Top-level files the operation transferred (not interior counts). Supplied by F5/F6,
   *  drag-and-drop, and clipboard paste; omitted only when a top-level kind probe came
   *  back partial, where the composer falls back to the flattened file-count wording. */
  fileCount?: number
  /** Top-level folders the operation transferred. */
  folderCount?: number
  /** Which of those top-level items landed nothing at all, split the same way. Absent from an
   *  engine that doesn't track it, and the phrase then falls back to `fileCount`/`folderCount`
   *  as the moved counts. See the BE's `TopLevelSkipped`. */
  topLevelSkipped?: TopLevelSkipped | null
  /** What a cross-filesystem move left in the source because it never carried it: items that
   *  appeared there mid-move (a download finishing, a sync app), and originals saved over after
   *  their copy started. Absent on the ordinary move, and on every copy. Adds a sentence per kind;
   *  the move still reads as a success. */
  appearedDuringMove?: AppearedDuringMove | null
  /** What the destination couldn't store of the sources' metadata (creation dates on a server,
   *  Finder tags on a phone). Absent when nothing worth keeping stayed behind, which is the
   *  ordinary case. Adds one calm sentence naming the classes; the copy still reads as a success. */
  metadataLeftBehind?: MetadataLeftBehind | null
  /** The Locked items a move left where they were: ones the person chose to skip, and ones macOS
   *  keeps locked (no answer can move those). Absent when it left none, which is the ordinary case.
   *  Adds a sentence per kind; the move still reads as a success. */
  lockedKept?: LockedKept | null
}

/** Composes the copy/move/trash/delete completion toast from catalog keys. */
export function composeTransferCompleteToast(input: TransferCompleteToastInput): string {
  const toast = withLeftBehind(composeOutcome(input), input.appearedDuringMove)
  return withLockedKept(withMetadataLeftBehind(toast, input.metadataLeftBehind), input.lockedKept)
}

/** The toasts a completion raises, for the started and the adopted progress dialog alike. */
export interface CompletionToastPlan {
  /** The completion toast, or `null` when a dialog kept open on its done state says it already. */
  message: string | null
  level: NonErrorToastLevel
  /** What a batch trash had to leave behind, or `null`. */
  refused: string | null
  /** A copy whose files aren't confirmed on the disk, or `null`. Raised whatever the dialog does:
   *  the done state doesn't say it. */
  durability: string | null
}

/**
 * Plans the toasts for a completed operation. A trash the OS refused part of, or a copy whose
 * files aren't confirmed on the disk, takes the level down to `warn`: nothing about either may
 * read as a clean success. `info` for the all-skipped case (nothing moved; a neutral outcome),
 * `success` everywhere else, including mixed. `keptOpen`: the progress dialog stays on its done
 * state, which says the counts, the skips, and what the OS refused, so only a completion toast
 * carrying something more still speaks.
 */
export function planCompletionToasts(
  input: TransferCompleteToastInput & {
    refused?: TrashRefusedItems | null
    durabilityUnconfirmed?: DurabilityUnconfirmed | null
  },
  keptOpen: boolean,
): CompletionToastPlan {
  const refused = composeTrashRefusedToast(input.refused)
  const durability = composeDurabilityUnconfirmedToast(input.durabilityUnconfirmed)
  const allSkipped = input.filesSkipped > 0 && input.filesSkipped === input.filesProcessed
  const level: NonErrorToastLevel = refused !== null || durability !== null ? 'warn' : allSkipped ? 'info' : 'success'
  const quiet = keptOpen && !completionToastCarriesMore(input)
  return {
    message: quiet ? null : composeTransferCompleteToast(input),
    level,
    refused: keptOpen ? null : refused,
    durability,
  }
}

/**
 * Whether the completion toast says more than the counts: what a move left in the source, what
 * the destination couldn't store, or Locked items kept. A progress dialog kept open on its done
 * state says the counts and the skips, never these, so a toast carrying one still has to speak.
 */
function completionToastCarriesMore(
  input: Pick<TransferCompleteToastInput, 'appearedDuringMove' | 'metadataLeftBehind' | 'lockedKept'>,
): boolean {
  const { appearedDuringMove, metadataLeftBehind, lockedKept } = input
  return (
    (appearedDuringMove?.itemCount ?? 0) > 0 ||
    (appearedDuringMove?.changedCount ?? 0) > 0 ||
    (metadataLeftBehind?.classes.length ?? 0) > 0 ||
    (lockedKept?.skipped ?? 0) > 0 ||
    (lockedKept?.systemLocked ?? 0) > 0
  )
}

/** Appends what a move left in place because it was Locked: what the person chose to skip, then
 *  what macOS keeps locked (`crates/cmdr-operations/DETAILS.md` § "Moving Locked items"). */
function withLockedKept(toast: string, kept: LockedKept | null | undefined): string {
  if (!kept) return toast
  const sentences = [toast]
  if (kept.skipped > 0) {
    sentences.push(tString('transfer.lockedSkipped', { count: kept.skipped, countText: formatNumber(kept.skipped) }))
  }
  if (kept.systemLocked > 0) {
    sentences.push(
      tString('transfer.systemLockedKept', { count: kept.systemLocked, countText: formatNumber(kept.systemLocked) }),
    )
  }
  return joinSentences(sentences)
}

/** The catalog name of each metadata class, as it reads inside the note's list. */
const METADATA_CLASS_KEYS: Record<LostMetadataClass, MessageKey> = {
  creationDates: 'transfer.metadataClass.creationDates',
  permissions: 'transfer.metadataClass.permissions',
  lockedAndHidden: 'transfer.metadataClass.lockedAndHidden',
  extendedAttributes: 'transfer.metadataClass.extendedAttributes',
  folderDates: 'transfer.metadataClass.folderDates',
}

/**
 * Appends the note about what the destination couldn't store, when it couldn't store something
 * a source item had (`crates/cmdr-operations/src/transfer/volume/DETAILS.md` §
 * "Copies keep the source's metadata"). Names
 * the classes in the backend's order and never the counts: the log carries those.
 */
function withMetadataLeftBehind(toast: string, left: MetadataLeftBehind | null | undefined): string {
  if (!left || left.classes.length === 0) return toast
  const classes = formatDisjunctionList(left.classes.map((lost) => tString(METADATA_CLASS_KEYS[lost.class])))
  return `${toast} ${tString('transfer.metadataLeftBehind', { classes })}`
}

/**
 * The warning beside a trash that took some items and was refused the rest.
 * `null` when nothing was refused, which is the ordinary ending.
 *
 * It rides in a toast of its own rather than as a clause on the completion
 * sentence: the two carry different weight (one offers Undo, this one is a
 * warning), and the completion sentences carry no terminal punctuation to
 * append to, which no locale would agree on anyway.
 *
 * A batch where EVERY item was refused never reaches here — that one fails
 * outright and the error dialog says so.
 */
export function composeTrashRefusedToast(refused: TrashRefusedItems | null | undefined): string | null {
  if (!refused || refused.itemCount <= 0) return null
  return trashRefusedCountSentence(refused.reason, refused.itemCount)
}

/**
 * The warning beside a copy whose closing flush couldn't confirm its files are on the disk:
 * every file landed, but a power loss or an unplugged drive could still take them. `null` when
 * the flush confirmed everything, which is the ordinary ending. Its own toast, like the trash
 * refusal: the completion sentence stays true, and this one is read on its own.
 */
export function composeDurabilityUnconfirmedToast(
  unconfirmed: DurabilityUnconfirmed | null | undefined,
): string | null {
  if (!unconfirmed) return null
  return unconfirmed.volumeName
    ? tString('transfer.durabilityUnconfirmed', { volumeName: unconfirmed.volumeName })
    : tString('transfer.durabilityUnconfirmedUnnamed')
}

/** Appends what the move left in the source, when it left anything: what appeared there, then what changed. */
function withLeftBehind(toast: string, left: AppearedDuringMove | null | undefined): string {
  if (!left) return toast
  const where = { scope: left.folderCount > 1 ? 'manyFolders' : 'oneFolder', folderName: left.folderName }
  const sentences = [toast]
  if (left.itemCount > 0) {
    sentences.push(
      tString('transfer.appearedDuringMove', {
        ...where,
        countText: formatNumber(left.itemCount),
        count: left.itemCount,
      }),
    )
  }
  if (left.changedCount > 0) {
    sentences.push(
      tString('transfer.changedDuringMove', {
        ...where,
        countText: formatNumber(left.changedCount),
        count: left.changedCount,
      }),
    )
  }
  return sentences.join(' ')
}

/** The outcome sentence itself: what went, and what was skipped on the way. */
function composeOutcome(input: TransferCompleteToastInput): string {
  const { operationType, filesProcessed, filesSkipped, fileCount, folderCount, topLevelSkipped } = input

  if (operationType === 'trash') {
    return tString('transfer.trash', { countText: formatNumber(filesProcessed), count: filesProcessed })
  }
  if (operationType === 'delete') {
    return tString('transfer.delete', { countText: formatNumber(filesProcessed), count: filesProcessed })
  }
  // Compress packs sources into ONE new zip and never skips, so it gets its own
  // dedicated wording rather than the copy/move "Copied/Moved" verb select.
  if (operationType === 'compress') {
    if (fileCount !== undefined && folderCount !== undefined) {
      const phrase = describeCounts(fileCount, folderCount)
      if (phrase !== null) return tString('transfer.compress.split', { phrase })
    }
    return tString('transfer.compress.fileOnly', { countText: formatNumber(filesProcessed), count: filesProcessed })
  }

  const verb = verbParam(operationType)

  // Selection split: report the top-level items the user picked, so each count
  // is reduced by the skips of ITS OWN kind.
  //
  // ❗ `filesSkipped` cannot do that job: it counts LEAVES, nested ones
  // included. Subtracting it from `fileCount` holds only while every skip is a
  // top-level file; once a folder's child can be skipped, a Skip-All over
  // `readme.txt` + `docs/` reads "Copied 1 folder, skipped 3 files" — the two
  // skipped children ate the top-level file that did copy. So an engine that
  // knows reports `topLevelSkipped`, and each count is reduced by its own kind.
  //
  // The fallback is that same old heuristic, kept deliberately: an engine that
  // doesn't report (cross-volume, in-archive) is one whose skips ARE top-level
  // files today, so it keeps the wording it has rather than silently switching
  // to "nothing was skipped".
  if (fileCount !== undefined && folderCount !== undefined) {
    const movedFiles = fileCount - (topLevelSkipped?.files ?? filesSkipped)
    const movedFolders = folderCount - (topLevelSkipped?.folders ?? 0)
    const phrase = describeCounts(movedFiles, movedFolders)

    if (phrase === null) {
      // Nothing landed at all: every selected item was skipped. The flat
      // wording is the only one that can say that without naming a count of
      // zero, so it takes over.
      return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
    }

    if (filesSkipped === 0) {
      return tString('transfer.split.clean', { verb, phrase })
    }
    return tString('transfer.split.skipped', {
      verb,
      phrase,
      skippedText: formatNumber(filesSkipped),
      skipped: filesSkipped,
    })
  }

  // Fallback (clipboard paste): no selection split available, report file counts.
  return composeFileOnlyToast(operationType, filesProcessed, filesSkipped)
}

/** `'copy'` stays `'copy'`; everything else maps to the `other` (move) ICU branch. */
function verbParam(operationType: TransferOperationType): string {
  return operationType === 'copy' ? 'copy' : 'move'
}

/**
 * Builds the "N files and M folders" phrase via the `kind`-discriminated ICU
 * message, omitting any zero part. Returns `null` when both counts are zero.
 */
function describeCounts(files: number, folders: number): string | null {
  if (files <= 0 && folders <= 0) return null
  const kind = files > 0 && folders > 0 ? 'both' : folders > 0 ? 'foldersOnly' : 'filesOnly'
  return tString('transfer.movedPhrase', {
    kind,
    filesText: formatNumber(files),
    files,
    foldersText: formatNumber(folders),
    folders,
  })
}

/** The file-count-only wording (clipboard paste + the all-skipped collapse). */
function composeFileOnlyToast(
  operationType: TransferOperationType,
  filesProcessed: number,
  filesSkipped: number,
): string {
  const transferred = filesProcessed - filesSkipped
  const verb = verbParam(operationType)

  // All transferred, none skipped.
  if (filesSkipped === 0) {
    return tString('transfer.fileOnly.allDone', {
      verb,
      transferredText: formatNumber(transferred),
      transferred,
    })
  }

  // Nothing transferred, all skipped.
  if (transferred === 0) {
    if (filesSkipped === 1) {
      return tString('transfer.fileOnly.allSkippedSingle', { verb })
    }
    return tString('transfer.fileOnly.allSkippedMany', {
      verb,
      skippedText: formatNumber(filesSkipped),
      skipped: filesSkipped,
    })
  }

  // Mixed: some transferred, some skipped.
  if (operationType === 'copy') {
    return tString('transfer.fileOnly.mixedCopy', {
      transferredText: formatNumber(transferred),
      skippedText: formatNumber(filesSkipped),
      processedText: formatNumber(filesProcessed),
      processed: filesProcessed,
    })
  }
  return tString('transfer.fileOnly.mixedMove', {
    transferredText: formatNumber(transferred),
    skippedText: formatNumber(filesSkipped),
    skipped: filesSkipped,
  })
}
