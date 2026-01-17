<script lang="ts">
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from './types'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { handleNavigationShortcut } from './keyboard-shortcuts'
    import { startDragTracking } from '$lib/drag-drop'
    import SortableHeader from './SortableHeader.svelte'
    import FileIcon from './FileIcon.svelte'
    import {
        getSyncIconPath,
        createParentEntry,
        getEntryAt as getEntryAtUtil,
        fetchVisibleRange as fetchVisibleRangeUtil,
        calculateFetchRange,
        isRangeCached,
        shouldResetCache,
    } from './file-list-utils'

    interface Props {
        listingId: string
        totalCount: number
        includeHidden: boolean
        cacheGeneration?: number
        cursorIndex: number
        isFocused?: boolean
        syncStatusMap?: Record<string, SyncStatus>
        selectedIndices?: Set<number>
        hasParent: boolean
        parentPath: string
        maxFilenameWidth?: number // From backend font metrics, if available
        sortBy: SortColumn
        sortOrder: SortOrder
        onSelect: (index: number, shiftKey?: boolean) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
        onSyncStatusRequest?: (paths: string[]) => void
        onSortChange?: (column: SortColumn) => void
    }

    const {
        listingId,
        totalCount,
        includeHidden,
        cacheGeneration = 0,
        cursorIndex,
        isFocused = true,
        syncStatusMap = {},
        selectedIndices = new Set<number>(),
        hasParent,
        parentPath,
        maxFilenameWidth: backendMaxWidth,
        sortBy,
        sortOrder,
        onSelect,
        onNavigate,
        onContextMenu,
        onSyncStatusRequest,
        onSortChange,
    }: Props = $props()

    // ==== Cached entries (prefetch buffer) ====
    let cachedEntries = $state<FileEntry[]>([])
    let cachedRange = $state({ start: 0, end: 0 })
    let isFetching = $state(false)

    // ==== Layout constants ====
    const ROW_HEIGHT = 20
    const BUFFER_COLUMNS = 2
    const MIN_COLUMN_WIDTH = 100
    // const COLUMN_PADDING = 8 // horizontal padding inside each column (unused for now)

    // ==== Container state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let containerWidth = $state(0)
    let scrollLeft = $state(0)

    // ==== Column layout calculations ====
    // Number of items that fit in one column
    const itemsPerColumn = $derived(Math.max(1, Math.floor(containerHeight / ROW_HEIGHT)))

    // For column width: use backend-calculated width if available, otherwise estimate
    // Backend calculation is based on actual font metrics and considers all filenames
    // Add space for: icon (16px) + gap (8px) + left padding (8px) + right padding (8px) + rounding buffer (2px)
    // The 2px buffer accounts for sub-pixel rendering differences between calculated and actual widths
    const COLUMN_PADDING = 16 + 8 + 8 + 8 + 2 // icon + gap + left padding + right padding + rounding buffer
    const calculatedColumnWidth = $derived(
        (backendMaxWidth ?? Math.min(200, Math.max(MIN_COLUMN_WIDTH, containerWidth / 3))) + COLUMN_PADDING,
    )
    // Cap column width to container width - columns should never be wider than the pane
    const maxFilenameWidth = $derived(
        containerWidth > 0 ? Math.min(calculatedColumnWidth, containerWidth) : calculatedColumnWidth,
    )

    // Total number of columns needed
    const totalColumns = $derived(Math.ceil(totalCount / itemsPerColumn))

    // ==== Virtual scrolling (horizontal) ====
    const virtualWindow = $derived(
        calculateVirtualWindow({
            direction: 'horizontal',
            itemSize: maxFilenameWidth,
            bufferSize: BUFFER_COLUMNS,
            containerSize: containerWidth,
            scrollOffset: scrollLeft,
            totalItems: totalColumns,
        }),
    )

    // Get entry at global index (handling ".." entry)
    export function getEntryAt(globalIndex: number): FileEntry | undefined {
        return getEntryAtUtil(globalIndex, hasParent, parentPath, cachedEntries, cachedRange)
    }

    // Fetch entries for the visible range
    async function fetchVisibleRange() {
        if (!listingId || isFetching) return

        // Calculate which backend indices we need (convert column range to item range)
        const startCol = virtualWindow.startIndex
        const endCol = virtualWindow.endIndex
        const startItem = startCol * itemsPerColumn
        const endItem = Math.min(endCol * itemsPerColumn, totalCount)

        // Check if range is already cached BEFORE setting isFetching
        // This prevents blocking subsequent fetches when data is already available
        const { fetchStart, fetchEnd } = calculateFetchRange({ startItem, endItem, hasParent, totalCount })
        if (isRangeCached(fetchStart, fetchEnd, cachedRange)) {
            return // Already cached
        }

        isFetching = true
        try {
            const result = await fetchVisibleRangeUtil({
                listingId,
                startItem,
                endItem,
                hasParent,
                totalCount,
                includeHidden,
                cachedRange,
                onSyncStatusRequest,
            })
            if (result) {
                cachedEntries = result.entries
                cachedRange = result.range
            }
        } catch {
            // Silently ignore fetch errors
        } finally {
            isFetching = false
        }
    }

    // Get visible columns with files
    // Note: We read cachedEntries/cachedRange here to establish reactive dependency
    const visibleColumns = $derived.by(() => {
        // MUST read reactive state to establish dependency tracking
        // Create local copies so the derived re-runs when these change
        const entries = [...cachedEntries] // Spread to read all elements
        const rangeStart = cachedRange.start
        const rangeEnd = cachedRange.end

        const columns: { columnIndex: number; files: { file: FileEntry; globalIndex: number }[] }[] = []
        for (let col = virtualWindow.startIndex; col < virtualWindow.endIndex; col++) {
            const startFileIndex = col * itemsPerColumn
            const endFileIndex = Math.min(startFileIndex + itemsPerColumn, totalCount)
            const columnFiles: { file: FileEntry; globalIndex: number }[] = []
            for (let i = startFileIndex; i < endFileIndex; i++) {
                // Inline getEntryAt logic to use local variables
                let entry: FileEntry | undefined
                if (hasParent && i === 0) {
                    entry = createParentEntry(parentPath)
                } else {
                    const backendIndex = hasParent ? i - 1 : i
                    if (backendIndex >= rangeStart && backendIndex < rangeEnd) {
                        entry = entries[backendIndex - rangeStart]
                    }
                }
                if (entry) {
                    columnFiles.push({ file: entry, globalIndex: i })
                }
            }
            if (columnFiles.length > 0) {
                columns.push({ columnIndex: col, files: columnFiles })
            }
        }
        return columns
    })

    // Fetch on scroll
    function handleScroll() {
        if (!scrollContainer) return
        scrollLeft = scrollContainer.scrollLeft
        void fetchVisibleRange()
    }

    // Handle file mousedown - selects and initiates drag tracking
    function handleMouseDown(event: MouseEvent, index: number) {
        // Always select on mousedown (pass shiftKey for range selection)
        onSelect(index, event.shiftKey)

        // Only start drag tracking for left mouse button and non-parent entries
        if (event.button !== 0) return
        const entry = getEntryAt(index)
        if (!entry || entry.name === '..') return

        // Start tracking for potential drag
        startDragTracking(event, entry.path, entry.iconId)
    }

    // Handle file click - for double-click detection
    let lastClickTime = 0
    let lastClickIndex = -1
    const DOUBLE_CLICK_MS = 300

    function handleClick(index: number) {
        const now = Date.now()
        if (lastClickIndex === index && now - lastClickTime < DOUBLE_CLICK_MS) {
            // Double click
            const entry = getEntryAt(index)
            if (entry) onNavigate(entry)
        }
        lastClickTime = now
        lastClickIndex = index
    }

    function handleDoubleClick(index: number) {
        const entry = getEntryAt(index)
        if (entry) onNavigate(entry)
    }

    // Scroll to a specific index
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return
        const columnIndex = Math.floor(index / itemsPerColumn)
        const position = getScrollToPosition(columnIndex, maxFilenameWidth, scrollLeft, containerWidth)
        if (position !== undefined) {
            scrollContainer.scrollLeft = position
        }
    }

    // Handle keyboard navigation
    export function handleKeyNavigation(key: string, event?: KeyboardEvent): number | undefined {
        // Try navigation shortcuts first (Home/End/PageUp/PageDown)
        if (event) {
            // Calculate number of visible columns for PageUp/PageDown
            const visibleColumns = Math.ceil(containerWidth / maxFilenameWidth)
            const result = handleNavigationShortcut(event, {
                currentIndex: cursorIndex,
                totalCount,
                itemsPerColumn,
                visibleColumns,
            })
            if (result) {
                return result.newIndex
            }
        }

        // Handle arrow keys
        if (key === 'ArrowUp') {
            return Math.max(0, cursorIndex - 1)
        }
        if (key === 'ArrowDown') {
            return Math.min(totalCount - 1, cursorIndex + 1)
        }
        if (key === 'ArrowLeft') {
            const newIndex = cursorIndex - itemsPerColumn
            return newIndex >= 0 ? newIndex : 0
        }
        if (key === 'ArrowRight') {
            const newIndex = cursorIndex + itemsPerColumn
            return newIndex < totalCount ? newIndex : totalCount - 1
        }
        return undefined
    }

    // Track previous values to detect actual changes
    let prevCacheProps = { listingId: '', includeHidden: false, totalCount: 0, cacheGeneration: 0 }

    // Single effect: fetch when ready, reset cache when listingId/includeHidden/totalCount/cacheGeneration changes
    $effect(() => {
        const currentProps = { listingId, includeHidden, totalCount, cacheGeneration }
        if (!listingId || containerHeight <= 0) return

        // Check if any tracked prop changed (totalCount changes on file add/remove, cacheGeneration on sort)
        if (shouldResetCache(currentProps, prevCacheProps)) {
            cachedEntries = []
            cachedRange = { start: 0, end: 0 }
            prevCacheProps = currentProps
        }

        void fetchVisibleRange()
    })

    // Track previous container height to detect resizes
    let prevContainerHeight = 0

    // Scroll to cursor index when container height changes (e.g., window resize)
    $effect(() => {
        const height = containerHeight
        // Only react to meaningful height changes (not initial 0)
        if (height > 0 && prevContainerHeight > 0 && height !== prevContainerHeight) {
            // Container height changed - scroll to keep cursor visible
            scrollToIndex(cursorIndex)
        }
        prevContainerHeight = height
    })
</script>

<div class="brief-list-container" class:is-focused={isFocused}>
    <!-- Header row with sort options -->
    <div class="header-row" role="row">
        <SortableHeader
            column="name"
            label="Name"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={(col: SortColumn) => onSortChange?.(col)}
        />
        <SortableHeader
            column="extension"
            label="Ext"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={(col: SortColumn) => onSortChange?.(col)}
        />
        <SortableHeader
            column="size"
            label="Size"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={(col: SortColumn) => onSortChange?.(col)}
        />
        <SortableHeader
            column="modified"
            label="Modified"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={(col: SortColumn) => onSortChange?.(col)}
        />
        <SortableHeader
            column="created"
            label="Created"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={(col: SortColumn) => onSortChange?.(col)}
        />
    </div>

    <!-- Scrollable file list -->
    <div
        class="brief-list"
        bind:this={scrollContainer}
        bind:clientHeight={containerHeight}
        bind:clientWidth={containerWidth}
        onscroll={handleScroll}
        tabindex="-1"
        role="listbox"
        aria-activedescendant={cursorIndex >= 0 ? `file-${String(cursorIndex)}` : undefined}
    >
        <!-- Spacer div provides accurate scrollbar for full list width -->
        <div class="virtual-spacer" style="width: {virtualWindow.totalSize}px; height: 100%;">
            <!-- Visible window positioned with translateX -->
            <div class="virtual-window" style="transform: translateX({virtualWindow.offset}px);">
                {#each visibleColumns as column (column.columnIndex)}
                    <div class="column" style="width: {maxFilenameWidth}px;">
                        {#each column.files as { file, globalIndex } (file.path)}
                            {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                            <!-- svelte-ignore a11y_click_events_have_key_events,a11y_interactive_supports_focus -->
                            <div
                                id={`file-${String(globalIndex)}`}
                                class="file-entry"
                                class:is-under-cursor={globalIndex === cursorIndex}
                                class:is-selected={selectedIndices.has(globalIndex)}
                                onmousedown={(e: MouseEvent) => {
                                    handleMouseDown(e, globalIndex)
                                }}
                                onclick={() => {
                                    handleClick(globalIndex)
                                }}
                                ondblclick={() => {
                                    handleDoubleClick(globalIndex)
                                }}
                                oncontextmenu={(e: MouseEvent) => {
                                    e.preventDefault()
                                    onSelect(globalIndex)
                                    onContextMenu?.(file)
                                }}
                                role="option"
                                aria-selected={globalIndex === cursorIndex}
                            >
                                <FileIcon {file} {syncIcon} />
                                <span class="name">{file.name}</span>
                            </div>
                        {/each}
                    </div>
                {/each}
            </div>
        </div>
    </div>
</div>

<style>
    .brief-list-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        width: 100%;
    }

    .header-row {
        display: flex;
        height: 22px;
        background: var(--color-bg-header);
        border-bottom: 1px solid var(--color-border);
        flex-shrink: 0;
        padding: 0 var(--spacing-xs);
    }

    /*noinspection CssUnusedSymbol*/
    .header-row :global(.sortable-header) {
        flex: 1;
        min-width: 0;
        justify-content: center;
        text-align: center;
    }

    .brief-list {
        overflow-x: auto;
        overflow-y: hidden;
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-sm);
        line-height: 1;
        flex: 1;
        outline: none;
    }

    .virtual-spacer {
        position: relative;
        display: flex;
    }

    .virtual-window {
        display: flex;
        will-change: transform;
        height: 100%;
    }

    .column {
        flex-shrink: 0;
        display: flex;
        flex-direction: column;
    }

    .file-entry {
        display: flex;
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
        white-space: nowrap;
        overflow: hidden;
    }

    .file-entry.is-under-cursor {
        background-color: rgba(204, 228, 247, 0.1);
    }

    .brief-list-container.is-focused .file-entry.is-under-cursor {
        background-color: var(--color-cursor-focused-bg);
    }

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .file-entry.is-selected .name {
        color: var(--color-selection-fg);
    }

    /* Selection color is preserved even under cursor */
    .brief-list-container.is-focused .file-entry.is-under-cursor.is-selected .name {
        color: var(--color-selection-fg);
    }

    @media (prefers-color-scheme: dark) {
        .file-entry.is-under-cursor {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
