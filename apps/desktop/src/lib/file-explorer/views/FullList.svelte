<script lang="ts">
    import IconCircleAlert from '~icons/lucide/circle-alert'
    import IconHourglass from '~icons/lucide/hourglass'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from '../types'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { startSelectionDragTracking, type DragFileInfo } from '../drag/drag-drop'
    import { startClickToRename, cancelClickToRename } from '../rename/rename-activation'
    import SortableHeader from '../selection/SortableHeader.svelte'
    import FileIcon from '../selection/FileIcon.svelte'
    import InlineRenameEditor from '../rename/InlineRenameEditor.svelte'
    import { fetchStatusMap, glyphFor, labelFor, type EntryStatusCode } from '../git/status-column'
    import {
        getSyncIconPath,
        createParentEntry,
        getEntryAt as getEntryAtUtil,
        fetchVisibleRange as fetchVisibleRangeUtil,
        calculateFetchRange,
        isRangeCached,
        shouldResetCache,
        refetchIconsForEntries,
        updateIndexSizesInPlace,
        type DirStats,
    } from './file-list-utils'
    import { getDirStatsBatch } from '$lib/tauri-commands'
    import { formatSizeForDisplay, formatNumber } from '../selection/selection-info-utils'
    import { pluralize } from '$lib/utils/pluralize'
    import { isScanning, isAggregating } from '$lib/indexing/index-state.svelte'
    import { isRestricted } from '$lib/stores/restricted-paths-store.svelte'
    import { restrictedFolderTooltip } from '$lib/system-strings.svelte'
    import InfoIcon from '~icons/lucide/info'

    const RESTRICTED_FOLDER_TOOLTIP = $derived(restrictedFolderTooltip())
    import {
        getVisibleItemsCount as getVisibleItemsCountUtil,
        getVirtualizationBufferRows,
        buildDirSizeTooltip,
        buildFileSizeTooltip,
        getDisplaySize,
        hasSizeMismatch,
        getDisplayExtension,
        getDisplayName,
        pickSizeDisplay,
    } from './full-list-utils'
    import { computeFullListColumnWidths } from './measure-column-widths'
    import {
        getRowHeight,
        getIconSize,
        getIsCompactDensity,
        formattedDate,
        formatFileSize,
        getSizeDisplayMode,
        getSizeMismatchWarning,
        getStripedRows,
        getHumanFriendlySizeUnits,
        getFileSizeFormat,
    } from '$lib/settings/reactive-settings.svelte'
    import { iconCacheCleared } from '$lib/icon-cache'
    import { onDebouncedScaleChange, getEffectiveScale } from '$lib/text-size.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import type { RenameState } from '../rename/rename-state.svelte'

    interface Props {
        listingId: string
        totalCount: number
        includeHidden: boolean
        cacheGeneration?: number
        /**
         * Bumped on every `directory-diff` event. Triggers a soft refresh
         * (refetch visible range in the background, keep existing entries
         * visible until new ones land). Use this instead of `cacheGeneration`
         * for diff-driven refreshes — `cacheGeneration` does a destructive
         * wipe that causes empty-pane flicker mid-bulk-operation.
         */
        softRefreshTick?: number
        cursorIndex: number
        isFocused?: boolean
        syncStatusMap?: Record<string, SyncStatus>
        selectedIndices?: Set<number>
        hasParent: boolean
        parentPath: string
        /** Path of the directory currently being listed (used to show its total on the ".." row). */
        currentPath: string
        sortBy: SortColumn
        sortOrder: SortOrder
        /**
         * Repo root for the optional Git status column. `null` when the path
         * isn't inside a worktree; `undefined` when the column is disabled.
         */
        gitRepoRoot?: string | null
        /** Whether the optional Git status column should render. */
        showGitColumn?: boolean
        /** Rename state for inline editing */
        renameState?: RenameState | null
        onSelect: (index: number, shiftKey?: boolean, metaKey?: boolean) => void
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
        /** Called when a drag actually initiates (threshold crossed) from this view. */
        onDragInitiate?: () => void
    }

    const {
        listingId,
        totalCount,
        includeHidden,
        cacheGeneration = 0,
        softRefreshTick = 0,
        cursorIndex,
        isFocused = true,
        syncStatusMap = {},
        selectedIndices = new Set<number>(),
        hasParent,
        parentPath,
        currentPath,
        sortBy,
        sortOrder,
        gitRepoRoot = null,
        showGitColumn = false,
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
        onDragInitiate,
    }: Props = $props()

    // ==== Cached entries (prefetch buffer) ====
    let cachedEntries = $state<FileEntry[]>([])
    let cachedRange = $state({ start: 0, end: 0 })
    let isFetching = $state(false)
    // Recursive stats for the CURRENT directory (shown on the ".." row so that space isn't wasted).
    let parentDirStats = $state<DirStats | null>(null)

    // ==== Virtual scrolling constants ====
    // Row height is reactive based on UI density setting
    const rowHeight = $derived(getRowHeight())
    // Buffer size is reactive based on settings
    const bufferSize = $derived(getVirtualizationBufferRows())
    // UI density for compact mode detection (uses reactive state from reactive-settings)
    const isCompact = $derived(getIsCompactDensity())

    // Size display mode (smart/logical/physical)
    const sizeDisplayMode = $derived(getSizeDisplayMode())

    // Size mismatch warning setting
    const showSizeMismatchWarning = $derived(getSizeMismatchWarning())

    // Striped rows setting
    const stripedRows = $derived(getStripedRows())

    // Human-friendly vs. raw-bytes size formatting
    const sizeFormatOpts = $derived({
        humanFriendly: getHumanFriendlySizeUnits(),
        format: getFileSizeFormat(),
    })

    // Drive index state: show spinner while scanning OR aggregating (sizes aren't ready until aggregation finishes)
    const indexing = $derived(isScanning() || isAggregating())

    // Column widths are declared after the virtual window, which gates parent-row inclusion.
    let columnWidths = $state({ ext: 60, size: 115, date: 80, dateLeft: 0 })
    let skipTransition = $state(false)

    /** Icon column width in the grid template, tracks density × text scale. */
    const iconColWidth = $derived(getIconSize())

    /**
     * Scale-settled "tick", bumped from `onDebouncedScaleChange` so the
     * column-width `$effect` re-runs after the user releases the text-size
     * slider (or the OS settles a new accessibility size). Live drag is
     * already covered by CSS reflow; this catches the canvas-measured
     * Ext / Size / Modified columns up to the new font.
     */
    let scaleSettleTick = $state(0)
    let unsubscribeScale: (() => void) | undefined
    $effect(() => {
        unsubscribeScale = onDebouncedScaleChange(() => {
            scaleSettleTick++
        })
        return () => {
            unsubscribeScale?.()
            unsubscribeScale = undefined
        }
    })

    /**
     * Whether the optional Git column should render in the layout. We gate on
     * both the user setting AND the presence of a repo root: outside a
     * worktree, the column would just show blank cells, so we omit it
     * entirely to keep the name column wide.
     */
    const gitColumnVisible = $derived(showGitColumn && !!gitRepoRoot)

    /** Reactive map from path-relative-to-repo → status code. `null` while loading. */
    let gitStatusMap = $state<Map<string, EntryStatusCode> | null>(null)

    /**
     * Single-glyph cell width. The header reads "Git" (3 chars at 12px ≈ 18px);
     * floor at 24px so the column doesn't collapse below the glyph + a hair
     * of breathing room.
     */
    const GIT_COLUMN_WIDTH = 28

    const gridTemplate = $derived(
        gitColumnVisible
            ? `${String(iconColWidth)}px 1fr ${String(GIT_COLUMN_WIDTH)}px ${String(columnWidths.ext)}px ${String(columnWidths.size)}px ${String(columnWidths.date)}px`
            : `${String(iconColWidth)}px 1fr ${String(columnWidths.ext)}px ${String(columnWidths.size)}px ${String(columnWidths.date)}px`,
    )

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

    // Shrink-wrapped column widths, measured strictly from the rows currently on
    // screen so the name column keeps every spare pixel. Widths refresh smoothly
    // (300ms CSS transition) as the user scrolls, resizes the window, or when new
    // entries stream into the prefetch buffer.
    //
    // Held across the "empty cache" window right after a dir switch so we don't
    // collapse to header-only widths and then snap outward again; `skipTransition`
    // handles the actual nav by suppressing the CSS transition for one paint.
    //
    // The ".." row's (often huge) recursive size only factors in when that row is
    // actually on screen; otherwise the size column stays oversized after scrolling.
    const firstVisibleGlobalIndex = $derived(rowHeight > 0 ? Math.floor(scrollTop / rowHeight) : 0)
    const lastVisibleGlobalIndex = $derived(
        rowHeight > 0 && containerHeight > 0
            ? Math.min(totalCount - 1, Math.floor((scrollTop + containerHeight - 1) / rowHeight))
            : -1,
    )
    const isParentRowVisible = $derived(hasParent && firstVisibleGlobalIndex === 0)

    $effect(() => {
        // Re-run when the scale settles (canvas measurer was just invalidated).
        // Reading the tick keeps it as a Svelte dep without affecting any logic.
        void scaleSettleTick
        const first = firstVisibleGlobalIndex
        const last = lastVisibleGlobalIndex
        const parentOffset = hasParent ? 1 : 0
        const firstBackend = Math.max(0, first - parentOffset)
        const lastBackend = last - parentOffset

        const visible: FileEntry[] = []
        for (let i = firstBackend; i <= lastBackend; i++) {
            if (i >= cachedRange.start && i < cachedRange.end) {
                visible.push(cachedEntries[i - cachedRange.start])
            }
        }

        const parentStats = isParentRowVisible ? parentDirStats : null
        if (visible.length === 0 && !parentStats) return
        // Reading getEffectiveScale() here makes the effect re-run when the
        // compounded scale changes (system multiplier resolves at startup, OS
        // accessibility size flips, user releases the slider). The 1s-debounced
        // `scaleSettleTick` covers the heavy backend font-metrics path; this
        // direct read is what prevents a startup race where a Full-mode
        // listing is measured at scale 1 and then never re-measured after the
        // real scale lands.
        void getEffectiveScale()
        columnWidths = computeFullListColumnWidths({
            entries: visible,
            parentDirStats: parentStats,
            formattedDate,
            sizeDisplayMode,
            indexing,
            showSizeMismatchWarning,
            sortBy,
            sizeFormatOpts,
            isRestricted,
        })
    })

    // Get entry at global index (handling ".." entry)
    export function getEntryAt(globalIndex: number): FileEntry | undefined {
        return getEntryAtUtil(
            globalIndex,
            hasParent,
            parentPath,
            cachedEntries,
            cachedRange,
            parentDirStats ?? undefined,
        )
    }

    /** Updates index size fields on cached directory entries AND on the ".." row. */
    export function refreshIndexSizes(): void {
        if (cachedEntries.length === 0 && !hasParent) return
        void updateIndexSizesInPlace(cachedEntries, hasParent ? currentPath : undefined).then((stats) => {
            parentDirStats = stats
        })
    }

    // Fetch entries for the visible range
    // `force=true` skips the "already cached" short-circuit; see BriefList for the rationale.
    async function fetchVisibleRange(force = false) {
        if (!listingId || isFetching) return

        const startItem = virtualWindow.startIndex
        const endItem = virtualWindow.endIndex

        // Check if range is already cached BEFORE setting isFetching
        // This prevents blocking subsequent fetches when data is already available
        const { fetchStart, fetchEnd } = calculateFetchRange({ startItem, endItem, hasParent, totalCount })
        if (!force && isRangeCached(fetchStart, fetchEnd, cachedRange)) {
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
                force,
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
                entry = createParentEntry(parentPath, parentDirStats ?? undefined)
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
        cancelClickToRename()
        const target = e.target as HTMLDivElement
        scrollTop = target.scrollTop
        void fetchVisibleRange()
    }

    // Click-to-rename: if clicking the entry already under the cursor (no modifiers),
    // start a timer that activates rename after 800ms. Drag tracking still runs in
    // `handleMouseDown` so the cursor item remains draggable; crossing the drag
    // threshold cancels the rename timer.
    function maybeStartClickToRename(event: MouseEvent, index: number) {
        if (index === cursorIndex && !event.shiftKey && !event.metaKey && !renameState?.active && onStartRename) {
            startClickToRename(event, onStartRename)
        } else {
            // Clicking a different entry cancels any pending click-to-rename timer
            cancelClickToRename()
        }
    }

    // Handle file mousedown - selects and initiates drag tracking
    function handleMouseDown(event: MouseEvent, index: number) {
        if (event.button !== 0) return

        // Let clicks inside the inline rename input pass through without
        // triggering selection/drag; the input handles its own focus.
        const target = event.target as HTMLElement
        if (target.closest('.rename-input')) return

        const entry = getEntryAt(index)
        if (!entry) return

        // ".." entry: just move cursor, no drag tracking
        if (entry.name === '..') {
            onSelect(index, event.shiftKey, event.metaKey)
            return
        }

        maybeStartClickToRename(event, index)

        const hasSelection = selectedIndices.size > 0

        if (!hasSelection) {
            // No selection: defer selection until drag threshold is crossed
            const fileInfo: DragFileInfo = { name: entry.name, isDirectory: entry.isDirectory, iconId: entry.iconId }
            startSelectionDragTracking(
                event,
                { type: 'single', path: entry.path, iconId: entry.iconId, index, fileInfo },
                {
                    onDragStart: () => {
                        onSelect(index, event.shiftKey, event.metaKey)
                    },
                    onDragCancel: () => {
                        // Just do a normal select on cancel (mouseup without drag)
                        onSelect(index, event.shiftKey, event.metaKey)
                    },
                    onDragInitiate,
                },
            )
        } else {
            // Has selection: move cursor immediately (Shift+click ranges, Cmd+click toggles)
            onSelect(index, event.shiftKey, event.metaKey)

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
                { onDragInitiate },
            )
        }
    }

    function handleDoubleClick(actualIndex: number) {
        cancelClickToRename()
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
    let prevCacheProps = { listingId: '', includeHidden: false, cacheGeneration: 0 }
    let prevTotalCount = 0
    let prevSoftTick = 0

    // Hard reset on cold context changes (nav, sort, hidden toggle): wipe
    // entries, refetch from scratch.
    // Soft refresh on totalCount or softRefreshTick changes (`directory-diff`
    // bursts, in-place renames): refetch in background and atomically replace,
    // keeping existing rows visible — no empty-pane flicker mid-bulk-op.
    $effect(() => {
        const currentProps = { listingId, includeHidden, cacheGeneration }
        const currentTotal = totalCount
        const currentTick = softRefreshTick
        if (!listingId || containerHeight <= 0) return

        if (shouldResetCache(currentProps, prevCacheProps)) {
            cachedEntries = []
            cachedRange = { start: 0, end: 0 }
            prevCacheProps = currentProps
            prevTotalCount = currentTotal
            prevSoftTick = currentTick
            // Suppress the grid-template-columns transition for the first paint after
            // a dir switch; otherwise the header (which persists across navs) slides
            // from the previous dir's widths to the new ones.
            skipTransition = true
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    skipTransition = false
                })
            })
            void fetchVisibleRange()
            return
        }

        if (currentTotal !== prevTotalCount || currentTick !== prevSoftTick) {
            prevTotalCount = currentTotal
            prevSoftTick = currentTick
            void fetchVisibleRange(true)
            return
        }

        void fetchVisibleRange()
    })

    // Returns the number of visible items (for Page Up/Down navigation)
    export function getVisibleItemsCount(): number {
        return getVisibleItemsCountUtil(containerHeight, rowHeight)
    }

    // Re-fetch icons when the icon cache is cleared (settings or theme change)
    $effect(() => {
        void $iconCacheCleared // Track the store value
        // Re-fetch icons for all cached entries
        if (cachedEntries.length > 0) {
            refetchIconsForEntries(cachedEntries)
        }
    })

    // Fetch the current folder's recursive stats so the ".." row can show the total.
    // Re-runs when the directory changes; cleared when we're at a volume root.
    $effect(() => {
        if (!hasParent || !currentPath) {
            parentDirStats = null
            return
        }
        // Re-run when cacheGeneration bumps (sort, refresh), currentPath is already tracked above.
        void cacheGeneration
        void getDirStatsBatch([currentPath])
            .then((results) => {
                parentDirStats = results[0] ?? null
            })
            .catch(() => {
                // Silently ignore -- indexing may not be initialized yet.
            })
    })

    // Report visible range to parent for MCP state sync
    $effect(() => {
        const startItem = virtualWindow.startIndex
        const endItem = virtualWindow.endIndex
        onVisibleRangeChange?.(startItem, endItem)
    })

    /**
     * Fetches the per-path git status map for `currentPath` and refreshes it
     * whenever the watcher emits `git-state-changed` for the active repo.
     *
     * The map is keyed by repo-relative path (forward slashes), which is what
     * `get_git_status_for_paths` returns. Cells look up by computing the
     * relative path on render (see `gitStatusFor`).
     */
    $effect(() => {
        if (!gitColumnVisible || !gitRepoRoot) {
            gitStatusMap = null
            return
        }
        const repo = gitRepoRoot
        const dir = currentPath
        // Track cacheGeneration so an explicit refresh reloads the map.
        void cacheGeneration

        let cancelled = false
        let unlisten: UnlistenFn | undefined

        async function load() {
            const map = await fetchStatusMap(repo, dir).catch(() => null)
            if (!cancelled) gitStatusMap = map
        }

        void load()
        void listen<{ repoRoot: string }>('git-state-changed', (event) => {
            if (event.payload.repoRoot === repo) void load()
        }).then((fn) => {
            if (cancelled) fn()
            else unlisten = fn
        })

        return () => {
            cancelled = true
            unlisten?.()
        }
    })

    /**
     * Maps a row's absolute path to a status code, or `null` when the row is
     * clean / outside the worktree. Repo-relative keys are computed against
     * the active repo root so directories with the repo root in the middle of
     * their path still hit.
     */
    function gitStatusFor(file: FileEntry): EntryStatusCode | null {
        if (!gitStatusMap || !gitRepoRoot) return null
        const root = gitRepoRoot.endsWith('/') ? gitRepoRoot : gitRepoRoot + '/'
        if (!file.path.startsWith(root)) return null
        const rel = file.path.slice(root.length)
        return gitStatusMap.get(rel) ?? null
    }
</script>

<div class="full-list-container" class:is-focused={isFocused} class:is-compact={isCompact}>
    <!-- Header row with sortable columns (outside scroll container for correct height calculation) -->
    <div
        class="header-row"
        class:no-transition={skipTransition}
        role="toolbar"
        aria-label="Sort columns"
        style="grid-template-columns: {gridTemplate};"
    >
        <span class="header-icon"></span>
        <SortableHeader
            column="name"
            label="Name"
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={onSortChange ?? (() => {})}
        />
        {#if gitColumnVisible}
            <span class="header-git" title="Git status of each file">Git</span>
        {/if}
        <SortableHeader
            column="extension"
            label="Ext"
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
        aria-label="File list"
        aria-activedescendant={cursorIndex >= 0 ? `file-${String(cursorIndex)}` : undefined}
    >
        <!-- Spacer div provides accurate scrollbar for full list size -->
        <div class="virtual-spacer" style="height: {virtualWindow.totalSize}px;">
            <!-- Visible window positioned with translateY -->
            <div class="virtual-window" style="transform: translateY({virtualWindow.offset}px);">
                {#each visibleFiles as { file, globalIndex } (file.path)}
                    {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                    {@const dirDisplaySize = file.isDirectory
                        ? getDisplaySize(file.recursiveSize, file.recursivePhysicalSize, sizeDisplayMode)
                        : undefined}
                    {@const fileDisplaySize = !file.isDirectory
                        ? getDisplaySize(file.size, file.physicalSize, sizeDisplayMode)
                        : undefined}
                    {@const fileIsRestricted = isRestricted(file.path)}
                    {@const sizeOverride = pickSizeDisplay(file, fileIsRestricted)}
                    {@const date = formattedDate(file.modifiedAt)}
                    <!-- svelte-ignore a11y_interactive_supports_focus -->
                    <div
                        id={`file-${String(globalIndex)}`}
                        class="file-entry"
                        class:is-under-cursor={globalIndex === cursorIndex}
                        class:is-selected={selectedIndices.has(globalIndex)}
                        class:is-striped={stripedRows && globalIndex % 2 === 1}
                        class:no-transition={skipTransition}
                        class:is-restricted={fileIsRestricted}
                        data-filename={file.name}
                        data-drop-target-path={file.isDirectory ? file.path : undefined}
                        style="height: {rowHeight}px; grid-template-columns: {gridTemplate};"
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
                        {#if renameState?.active && renameState.target?.index === globalIndex}
                            <div class="col-rename" class:has-git={gitColumnVisible}>
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
                            </div>
                        {:else}
                            <span
                                class="col-name"
                                use:tooltip={{
                                    text: file.redirectToPath ? `Opens ${file.redirectToPath}` : file.name,
                                    overflowOnly: !file.redirectToPath,
                                }}
                            >{getDisplayName(file.name, file.isDirectory)}{#if fileIsRestricted}<span
                                    class="restricted-indicator"
                                    aria-hidden="true"
                                    use:tooltip={RESTRICTED_FOLDER_TOOLTIP}
                                ><InfoIcon /></span>{/if}</span>
                            {#if gitColumnVisible}
                                {@const status = gitStatusFor(file)}
                                <span
                                    class="col-git"
                                    class:has-status={status !== null}
                                    aria-label={status ? labelFor(status) : ''}
                                    title={status ? labelFor(status) : ''}
                                >
                                    {status ? glyphFor(status) : ''}
                                </span>
                            {/if}
                            <span
                                class="col-ext"
                                use:useShortenMiddle={{
                                    text: getDisplayExtension(file.name, file.isDirectory),
                                    tooltipWhenTruncated: true,
                                }}
                            ></span>
                        {/if}
                        <span
                            class="col-size"
                            aria-label={sizeOverride.tooltip ?? sizeOverride.override}
                            use:tooltip={sizeOverride.override !== undefined
                                ? (sizeOverride.tooltip ?? sizeOverride.override)
                                : file.isDirectory
                                  ? buildDirSizeTooltip(
                                        file.recursiveSize,
                                        file.recursivePhysicalSize,
                                        file.recursiveFileCount ?? 0,
                                        file.recursiveDirCount ?? 0,
                                        indexing,
                                        formatFileSize,
                                        formatNumber,
                                        pluralize,
                                    )
                                  : buildFileSizeTooltip(file.size, file.physicalSize, formatFileSize)}
                        >
                            {#if sizeOverride.override !== undefined}
                                <span class="size-text">{sizeOverride.override}</span>
                            {:else if file.isDirectory}
                                {#if dirDisplaySize != null}
                                    <span class="size-text"
                                        >{#each formatSizeForDisplay(dirDisplaySize, sizeFormatOpts) as triad, i (i)}<span
                                                class={triad.tierClass}>{triad.value}</span
                                            >{/each}</span
                                    >
                                    {#if indexing}
                                        <span class="size-stale icon-indicator" use:tooltip={'Updating index: size may change.'}
                                            ><IconHourglass width="12" height="12" /></span
                                        >
                                    {/if}
                                    {#if showSizeMismatchWarning && hasSizeMismatch(file.recursiveSize, file.recursivePhysicalSize)}
                                        {@const dirTooltip = buildDirSizeTooltip(
                                            file.recursiveSize,
                                            file.recursivePhysicalSize,
                                            file.recursiveFileCount ?? 0,
                                            file.recursiveDirCount ?? 0,
                                            indexing,
                                            formatFileSize,
                                            formatNumber,
                                            pluralize,
                                        )}
                                        {@const dirTooltipHtml =
                                            typeof dirTooltip === 'object' ? dirTooltip.html : dirTooltip}
                                        <span
                                            class="size-mismatch icon-indicator"
                                            use:tooltip={{
                                                html:
                                                    'Content and on-disk sizes differ significantly.<br><br>' +
                                                    dirTooltipHtml,
                                            }}
                                        >
                                            <IconCircleAlert width="12" height="12" />
                                        </span>
                                    {/if}
                                {:else if indexing}
                                    <span class="size-scanning">Scanning...</span>
                                {:else}
                                    <span class="size-dir">&lt;dir&gt;</span>
                                {/if}
                            {:else if fileDisplaySize != null}
                                <span class="size-text"
                                    >{#each formatSizeForDisplay(fileDisplaySize, sizeFormatOpts) as triad, i (i)}<span
                                            class={triad.tierClass}>{triad.value}</span
                                        >{/each}</span
                                >
                            {/if}
                        </span>
                        <span class="col-date">
                            {#if date.parts.right !== null && columnWidths.dateLeft > 0}
                                <span class="date-left" style="width: {columnWidths.dateLeft}px"
                                    >{#each date.parts.left as seg, i (i)}{#if seg.ageClass}<span
                                                class={seg.ageClass}>{seg.text}</span
                                            >{:else}{seg.text}{/if}{/each}</span
                                ><span class="date-right"
                                    >{#each date.parts.right as seg, i (i)}{#if seg.ageClass}<span
                                                class={seg.ageClass}>{seg.text}</span
                                            >{:else}{seg.text}{/if}{/each}</span
                                >
                            {:else}
                                {#each date.parts.left as seg, i (i)}{#if seg.ageClass}<span class={seg.ageClass}
                                            >{seg.text}</span
                                        >{:else}{seg.text}{/if}{/each}
                            {/if}
                        </span>
                    </div>
                {/each}
            </div>
        </div>
        {#if (hasParent ? totalCount - 1 : totalCount) === 0}
            <div class="empty-folder-message" style="height: {containerHeight - (hasParent ? rowHeight : 0)}px;">
                Empty folder
            </div>
        {/if}
    </div>
</div>

<style>
    .full-list-container {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
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
        /* grid-template-columns set via inline style for shrink-wrapped column widths */
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-header);
        border-bottom: 1px solid var(--color-border);
        height: calc(22px * var(--font-scale));
        flex-shrink: 0;
        transition: grid-template-columns 300ms ease;
    }

    .header-icon {
        width: var(--spacing-icon-size);
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
        /* Guarantee one visual line per row regardless of cell content length */
        white-space: nowrap;
        transition: grid-template-columns 300ms ease;
    }

    /* TCC-restricted rows: italic + opacity to match the sidebar treatment.
       The (i) icon next to the name carries the tooltip pointing at System Settings. */
    .file-entry.is-restricted .col-name,
    .file-entry.is-restricted .col-size,
    .file-entry.is-restricted .col-date {
        font-style: italic;
        opacity: 0.6;
    }

    .restricted-indicator {
        display: inline-flex;
        align-items: center;
        margin-left: var(--spacing-xxs);
        opacity: 0.7;
        font-size: var(--font-size-sm);
        vertical-align: text-bottom;
    }

    .header-row.no-transition,
    .file-entry.no-transition {
        transition: none;
    }

    /* Soften the selection/cursor color flip on the cells whose color changes. */
    .file-entry .col-name,
    .file-entry .col-ext,
    .file-entry .col-size,
    .file-entry .col-date,
    .file-entry .col-git,
    .file-entry .size-dir,
    .file-entry :global(.size-bytes),
    .file-entry :global(.size-kb),
    .file-entry :global(.size-mb),
    .file-entry :global(.size-gb),
    .file-entry :global(.size-tb) {
        transition: color 50ms ease;
    }

    @media (prefers-reduced-motion: reduce) {
        .header-row,
        .file-entry,
        .file-entry .col-name,
        .file-entry .col-ext,
        .file-entry .col-size,
        .file-entry .col-date,
        .file-entry .col-git,
        .file-entry .size-dir,
        .file-entry :global(.size-bytes),
        .file-entry :global(.size-kb),
        .file-entry :global(.size-mb),
        .file-entry :global(.size-gb),
        .file-entry :global(.size-tb) {
            transition: none;
        }
    }

    /* In compact mode, use symmetric padding to match BriefList alignment */
    .full-list-container.is-compact .file-entry {
        padding-top: 0;
        padding-bottom: var(--spacing-xs);
    }

    .file-entry.is-striped {
        background-color: var(--color-bg-stripe);
    }

    /* Selected rows: darker bg (dark mode only — tokens default to
       transparent in light) overrides the stripe so the selection reads
       as a single block. Cursor rules win by specificity (see below), so
       cursor-on-selected still shows the cursor highlight. */
    .file-entry.is-selected {
        background-color: var(--color-selection-bg);
    }

    /* Faint hairline between two consecutive selected rows so dense
       selections stay countable. `box-shadow: inset` draws on top of
       `background-color` and takes zero layout space, so row height
       doesn't jump. Skipped when the row is under the cursor — cursor
       is already a strong visual signal, no need for the divider on
       top of it. */
    .file-entry.is-selected + .file-entry.is-selected:not(.is-under-cursor) {
        box-shadow: inset 0 1px 0 var(--color-selection-border);
    }

    .file-entry.is-under-cursor {
        background-color: var(--color-cursor-inactive);
    }

    .full-list-container.is-focused .file-entry.is-under-cursor {
        background-color: var(--color-cursor-active);
    }

    .col-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* During rename, span the name + ext columns for more editing room */
    .col-rename {
        grid-column: 2 / span 2;
        min-width: 0;
        height: 100%;
    }

    /* When the optional Git column is on, the editor also spans it. */
    .col-rename.has-git {
        grid-column: 2 / span 3;
    }

    .header-git {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        text-align: center;
        align-self: center;
        white-space: nowrap;
        cursor: default;
    }

    .col-git {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        text-align: center;
        color: var(--color-git-portal);
        white-space: nowrap;
        overflow: hidden;
    }

    .col-git.has-status {
        font-weight: 600;
    }

    .file-entry.is-selected .col-git {
        color: var(--color-selection-fg);
    }

    .col-ext {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .col-size {
        display: flex;
        justify-content: flex-end;
        align-items: center;
        gap: var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    /* Groups the number triads into one flex item so the right-edge alignment is
       predictable when the row also has an icon next to the number. */
    .size-text {
        display: inline;
    }

    .size-dir {
        color: var(--color-text-secondary);
    }

    .icon-indicator {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- small icon indicator, not body text */
        color: var(--color-accent);
    }

    .size-stale {
        display: inline-flex;
        align-items: center;
        cursor: help;
    }

    .size-mismatch {
        display: inline-flex;
        align-items: center;
        cursor: help;
    }

    .size-scanning {
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        white-space: nowrap;
    }

    .col-date {
        overflow: hidden;
        text-overflow: ellipsis;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    /* The age class lives on child spans. On selected or cursor-active rows,
       neutralize them so the gold / default-text rule on the parent cell
       isn't overridden by colored segments. Order matters here: the
       cursor-only rule and the selected+cursor rule have the same specificity
       count (both are .full-list-container.is-focused .file-entry.is-* …),
       so selected+cursor must come last to win when both conditions hold. */
    .file-entry.is-selected .col-date :global(.age-fresh),
    .file-entry.is-selected .col-date :global(.age-recent),
    .file-entry.is-selected .col-date :global(.age-aging),
    .file-entry.is-selected .col-date :global(.age-old) {
        color: var(--color-selection-fg);
    }
    .full-list-container.is-focused .file-entry.is-under-cursor .col-date :global(.age-fresh),
    .full-list-container.is-focused .file-entry.is-under-cursor .col-date :global(.age-recent),
    .full-list-container.is-focused .file-entry.is-under-cursor .col-date :global(.age-aging),
    .full-list-container.is-focused .file-entry.is-under-cursor .col-date :global(.age-old) {
        color: var(--color-text-primary);
    }
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-date :global(.age-fresh),
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-date :global(.age-recent),
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-date :global(.age-aging),
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-date :global(.age-old) {
        color: var(--color-selection-fg);
    }

    /* Split date cells: `.date-left` is fixed-width (set inline from the
       column-widths measurer) so the right halves align across rows. The 4px
       margin on `.date-right` is mirrored as `DATE_PARTS_GAP` in
       `measure-column-widths.ts`; keep them in sync. */
    .date-left {
        display: inline-block;
        text-align: right;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        vertical-align: bottom;
    }

    .date-right {
        margin-left: var(--spacing-xs);
    }

    .file-entry.is-selected .col-name {
        color: var(--color-selection-fg);
    }

    .file-entry.is-selected .col-ext {
        color: var(--color-selection-fg);
    }

    .file-entry.is-selected .col-date {
        color: var(--color-selection-fg);
    }

    .file-entry.is-selected .size-dir {
        color: var(--color-selection-fg);
    }

    /* Size tiers follow a gold depth progression when selected */
    .file-entry.is-selected :global(.size-bytes) {
        color: var(--color-size-bytes-selected);
    }

    .file-entry.is-selected :global(.size-kb) {
        color: var(--color-size-kb-selected);
    }

    .file-entry.is-selected :global(.size-mb) {
        color: var(--color-size-mb-selected);
    }

    .file-entry.is-selected :global(.size-gb) {
        color: var(--color-size-gb-selected);
    }

    .file-entry.is-selected :global(.size-tb) {
        color: var(--color-size-tb-selected);
    }

    /* Selection colors preserved even under cursor */
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-name {
        color: var(--color-selection-fg);
    }

    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-ext {
        color: var(--color-selection-fg);
    }

    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-date {
        color: var(--color-selection-fg);
    }

    /* The cursor highlight is app-colored, so wilting greens and browns sit
       awkwardly against it. Neutralize the date age coloring to the default
       text color while the row is under the focused cursor. The selected
       case above keeps winning by additional specificity. */
    .full-list-container.is-focused .file-entry.is-under-cursor .col-date {
        color: var(--color-text-primary);
    }

    .empty-folder-message {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: 1;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
