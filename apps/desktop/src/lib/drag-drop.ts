// Drag and drop utilities for file items
// Handles the temp icon file and calling the native startDrag API

import { startDrag } from '@crabnebula/tauri-plugin-drag'
import { tempDir, join } from '@tauri-apps/api/path'
import { getCachedIcon } from './icon-cache'

/** Minimum distance (in pixels) to trigger drag */
const DRAG_THRESHOLD = 5

/** Name of the temp icon file */
const TEMP_ICON_FILENAME = 'drag-icon.png'

/** Global state for active drag operation */
let activeDrag: {
    startX: number
    startY: number
    filePath: string
    iconId: string
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
 * Starts tracking a potential drag operation.
 * Call on mousedown for a file entry.
 */
export function startDragTracking(event: MouseEvent, filePath: string, iconId: string): void {
    // Cancel any existing drag
    cancelDragTracking()

    const handleMouseMove = (moveEvent: MouseEvent) => {
        if (!activeDrag) return

        const dx = moveEvent.clientX - activeDrag.startX
        const dy = moveEvent.clientY - activeDrag.startY
        const distance = Math.sqrt(dx * dx + dy * dy)

        if (distance >= DRAG_THRESHOLD) {
            // Trigger the actual drag
            // Alt/Option key = copy mode, otherwise move mode (matches Finder behavior)
            const mode = moveEvent.altKey ? 'copy' : 'move'
            void performDrag(activeDrag.filePath, activeDrag.iconId, mode)
            cancelDragTracking()
        }
    }

    const handleMouseUp = () => {
        cancelDragTracking()
    }

    const cleanup = () => {
        document.removeEventListener('mousemove', handleMouseMove)
        document.removeEventListener('mouseup', handleMouseUp)
    }

    activeDrag = {
        startX: event.clientX,
        startY: event.clientY,
        filePath,
        iconId,
        cleanup,
    }

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
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
 * Performs the actual native drag operation.
 * @param filePath - Absolute filesystem path (e.g., "/Users/foo/bar.txt"). Currently, always
 *                   a local path; will need adjustment when we add remote volume support.
 * @param iconId - Icon cache key: extension for files (e.g., "png"), or full path for
 *                 directories (e.g., "/Users/foo/MyFolder"), or "dir" for generic folders.
 * @param mode - 'move' (default, like Finder) or 'copy' (when Alt/Option held)
 */
async function performDrag(filePath: string, iconId: string, mode: 'copy' | 'move'): Promise<void> {
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
