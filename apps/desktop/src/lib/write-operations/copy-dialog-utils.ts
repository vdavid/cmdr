/**
 * Utility functions for the copy dialog
 */

/**
 * Generates a dialog title with proper pluralization for files and folders.
 * @param files - Number of files being copied
 * @param folders - Number of folders being copied
 * @returns Formatted title string like "Copy 1 file", "Copy 2 files and 3 folders"
 */
export function generateTitle(files: number, folders: number): string {
    const parts: string[] = []
    if (files > 0) {
        parts.push(`${String(files)} ${files === 1 ? 'file' : 'files'}`)
    }
    if (folders > 0) {
        parts.push(`${String(folders)} ${folders === 1 ? 'folder' : 'folders'}`)
    }
    if (parts.length === 0) {
        return 'Copy'
    }
    return `Copy ${parts.join(' and ')}`
}

/**
 * Extracts the folder name from a full path.
 * @param path - Full path like "/Users/john/Documents"
 * @returns The last path component, e.g., "Documents"
 */
export function getFolderName(path: string): string {
    if (path === '/') return '/'
    const normalized = path.endsWith('/') ? path.slice(0, -1) : path
    const parts = normalized.split('/')
    return parts[parts.length - 1] || '/'
}
