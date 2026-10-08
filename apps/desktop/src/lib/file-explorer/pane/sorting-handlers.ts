import type { SortColumn, SortOrder } from '../types'
import { defaultSortOrders } from '../types'
import { toBackendIndices } from '$lib/file-operations/transfer/transfer-dialog-utils'
export { toBackendIndices }
import type { FilePaneAPI } from './types'

/** Determines the new sort order when clicking a column header. */
export function getNewSortOrder(newColumn: SortColumn, currentColumn: SortColumn, currentOrder: SortOrder): SortOrder {
  if (newColumn === currentColumn) {
    return currentOrder === 'ascending' ? 'descending' : 'ascending'
  }
  return defaultSortOrders[newColumn]
}

/** Converts backend indices to frontend indices (adding 1 for ".." entry). */
export function toFrontendIndices(backendIndices: number[], hasParent: boolean): number[] {
  if (!hasParent) return backendIndices
  return backendIndices.map((i) => i + 1)
}

/** Collects current sort-relevant state from a pane ref, with selection indices converted to backend space. */
export function collectSortState(paneRef: FilePaneAPI | undefined): {
  cursorFilename: string | undefined
  backendSelectedIndices: number[] | undefined
  allSelected: boolean | undefined
  hasParent: boolean
} {
  const cursorFilename = paneRef?.getFilenameUnderCursor()
  const frontendIndices = paneRef?.getSelectedIndices()
  const allSelected = paneRef?.isAllSelected()
  const hasParent = paneRef?.hasParentEntry() ?? false

  const backendSelectedIndices = frontendIndices ? toBackendIndices(frontendIndices, hasParent) : undefined
  return { cursorFilename, backendSelectedIndices, allSelected, hasParent }
}
