/**
 * Naming the copy, the moment a duplicate makes it.
 *
 * A transfer that copies ONE item into the folder it already lives in ends by
 * landing the cursor on the new item and opening the inline rename editor with
 * the stem selected: naming the copy costs one keystroke sequence, and Esc keeps
 * the generated ` (N)` name.
 *
 * Two things here are deliberate and easy to undo by accident:
 *
 * - **The trigger says yes, never the shape of the copy.** `duplicateFollowUp`
 *   is a REQUIRED field on the transfer config, so a gesture that dispatches the
 *   same backend copy has to state its answer rather than inherit one. Which
 *   gestures opt in, and why the others must not:
 *   `$lib/file-operations/transfer/DETAILS.md` § "Only paste and F5 end a duplicate in the rename editor".
 * - **The new name is READ, never recomputed.** No terminal event carries it, so
 *   it comes out of the operation journal. A frontend reimplementation of the
 *   backend's ` (N)` picker would rot the moment the sequence rule changes. An
 *   absent row means no editor: never an error, never a retry loop.
 * - **The journal is read on `write-settled`, ❌ never on `write-complete`.** The
 *   journal batches item rows in memory and flushes them inside its finalize
 *   barrier, which runs after the handler emitted the terminal event, so a
 *   single-item duplicate has NO readable row at complete time. Reading there
 *   returns an empty page and the editor silently never opens.
 *   `$lib/file-operations/settled-operations.ts`.
 */

import { findFileIndex, getOperationLogDetail, onDirectoryDiff, type OperationItemView } from '$lib/tauri-commands'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import { whenOperationSettled } from '$lib/file-operations/settled-operations'
import { getAppLogger } from '$lib/logging/logger'
import type { TransferOperationType } from '../types'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

/**
 * What a transfer does once it has duplicated exactly one item in the folder
 * that item already lived in.
 *
 * - `openRenameEditor`: land the cursor on the new item and open the inline
 *   rename editor on it.
 * - `nothing`: the copy is made and that's the whole gesture.
 */
export type DuplicateFollowUp = 'openRenameEditor' | 'nothing'

/** What the decision reads off a settled transfer's birth context. */
export interface DuplicateRenameContext {
  duplicateFollowUp: DuplicateFollowUp
  operationType: TransferOperationType
  sourcePaths: string[]
  sourceFolderPath: string
  destinationPath?: string
}

/** Everything the follow-up needs once its operation has settled. */
export interface DuplicateRenameRequest {
  /** The settled operation's birth context, or `null` when the slot is empty. */
  context: DuplicateRenameContext | null
  /** The operation whose journal holds the resolved name. */
  operationId: string | null
  /** The pane the user is looking at. */
  paneRef: FilePaneAPI | undefined
  showHiddenFiles: boolean
}

/** A directory path with its trailing slashes dropped, or `null` when there's none.
 *  Root stays `/`, which is the one path that IS a trailing slash. */
function normalizeDir(path: string | undefined): string | null {
  if (path === undefined || path === '') return null
  const trimmed = path.replace(/\/+$/, '')
  return trimmed === '' ? '/' : trimmed
}

/**
 * The folder a settled transfer should open the rename editor in, or `null` when
 * this transfer isn't a duplicate anyone asked to name.
 *
 * "Exactly one item" is asked of the SOURCES, not of the counts the completion
 * event carries: those count leaf files, so a duplicated folder of 12 photos
 * would read as 12 items when it's one thing to name.
 */
export function duplicateRenameDestination(context: DuplicateRenameContext | null): string | null {
  if (!context) return null
  if (context.duplicateFollowUp !== 'openRenameEditor') return null
  if (context.operationType !== 'copy') return null
  if (context.sourcePaths.length !== 1) return null
  const destination = normalizeDir(context.destinationPath)
  if (destination === null || destination !== normalizeDir(context.sourceFolderPath)) return null
  return destination
}

/**
 * The name of the top-level entry a journal row landed under `destination`, or
 * `null` when the row can't answer.
 *
 * A duplicated FILE journals one row that is the new file itself. A duplicated
 * FOLDER journals its leaves (and its created dirs after them), so the first
 * path segment below the destination is the answer in both cases: `docs (1)`
 * out of `…/docs (1)/sub/b.txt` just as `photo (1).jpg` out of
 * `…/photo (1).jpg`.
 */
export function duplicatedEntryName(destination: string, item: OperationItemView | undefined): string | null {
  if (!item || item.outcome !== 'done') return null
  const destPath = item.destPath
  if (destPath === null || destPath === '') return null
  const prefix = destination === '/' ? '/' : `${destination}/`
  if (!destPath.startsWith(prefix)) return null
  const rest = destPath.slice(prefix.length)
  const name = rest.split('/')[0]
  return name === '' ? null : name
}

/**
 * Runs the follow-up: reads the resolved name out of the journal, lands the
 * cursor on it, and opens the editor.
 *
 * Every way this can't be done is a silent return. The user has their duplicate
 * either way, and the one thing that would be worse than no editor is an editor
 * on the wrong file, which `expectedName` also refuses independently.
 */
export async function openRenameOnDuplicate(request: DuplicateRenameRequest): Promise<void> {
  const destination = duplicateRenameDestination(request.context)
  if (destination === null || request.operationId === null) return

  // The editor opens where the user is looking, so the focused pane has to be
  // showing the folder the copy landed in. An F5 aimed at the other pane, or a
  // pane that navigated away mid-transfer, simply gets no editor.
  const paneRef = request.paneRef
  if (!paneRef || normalizeDir(paneRef.getCurrentPath()) !== destination) return
  const listingId = paneRef.getListingId()
  if (!listingId) return

  // The operation's rows only become readable at `write-settled`, so wait for it
  // before asking. A settle that never arrives means no editor, not a retry.
  if (!(await whenOperationSettled(request.operationId))) {
    log.warn('op={operationId} never settled, so its duplicate kept the generated name', {
      operationId: request.operationId,
    })
    return
  }

  let items: OperationItemView[]
  try {
    const detail = await getOperationLogDetail(request.operationId, 1, 0)
    items = detail?.items ?? []
  } catch (error) {
    // The journal is a convenience here, not a contract: it can be closed, pruned,
    // or simply behind. Nothing is wrong with the duplicate itself.
    log.warn('The duplicate for op={operationId} kept its generated name: {error}', {
      operationId: request.operationId,
      error,
    })
    return
  }

  const name = duplicatedEntryName(destination, items.length > 0 ? items[0] : undefined)
  if (name === null) return

  await moveCursorToNewFolder(
    listingId,
    name,
    paneRef,
    paneRef.hasParentEntry(),
    request.showHiddenFiles,
    onDirectoryDiff,
    (args) => findFileIndex(args.listingId, args.filename, args.showHiddenFiles),
  )

  // `expectedName` refuses to activate on anything but the new item, and gives up
  // silently if the row never lands. `suppressExtensionWarning` keeps an edit of
  // an auto-numbered stem from raising the extension-change confirm.
  paneRef.startRename({ suppressExtensionWarning: true, expectedName: name })
}
