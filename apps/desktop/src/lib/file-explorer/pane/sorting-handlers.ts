import type { SortColumn, SortOrder } from '../types'
import { defaultSortOrders } from '../types'
import type FilePane from './FilePane.svelte'

/** Determines the new sort order when clicking a column header. */
export function getNewSortOrder(newColumn: SortColumn, currentColumn: SortColumn, currentOrder: SortOrder): SortOrder {
    if (newColumn === currentColumn) {
        return currentOrder === 'ascending' ? 'descending' : 'ascending'
    }
    return defaultSortOrders[newColumn]
}

/** Converts frontend indices (which include ".." at index 0) to backend indices. */
function toBackendIndices(frontendIndices: number[], hasParent: boolean): number[] {
    if (!hasParent) return frontendIndices
    return frontendIndices.filter((i) => i > 0).map((i) => i - 1)
}

/** Converts backend indices to frontend indices (adding 1 for ".." entry). */
function toFrontendIndices(backendIndices: number[], hasParent: boolean): number[] {
    if (!hasParent) return backendIndices
    return backendIndices.map((i) => i + 1)
}

/** Applies re-sort results (new cursor + selection positions) to a pane, adjusting for ".." offset. */
export function applySortResult(
    paneRef: FilePane | undefined,
    result: { newCursorIndex?: number; newSelectedIndices?: number[] },
    hasParent: boolean,
) {
    if (result.newCursorIndex !== undefined) {
        const frontendIndex = hasParent ? result.newCursorIndex + 1 : result.newCursorIndex
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.setCursorIndex?.(frontendIndex)
    }
    if (result.newSelectedIndices !== undefined) {
        const frontendIndices = toFrontendIndices(result.newSelectedIndices, hasParent)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.setSelectedIndices?.(frontendIndices)
    }
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    paneRef?.refreshView?.()
}

/** Collects current sort-relevant state from a pane ref, with selection indices converted to backend space. */
export function collectSortState(paneRef: FilePane | undefined): {
    cursorFilename: string | undefined
    backendSelectedIndices: number[] | undefined
    allSelected: boolean | undefined
    hasParent: boolean
} {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorFilename = paneRef?.getFilenameUnderCursor?.() as string | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const frontendIndices = paneRef?.getSelectedIndices?.() as number[] | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const allSelected = paneRef?.isAllSelected?.() as boolean | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const hasParent = (paneRef?.hasParentEntry?.() as boolean | undefined) ?? false

    const backendSelectedIndices = frontendIndices ? toBackendIndices(frontendIndices, hasParent) : undefined
    return { cursorFilename, backendSelectedIndices, allSelected, hasParent }
}
