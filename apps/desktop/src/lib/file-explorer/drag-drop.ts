// Drag and drop utilities for file items
// Handles both single-file drag and selection-based multi-file drag

import { startDrag } from '@crabnebula/tauri-plugin-drag'
import { tempDir, join } from '@tauri-apps/api/path'
import { getCachedIcon } from '$lib/icon-cache'
import { startSelectionDrag } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings/settings-store'
import { renderDragImage } from './drag-image-renderer'

/** Gets the drag threshold from settings (minimum distance in pixels to trigger drag) */
export function getDragThreshold(): number {
    return getSetting('advanced.dragThreshold')
}

/** Name of the temp icon file */
const TEMP_ICON_FILENAME = 'drag-icon.png'

/** Name of the temp rendered drag image file */
const TEMP_DRAG_IMAGE_FILENAME = 'drag-image.png'

/** Info for a file being dragged, used to render the drag image and overlay icons. */
export interface DragFileInfo {
    name: string
    isDirectory: boolean
    iconId: string
}

/** Context for a single file drag (no prior selection) */
interface SingleFileDragContext {
    type: 'single'
    path: string
    iconId: string
    index: number
    /** File info for the drag image renderer */
    fileInfo?: DragFileInfo
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
    /** File info for the drag image renderer (first N files of selection) */
    fileInfos?: DragFileInfo[]
}

/** Callbacks for drag lifecycle events */
interface DragCallbacks {
    /** Called when drag threshold is crossed (for single-file case, to trigger selection) */
    onDragStart?: () => void
    /** Called when drag is cancelled (ESC key or mouseup before threshold) */
    onDragCancel?: () => void
}

/** Tracks whether the current native drag originated from this app (pane-to-pane or self-drop). */
let draggingFromSelf = false

/** Getter to read the flag reliably across modules (avoids ES module live-binding timing issues). */
export function getIsDraggingFromSelf(): boolean {
    return draggingFromSelf
}

/** Resets the self-drag flag. Call from the drop event handler after processing. */
export function resetDraggingFromSelf(): void {
    draggingFromSelf = false
}

/** Restores the draggingFromSelf flag (for re-entry detection). */
export function markAsSelfDrag(): void {
    draggingFromSelf = true
}

/** Fingerprint of the last self-initiated drag for re-entry detection. */
interface DragFingerprint {
    count: number
    samplePaths: string[]
}

let selfDragFingerprint: DragFingerprint | null = null
/** File info stored from self-drag for overlay icon rendering. */
let selfDragFileInfos: DragFileInfo[] | null = null

/** Stores a fingerprint from the current drag's paths for re-entry detection. */
export function storeSelfDragFingerprint(paths: string[], fileInfos?: DragFileInfo[]): void {
    selfDragFingerprint = {
        count: paths.length,
        samplePaths: paths.slice(0, 5),
    }
    if (fileInfos) {
        selfDragFileInfos = fileInfos
    }
}

/** Checks if incoming drag paths match a stored self-drag fingerprint. O(1) for 50k+ files. */
export function matchesSelfDragFingerprint(paths: string[]): boolean {
    if (!selfDragFingerprint) return false
    if (paths.length !== selfDragFingerprint.count) return false
    return selfDragFingerprint.samplePaths.every((p, i) => paths[i] === p)
}

/** Returns stored file infos from self-drag (for overlay icons), or null. */
export function getSelfDragFileInfos(): DragFileInfo[] | null {
    return selfDragFileInfos
}

/** Clears both the fingerprint and stored file infos. Call on drop completion. */
export function clearSelfDragFingerprint(): void {
    selfDragFingerprint = null
    selfDragFileInfos = null
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

/** Writes a canvas drag image to a temp PNG file and returns the path. */
async function writeDragImageToTemp(canvas: HTMLCanvasElement): Promise<string> {
    const tempPath = await tempDir()
    const imagePath = await join(tempPath, TEMP_DRAG_IMAGE_FILENAME)

    const blob = await new Promise<Blob>((resolve, reject) => {
        canvas.toBlob((result) => {
            if (result) resolve(result)
            else reject(new Error('Canvas toBlob failed'))
        }, 'image/png')
    })

    const buffer = await blob.arrayBuffer()
    const bytes = new Uint8Array(buffer)
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    await writeFile(imagePath, bytes)
    return imagePath
}

/** Cleans up the temp drag image file */
async function cleanupTempDragImage(): Promise<void> {
    try {
        const tempPath = await tempDir()
        const imagePath = await join(tempPath, TEMP_DRAG_IMAGE_FILENAME)
        const { remove } = await import('@tauri-apps/plugin-fs')
        await remove(imagePath)
    } catch {
        // Ignore cleanup errors
    }
}

/**
 * Resolves the drag icon path: if file infos are available, renders a rich canvas image.
 * Falls back to the simple cached icon.
 */
async function resolveDragIconPath(
    iconId: string,
    fileInfos: DragFileInfo[] | undefined,
): Promise<{ path: string; usedCanvas: boolean } | null> {
    // Try rich canvas image first
    if (fileInfos && fileInfos.length > 0) {
        try {
            const canvas = await renderDragImage(fileInfos)
            const path = await writeDragImageToTemp(canvas)
            return { path, usedCanvas: true }
        } catch {
            // Fall through to simple icon
        }
    }

    // Fall back to simple icon
    const iconDataUrl = getCachedIcon(iconId)
    if (!iconDataUrl) return null

    try {
        const path = await writeIconToTemp(iconDataUrl)
        return { path, usedCanvas: false }
    } catch {
        return null
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

        if (distance >= getDragThreshold()) {
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
                void performSingleFileDrag(ctx.path, ctx.iconId, mode, ctx.fileInfo)
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
    draggingFromSelf = false
}

/**
 * Performs a single-file native drag operation.
 */
async function performSingleFileDrag(
    filePath: string,
    iconId: string,
    mode: 'copy' | 'move',
    fileInfo?: DragFileInfo,
): Promise<void> {
    const fileInfos = fileInfo ? [fileInfo] : undefined
    const resolved = await resolveDragIconPath(iconId, fileInfos)
    if (!resolved) return

    draggingFromSelf = true
    try {
        await startDrag({
            item: [filePath],
            icon: resolved.path,
            mode,
        })
    } finally {
        // Don't reset draggingFromSelf here — startDrag may resolve before
        // the OS delivers drop/leave events. The flag is cleared by the
        // drop event handler via resetDraggingFromSelf().
        if (resolved.usedCanvas) void cleanupTempDragImage()
        else void cleanupTempIcon()
    }
}

/**
 * Performs a selection-based drag operation via the backend.
 * This avoids transferring file paths over IPC for large selections.
 */
async function performSelectionDrag(context: SelectionDragContext, mode: 'copy' | 'move'): Promise<void> {
    const resolved = await resolveDragIconPath(context.iconId, context.fileInfos)
    if (!resolved) return

    draggingFromSelf = true
    try {
        // Start the drag via backend (paths are looked up from cache)
        await startSelectionDrag(
            context.listingId,
            context.indices,
            context.includeHidden,
            context.hasParent,
            mode,
            resolved.path,
        )
    } finally {
        // Don't reset draggingFromSelf here — see performSingleFileDrag comment.
        if (resolved.usedCanvas) void cleanupTempDragImage()
        else void cleanupTempIcon()
    }
}
