<script lang="ts">
    import { untrack } from 'svelte'
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from '../types'
    import { calculateVirtualWindowVariable, getScrollToPositionVariable } from './virtual-scroll'
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
        updateIndexSizesInPlace,
        type DirStats,
    } from './file-list-utils'
    import { commands } from '$lib/ipc/bindings'
    import { ensureFontMetricsLoaded, getCurrentFontId } from '$lib/font-metrics'
    import { getDirStatsBatch } from '$lib/tauri-commands'
    import { buildDirSizeTooltip, hasSizeMismatch } from './full-list-utils'
    import {
        getRowHeight,
        formatFileSize,
        getSizeMismatchWarning,
        getStripedRows,
        getBriefColumnWidthMode,
        getBriefColumnWidthMaxPx,
    } from '$lib/settings/reactive-settings.svelte'
    import { onDebouncedScaleChange } from '$lib/text-size.svelte'
    import { getSetting } from '$lib/settings/settings-store'
    import { formatNumber } from '../selection/selection-info-utils'
    import { pluralize } from '$lib/utils/pluralize'
    import { isScanning, isAggregating } from '$lib/indexing/index-state.svelte'
    import { isRestricted } from '$lib/stores/restricted-paths-store.svelte'
    import { restrictedFolderTooltip } from '$lib/system-strings.svelte'
    import InfoIcon from '~icons/lucide/info'

    const RESTRICTED_FOLDER_TOOLTIP = $derived(restrictedFolderTooltip())
    import { iconCacheCleared } from '$lib/icon-cache'
    import { escapeHtml, tooltip } from '$lib/tooltip/tooltip'
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

    // Drive index state: show spinner while scanning OR aggregating (sizes aren't ready until aggregation finishes)
    const indexing = $derived(isScanning() || isAggregating())

    // ==== Layout constants ====
    // Row height is reactive based on UI density setting
    const rowHeight = $derived(getRowHeight())
    // Buffer columns is reactive based on settings
    const bufferColumns = $derived(getSetting('advanced.virtualizationBufferColumns'))
    const MIN_COLUMN_WIDTH = 100
    // Add space for: icon (16px) + gap (8px) + left padding (8px) + right padding (8px) + rounding buffer (2px)
    // The 2px buffer accounts for sub-pixel rendering differences between calculated and actual widths.
    const COLUMN_PADDING = 16 + 8 + 8 + 8 + 2

    // ==== Container state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let containerWidth = $state(0)
    let scrollLeft = $state(0)

    // ==== Column layout calculations ====
    // Number of items that fit in one column
    const itemsPerColumn = $derived(Math.max(1, Math.floor(containerHeight / rowHeight)))

    /** Per-column widths, chrome-inclusive and clamped, returned by the backend + FE chrome/clamp pass. */
    let columnWidths = $state<number[]>([])
    /**
     * Snap the column-width CSS transition for one paint when columns appear from `[]`
     * (initial load) or the listing is reset. Matches the same trick used in `shouldResetCache`.
     */
    let skipTransition = $state(false)

    // Total number of columns needed (FE-derived; backend uses the same formula).
    const totalColumns = $derived(Math.ceil(totalCount / itemsPerColumn))

    /**
     * Cap applied to each column AFTER chrome is added.
     *
     * - 'paneWidth' mode (default): columns can grow to fill the pane.
     * - 'limited' mode: columns also can't exceed the user-chosen pixel cap.
     *
     * `containerWidth` is always the outer ceiling: a column wider than the pane has no value.
     */
    const capPx = $derived.by(() => {
        const userCap = getBriefColumnWidthMode() === 'limited' ? getBriefColumnWidthMaxPx() : Number.POSITIVE_INFINITY
        if (containerWidth <= 0) return Math.min(userCap, 1000)
        return Math.min(containerWidth, userCap)
    })

    /**
     * Running cumulative width totals: `prefixSums[i] = sum(widths[0..i))`.
     * Length is `totalColumns + 1`. Columns beyond `columnWidths.length` (in-flight or
     * post-FontMetricsNotReady fallback) use the live `capPx` so the scrollbar and
     * virtual window stay roughly accurate during the brief widths-loading window.
     */
    const prefixSums = $derived.by(() => {
        const sums = new Array<number>(totalColumns + 1)
        sums[0] = 0
        for (let i = 0; i < totalColumns; i++) {
            const w = columnWidths[i] ?? capPx
            sums[i + 1] = sums[i] + w
        }
        return sums
    })

    // ==== Virtual scrolling (horizontal) ====
    const virtualWindow = $derived(
        calculateVirtualWindowVariable(prefixSums, bufferColumns, containerWidth, scrollLeft, totalColumns),
    )

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

    // Fetch entries for the visible range.
    // `force=true` skips the "already cached" short-circuit; used when the
    // backing listing changed (file watcher diff) and the cached entries are
    // stale even though the range indices may still match.
    async function fetchVisibleRange(force = false) {
        if (!listingId || isFetching) return

        // Calculate which backend indices we need (convert column range to item range)
        const startCol = virtualWindow.startIndex
        const endCol = virtualWindow.endIndex
        const startItem = startCol * itemsPerColumn
        const endItem = Math.min(endCol * itemsPerColumn, totalCount)

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
                    entry = createParentEntry(parentPath, parentDirStats ?? undefined)
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

    // ==== Per-column widths (backend-driven) ====
    //
    // The backend computes the text-only pixel width of the widest filename in every
    // column for the current `(listingId, itemsPerColumn, hasParent, fontId, includeHidden)`.
    // The FE adds chrome + clamps to `[MIN_COLUMN_WIDTH, capPx]` and stores the resulting
    // chrome-inclusive widths in `columnWidths`. Prefix sums of `columnWidths` drive both
    // the virtual-scroll math and `scrollToIndex`.
    //
    // Race guard: every fetch captures `(listingId, generation)` and ignores stale responses.
    // Generation bumps only inside the debounced fire, not per `fetchColumnWidths()` call,
    // so a burst of triggers in 50 ms produces one IPC and one generation bump.
    let widthsGeneration = 0
    let prevItemsPerColumn = 0
    let prevCapPx = 0
    let pendingFetchTimer: ReturnType<typeof setTimeout> | null = null

    function cancelPendingFetch() {
        if (pendingFetchTimer !== null) {
            clearTimeout(pendingFetchTimer)
            pendingFetchTimer = null
        }
    }

    async function doFetchColumnWidths(retry: boolean): Promise<void> {
        widthsGeneration++
        const capturedListingId = listingId
        const capturedGeneration = widthsGeneration
        const fontId = getCurrentFontId()
        const fetchItemsPerColumn = Math.max(1, itemsPerColumn)
        try {
            const result = await commands.getBriefColumnTextWidths(
                capturedListingId,
                fetchItemsPerColumn,
                hasParent,
                fontId,
                includeHidden,
            )
            if (capturedListingId !== listingId || capturedGeneration !== widthsGeneration) return
            if (result.status === 'error') {
                if (result.error.message === 'font_metrics_not_ready' && !retry) {
                    await ensureFontMetricsLoaded()
                    if (capturedListingId !== listingId || capturedGeneration !== widthsGeneration) return
                    await doFetchColumnWidths(true)
                    return
                }
                // Bail: leave `columnWidths` untouched. Fallback (`capPx`) covers rendering
                // until the next trigger arrives.
                return
            }
            const textWidths = result.data
            const clamped = new Array<number>(textWidths.length)
            const wasEmpty = columnWidths.length === 0
            for (let i = 0; i < textWidths.length; i++) {
                const chromeInclusive = textWidths[i] + COLUMN_PADDING
                clamped[i] = Math.max(MIN_COLUMN_WIDTH, Math.min(capPx, chromeInclusive))
            }
            // First arrival ([] → non-empty): snap the CSS width transition so the columns
            // don't visibly slide from the `capPx` fallback to their measured widths.
            if (wasEmpty && clamped.length > 0) {
                skipTransition = true
                requestAnimationFrame(() => {
                    requestAnimationFrame(() => {
                        skipTransition = false
                    })
                })
            }
            columnWidths = clamped
        } catch {
            // IPC threw outside the typed-error path (timeout, missing handler). Leave widths.
        }
    }

    function fetchColumnWidths() {
        if (!listingId || itemsPerColumn <= 0) return
        cancelPendingFetch()
        // First fetch (no widths yet, for example after entering a new dir) fires
        // immediately so the cursor-hidden gap is as short as possible. Subsequent
        // re-fetches keep the 50 ms coalesce to absorb resize bursts.
        if (columnWidths.length === 0) {
            void doFetchColumnWidths(false)
            return
        }
        pendingFetchTimer = setTimeout(() => {
            pendingFetchTimer = null
            void doFetchColumnWidths(false)
        }, 50)
    }

    /** Imperative refetch, exposed to `FilePane` for the post-diff path. */
    export function refetchColumnWidths(): void {
        fetchColumnWidths()
    }

    /** Re-fetch on `itemsPerColumn` change (height resize reshuffles which files land in which column). */
    $effect(() => {
        if (itemsPerColumn !== prevItemsPerColumn) {
            prevItemsPerColumn = itemsPerColumn
            fetchColumnWidths()
        }
    })

    /** Re-fetch on `capPx` change with 4 px hysteresis (avoids scrollbar-gutter flicker). */
    $effect(() => {
        const cap = capPx
        if (Math.abs(cap - prevCapPx) >= 4) {
            prevCapPx = cap
            fetchColumnWidths()
        }
    })

    /** Re-fetch when the text scale settles (font ID changed, font metrics re-measured). */
    $effect(() => {
        return onDebouncedScaleChange(() => {
            fetchColumnWidths()
        })
    })

    /** Cancel any pending widths fetch when the component unmounts. */
    $effect(() => {
        return () => {
            cancelPendingFetch()
        }
    })

    function getColumnWidth(colIndex: number): number {
        return columnWidths[colIndex] ?? capPx
    }

    // Fetch on scroll
    function handleScroll() {
        cancelClickToRename()
        if (!scrollContainer) return
        scrollLeft = scrollContainer.scrollLeft
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
        // triggering selection/drag. The input handles its own focus.
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

    // Handle file click - for double-click detection
    let lastClickTime = 0
    let lastClickIndex = -1
    const DOUBLE_CLICK_MS = 300

    function handleClick(index: number) {
        const now = Date.now()
        if (lastClickIndex === index && now - lastClickTime < DOUBLE_CLICK_MS) {
            // Double click: cancel any pending click-to-rename
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
        if (columnIndex < 0 || columnIndex >= totalColumns) return
        const position = getScrollToPositionVariable(prefixSums, columnIndex, scrollLeft, containerWidth)
        if (position !== undefined) {
            scrollContainer.scrollLeft = position
            // Also update state directly to trigger reactive chain immediately
            // (scroll events may be batched or delayed by the browser)
            scrollLeft = position
            // Fetch entries for the new visible range
            void fetchVisibleRange()
        }
    }

    /**
     * Count of columns at least partially visible in the current scroll window.
     * Used as the PageUp/PageDown step size, content-dependent (a "page" of skinny
     * columns moves more files than a "page" of wide ones), which matches user intent.
     */
    function countVisibleColumns(): number {
        if (totalColumns === 0) return 1
        const left = scrollLeft
        const right = scrollLeft + containerWidth
        let count = 0
        for (let c = 0; c < totalColumns; c++) {
            if (prefixSums[c] < right && prefixSums[c + 1] > left) {
                count++
            }
        }
        return Math.max(1, count)
    }

    // Handle keyboard navigation.
    // `overflow` = the requested step was clamped by a list boundary; used by
    // Shift+nav to decide whether to include the landing item in the range fill.
    export function handleKeyNavigation(
        key: string,
        event?: KeyboardEvent,
    ): { newIndex: number; overflow: boolean } | undefined {
        // Try navigation shortcuts first (Home/End/PageUp/PageDown)
        if (event) {
            const visibleColumns = countVisibleColumns()
            const result = handleNavigationShortcut(event, {
                currentIndex: cursorIndex,
                totalCount,
                itemsPerColumn,
                visibleColumns,
            })
            if (result) {
                return { newIndex: result.newIndex, overflow: result.overflow }
            }
        }

        // Handle arrow keys
        if (key === 'ArrowUp') {
            const newIndex = Math.max(0, cursorIndex - 1)
            return { newIndex, overflow: newIndex === cursorIndex }
        }
        if (key === 'ArrowDown') {
            const newIndex = Math.min(totalCount - 1, cursorIndex + 1)
            return { newIndex, overflow: newIndex === cursorIndex }
        }
        if (key === 'ArrowLeft') {
            const raw = cursorIndex - itemsPerColumn
            return { newIndex: raw >= 0 ? raw : 0, overflow: raw < 0 }
        }
        if (key === 'ArrowRight') {
            const raw = cursorIndex + itemsPerColumn
            return { newIndex: raw < totalCount ? raw : totalCount - 1, overflow: raw >= totalCount }
        }
        return undefined
    }

    // Track previous values to detect actual changes
    let prevCacheProps = { listingId: '', includeHidden: false, cacheGeneration: 0 }
    let prevSoftTick = 0
    let prevTotalCount = 0

    // Hard reset on cold context changes (nav, sort, hidden toggle, explicit
    // refresh): wipe entries and widths, refetch from scratch.
    // Soft refresh on totalCount or softRefreshTick changes (caused by
    // `directory-diff` events during bulk ops, or renames that don't change
    // count): refetch in the background and atomically replace, keeping
    // existing entries and widths visible until the new ones land — no
    // empty/first-column flicker.
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
            // Drop measured widths so the new listing starts fresh. Bumping `widthsGeneration`
            // BEFORE the refetch ensures any in-flight response for the previous listing
            // is discarded by the `(listingId, generation)` guard.
            widthsGeneration++
            columnWidths = []
            skipTransition = true
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    skipTransition = false
                })
            })
            fetchColumnWidths()
            void fetchVisibleRange()
            return
        }

        if (currentTotal !== prevTotalCount || currentTick !== prevSoftTick) {
            prevTotalCount = currentTotal
            prevSoftTick = currentTick
            // `force=true` bypasses the cached-range short-circuit so stale
            // entries within an unchanged range get replaced. Column widths
            // are refreshed by FilePane's throttled `refetchColumnWidths`
            // call, not here, so a 10 k-file delete doesn't fire one width
            // IPC per coalesced event.
            void fetchVisibleRange(true)
            return
        }

        void fetchVisibleRange()
    })

    /**
     * Single "keep cursor in view" effect. Replaces the older height-only effect and
     * the implicit reliance on FilePane calling `scrollToIndex` on cursor moves.
     * width resize (drag pane resizer, window narrow) now also retriggers naturally.
     * Reads `columnWidths.length` to depend on the widths-arrival reassignment.
     *
     * `scrollToIndex` is wrapped in `untrack` because its body reads `scrollLeft` (and
     * other reactive state) which would otherwise turn user-initiated scrollbar drags
     * into a 60 Hz tug-of-war: the drag would move `scrollLeft`, refire this effect,
     * which would snap back to the cursor, repeat. Only the explicit `void X`
     * dependencies above should trigger this effect.
     */
    $effect(() => {
        void cursorIndex
        void containerWidth
        void containerHeight
        void columnWidths.length
        if (containerHeight > 0 && containerWidth > 0) {
            untrack(() => {
                scrollToIndex(cursorIndex)
            })
        }
    })

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
        void cacheGeneration
        void getDirStatsBatch([currentPath])
            .then((results) => {
                parentDirStats = results[0] ?? null
            })
            .catch(() => {
                // Silently ignore -- indexing may not be initialized yet.
            })
    })

    // Size mismatch warning setting
    const showSizeMismatchWarning = $derived(getSizeMismatchWarning())

    // Striped rows setting
    const stripedRows = $derived(getStripedRows())

    /** Build tooltip for a directory entry showing recursive size info. */
    function buildDirTooltip(file: FileEntry): string | { html: string } | undefined {
        if (!file.isDirectory) return undefined
        const base = buildDirSizeTooltip(
            file.recursiveSize,
            file.recursivePhysicalSize,
            file.recursiveFileCount ?? 0,
            file.recursiveDirCount ?? 0,
            indexing,
            formatFileSize,
            formatNumber,
            pluralize,
        )
        if (!base) return undefined

        // Prepend mismatch warning when applicable
        if (showSizeMismatchWarning && hasSizeMismatch(file.recursiveSize, file.recursivePhysicalSize)) {
            const baseHtml = typeof base === 'object' ? base.html : base
            return { html: 'Content and on-disk sizes differ significantly.<br><br>' + baseHtml }
        }

        return base
    }

    /** Build tooltip for the filename span: shows name when overflowing; for dirs, combines name with dir-size info. */
    function buildNameTooltip(file: FileEntry): { text?: string; html?: string; overflowOnly: true } {
        if (!file.isDirectory) {
            return { text: file.name, overflowOnly: true }
        }
        const dirTip = buildDirTooltip(file)
        if (!dirTip) {
            return { text: file.name, overflowOnly: true }
        }
        const dirHtml = typeof dirTip === 'object' ? dirTip.html : escapeHtml(dirTip)
        return { html: `${escapeHtml(file.name)}<br><br>${dirHtml}`, overflowOnly: true }
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
    <div class="header-row" role="toolbar" aria-label="Sort columns">
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
        aria-label="File list"
        aria-activedescendant={cursorIndex >= 0 ? `file-${String(cursorIndex)}` : undefined}
    >
        <!-- Spacer div provides accurate scrollbar for full list width -->
        <div class="virtual-spacer" style="width: {virtualWindow.totalSize}px; height: 100%;">
            <!-- Visible window positioned with translateX -->
            <div class="virtual-window" style="transform: translateX({virtualWindow.offset}px);">
                {#each visibleColumns as column (column.columnIndex)}
                    <div
                        class="column"
                        class:no-transition={skipTransition}
                        style="width: {getColumnWidth(column.columnIndex)}px;"
                    >
                        {#each column.files as { file, globalIndex } (file.path)}
                            {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                            {@const fileIsRestricted = isRestricted(file.path)}
                            <!-- svelte-ignore a11y_click_events_have_key_events,a11y_interactive_supports_focus -->
                            <div
                                id={`file-${String(globalIndex)}`}
                                class="file-entry"
                                class:is-under-cursor={globalIndex === cursorIndex && columnWidths.length > 0}
                                class:is-selected={selectedIndices.has(globalIndex)}
                                class:is-striped={stripedRows && globalIndex % 2 === 1}
                                class:is-restricted={fileIsRestricted}
                                data-filename={file.name}
                                data-drop-target-path={file.isDirectory ? file.path : undefined}
                                use:tooltip={buildDirTooltip(file)}
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
                                    <span class="name" use:tooltip={buildNameTooltip(file)}
                                        >{file.name}{#if fileIsRestricted}<span
                                                class="restricted-indicator"
                                                aria-hidden="true"
                                                use:tooltip={RESTRICTED_FOLDER_TOOLTIP}
                                            ><InfoIcon /></span>{/if}</span>
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
        flex: 1;
        min-height: 0;
        width: 100%;
        position: relative;
    }

    .header-row {
        display: flex;
        height: calc(22px * var(--font-scale));
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
        transition: width 300ms ease;
    }

    .column.no-transition {
        transition: none;
    }

    @media (prefers-reduced-motion: reduce) {
        .column {
            transition: none;
        }
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

    /* TCC-restricted rows: italic + opacity to match the sidebar treatment.
       The (i) icon next to the name carries the tooltip pointing at System Settings. */
    .file-entry.is-restricted .name {
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

    .file-entry.is-striped {
        background-color: var(--color-bg-stripe);
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
        /* Soften the selection color flip. */
        transition: color 50ms ease;
    }

    @media (prefers-reduced-motion: reduce) {
        .name {
            transition: none;
        }
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
        top: calc(22px * var(--font-scale)); /* Below the header row */
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
