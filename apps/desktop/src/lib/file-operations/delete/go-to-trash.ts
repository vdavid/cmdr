/**
 * "Go to trash": point a pane at the trash, either in general or at the exact
 * items an operation just put there.
 *
 * macOS keeps one trash per VOLUME, so there is no single directory to open. Both
 * entries below resolve the right one through `getTrashDir`, which asks Cocoa the
 * same question the trash move asked (`write_operations/delete/trash.rs`).
 *
 * The two entries differ in what they know:
 *
 * - `goToTrash` knows only where the user is standing, so it opens the trash of
 *   the focused pane's volume. That's the command-palette entry.
 * - `goToTrashedItems` knows the operation, so it reads the recorded in-trash
 *   location out of the journal and lands the cursor on the item itself. That's
 *   the trash toast's button, and it's strictly better when it's available: the
 *   journal holds the OS's own answer, de-duplicated name and all.
 */

import { getTrashDir, getOperationLogDetail } from '$lib/tauri-commands'
import { whenOperationSettled } from '../settled-operations'
import {
  resolveLocationOrToast,
  navigateToDirInBestPane,
  revealFileInBestPane,
  type PaneRevealAPI,
} from '$lib/file-explorer/navigation/navigate-and-select'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileOperations')

/**
 * How many item rows to read when looking for the trashed item to land on. Only
 * the first `rollbackUnit` row is used, but interior `searchOnly` rows share the
 * page, so this reads enough to get past a handful of them.
 */
const ITEM_PAGE_SIZE = 50

/** Dedup id, so repeating the action can't stack copies of the same notice. */
const NO_TRASH_TOAST_ID = 'go-to-trash-unavailable'

/**
 * Open the trash of the focused pane's volume.
 *
 * The pane's own directory is what picks the volume: standing in an external
 * drive and asking for the trash should open THAT drive's trash, not the boot
 * volume's. A volume with no trash says so rather than navigating somewhere
 * arbitrary.
 */
export async function goToTrash(explorer: PaneRevealAPI | undefined): Promise<void> {
  if (!explorer) {
    log.debug('goToTrash: no explorer; skipping (HMR or pre-mount)')
    return
  }
  const { path } = explorer.getPaneLocation(explorer.getFocusedPane())
  await openTrashDirFor(explorer, path)
}

/**
 * Show what `operationId` moved to the trash: navigate to the trash directory the
 * items actually landed in and put the cursor on the first of them.
 *
 * Falls back to `fromPath`'s volume trash when the journal has no in-trash
 * location to offer (Linux records none, and a row can be missing after a crash).
 * Getting the user to the right trash is the point; the cursor is a bonus.
 */
export async function goToTrashedItems(
  explorer: PaneRevealAPI | undefined,
  operationId: string,
  fromPath: string,
): Promise<void> {
  if (!explorer) {
    log.debug('goToTrashedItems: no explorer; skipping (HMR or pre-mount)')
    return
  }

  const landed = await firstTrashedItemPath(operationId)
  if (!landed) {
    await openTrashDirFor(explorer, fromPath)
    return
  }

  const location = await resolveLocationOrToast(landed.dir)
  if (!location) return

  // ⚠️ `moveCursor` THROWS on a name the visible listing doesn't hold, and a
  // trashed DOTFILE is exactly that when "show hidden files" is off. The
  // navigation happens first, so by then the user is already looking at the right
  // trash: the cursor is the bonus, and losing it must not surface as a fault.
  try {
    await revealFileInBestPane(explorer, location, landed.name)
  } catch (error) {
    log.debug('Landed in the trash but not on {name} (hidden, or gone since): {error}', {
      name: landed.name,
      error: String(error),
    })
  }
}

/** Resolve `path`'s volume trash and open it, or say the volume hasn't got one. */
async function openTrashDirFor(explorer: PaneRevealAPI, path: string): Promise<void> {
  const trashDir = await getTrashDir(path)
  if (!trashDir) {
    log.info('No trash directory for {path}; nowhere to go', { path })
    addToast(tString('fileOperations.trash.noTrashHere'), { level: 'info', id: NO_TRASH_TOAST_ID })
    return
  }
  const location = await resolveLocationOrToast(trashDir)
  if (!location) return
  await navigateToDirInBestPane(explorer, location)
}

/**
 * Where the operation's first top-level item ended up, split into the directory to
 * open and the name to land on.
 *
 * Waits out the settle first: the journal batches item rows in memory and flushes
 * the tail in the finalize barrier, so reading at completion time hands back an
 * empty page (`settled-operations.ts`). Only `rollbackUnit` rows are the user's
 * own items; the `searchOnly` rows are interior leaves nobody asked to see.
 */
async function firstTrashedItemPath(operationId: string): Promise<{ dir: string; name: string } | null> {
  const settled = await whenOperationSettled(operationId)
  if (!settled) {
    log.warn('Operation {operationId} never settled; reading its items would come back empty', { operationId })
    return null
  }

  const detail = await getOperationLogDetail(operationId, ITEM_PAGE_SIZE, 0)
  const landed = detail?.items.find((item) => item.rowRole === 'rollbackUnit' && item.destPath !== null)
  if (!landed?.destPath) return null

  const lastSlash = landed.destPath.lastIndexOf('/')
  if (lastSlash <= 0) return null
  return { dir: landed.destPath.slice(0, lastSlash), name: landed.destPath.slice(lastSlash + 1) }
}
