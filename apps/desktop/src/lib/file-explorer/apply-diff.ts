// Apply a diff to a file list while preserving cursor position
// Extracted to enable thorough unit testing

import type { FileEntry, DiffChange } from './types'

/**
 * Apply a diff from the file watcher to the file list.
 * Maintains sort order (directories first, then alphabetically).
 * Preserves cursor position: cursor stays on selected file by path, or resets to 0 if deleted.
 *
 * @param files - Current file list (will be mutated)
 * @param selectedIndex - Current cursor position
 * @param changes - Diff changes to apply
 * @returns New selectedIndex after applying changes
 */
export function applyDiff(files: FileEntry[], selectedIndex: number, changes: DiffChange[]): number {
    // Capture the currently selected file's path before any changes
    const selectedPath = files[selectedIndex]?.path

    // Apply all changes
    for (const change of changes) {
        if (change.type === 'add') {
            // Insert in sorted position
            const entry = change.entry
            let insertIndex = files.findIndex((f) => {
                // Keep ".." at the top
                if (f.name === '..') return false
                if (entry.name === '..') return true
                // Directories come before files
                if (entry.isDirectory && !f.isDirectory) return true
                if (!entry.isDirectory && f.isDirectory) return false
                // Alphabetical within same type
                return f.name.toLowerCase() > entry.name.toLowerCase()
            })
            if (insertIndex === -1) insertIndex = files.length
            files.splice(insertIndex, 0, entry)
        } else if (change.type === 'remove') {
            const idx = files.findIndex((f) => f.path === change.entry.path)
            if (idx >= 0) {
                files.splice(idx, 1)
            }
        } else {
            // change.type === 'modify'
            const idx = files.findIndex((f) => f.path === change.entry.path)
            if (idx >= 0) {
                files[idx] = change.entry
            }
        }
    }

    // Restore cursor position: find the originally selected file by path
    if (selectedPath) {
        const newIndex = files.findIndex((f) => f.path === selectedPath)
        if (newIndex >= 0) {
            // File still exists - keep cursor on it
            return newIndex
        } else {
            // Selected file was deleted - reset cursor to first entry
            return 0
        }
    } else if (files.length > 0) {
        return 0
    }

    return 0
}
