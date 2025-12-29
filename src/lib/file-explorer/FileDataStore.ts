import type { FileEntry } from './types'

/**
 * Non-reactive file data store.
 *
 * Stores file data outside of Svelte's reactivity system to avoid O(n) reactivity
 * costs when loading large directories (20k+ files). UI components request only
 * the visible range (~50-100 items) to put into reactive state.
 *
 * See ADR-009 for rationale.
 */

type UpdateCallback = () => void

/** Width calculation result from measureFilenameWidths */
interface WidthMeasurement {
    maxFilenameWidth: number
    /** Whether any files have sync status (affects width calculation) */
    hasSyncStatus: boolean
}

/**
 * Measures text width using canvas for optimal performance.
 * Reuses a single canvas instance across calls.
 */
let measureCanvas: HTMLCanvasElement | undefined
const defaultFont = '13px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'

function measureTextWidth(text: string, font: string = defaultFont): number {
    if (!measureCanvas) {
        measureCanvas = document.createElement('canvas')
    }
    const ctx = measureCanvas.getContext('2d')
    if (!ctx) return 100
    ctx.font = font
    return ctx.measureText(text).width
}

/**
 * Calculates max filename width for Brief mode horizontal scrollbar.
 * Uses canvas.measureText() which is ~1μs per call (50k ≈ 50ms).
 */
function measureFilenameWidths(
    entries: FileEntry[],
    syncStatusMap: Record<string, string | undefined>,
): WidthMeasurement {
    const minColumnWidth = 100
    const iconWidth = 16 // file/folder icon
    const iconGap = 8
    const columnPadding = 8 * 2
    const syncIconWidth = 16 // 12px icon + 4px gap

    if (entries.length === 0) {
        return { maxFilenameWidth: minColumnWidth, hasSyncStatus: false }
    }

    let maxWidth = 0
    let hasSyncStatus = false

    for (const file of entries) {
        const width = measureTextWidth(file.name)
        if (width > maxWidth) maxWidth = width

        if (!hasSyncStatus && syncStatusMap[file.path]) {
            hasSyncStatus = true
        }
    }

    const totalWidth = maxWidth + iconWidth + iconGap + (hasSyncStatus ? syncIconWidth : 0) + columnPadding

    return {
        maxFilenameWidth: Math.max(minColumnWidth, totalWidth),
        hasSyncStatus,
    }
}

/**
 * Creates a new FileDataStore instance.
 * Each pane should have its own store.
 */
export function createFileDataStore() {
    // Internal state (plain JS, not reactive)
    let allFiles: FileEntry[] = []
    let filteredFiles: FileEntry[] = []
    let showHiddenFiles = true
    let _totalCount = 0
    let _maxFilenameWidth = 100
    let _syncStatusMap: Record<string, string | undefined> = {}

    // Callbacks for UI updates
    const updateCallbacks: Set<UpdateCallback> = new Set()

    /** Filters files based on hidden file visibility */
    function applyFilter(entries: FileEntry[], showHidden: boolean): FileEntry[] {
        if (showHidden) return entries
        // Always keep ".." visible for parent navigation
        return entries.filter((e) => !e.name.startsWith('.') || e.name === '..')
    }

    /** Recalculates filtered files and notifies listeners */
    function recalculate() {
        filteredFiles = applyFilter(allFiles, showHiddenFiles)
        _totalCount = filteredFiles.length

        // Recalculate max filename width
        const measurement = measureFilenameWidths(filteredFiles, _syncStatusMap)
        _maxFilenameWidth = measurement.maxFilenameWidth

        notifyListeners()
    }

    /** Notifies all registered callbacks of a change */
    function notifyListeners() {
        for (const callback of updateCallbacks) {
            callback()
        }
    }

    return {
        /** Total count of visible files (after filtering) */
        get totalCount(): number {
            return _totalCount
        },

        /** Max filename width in pixels (for Brief mode horizontal scrollbar) */
        get maxFilenameWidth(): number {
            return _maxFilenameWidth
        },

        /**
         * Returns a slice of visible files for the virtual scroll window.
         * This is the only method that returns FileEntry[] to minimize data transfer.
         */
        getRange(start: number, end: number): FileEntry[] {
            return filteredFiles.slice(start, Math.min(end, filteredFiles.length))
        },

        /**
         * Returns a single file at the given index, or undefined if out of bounds.
         */
        getAt(index: number): FileEntry | undefined {
            return filteredFiles[index]
        },

        /**
         * Bulk sets all files, replacing any existing data.
         * Also updates totalCount and maxFilenameWidth.
         */
        setFiles(entries: FileEntry[]): void {
            allFiles = entries
            recalculate()
        },

        /**
         * Appends files to the existing list (for chunked loading).
         */
        appendFiles(entries: FileEntry[]): void {
            allFiles = [...allFiles, ...entries]
            recalculate()
        },

        /**
         * Clears all file data (for navigation).
         */
        clear(): void {
            allFiles = []
            filteredFiles = []
            _totalCount = 0
            _maxFilenameWidth = 100
            _syncStatusMap = {}
            notifyListeners()
        },

        /**
         * Updates the hidden files visibility setting.
         * Recalculates filtered list and notifies listeners.
         */
        setShowHiddenFiles(show: boolean): void {
            if (showHiddenFiles === show) return
            showHiddenFiles = show
            recalculate()
        },

        /**
         * Updates the sync status map and recalculates widths if needed.
         */
        setSyncStatusMap(statusMap: Record<string, string | undefined>): void {
            const hadSyncStatus = Object.values(_syncStatusMap).some(Boolean)
            _syncStatusMap = statusMap
            const hasSyncStatus = Object.values(statusMap).some(Boolean)

            // Only recalculate width if sync status presence changed
            if (hadSyncStatus !== hasSyncStatus) {
                const measurement = measureFilenameWidths(filteredFiles, _syncStatusMap)
                _maxFilenameWidth = measurement.maxFilenameWidth
            }

            notifyListeners()
        },

        /**
         * Gets the current sync status map.
         */
        get syncStatusMap(): Record<string, string | undefined> {
            return _syncStatusMap
        },

        /**
         * Finds the index of a file by name in the filtered list.
         * Returns -1 if not found.
         */
        findIndex(name: string): number {
            return filteredFiles.findIndex((f) => f.name === name)
        },

        /**
         * Applies a diff (add/remove/modify) to the file list.
         * Returns the new selected index if the current selection changed.
         */
        applyDiff(
            changes: { type: 'add' | 'remove' | 'modify'; entry: FileEntry }[],
            currentSelectedIndex: number,
        ): number {
            // Apply changes to allFiles (import logic from apply-diff.ts pattern)
            let newSelectedIndex = currentSelectedIndex

            for (const change of changes) {
                if (change.type === 'add') {
                    // Insert in sorted position (by name, directories first)
                    const insertIndex = allFiles.findIndex((f) => {
                        if (f.name === '..') return false
                        if (change.entry.isDirectory !== f.isDirectory) {
                            return change.entry.isDirectory
                        }
                        return change.entry.name.localeCompare(f.name) < 0
                    })

                    if (insertIndex === -1) {
                        allFiles.push(change.entry)
                    } else {
                        allFiles.splice(insertIndex, 0, change.entry)
                    }
                } else if (change.type === 'remove') {
                    const removeIndex = allFiles.findIndex((f) => f.path === change.entry.path)
                    if (removeIndex !== -1) {
                        allFiles.splice(removeIndex, 1)
                        // Adjust selection if removed item was before or at selection
                        if (removeIndex < newSelectedIndex) {
                            newSelectedIndex = Math.max(0, newSelectedIndex - 1)
                        } else if (removeIndex === newSelectedIndex) {
                            newSelectedIndex = Math.min(newSelectedIndex, allFiles.length - 1)
                        }
                    }
                } else {
                    // 'modify' case
                    const modifyIndex = allFiles.findIndex((f) => f.path === change.entry.path)
                    if (modifyIndex !== -1) {
                        allFiles[modifyIndex] = change.entry
                    }
                }
            }

            recalculate()
            return Math.max(0, Math.min(newSelectedIndex, _totalCount - 1))
        },

        /**
         * Registers a callback to be called when data changes.
         * Returns an unsubscribe function.
         */
        onUpdate(callback: UpdateCallback): () => void {
            updateCallbacks.add(callback)
            return () => updateCallbacks.delete(callback)
        },

        /**
         * Returns all filtered files (for operations that need full list).
         * Use sparingly - prefer getRange for virtual scrolling.
         */
        getAllFiltered(): FileEntry[] {
            return filteredFiles
        },
    }
}
