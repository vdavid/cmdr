<script lang="ts">
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from './types'
    import { getCachedIcon, iconCacheVersion, prefetchIcons } from '$lib/icon-cache'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { getFileRange } from '$lib/tauri-commands'
    import { handleNavigationShortcut } from './keyboard-shortcuts'
    import { startDragTracking } from '$lib/drag-drop'
    import SortableHeader from './SortableHeader.svelte'

    /** Prefetch buffer - load this many items around visible range */
    const PREFETCH_BUFFER = 200

    interface Props {
        listingId: string
        totalCount: number
        includeHidden: boolean
        cacheGeneration?: number
        selectedIndex: number
        isFocused?: boolean
        syncStatusMap?: Record<string, SyncStatus>
        hasParent: boolean
        parentPath: string
        maxFilenameWidth?: number // From backend font metrics, if available
        sortBy: SortColumn
        sortOrder: SortOrder
        onSelect: (index: number) => void
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
        selectedIndex,
        isFocused = true,
        syncStatusMap = {},
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

    // Sync status icon paths - returns undefined if no icon should be shown
    function getSyncIconPath(status: SyncStatus | undefined): string | undefined {
        if (!status) return undefined
        const iconMap: Record<SyncStatus, string | undefined> = {
            synced: '/icons/sync-synced.svg',
            online_only: '/icons/sync-online-only.svg',
            uploading: '/icons/sync-uploading.svg',
            downloading: '/icons/sync-downloading.svg',
            unknown: undefined,
        }
        return iconMap[status]
    }

    // Width of sync icon + gap (only added when sync status is available)
    // const SYNC_ICON_WIDTH = 16 // 12px icon + 4px gap (unused for now)

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

    // Debug logging
    $effect(() => {
        // eslint-disable-next-line no-console
        console.log('[BRIEF] maxFilenameWidth:', {
            backendMaxWidth,
            containerWidth,
            calculatedColumnWidth,
            maxFilenameWidth,
            COLUMN_PADDING,
        })
    })

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

    // Create parent entry
    function createParentEntry(): FileEntry {
        return {
            name: '..',
            path: parentPath,
            isDirectory: true,
            isSymlink: false,
            permissions: 0o755,
            owner: '',
            group: '',
            iconId: 'dir',
            extendedMetadataLoaded: true,
        }
    }

    // Get entry at global index (handling ".." entry)
    export function getEntryAt(globalIndex: number): FileEntry | undefined {
        if (hasParent && globalIndex === 0) {
            return createParentEntry()
        }

        // Backend index (without ".." entry)
        const backendIndex = hasParent ? globalIndex - 1 : globalIndex

        // Find in cached entries
        if (backendIndex >= cachedRange.start && backendIndex < cachedRange.end) {
            return cachedEntries[backendIndex - cachedRange.start]
        }

        return undefined
    }

    // Fetch entries for the visible range
    async function fetchVisibleRange() {
        if (!listingId || isFetching) return

        // Calculate which backend indices we need
        const startCol = virtualWindow.startIndex
        const endCol = virtualWindow.endIndex

        // Convert column range to item range
        let startItem = startCol * itemsPerColumn
        let endItem = Math.min(endCol * itemsPerColumn, totalCount)

        // Account for ".." entry
        if (hasParent) {
            startItem = Math.max(0, startItem - 1)
            endItem = Math.max(0, endItem - 1)
        }

        // Add prefetch buffer
        const fetchStart = Math.max(0, startItem - PREFETCH_BUFFER / 2)
        const fetchEnd = Math.min(hasParent ? totalCount - 1 : totalCount, endItem + PREFETCH_BUFFER / 2)

        // Only fetch if needed range isn't cached
        if (fetchStart >= cachedRange.start && fetchEnd <= cachedRange.end) {
            return // Already cached
        }

        isFetching = true
        try {
            const entries = await getFileRange(listingId, fetchStart, fetchEnd - fetchStart, includeHidden)
            cachedEntries = entries
            cachedRange = { start: fetchStart, end: fetchStart + entries.length }

            // Prefetch icons for visible entries
            const iconIds = entries.map((e) => e.iconId).filter((id) => id)
            void prefetchIcons(iconIds)

            // Request sync status for visible paths
            const paths = entries.map((e) => e.path)
            onSyncStatusRequest?.(paths)
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
                    entry = createParentEntry()
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

    // Get icon URL for a file
    // Subscribe to cache version - this makes getIconUrl reactive
    const _cacheVersion = $derived($iconCacheVersion)

    function getIconUrl(file: FileEntry): string | undefined {
        void _cacheVersion // Track cache version for reactivity
        return getCachedIcon(file.iconId)
    }

    // Fallback emoji for files without icons
    function getFallbackEmoji(file: FileEntry): string {
        if (file.isSymlink) return '🔗'
        if (file.isDirectory) return '📁'
        return '📄'
    }

    // Handle file mousedown - selects and initiates drag tracking
    function handleMouseDown(event: MouseEvent, index: number) {
        // Always select on mousedown
        onSelect(index)

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
                currentIndex: selectedIndex,
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
            return Math.max(0, selectedIndex - 1)
        }
        if (key === 'ArrowDown') {
            return Math.min(totalCount - 1, selectedIndex + 1)
        }
        if (key === 'ArrowLeft') {
            const newIndex = selectedIndex - itemsPerColumn
            return newIndex >= 0 ? newIndex : 0
        }
        if (key === 'ArrowRight') {
            const newIndex = selectedIndex + itemsPerColumn
            return newIndex < totalCount ? newIndex : totalCount - 1
        }
        return undefined
    }

    // Track previous values to detect actual changes
    let prevListingId = ''
    let prevIncludeHidden = false
    let prevTotalCount = 0
    let prevCacheGeneration = 0

    // Single effect: fetch when ready, reset cache when listingId/includeHidden/totalCount/cacheGeneration changes
    $effect(() => {
        // Read reactive dependencies
        const currentListingId = listingId
        const currentIncludeHidden = includeHidden
        const currentTotalCount = totalCount
        const currentCacheGeneration = cacheGeneration
        if (!currentListingId || containerHeight <= 0) return

        // Check if any tracked prop changed
        // totalCount changes when files are added/removed by the file watcher
        // cacheGeneration changes when sorting is changed (forces re-fetch)
        if (
            currentListingId !== prevListingId ||
            currentIncludeHidden !== prevIncludeHidden ||
            currentTotalCount !== prevTotalCount ||
            currentCacheGeneration !== prevCacheGeneration
        ) {
            // Reset cache for new listing, filter change, file count change, or cache invalidation
            cachedEntries = []
            cachedRange = { start: 0, end: 0 }
            prevListingId = currentListingId
            prevIncludeHidden = currentIncludeHidden
            prevTotalCount = currentTotalCount
            prevCacheGeneration = currentCacheGeneration
        }

        void fetchVisibleRange()
    })

    // Track previous container height to detect resizes
    let prevContainerHeight = 0

    // Scroll to selected index when container height changes (e.g., window resize)
    $effect(() => {
        const height = containerHeight
        // Only react to meaningful height changes (not initial 0)
        if (height > 0 && prevContainerHeight > 0 && height !== prevContainerHeight) {
            // Container height changed - scroll to keep selection visible
            scrollToIndex(selectedIndex)
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
        aria-activedescendant={selectedIndex >= 0 ? `file-${String(selectedIndex)}` : undefined}
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
                                class:is-directory={file.isDirectory}
                                class:is-selected={globalIndex === selectedIndex}
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
                                aria-selected={globalIndex === selectedIndex}
                            >
                                <span class="icon-wrapper">
                                    {#if getIconUrl(file)}
                                        <img class="icon" src={getIconUrl(file)} alt="" width="16" height="16" />
                                    {:else}
                                        <span class="icon-emoji">{getFallbackEmoji(file)}</span>
                                    {/if}
                                    {#if file.isSymlink}
                                        <span class="symlink-badge" class:has-sync={!!syncIcon}>🔗</span>
                                    {/if}
                                    {#if syncIcon}
                                        <img class="sync-badge" src={syncIcon} alt="" width="10" height="10" />
                                    {/if}
                                </span>
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

    .file-entry.is-selected {
        background-color: rgba(204, 228, 247, 0.1);
    }

    .brief-list-container.is-focused .file-entry.is-selected {
        background-color: var(--color-selection-bg);
    }

    .icon-wrapper {
        position: relative;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
    }

    .icon {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-sm);
        width: 16px;
        text-align: center;
        display: block;
    }

    .symlink-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        font-size: 8px;
        line-height: 1;
    }

    .symlink-badge.has-sync {
        bottom: auto;
        right: auto;
        top: -2px;
        left: -2px;
    }

    .sync-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        width: 10px;
        height: 10px;
    }

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    @media (prefers-color-scheme: dark) {
        .file-entry.is-selected {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
