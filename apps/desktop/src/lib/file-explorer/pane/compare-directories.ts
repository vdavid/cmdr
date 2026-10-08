/**
 * Compare directories (⇧F2), Total Commander's "Compare directories": mark, in
 * each pane, the files the other pane lacks plus the copies that are newer (or,
 * per mode, differ in size). Equal files and the older copy end up unmarked, so
 * F5 afterwards brings the other side up to date.
 *
 * The comparison is the backend's (`src-tauri/src/file_system/listing/compare.rs`),
 * read off both cached listings at once; this replaces each pane's selection with
 * the answer (adding the `..` offset) and says what happened in a toast.
 *
 * Row numbers only fit a pane showing the state they were read from: a file that
 * appeared since would shift them onto other files. So the answer is applied only
 * when it's `settled` and each pane's last applied diff sequence is the one it was
 * read at; otherwise the diffs are still on their way, and it asks again.
 */

import {
  compareDirectories as compareDirectoriesIpc,
  type CompareDirectoriesMode,
  type CompareDirectoriesResult,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

/** How many times to ask while the folders keep changing under the comparison. */
export const COMPARE_ATTEMPTS = 5
/** Two of the backend's 50 ms diff flush windows: long enough for a pending diff to land. */
export const COMPARE_RETRY_DELAY_MS = 100

export interface CompareDirectoriesDeps {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getShowHiddenFiles: () => boolean
}

/** What the comparison was asked about: the two panes as they were when it started. */
interface Asked {
  left: FilePaneAPI
  right: FilePaneAPI
  leftListingId: string
  rightListingId: string
  includeHidden: boolean
  leftGeneration: number
  rightGeneration: number
}

const latestRequests = new WeakMap<FilePaneAPI, object>()

const NO_DIFFERENCES: Record<CompareDirectoriesMode, () => string> = {
  newerAndMissing: () => tString('fileExplorer.compareDirectories.noDifferences.newerAndMissing'),
  missing: () => tString('fileExplorer.compareDirectories.noDifferences.missing'),
  sizeAndMissing: () => tString('fileExplorer.compareDirectories.noDifferences.sizeAndMissing'),
}

export async function compareDirectories(deps: CompareDirectoriesDeps, mode: CompareDirectoriesMode): Promise<void> {
  const asked = askedAbout(deps)
  // A network hub or a search-results snapshot has no folder listing to compare.
  if (!asked) {
    addToast(tString('fileExplorer.compareDirectories.needsTwoFolders'), { level: 'info' })
    return
  }
  const request = {}
  latestRequests.set(asked.left, request)
  latestRequests.set(asked.right, request)
  const superseded = () => latestRequests.get(asked.left) !== request || latestRequests.get(asked.right) !== request
  for (let attempt = 1; attempt <= COMPARE_ATTEMPTS; attempt++) {
    if (superseded() || movedOn(deps, asked)) return
    const result = await requestComparison(asked, mode)
    if (!result || superseded() || movedOn(deps, asked)) return
    if (fitsPanes(asked, result)) {
      markAndReport(asked, result, mode)
      return
    }
    await new Promise((resolve) => setTimeout(resolve, COMPARE_RETRY_DELAY_MS))
  }
  log.info('compare directories gave up: the folders kept changing over {attempts}', {
    attempts: `${String(COMPARE_ATTEMPTS)} ${pluralize(COMPARE_ATTEMPTS, 'attempt')}`,
  })
  addToast(tString('fileExplorer.compareDirectories.keptChanging'), { level: 'warn' })
}

function askedAbout(deps: CompareDirectoriesDeps): Asked | null {
  const left = deps.getPaneRef('left')
  const right = deps.getPaneRef('right')
  const leftListingId = left?.getListingId() ?? ''
  const rightListingId = right?.getListingId() ?? ''
  if (!left || !right || leftListingId === '' || rightListingId === '') return null
  return {
    left,
    right,
    leftListingId,
    rightListingId,
    includeHidden: deps.getShowHiddenFiles(),
    leftGeneration: left.getViewGeneration(),
    rightGeneration: right.getViewGeneration(),
  }
}

/** The backend's answer, or `null` when there's none to apply (it said why in the log and maybe a toast). */
async function requestComparison(asked: Asked, mode: CompareDirectoriesMode): Promise<CompareDirectoriesResult | null> {
  try {
    return await compareDirectoriesIpc(
      asked.leftListingId,
      asked.includeHidden,
      asked.rightListingId,
      asked.includeHidden,
      mode,
    )
  } catch (e) {
    const reason = (e as { type?: string }).type
    log.warn("compare directories couldn't run: {reason}", { reason: reason ?? String(e) })
    // A listing that went away mid-compare (the user navigated) has nothing to mark;
    // a comparison that ran out of time or broke tells the user.
    if (reason === 'timedOut' || reason === 'internal') {
      addToast(tString('fileExplorer.compareDirectories.couldNotFinish'), { level: 'warn' })
    }
    return null
  }
}

/** The panes moved on, or hidden files were toggled: the answer is for folders no longer shown. */
function movedOn(deps: CompareDirectoriesDeps, asked: Asked): boolean {
  return (
    deps.getPaneRef('left') !== asked.left ||
    deps.getPaneRef('right') !== asked.right ||
    asked.left.getViewGeneration() !== asked.leftGeneration ||
    asked.right.getViewGeneration() !== asked.rightGeneration ||
    asked.left.getListingId() !== asked.leftListingId ||
    asked.right.getListingId() !== asked.rightListingId ||
    deps.getShowHiddenFiles() !== asked.includeHidden
  )
}

/** Each pane shows exactly the state the rows were read from. */
function fitsPanes(asked: Asked, result: CompareDirectoriesResult): boolean {
  return (
    result.settled &&
    asked.left.isRowStateReady() &&
    asked.right.isRowStateReady() &&
    asked.left.getLastSequence() === result.leftSequence &&
    asked.right.getLastSequence() === result.rightSequence
  )
}

function markAndReport(asked: Asked, result: CompareDirectoriesResult, mode: CompareDirectoriesMode): void {
  const withParentOffset = (pane: FilePaneAPI, rows: number[]): number[] =>
    pane.hasParentEntry() ? rows.map((row) => row + 1) : rows
  asked.left.setSelectedIndices(withParentOffset(asked.left, result.left))
  asked.right.setSelectedIndices(withParentOffset(asked.right, result.right))

  const leftCount = result.left.length
  const rightCount = result.right.length
  if (leftCount === 0 && rightCount === 0) {
    addToast(NO_DIFFERENCES[mode](), { level: 'info' })
    return
  }
  addToast(
    tString('fileExplorer.compareDirectories.marked', {
      leftCount,
      leftCountText: formatNumber(leftCount),
      rightCount,
      rightCountText: formatNumber(rightCount),
    }),
    { level: 'info' },
  )
}
