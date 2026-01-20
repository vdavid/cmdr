// Drag and drop utilities for file items
// Handles both single-file drag and selection-based multi-file drag

import { startDrag } from '@crabnebula/tauri-plugin-drag'
import { tempDir, join } from '@tauri-apps/api/path'
import { getCachedIcon } from './icon-cache'
import { startSelectionDrag } from './tauri-commands'

/** Minimum distance (in pixels) to trigger drag */
export const DRAG_THRESHOLD = 5

/** Name of the temp icon file */
const TEMP_ICON_FILENAME = 'drag-icon.png'

/** Context for a single file drag (no prior selection) */
interface SingleFileDragContext {
    type: 'single'
    path: string
    iconId: string
    index: number
}

/** Context for a selection-based drag */
interface SelectionDragContext {
    type: 'selection'
    listingId: string
    indices: number[]
    includeHidden: boolean
    hasParent: boolean
    /** Icon ID to use for the drag preview (first selected file) */
    iconId: string
}

/** Callbacks for drag lifecycle events */
interface DragCallbacks {
    /** Called when drag threshold is crossed (for single-file case, to trigger selection) */
    onDragStart?: () => void
    /** Called when drag is cancelled (ESC key or mouseup before threshold) */
    onDragCancel?: () => void
}

/** Global state for active drag operation */
let activeDrag: {
    startX: number
    startY: number
    context: SingleFileDragContext | SelectionDragContext
    callbacks: DragCallbacks
    cleanup: () => void
} | null = null

/** Decodes a base64 data URL and writes it to a temp file, returning the file path */
async function writeIconToTemp(dataUrl: string): Promise<string> {
    // Get temp directory and build path
    const tempPath = await tempDir()
    const iconPath = await join(tempPath, TEMP_ICON_FILENAME)

    // Extract base64 data from data URL (format: data:image/png;base64,...)
    const base64Match = dataUrl.match(/^data:image\/\w+;base64,(.+)$/)
    if (!base64Match) {
        throw new Error('Invalid data URL format')
    }
    const base64Data = base64Match[1]

    // Convert base64 to binary
    const binaryString = atob(base64Data)
    const bytes = new Uint8Array(binaryString.length)
    for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i)
    }

    // Write to file using the Tauri fs API
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    await writeFile(iconPath, bytes)

    return iconPath
}

/** Cleans up the temp icon file */
async function cleanupTempIcon(): Promise<void> {
    try {
        const tempPath = await tempDir()
        const iconPath = await join(tempPath, TEMP_ICON_FILENAME)
        const { remove } = await import('@tauri-apps/plugin-fs')
        await remove(iconPath)
    } catch {
        // Ignore cleanup errors (file may not exist)
    }
}

/**
 * Starts tracking a potential drag operation with selection awareness.
 *
 * For single-file drags (no prior selection), the file is selected only when the
 * drag threshold is crossed. For selection drags, all selected files are dragged.
 *
 * @param event - The mousedown event
 * @param context - Either a single file or a selection to drag
 * @param callbacks - Optional callbacks for drag lifecycle events
 */
export function startSelectionDragTracking(
    event: MouseEvent,
    context: SingleFileDragContext | SelectionDragContext,
    callbacks: DragCallbacks = {},
): void {
    // Cancel any existing drag
    cancelDragTracking()

    const handleMouseMove = (moveEvent: MouseEvent) => {
        if (!activeDrag) return

        const dx = moveEvent.clientX - activeDrag.startX
        const dy = moveEvent.clientY - activeDrag.startY
        const distance = Math.sqrt(dx * dx + dy * dy)

        if (distance >= DRAG_THRESHOLD) {
            // Threshold crossed - trigger the drag
            const ctx = activeDrag.context
            const cbs = activeDrag.callbacks

            // For single-file drag, call onDragStart to select the file first
            if (ctx.type === 'single') {
                cbs.onDragStart?.()
            }

            // Alt/Option key = copy mode, otherwise move mode (matches Finder behavior)
            const mode = moveEvent.altKey ? 'copy' : 'move'

            if (ctx.type === 'single') {
                void performSingleFileDrag(ctx.path, ctx.iconId, mode)
            } else {
                void performSelectionDrag(ctx, mode)
            }

            cancelDragTracking()
        }
    }

    const handleMouseUp = () => {
        // Mouse released before threshold - cancel
        activeDrag?.callbacks.onDragCancel?.()
        cancelDragTracking()
    }

    const handleKeyDown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
            // ESC pressed - cancel drag
            activeDrag?.callbacks.onDragCancel?.()
            cancelDragTracking()
        }
    }

    const cleanup = () => {
        document.removeEventListener('mousemove', handleMouseMove)
        document.removeEventListener('mouseup', handleMouseUp)
        document.removeEventListener('keydown', handleKeyDown)
    }

    activeDrag = {
        startX: event.clientX,
        startY: event.clientY,
        context,
        callbacks,
        cleanup,
    }

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
    document.addEventListener('keydown', handleKeyDown)
}

/**
 * Cancels any active drag tracking.
 */
export function cancelDragTracking(): void {
    if (activeDrag) {
        activeDrag.cleanup()
        activeDrag = null
    }
}

/**
 * Performs a single-file native drag operation.
 */
async function performSingleFileDrag(filePath: string, iconId: string, mode: 'copy' | 'move'): Promise<void> {
    // Get the icon from cache
    const iconDataUrl = getCachedIcon(iconId)

    // If no icon is available, skip the native drag (the API requires an icon)
    if (!iconDataUrl) {
        return
    }

    let iconPath: string
    try {
        iconPath = await writeIconToTemp(iconDataUrl)
    } catch {
        // Can't write temp icon, skip drag
        return
    }

    try {
        // Start the native drag operation with the specified mode
        await startDrag({
            item: [filePath],
            icon: iconPath,
            mode,
        })
    } finally {
        // Clean up temp icon after drag completes
        void cleanupTempIcon()
    }
}

/**
 * Performs a selection-based drag operation via the backend.
 * This avoids transferring file paths over IPC for large selections.
 */
async function performSelectionDrag(context: SelectionDragContext, mode: 'copy' | 'move'): Promise<void> {
    // Get the icon from cache for the drag preview
    const iconDataUrl = getCachedIcon(context.iconId)

    // If no icon is available, skip the drag
    if (!iconDataUrl) {
        return
    }

    let iconPath: string
    try {
        iconPath = await writeIconToTemp(iconDataUrl)
    } catch {
        // Can't write temp icon, skip drag
        return
    }

    try {
        // Start the drag via backend (paths are looked up from cache)
        await startSelectionDrag(
            context.listingId,
            context.indices,
            context.includeHidden,
            context.hasParent,
            mode,
            iconPath,
        )
    } finally {
        // Clean up temp icon after drag completes
        void cleanupTempIcon()
    }
}
