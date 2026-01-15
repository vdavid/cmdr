<script lang="ts">
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from './types'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
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
        hasParent: boolean
        parentPath: string
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
        cursorIndex,
        isFocused = true,
        syncStatusMap = {},
        hasParent,
        parentPath,
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

    // ==== Virtual scrolling constants ====
    const ROW_HEIGHT = 20
    const BUFFER_SIZE = 20

    // Size tier colors for digit triads
    const sizeTierClasses = ['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']

    // ==== Virtual scrolling state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let scrollTop = $state(0)

    // ==== Virtual scrolling derived calculations ====
    const virtualWindow = $derived(
        calculateVirtualWindow({
            direction: 'vertical',
            itemSize: ROW_HEIGHT,
            bufferSize: BUFFER_SIZE,
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

    /** Formats a number into digit triads with CSS classes for coloring */
    function formatSizeTriads(bytes: number): { value: string; tierClass: string }[] {
        const str = String(bytes)
        const triads: { value: string; tierClass: string }[] = []

        let remaining = str
        let tierIndex = 0
        while (remaining.length > 0) {
            const start = Math.max(0, remaining.length - 3)
            const triad = remaining.slice(start)
            remaining = remaining.slice(0, start)

            triads.unshift({
                value: triad,
                tierClass: sizeTierClasses[Math.min(tierIndex, sizeTierClasses.length - 1)],
            })
            tierIndex++
        }

        return triads.map((t, i) => ({
            ...t,
            value: i < triads.length - 1 ? t.value + '\u2009' : t.value,
        }))
    }

    /** Formats bytes as human-readable (for tooltip) */
    function formatHumanReadable(bytes: number): string {
        const units = ['bytes', 'KB', 'MB', 'GB', 'TB']
        let value = bytes
        let unitIndex = 0
        while (value >= 1024 && unitIndex < units.length - 1) {
            value /= 1024
            unitIndex++
        }
        const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
        return `${valueStr} ${units[unitIndex]}`
    }

    /** Formats timestamp as YYYY-MM-DD hh:mm */
    function formatDate(timestamp: number | undefined): string {
        if (timestamp === undefined) return ''
        const date = new Date(timestamp * 1000)
        const pad = (n: number) => String(n).padStart(2, '0')
        const year = date.getFullYear()
        const month = pad(date.getMonth() + 1)
        const day = pad(date.getDate())
        const hours = pad(date.getHours())
        const mins = pad(date.getMinutes())
        return `${String(year)}-${month}-${day} ${hours}:${mins}`
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

    function handleDoubleClick(actualIndex: number) {
        const entry = getEntryAt(actualIndex)
        if (entry) onNavigate(entry)
    }

    // Exported for parent to call when arrow keys change cursor position
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return
        const newScrollTop = getScrollToPosition(index, ROW_HEIGHT, scrollTop, containerHeight)
        if (newScrollTop !== undefined) {
            scrollContainer.scrollTop = newScrollTop
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
        return Math.ceil(containerHeight / ROW_HEIGHT)
    }
</script>

<div
    class="full-list"
    class:is-focused={isFocused}
    bind:this={scrollContainer}
    bind:clientHeight={containerHeight}
    onscroll={handleScroll}
    tabindex="-1"
    role="listbox"
    aria-activedescendant={cursorIndex >= 0 ? `file-${String(cursorIndex)}` : undefined}
>
    <!-- Header row with sortable columns -->
    <div class="header-row">
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
                    class:is-directory={file.isDirectory}
                    class:is-under-cursor={globalIndex === cursorIndex}
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
                    <span class="col-size" title={file.size !== undefined ? formatHumanReadable(file.size) : ''}>
                        {#if file.isDirectory}
                            <span class="size-dir">&lt;dir&gt;</span>
                        {:else if file.size !== undefined}
                            {#each formatSizeTriads(file.size) as triad, i (i)}
                                <span class={triad.tierClass}>{triad.value}</span>
                            {/each}
                        {/if}
                    </span>
                    <span class="col-date">{formatDate(file.modifiedAt)}</span>
                </div>
            {/each}
        </div>
    </div>
</div>

<style>
    .full-list {
        overflow-y: auto;
        overflow-x: hidden;
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-sm);
        line-height: 1;
        flex: 1;
        outline: none;
        display: flex;
        flex-direction: column;
    }

    .header-row {
        display: grid;
        grid-template-columns: 16px 1fr 85px 120px;
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
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
        grid-template-columns: 16px 1fr 85px 120px;
    }

    .file-entry.is-under-cursor {
        background-color: rgba(204, 228, 247, 0.1);
    }

    .full-list.is-focused .file-entry.is-under-cursor {
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

    /*noinspection CssUnusedSymbol*/
    .size-bytes {
        color: var(--color-text-secondary);
    }

    /*noinspection CssUnusedSymbol*/
    .size-kb {
        color: var(--color-size-kb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-mb {
        color: var(--color-size-mb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-gb {
        color: var(--color-size-gb);
    }

    /*noinspection CssUnusedSymbol*/
    .size-tb {
        color: var(--color-size-tb);
    }

    .col-date {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    @media (prefers-color-scheme: dark) {
        .file-entry.is-under-cursor {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
