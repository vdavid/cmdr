import type { SortColumn, SortOrder } from './types'
import { defaultSortOrders } from './types'
import type FilePane from './FilePane.svelte'

/** Determines the new sort order when clicking a column header. */
export function getNewSortOrder(newColumn: SortColumn, currentColumn: SortColumn, currentOrder: SortOrder): SortOrder {
    if (newColumn === currentColumn) {
        return currentOrder === 'ascending' ? 'descending' : 'ascending'
    }
    return defaultSortOrders[newColumn]
}

/** Applies re-sort results (new cursor + selection positions) to a pane. */
export function applySortResult(
    paneRef: FilePane | undefined,
    result: { newCursorIndex?: number; newSelectedIndices?: number[] },
) {
    if (result.newCursorIndex !== undefined) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.setCursorIndex?.(result.newCursorIndex)
    }
    if (result.newSelectedIndices !== undefined) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.setSelectedIndices?.(result.newSelectedIndices)
    }
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    paneRef?.refreshView?.()
}

/** Collects current sort-relevant state from a pane ref (cursor filename, selection, allSelected). */
export function collectSortState(paneRef: FilePane | undefined): {
    cursorFilename: string | undefined
    selectedIndices: number[] | undefined
    allSelected: boolean | undefined
} {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorFilename = paneRef?.getFilenameUnderCursor?.() as string | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const selectedIndices = paneRef?.getSelectedIndices?.() as number[] | undefined
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const allSelected = paneRef?.isAllSelected?.() as boolean | undefined
    return { cursorFilename, selectedIndices, allSelected }
}
