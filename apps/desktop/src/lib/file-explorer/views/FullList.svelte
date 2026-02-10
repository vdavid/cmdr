<script lang="ts">
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from '../types'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { startSelectionDragTracking, type DragFileInfo } from '../drag-drop'
    import SortableHeader from '../selection/SortableHeader.svelte'
    import FileIcon from '../selection/FileIcon.svelte'
    import {
        getSyncIconPath,
        createParentEntry,
        getEntryAt as getEntryAtUtil,
        fetchVisibleRange as fetchVisibleRangeUtil,
        calculateFetchRange,
        isRangeCached,
        shouldResetCache,
        refetchIconsForEntries,
    } from './file-list-utils'
    import { formatSizeTriads } from '../selection/selection-info-utils'
    import {
        getVisibleItemsCount as getVisibleItemsCountUtil,
        getVirtualizationBufferRows,
        measureDateColumnWidth,
    } from './full-list-utils'
    import {
        getRowHeight,
        getIsCompactDensity,
        formatDateTime,
        formatFileSize,
    } from '$lib/settings/reactive-settings.svelte'
    import { extensionCacheCleared } from '$lib/icon-cache'

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
        sortBy: SortColumn
        sortOrder: SortOrder
        onSelect: (index: number, shiftKey?: boolean) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
        onSyncStatusRequest?: (paths: string[]) => void
        onSortChange?: (column: SortColumn) => void
        onVisibleRangeChange?: (start: number, end: number) => void
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
        sortBy,
        sortOrder,
        onSelect,
        onNavigate,
        onContextMenu,
        onSyncStatusRequest,
        onSortChange,
        onVisibleRangeChange,
    }: Props = $props()

    // ==== Cached entries (prefetch buffer) ====
    let cachedEntries = $state<FileEntry[]>([])
    let cachedRange = $state({ start: 0, end: 0 })
    let isFetching = $state(false)

    // ==== Virtual scrolling constants ====
    // Row height is reactive based on UI density setting
    const rowHeight = $derived(getRowHeight())
    // Buffer size is reactive based on settings
    const bufferSize = $derived(getVirtualizationBufferRows())
    // UI density for compact mode detection (uses reactive state from reactive-settings)
    const isCompact = $derived(getIsCompactDensity())

    // Dynamic date column width based on measured text width using the actual font.
    // Measures multiple sample dates to find the maximum width needed.
    const dateColumnWidth = $derived(measureDateColumnWidth(formatDateTime))

    // ==== Virtual scrolling state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let scrollTop = $state(0)

    // ==== Virtual scrolling derived calculations ====
    const virtualWindow = $derived(
        calculateVirtualWindow({
            direction: 'vertical',
            itemSize: rowHeight,
            bufferSize,
            containerSize: containerHeight,
            scrollOffset: scrollTop,
            totalItems: totalCount,
        }),
    )

    // Get entry at global index (handling ".." entry)
    export function getEntryAt(globalIndex: number): FileEntry | undefined {
        return getEntryAtUtil(globalIndex, hasParent, parentPath, cachedEntries, cachedRange)
    }

    // Fetch entries for the visible range
    async function fetchVisibleRange() {
        if (!listingId || isFetching) return

        const startItem = virtualWindow.startIndex
        const endItem = virtualWindow.endIndex

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

    // Get visible files for rendering
    // Note: We read cachedEntries/cachedRange here to establish reactive dependency
    const visibleFiles = $derived.by(() => {
        // MUST read reactive state to establish dependency tracking
        // Create local copies so the derived re-runs when these change
        const entries = [...cachedEntries] // Spread to read all elements
        const rangeStart = cachedRange.start
        const rangeEnd = cachedRange.end

        const files: { file: FileEntry; globalIndex: number }[] = []
        for (let i = virtualWindow.startIndex; i < virtualWindow.endIndex; i++) {
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
                files.push({ file: entry, globalIndex: i })
            }
        }
        return files
    })

    function handleScroll(e: Event) {
        const target = e.target as HTMLDivElement
        scrollTop = target.scrollTop
        void fetchVisibleRange()
    }

    // Handle file mousedown - selects and initiates drag tracking
    function handleMouseDown(event: MouseEvent, index: number) {
        if (event.button !== 0) return
        const entry = getEntryAt(index)
        if (!entry) return

        // ".." entry: just move cursor, no drag tracking
        if (entry.name === '..') {
            onSelect(index, event.shiftKey)
            return
        }

        const hasSelection = selectedIndices.size > 0

        if (!hasSelection) {
            // No selection: defer selection until drag threshold is crossed
            const fileInfo: DragFileInfo = { name: entry.name, isDirectory: entry.isDirectory, iconId: entry.iconId }
            startSelectionDragTracking(
                event,
                { type: 'single', path: entry.path, iconId: entry.iconId, index, fileInfo },
                {
                    onDragStart: () => {
                        onSelect(index, event.shiftKey)
                    },
                    onDragCancel: () => {
                        // Just do a normal select on cancel (mouseup without drag)
                        onSelect(index, event.shiftKey)
                    },
                },
            )
        } else {
            // Has selection: move cursor immediately (Shift+click still does range selection)
            onSelect(index, event.shiftKey)

            // Always drag the selection (regardless of which file clicked)
            // Find the first selected file's icon for the drag preview
            const firstSelectedIndex = Math.min(...selectedIndices)
            const firstSelectedEntry = getEntryAt(firstSelectedIndex)
            const iconId = firstSelectedEntry?.iconId ?? entry.iconId

            // Collect file info for the drag image (only from cached/visible entries)
            const fileInfos: DragFileInfo[] = []
            for (const idx of selectedIndices) {
                const e = getEntryAt(idx)
                if (e) fileInfos.push({ name: e.name, isDirectory: e.isDirectory, iconId: e.iconId })
            }

            startSelectionDragTracking(
                event,
                {
                    type: 'selection',
                    listingId,
                    indices: [...selectedIndices],
                    includeHidden,
                    hasParent,
                    iconId,
                    fileInfos,
                },
                {},
            )
        }
    }

    function handleDoubleClick(actualIndex: number) {
        const entry = getEntryAt(actualIndex)
        if (entry) onNavigate(entry)
    }

    // Exported for parent to call when arrow keys change cursor position
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return
        const newScrollTop = getScrollToPosition(index, rowHeight, scrollTop, containerHeight)
        if (newScrollTop !== undefined) {
            scrollContainer.scrollTop = newScrollTop
            // Also update state directly to trigger reactive chain immediately
            // (scroll events may be batched or delayed by the browser)
            scrollTop = newScrollTop
            // Fetch entries for the new visible range
            void fetchVisibleRange()
        }
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

    // Returns the number of visible items (for Page Up/Down navigation)
    export function getVisibleItemsCount(): number {
        return getVisibleItemsCountUtil(containerHeight, rowHeight)
    }

    // Re-fetch icons when the extension icon cache is cleared (settings change)
    $effect(() => {
        void $extensionCacheCleared // Track the store value
        // Re-fetch icons for all cached entries
        if (cachedEntries.length > 0) {
            refetchIconsForEntries(cachedEntries)
        }
    })

    // Report visible range to parent for MCP state sync
    $effect(() => {
        const startItem = virtualWindow.startIndex
        const endItem = virtualWindow.endIndex
        onVisibleRangeChange?.(startItem, endItem)
    })
</script>

<div class="full-list-container" class:is-focused={isFocused} class:is-compact={isCompact}>
    <!-- Header row with sortable columns (outside scroll container for correct height calculation) -->
    <div class="header-row" style="grid-template-columns: 16px 1fr 85px {dateColumnWidth}px;">
        <span class="header-icon"></span>
        <SortableHeader
            column="name"
            label="Name"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={onSortChange ?? (() => {})}
        />
        <SortableHeader
            column="size"
            label="Size"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={onSortChange ?? (() => {})}
            align="right"
        />
        <SortableHeader
            column="modified"
            label="Modified"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={onSortChange ?? (() => {})}
        />
    </div>
    <!-- Scrollable file list -->
    <div
        class="full-list"
        bind:this={scrollContainer}
        bind:clientHeight={containerHeight}
        onscroll={handleScroll}
        tabindex="-1"
        role="listbox"
        aria-activedescendant={cursorIndex >= 0 ? `file-${String(cursorIndex)}` : undefined}
    >
        <!-- Spacer div provides accurate scrollbar for full list size -->
        <div class="virtual-spacer" style="height: {virtualWindow.totalSize}px;">
            <!-- Visible window positioned with translateY -->
            <div class="virtual-window" style="transform: translateY({virtualWindow.offset}px);">
                {#each visibleFiles as { file, globalIndex } (file.path)}
                    {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                    <!-- svelte-ignore a11y_interactive_supports_focus -->
                    <div
                        id={`file-${String(globalIndex)}`}
                        class="file-entry"
                        class:is-under-cursor={globalIndex === cursorIndex}
                        class:is-selected={selectedIndices.has(globalIndex)}
                        data-drop-target-path={file.isDirectory && file.name !== '..' ? file.path : undefined}
                        style="height: {rowHeight}px; grid-template-columns: 16px 1fr 85px {dateColumnWidth}px;"
                        onmousedown={(e: MouseEvent) => {
                            handleMouseDown(e, globalIndex)
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
                        <span class="col-name">{file.name}</span>
                        <span class="col-size" title={file.size !== undefined ? formatFileSize(file.size) : ''}>
                            {#if file.isDirectory}
                                <span class="size-dir">&lt;dir&gt;</span>
                            {:else if file.size !== undefined}
                                {#each formatSizeTriads(file.size) as triad, i (i)}
                                    <span class={triad.tierClass}>{triad.value}</span>
                                {/each}
                            {/if}
                        </span>
                        <span class="col-date">{formatDateTime(file.modifiedAt)}</span>
                    </div>
                {/each}
            </div>
        </div>
    </div>
</div>

<style>
    .full-list-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        width: 100%;
    }

    .full-list {
        overflow-y: auto;
        overflow-x: hidden;
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-sm);
        line-height: 1;
        flex: 1;
        outline: none;
    }

    .header-row {
        display: grid;
        /* grid-template-columns set via inline style for dynamic date column width */
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-header);
        border-bottom: 1px solid var(--color-border);
        height: 22px;
        flex-shrink: 0;
    }

    .header-icon {
        width: 16px;
    }

    .virtual-spacer {
        position: relative;
    }

    .virtual-window {
        will-change: transform;
    }

    .file-entry {
        display: grid;
        /* height and grid-template-columns set via inline style for reactivity */
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
    }

    /* In compact mode, use symmetric padding to match BriefList alignment */
    .full-list-container.is-compact .file-entry {
        padding-top: 0;
        padding-bottom: 4px;
    }

    .file-entry.is-under-cursor {
        background-color: var(--color-cursor-unfocused-bg);
    }

    .full-list-container.is-focused .file-entry.is-under-cursor {
        background-color: var(--color-cursor-focused-bg);
    }

    .col-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-size {
        text-align: right;
        font-size: var(--font-size-xs);
    }

    .size-dir {
        color: var(--color-text-secondary);
    }

    .col-date {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .file-entry.is-selected .col-name {
        color: var(--color-selection-fg);
    }

    /* Selection color is preserved even under cursor */
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-name {
        color: var(--color-selection-fg);
    }
</style>
