<script lang="ts">
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from '../types'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { startSelectionDragTracking, type DragFileInfo } from '../drag/drag-drop'
    import { startClickToRename, cancelClickToRename } from '../rename/rename-activation'
    import SortableHeader from '../selection/SortableHeader.svelte'
    import FileIcon from '../selection/FileIcon.svelte'
    import InlineRenameEditor from '../rename/InlineRenameEditor.svelte'
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
    import { getRowHeight, formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { getSetting } from '$lib/settings/settings-store'
    import { formatNumber, pluralize } from '../selection/selection-info-utils'
    import { isScanning } from '$lib/indexing/index-state.svelte'
    import { extensionCacheCleared } from '$lib/icon-cache'
    import type { RenameState } from '../rename/rename-state.svelte'

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
        /** Rename state for inline editing */
        renameState?: RenameState | null
        onSelect: (index: number, shiftKey?: boolean) => void
        onNavigate: (entry: FileEntry) => void
        onContextMenu?: (entry: FileEntry) => void
        onSyncStatusRequest?: (paths: string[]) => void
        onSortChange?: (column: SortColumn) => void
        onVisibleRangeChange?: (start: number, end: number) => void
        /** Called when rename input value changes */
        onRenameInput?: (value: string) => void
        /** Called when rename is submitted (Enter) */
        onRenameSubmit?: () => void
        /** Called when rename is cancelled */
        onRenameCancel?: () => void
        /** Called when shake animation ends */
        onRenameShakeEnd?: () => void
        /** Called when click-to-rename timer fires (user held click on cursor entry) */
        onStartRename?: () => void
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
        renameState = null,
        onSelect,
        onNavigate,
        onContextMenu,
        onSyncStatusRequest,
        onSortChange,
        onVisibleRangeChange,
        onRenameInput,
        onRenameSubmit,
        onRenameCancel,
        onRenameShakeEnd,
        onStartRename,
    }: Props = $props()

    // ==== Cached entries (prefetch buffer) ====
    let cachedEntries = $state<FileEntry[]>([])
    let cachedRange = $state({ start: 0, end: 0 })
    let isFetching = $state(false)

    // Drive index scanning state — used for directory size tooltips
    const scanning = $derived(isScanning())

    // ==== Layout constants ====
    // Row height is reactive based on UI density setting
    const rowHeight = $derived(getRowHeight())
    // Buffer columns is reactive based on settings
    const bufferColumns = $derived(getSetting('advanced.virtualizationBufferColumns'))
    const MIN_COLUMN_WIDTH = 100
    // const COLUMN_PADDING = 8 // horizontal padding inside each column (unused for now)

    // ==== Container state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let containerWidth = $state(0)
    let scrollLeft = $state(0)

    // ==== Column layout calculations ====
    // Number of items that fit in one column
    const itemsPerColumn = $derived(Math.max(1, Math.floor(containerHeight / rowHeight)))

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
            bufferSize: bufferColumns,
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
        cancelClickToRename()
        if (!scrollContainer) return
        scrollLeft = scrollContainer.scrollLeft
        void fetchVisibleRange()
    }

    // Handle file mousedown - selects and initiates drag tracking
    function handleMouseDown(event: MouseEvent, index: number) {
        if (event.button !== 0) return

        // Let clicks inside the inline rename input pass through without
        // triggering selection/drag — the input handles its own focus.
        const target = event.target as HTMLElement
        if (target.closest('.rename-input')) return

        const entry = getEntryAt(index)
        if (!entry) return

        // ".." entry: just move cursor, no drag tracking
        if (entry.name === '..') {
            onSelect(index, event.shiftKey)
            return
        }

        // Click-to-rename: if clicking the entry already under the cursor
        // (without Shift), start a timer that activates rename after 800ms.
        // Skip when rename is already active.
        if (index === cursorIndex && !event.shiftKey && !renameState?.active && onStartRename) {
            startClickToRename(event, onStartRename)
            return
        }

        // Clicking a different entry cancels any pending click-to-rename timer
        cancelClickToRename()

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

    // Handle file click - for double-click detection
    let lastClickTime = 0
    let lastClickIndex = -1
    const DOUBLE_CLICK_MS = 300

    function handleClick(index: number) {
        const now = Date.now()
        if (lastClickIndex === index && now - lastClickTime < DOUBLE_CLICK_MS) {
            // Double click — cancel any pending click-to-rename
            cancelClickToRename()
            const entry = getEntryAt(index)
            if (entry) onNavigate(entry)
        }
        lastClickTime = now
        lastClickIndex = index
    }

    function handleDoubleClick(index: number) {
        cancelClickToRename()
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
            // Also update state directly to trigger reactive chain immediately
            // (scroll events may be batched or delayed by the browser)
            scrollLeft = position
            // Fetch entries for the new visible range
            void fetchVisibleRange()
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

    // Scroll to cursor index when container height changes (for example, window resize)
    $effect(() => {
        const height = containerHeight
        // Only react to meaningful height changes (not initial 0)
        if (height > 0 && prevContainerHeight > 0 && height !== prevContainerHeight) {
            // Container height changed - scroll to keep cursor visible
            scrollToIndex(cursorIndex)
        }
        prevContainerHeight = height
    })

    // Re-fetch icons when the extension icon cache is cleared (settings change)
    $effect(() => {
        void $extensionCacheCleared // Track the store value
        // Re-fetch icons for all cached entries
        if (cachedEntries.length > 0) {
            refetchIconsForEntries(cachedEntries)
        }
    })

    /** Build tooltip for a directory entry showing recursive size info. */
    function buildDirTooltip(file: FileEntry): string | undefined {
        if (!file.isDirectory) return undefined
        if (file.recursiveSize !== undefined) {
            const sizeInfo = `${formatFileSize(file.recursiveSize)} · ${formatNumber(file.recursiveFileCount ?? 0)} ${pluralize(file.recursiveFileCount ?? 0, 'file', 'files')} · ${formatNumber(file.recursiveDirCount ?? 0)} ${pluralize(file.recursiveDirCount ?? 0, 'folder', 'folders')}`
            return scanning ? `${sizeInfo} — Might be outdated` : sizeInfo
        }
        if (scanning) return 'Scanning...'
        return undefined
    }

    // Report visible range to parent for MCP state sync
    $effect(() => {
        // Calculate visible item range from column range
        const startCol = virtualWindow.startIndex
        const endCol = virtualWindow.endIndex
        const startItem = startCol * itemsPerColumn
        const endItem = Math.min(endCol * itemsPerColumn, totalCount)
        onVisibleRangeChange?.(startItem, endItem)
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
                                data-drop-target-path={file.isDirectory && file.name !== '..' ? file.path : undefined}
                                title={buildDirTooltip(file)}
                                style="height: {rowHeight}px;"
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
                                {#if renameState?.active && renameState.target?.index === globalIndex}
                                    <InlineRenameEditor
                                        value={renameState.currentName}
                                        severity={renameState.validation.severity}
                                        shaking={renameState.shaking}
                                        ariaLabel={`Rename ${renameState.target.originalName}`}
                                        ariaInvalid={renameState.validation.severity === 'error'}
                                        validationMessage={renameState.validation.message}
                                        focusTrigger={renameState.focusTrigger}
                                        onInput={(v: string) => onRenameInput?.(v)}
                                        onSubmit={() => onRenameSubmit?.()}
                                        onCancel={() => onRenameCancel?.()}
                                        onShakeEnd={() => onRenameShakeEnd?.()}
                                    />
                                {:else}
                                    <span class="name">{file.name}</span>
                                {/if}
                            </div>
                        {/each}
                    </div>
                {/each}
            </div>
        </div>
    </div>
    {#if (hasParent ? totalCount - 1 : totalCount) === 0}
        <div class="empty-folder-overlay">Empty folder</div>
    {/if}
</div>

<style>
    .brief-list-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        width: 100%;
        position: relative;
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
        /* height is set via inline style for reactivity */
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
        white-space: nowrap;
        overflow: hidden;
    }

    .file-entry.is-under-cursor {
        background-color: var(--color-cursor-inactive);
    }

    .brief-list-container.is-focused .file-entry.is-under-cursor {
        background-color: var(--color-cursor-active);
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

    .empty-folder-overlay {
        position: absolute;
        top: 22px; /* Below the header row */
        left: 0;
        right: 0;
        bottom: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        pointer-events: none;
    }
</style>
