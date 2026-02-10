/**
 * Utility functions for the transfer (copy/move) dialog
 */

import type { TransferOperationType } from '$lib/file-explorer/types'

const operationLabelMap: Record<TransferOperationType, string> = {
    copy: 'Copy',
    move: 'Move',
}

/**
 * Generates a dialog title with proper pluralization for files and folders.
 * @returns Formatted title string like "Copy 1 file", "Move 2 files and 3 folders"
 */
export function generateTitle(operationType: TransferOperationType, files: number, folders: number): string {
    const verb = operationLabelMap[operationType]
    const parts: string[] = []
    if (files > 0) {
        parts.push(`${String(files)} ${files === 1 ? 'file' : 'files'}`)
    }
    if (folders > 0) {
        parts.push(`${String(folders)} ${folders === 1 ? 'folder' : 'folders'}`)
    }
    if (parts.length === 0) {
        return verb
    }
    return `${verb} ${parts.join(' and ')}`
}

/**
 * Extracts the folder name from a full path.
 * @param path - Full path like "/Users/john/Documents"
 * @returns The last path component, like "Documents"
 */
export function getFolderName(path: string): string {
    if (path === '/') return '/'
    const normalized = path.endsWith('/') ? path.slice(0, -1) : path
    const parts = normalized.split('/')
    return parts[parts.length - 1] || '/'
}

/**
 * Converts frontend indices to backend indices.
 *
 * When a directory listing has a parent entry ("..") shown at index 0,
 * the frontend indices are offset by 1 from the backend indices.
 * This function adjusts for that offset and filters out invalid indices.
 *
 * @example
 * // With hasParent=true, frontend [1,2,3] becomes backend [0,1,2]
 * toBackendIndices([1, 2, 3], true) // => [0, 1, 2]
 *
 * // With hasParent=false, indices pass through unchanged
 * toBackendIndices([0, 1, 2], false) // => [0, 1, 2]
 *
 * // Index 0 with hasParent=true is filtered (it's the ".." entry)
 * toBackendIndices([0, 1, 2], true) // => [0, 1]
 */
export function toBackendIndices(frontendIndices: number[], hasParent: boolean): number[] {
    return frontendIndices.map((i) => (hasParent ? i - 1 : i)).filter((i) => i >= 0)
}

/**
 * Converts a frontend cursor index to a backend index.
 *
 * Returns null if the cursor is on the ".." entry (index 0 when hasParent=true)
 * or if the index is invalid.
 *
 * @example
 * toBackendCursorIndex(5, true)  // => 4 (adjusted for ".." entry)
 * toBackendCursorIndex(5, false) // => 5 (no adjustment needed)
 * toBackendCursorIndex(0, true)  // => null (cursor on ".." entry)
 * toBackendCursorIndex(-1, false) // => null (invalid index)
 */
export function toBackendCursorIndex(frontendIndex: number, hasParent: boolean): number | null {
    if (frontendIndex < 0) return null
    if (hasParent && frontendIndex === 0) return null // ".." entry
    return hasParent ? frontendIndex - 1 : frontendIndex
}
