/** Resolved drop target: either a specific folder row or a pane-level target. */
export type DropTarget =
    | { type: 'folder'; path: string; element: HTMLElement; paneId: 'left' | 'right' }
    | { type: 'pane'; paneId: 'left' | 'right' }

/**
 * Resolves the drop target at (x, y) using the browser's hit-testing.
 * If the cursor is over a directory row with `data-drop-target-path`, returns a folder target.
 * Otherwise falls back to pane-level targeting.
 * Returns null if the cursor is outside both panes.
 */
export function resolveDropTarget(
    x: number,
    y: number,
    leftPaneEl: HTMLElement | undefined,
    rightPaneEl: HTMLElement | undefined,
): DropTarget | null {
    const el = document.elementFromPoint(x, y)
    if (!el) return null

    // Determine which pane contains the element
    let paneId: 'left' | 'right' | null = null
    if (leftPaneEl?.contains(el)) paneId = 'left'
    else if (rightPaneEl?.contains(el)) paneId = 'right'
    if (!paneId) return null

    // Walk up to the closest .file-entry and check for the drop target attribute
    const fileEntry = el.closest('.file-entry')
    if (fileEntry) {
        const dropPath = fileEntry.getAttribute('data-drop-target-path')
        if (dropPath) {
            return { type: 'folder', path: dropPath, element: fileEntry as HTMLElement, paneId }
        }
    }

    return { type: 'pane', paneId }
}
