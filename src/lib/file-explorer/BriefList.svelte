<script lang="ts">
    import type { FileEntry, SyncStatus } from './types'
    import { getCachedIcon, prefetchIcons, iconCacheVersion } from '$lib/icon-cache'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'

    interface Props {
        files: FileEntry[]
        selectedIndex: number
        isFocused?: boolean
        syncStatusMap?: Record<string, SyncStatus>
        onSelect: (index: number) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
    }

    const {
        files,
        selectedIndex,
        isFocused = true,
        syncStatusMap = {},
        onSelect,
        onNavigate,
        onContextMenu,
    }: Props = $props()

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
    const SYNC_ICON_WIDTH = 16 // 12px icon + 4px gap

    // ==== Layout constants ====
    const ROW_HEIGHT = 20
    const BUFFER_COLUMNS = 2
    const MIN_COLUMN_WIDTH = 100
    const COLUMN_PADDING = 8 // horizontal padding inside each column

    // ==== Container state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let containerWidth = $state(0)
    let scrollLeft = $state(0)

    // ==== Column layout calculations ====
    // Number of items that fit in one column
    const itemsPerColumn = $derived(Math.max(1, Math.floor(containerHeight / ROW_HEIGHT)))

    // Calculate column width based on longest filename
    // Uses a canvas to measure text width for performance
    let measureCanvas: HTMLCanvasElement | undefined
    function measureTextWidth(text: string, font: string): number {
        if (!measureCanvas) {
            measureCanvas = document.createElement('canvas')
        }
        const ctx = measureCanvas.getContext('2d')
        if (!ctx) return MIN_COLUMN_WIDTH
        ctx.font = font
        return ctx.measureText(text).width
    }

    // Calculate max filename width - only recalculates when files change
    const maxFilenameWidth = $derived.by(() => {
        if (files.length === 0) return MIN_COLUMN_WIDTH

        // Use system font matching CSS
        const font = '13px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'
        let maxWidth = 0

        // Check if any file has a sync status (need extra width for icon)
        const hasSyncStatus = files.some((f) => getSyncIconPath(syncStatusMap[f.path]))
        const syncIconExtra = hasSyncStatus ? SYNC_ICON_WIDTH : 0

        for (const file of files) {
            const width = measureTextWidth(file.name, font)
            if (width > maxWidth) maxWidth = width
        }

        // Add space for icon (16px) + gap (8px) + sync icon (if any) + padding
        const totalWidth = maxWidth + 16 + 8 + syncIconExtra + COLUMN_PADDING * 2

        // Clamp: minimum width, and max is containerWidth - 10px so next column peeks
        const maxAllowed = containerWidth > 10 ? containerWidth - 10 : containerWidth
        return Math.max(MIN_COLUMN_WIDTH, Math.min(totalWidth, maxAllowed))
    })

    // Total number of columns needed
    const totalColumns = $derived(Math.ceil(files.length / itemsPerColumn))

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

    // Get files for visible columns
    const visibleColumns = $derived.by(() => {
        const columns: { columnIndex: number; files: { file: FileEntry; globalIndex: number }[] }[] = []
        for (let col = virtualWindow.startIndex; col < virtualWindow.endIndex; col++) {
            const startFileIndex = col * itemsPerColumn
            const endFileIndex = Math.min(startFileIndex + itemsPerColumn, files.length)
            const columnFiles: { file: FileEntry; globalIndex: number }[] = []
            for (let i = startFileIndex; i < endFileIndex; i++) {
                columnFiles.push({ file: files[i], globalIndex: i })
            }
            if (columnFiles.length > 0) {
                columns.push({ columnIndex: col, files: columnFiles })
            }
        }
        return columns
    })

    function handleScroll(e: Event) {
        const target = e.target as HTMLDivElement
        scrollLeft = target.scrollLeft
    }

    // Icon prefetching
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const prefetchedSet: Set<string> = new Set()

    $effect(() => {
        const newIconIds = visibleColumns
            .flatMap((col) => col.files.map((f) => f.file.iconId))
            .filter((id) => id && !prefetchedSet.has(id))
        if (newIconIds.length > 0) {
            newIconIds.forEach((id) => prefetchedSet.add(id))
            void prefetchIcons(newIconIds)
        }
    })

    // Icon cache reactivity
    const _cacheVersion = $derived($iconCacheVersion)

    function getIconUrl(file: FileEntry): string | undefined {
        void _cacheVersion
        if (file.isDirectory) {
            const pathIcon = getCachedIcon(`path:${file.path}`)
            if (pathIcon) return pathIcon
        }
        return getCachedIcon(file.iconId)
    }

    function getFallbackEmoji(file: FileEntry): string {
        if (file.isSymlink) return '🔗'
        if (file.isDirectory) return '📁'
        return '📄'
    }

    function handleClick(globalIndex: number) {
        onSelect(globalIndex)
    }

    function handleDoubleClick(globalIndex: number) {
        onNavigate(files[globalIndex])
    }

    // Keep selected item in view when container size changes (window resize)
    $effect(() => {
        // Track containerHeight and containerWidth to trigger on resize
        if (containerHeight > 0 && containerWidth > 0 && scrollContainer && itemsPerColumn > 0) {
            const columnIndex = Math.floor(selectedIndex / itemsPerColumn)
            const newScrollLeft = getScrollToPosition(columnIndex, maxFilenameWidth, scrollLeft, containerWidth)
            if (newScrollLeft !== undefined) {
                scrollContainer.scrollLeft = newScrollLeft
            }
        }
    })

    // Scroll to bring selected item into view
    export function scrollToIndex(index: number) {
        if (!scrollContainer || itemsPerColumn === 0) return

        // Find which column contains this index
        const columnIndex = Math.floor(index / itemsPerColumn)
        const newScrollLeft = getScrollToPosition(columnIndex, maxFilenameWidth, scrollLeft, containerWidth)
        if (newScrollLeft !== undefined) {
            scrollContainer.scrollLeft = newScrollLeft
        }
    }

    // Keyboard navigation - provided by parent
    // Up/Down = prev/next item
    // Left/Right = jump between columns
    export function handleKeyNavigation(key: string): number | undefined {
        if (key === 'ArrowUp') {
            return Math.max(0, selectedIndex - 1)
        }
        if (key === 'ArrowDown') {
            return Math.min(files.length - 1, selectedIndex + 1)
        }
        if (key === 'ArrowLeft') {
            // Jump to same row in previous column, or first item if in leftmost column
            const newIndex = selectedIndex - itemsPerColumn
            return newIndex >= 0 ? newIndex : 0
        }
        if (key === 'ArrowRight') {
            // Jump to same row in next column, or last item if in rightmost column
            const newIndex = selectedIndex + itemsPerColumn
            return newIndex < files.length ? newIndex : files.length - 1
        }
        return undefined
    }

    // Returns paths of currently visible files (for sync status polling)
    export function getVisiblePaths(): string[] {
        return visibleColumns.flatMap((col) => col.files.map((f) => f.file.path))
    }
</script>

<div
    class="brief-list"
    class:is-focused={isFocused}
    bind:this={scrollContainer}
    bind:clientHeight={containerHeight}
    bind:clientWidth={containerWidth}
    onscroll={handleScroll}
    tabindex="-1"
    role="listbox"
    aria-activedescendant={files[selectedIndex] ? `file-${String(selectedIndex)}` : undefined}
>
    <!-- Spacer div provides accurate scrollbar for full list width -->
    <div class="virtual-spacer" style="width: {virtualWindow.totalSize}px; height: 100%;">
        <!-- Visible window positioned with translateX -->
        <div class="virtual-window" style="transform: translateX({virtualWindow.offset}px);">
            {#each visibleColumns as column (column.columnIndex)}
                <div class="column" style="width: {maxFilenameWidth}px;">
                    {#each column.files as { file, globalIndex } (file.path)}
                        {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                        <!-- svelte-ignore a11y_click_events_have_key_events a11y_interactive_supports_focus -->
                        <div
                            id={`file-${String(globalIndex)}`}
                            class="file-entry"
                            class:is-directory={file.isDirectory}
                            class:is-selected={globalIndex === selectedIndex}
                            onclick={() => {
                                handleClick(globalIndex)
                            }}
                            ondblclick={() => {
                                handleDoubleClick(globalIndex)
                            }}
                            oncontextmenu={(e) => {
                                e.preventDefault()
                                onSelect(globalIndex)
                                onContextMenu?.(files[globalIndex])
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
                                    <span class="symlink-badge">🔗</span>
                                {/if}
                            </span>
                            <span class="name">{file.name}</span>
                            {#if syncIcon}
                                <img class="sync-icon" src={syncIcon} alt="" width="12" height="12" />
                            {/if}
                        </div>
                    {/each}
                </div>
            {/each}
        </div>
    </div>
</div>

<style>
    .brief-list {
        margin: 0;
        padding: 0;
        overflow-x: auto;
        overflow-y: hidden;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
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
        padding: var(--spacing-xxs) var(--spacing-sm);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        white-space: nowrap;
        overflow: hidden;
        height: 20px;
        box-sizing: border-box;
    }

    .file-entry.is-selected {
        background-color: rgba(204, 228, 247, 0.1);
    }

    .brief-list.is-focused .file-entry.is-selected {
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

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .is-directory .name {
        font-weight: 600;
    }

    .sync-icon {
        flex-shrink: 0;
        margin-left: auto;
        opacity: 0.9;
    }

    @media (prefers-color-scheme: dark) {
        .file-entry.is-selected {
            background-color: rgba(10, 80, 208, 0.1);
        }
    }
</style>
