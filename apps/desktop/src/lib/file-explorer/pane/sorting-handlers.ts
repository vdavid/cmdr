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

/** Applies re-sort results (new cursor + selection positions) to a pane, adjusting for ".." offset. */
export function applySortResult(
    paneRef: FilePaneAPI | undefined,
    result: { newCursorIndex?: number; newSelectedIndices?: number[] },
    hasParent: boolean,
) {
    if (result.newCursorIndex !== undefined) {
        const frontendIndex = hasParent ? result.newCursorIndex + 1 : result.newCursorIndex
        void paneRef?.setCursorIndex(frontendIndex)
    }
    if (result.newSelectedIndices !== undefined) {
        const frontendIndices = toFrontendIndices(result.newSelectedIndices, hasParent)
        paneRef?.setSelectedIndices(frontendIndices)
    }
    paneRef?.refreshView()
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
