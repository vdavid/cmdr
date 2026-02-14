/**
 * Click-to-rename: click name area of entry already under cursor,
 * wait ~800 ms without moving >10 px or double-clicking.
 *
 * Matches Total Commander behavior: click on an already-selected entry,
 * release the mouse, and if you don't move away for 800 ms, rename
 * activates. The timer survives mouseup — it's only cancelled by
 * mouse movement, double-click, scrolling, or keyboard actions.
 */

const RENAME_DELAY_MS = 800
const MOVE_THRESHOLD_PX = 10

interface ClickToRenameState {
    timer: ReturnType<typeof setTimeout> | null
    startX: number
    startY: number
    moveHandler: ((e: MouseEvent) => void) | null
}

let current: ClickToRenameState | null = null

/** Cancels any pending click-to-rename timer. Safe to call any time. */
export function cancelClickToRename(): void {
    if (!current) return
    if (current.timer !== null) {
        clearTimeout(current.timer)
    }
    if (current.moveHandler) {
        document.removeEventListener('mousemove', current.moveHandler)
    }
    current = null
}

/**
 * Starts click-to-rename tracking. The callback fires after 800 ms
 * if the mouse doesn't move >10 px from the click point.
 *
 * Call this on mousedown on a name cell when the entry is already
 * under the cursor (not on first click to select).
 */
export function startClickToRename(event: MouseEvent, onActivate: () => void): void {
    // Cancel any existing timer
    cancelClickToRename()

    const startX = event.clientX
    const startY = event.clientY

    const state: ClickToRenameState = {
        timer: null,
        startX,
        startY,
        moveHandler: null,
    }

    state.moveHandler = (e: MouseEvent) => {
        const dx = e.clientX - startX
        const dy = e.clientY - startY
        if (Math.sqrt(dx * dx + dy * dy) > MOVE_THRESHOLD_PX) {
            cancelClickToRename()
        }
    }

    state.timer = setTimeout(() => {
        // Timer fired — activate rename
        cancelClickToRename()
        onActivate()
    }, RENAME_DELAY_MS)

    document.addEventListener('mousemove', state.moveHandler)

    current = state
}

/** @public */
export function isClickToRenamePending(): boolean {
    return current !== null && current.timer !== null
}
