<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import type { FileEntry, SortColumn, SortOrder, SyncStatus } from '../types'
    import type { FileIndexState, FolderCoverage } from '$lib/tauri-commands'
    import { calculateVirtualWindow, getScrollToPosition } from './virtual-scroll'
    import { startSelectionDragTracking } from '../drag/drag-drop'
    import {
        computeDragAutoScrollStep,
        type DragAutoScrollFrameResult,
        type DragAutoScrollPointer,
    } from '../drag/drag-auto-scroll'
    import { startClickToRename, cancelClickToRename } from '../rename/rename-activation'
    import FullListHeader from './FullListHeader.svelte'
    import FileIcon from '../selection/FileIcon.svelte'
    import TagDots from '../selection/TagDots.svelte'
    import InlineRenameEditor from '../rename/InlineRenameEditor.svelte'
    import { shouldMountRenameEditor } from '../rename/rename-mount'
    import { glyphFor, labelFor } from '../git/status-column'
    import { getSyncIconPath, getImageIndexBadge, getFolderCoverageBadge } from './file-list-utils'
    import { createFullListCache } from './full-list-cache.svelte'
    import { createGitStatusColumn } from './full-list-git-column.svelte'
    import { planRowMouseDown } from './full-list-mouse'
    import { formatSizeForDisplay, formatNumber } from '../selection/selection-info-utils'
    import { isVolumeAggregating, getWalkedGround } from '$lib/indexing/index-state.svelte'
    import { isPathAffectedByWalk } from '$lib/indexing/walked-ground'
    import { isRestricted } from '$lib/stores/restricted-paths-store.svelte'
    import { restrictedFolderTooltip } from '$lib/system-strings.svelte'
    const RESTRICTED_FOLDER_TOOLTIP = $derived(restrictedFolderTooltip())
    import {
        getVisibleItemsCount as getVisibleItemsCountUtil,
        getVirtualizationBufferRows,
        buildDirSizeTooltip,
        buildFileSizeTooltip,
        getDisplaySize,
        getDirSizeDisplayState,
        isDirSizeUpdating,
        LOWER_BOUND_GLYPH,
        hasSizeMismatch,
        getDisplayExtension,
        getNameColumnText,
        pickSizeDisplay,
    } from './full-list-utils'
    import { computeFullListColumnWidths } from './measure-column-widths'
    import {
        getRowHeight,
        getIconSize,
        getIsCompactDensity,
        formattedDate,
        getSizeDisplayMode,
        getSizeMismatchWarning,
        getStripedRows,
        getFileSizeUnit,
        getFileSizeFormat,
        getShowExtensionInName,
        getShowTags,
    } from '$lib/settings/reactive-settings.svelte'
    import { iconCacheCleared } from '$lib/icon-cache'
    import { onDebouncedScaleChange, getEffectiveScale } from '$lib/text-size.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import type { RenameState, RenameSessionId } from '../rename/rename-state.svelte'
    import type { RenameStepDirection } from '../rename/rename-step'
    import { formatByteSize } from '$lib/units'

    interface Props {
        listingId: string
        /** Volume id of the host pane. Recorded into the self-drag identity at
         *  drag start so an in-app drop builds its transfer from the source
         *  volume + the paths the volume knows, never the lossy pasteboard
         *  round-trip. `'root'` for a local pane. */
        volumeId: string
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
        indexStatusMap?: Record<string, FileIndexState>
        /** Per-folder image-index coverage, keyed by folder path (folder-icon overlay). */
        folderCoverageMap?: Record<string, FolderCoverage>
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
        onIndexStatusRequest?: (paths: string[]) => void
        onFolderCoverageRequest?: (folderPaths: string[]) => void
        onSortChange?: (column: SortColumn) => void
        onVisibleRangeChange?: (start: number, end: number) => void
        /** Called when rename input value changes */
        onRenameInput?: (value: string) => void
        /** Called when rename is submitted (Enter) */
        onRenameSubmit?: () => void
        /** Called when rename is cancelled */
        onRenameCancel?: (sessionId: RenameSessionId) => void
        /** Called when the user clicks outside the rename editor (which saves) */
        onRenameClickAway?: () => void
        /** Called on a bare arrow in the editor: chain the rename to the row above or below */
        onRenameStep?: (direction: RenameStepDirection, sessionId: RenameSessionId) => void
        /** Called when shake animation ends */
        onRenameShakeEnd?: () => void
        /** Called when click-to-rename timer fires (user held click on cursor entry) */
        onStartRename?: () => void
        /** Called when a drag actually initiates (threshold crossed) from this view. */
        onDragInitiate?: () => void
        /**
         * Static, frontend-owned entries to render instead of fetching from the
         * backend `LISTING_CACHE` by `listingId`. Used by the search-results
         * virtual volume (which has no backing backend listing, just an
         * in-memory snapshot). When set, `listingId` is ignored, no IPC calls
         * are made for cached fetches, soft-refresh / cache-generation are
         * inert, and `totalCount` is derived from the array length. The host
         * pane is responsible for forcing a re-render when the array changes
         * (Svelte tracks the prop reference for that). Normal panes leave this
         * unset — the listing-cache path remains the default.
         */
        staticEntries?: FileEntry[]
    }

    const {
        listingId,
        volumeId,
        totalCount,
        includeHidden,
        cacheGeneration = 0,
        softRefreshTick = 0,
        cursorIndex,
        isFocused = true,
        syncStatusMap = {},
        indexStatusMap = {},
        folderCoverageMap = {},
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
        onIndexStatusRequest,
        onFolderCoverageRequest,
        onSortChange,
        onVisibleRangeChange,
        onRenameInput,
        onRenameSubmit,
        onRenameCancel,
        onRenameClickAway,
        onRenameStep,
        onRenameShakeEnd,
        onStartRename,
        onDragInitiate,
        staticEntries,
    }: Props = $props()

    /**
     * True when the host pane has supplied a static entries array (search-results
     * virtual volume). In that branch the backend listing cache is bypassed
     * entirely; see `full-list-cache.svelte.ts` for what goes inert.
     */
    const usingStaticEntries = $derived(staticEntries !== undefined)

    /**
     * The prefetch buffer: which entries are in hand, and every rule for keeping
     * them in step with the backend listing (`full-list-cache.svelte.ts`). Props
     * cross as one getter EACH, so each of the effects below subscribes to exactly
     * the props that method reads and nothing more.
     */
    const cache = createFullListCache({
        listingId: () => listingId,
        volumeId: () => volumeId,
        totalCount: () => totalCount,
        includeHidden: () => includeHidden,
        hasParent: () => hasParent,
        parentPath: () => parentPath,
        currentPath: () => currentPath,
        cacheGeneration: () => cacheGeneration,
        softRefreshTick: () => softRefreshTick,
        staticEntries: () => staticEntries,
        onSyncStatusRequest: () => onSyncStatusRequest,
        onIndexStatusRequest: () => onIndexStatusRequest,
        onFolderCoverageRequest: () => onFolderCoverageRequest,
    })

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

    // When on, the Name column shows the full filename and the Ext column (header
    // included) is hidden. When off (default), Name and Ext split the filename.
    const showExtensionInName = $derived(getShowExtensionInName())

    // When on (default), colored Finder-tag dots render at the right edge of the
    // Name cell. Gates both the render and the `enrich_tags` visible-range pass.
    const showTags = $derived(getShowTags())

    // Size column rendering: user-picked unit (dynamic / bytes / kB / MB / GB) × binary/SI base.
    const sizeFormatOpts = $derived({
        unit: getFileSizeUnit(),
        format: getFileSizeFormat(),
    })

    // What this pane's drive is having walked right now, plus its aggregation.
    // Both are per-volume, so another drive's work never lights up these rows.
    const walkedGround = $derived(getWalkedGround(volumeId))
    const aggregating = $derived(isVolumeAggregating(volumeId))

    /**
     * Whether THIS row's folder size is in flux. The one answer the size cell and
     * the column measurer both read, so the glyph that gets drawn and the width
     * reserved for it can't disagree.
     *
     * The walk test is bidirectional (`isPathAffectedByWalk`): the roll-up
     * repairs the ancestor chain, so a walk below a row moves that row's size too.
     */
    const isSizeUpdating = $derived((file: FileEntry) =>
        isDirSizeUpdating(
            aggregating || isPathAffectedByWalk(walkedGround, file.path),
            file.recursiveSizePending ?? false,
        ),
    )

    // Column widths are declared after the virtual window, which gates parent-row inclusion.
    let columnWidths = $state({ ext: 60, size: 115, date: 80 })
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

    /** The Git column's status map + per-row lookup (`full-list-git-column.svelte.ts`). */
    const gitColumn = createGitStatusColumn()
    $effect(() => {
        // Tracking `cacheGeneration` makes an explicit refresh reload the map.
        void cacheGeneration
        return gitColumn.watch(gitColumnVisible ? gitRepoRoot : null, currentPath)
    })

    /**
     * Single-glyph cell width. The header reads "Git" (3 chars at 12px ≈ 18px);
     * floor at 24px so the column doesn't collapse below the glyph + a hair
     * of breathing room.
     */
    const GIT_COLUMN_WIDTH = 28

    const gridTemplate = $derived.by(() => {
        const icon = `${String(iconColWidth)}px`
        // The Ext column is dropped entirely when its content rides in the Name
        // column (`showExtensionInName`); the measurer returns `ext: 0` to match.
        const ext = showExtensionInName ? '' : `${String(columnWidths.ext)}px `
        const size = `${String(columnWidths.size)}px`
        const date = `${String(columnWidths.date)}px`
        const git = gitColumnVisible ? `${String(GIT_COLUMN_WIDTH)}px ` : ''
        return `${icon} 1fr ${git}${ext}${size} ${date}`
    })

    // ==== Virtual scrolling state ====
    let scrollContainer: HTMLDivElement | undefined = $state()
    // The header is a sibling ABOVE the scroll container, so the scroller's own
    // height already IS the row area, and `scrollTop` maps to the spacer's offset
    // with no header translation (only the gutter correction below). ❌ Don't
    // reintroduce a shift-then-clamp-at-0 model: it collapses a band of `scrollTop`
    // values to one spacer state and hides row 0 (the `..` cursor). Pinned by
    // `test/e2e-playwright/full-cursor-page-nav.spec.ts`.
    let rowAreaHeight = $state(0)
    let scrollTop = $state(0)
    /**
     * How much width the vertical scrollbar takes from the scroller's content box:
     * 0 for macOS overlay scrollbars, ~15px under "Always show scroll bars", and 0
     * again whenever the listing is short enough not to overflow. The header sits
     * outside the scroll container, so it has to re-add exactly this to its right
     * padding to keep its columns over the rows'. Both bindings are
     * ResizeObserver-backed, so a window resize, a font-scale change, a listing that
     * stops overflowing, or the user flipping that System Setting all re-measure on
     * their own — nothing here needs a manual trigger.
     */
    let listClientWidth = $state(0)
    let listOffsetWidth = $state(0)
    const scrollbarWidth = $derived(Math.max(0, listOffsetWidth - listClientWidth))
    /**
     * `.listbox-region`'s gutter sits ABOVE the spacer in the scroll content, so the
     * container's `scrollTop` runs `GUTTER_PX` ahead of the spacer's own offset. Every
     * conversion between the two goes through here and `scrollToIndex` below; keep the
     * constant in sync with `.listbox-region`'s `padding-block`.
     */
    const GUTTER_PX = 4
    const spacerScrollTop = $derived(Math.max(0, scrollTop - GUTTER_PX))

    // ==== Virtual scrolling derived calculations ====
    const virtualWindow = $derived(
        calculateVirtualWindow({
            direction: 'vertical',
            itemSize: rowHeight,
            bufferSize,
            containerSize: rowAreaHeight,
            scrollOffset: spacerScrollTop,
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
    const firstVisibleGlobalIndex = $derived(rowHeight > 0 ? Math.floor(spacerScrollTop / rowHeight) : 0)
    const lastVisibleGlobalIndex = $derived(
        rowHeight > 0 && rowAreaHeight > 0
            ? Math.min(totalCount - 1, Math.floor((spacerScrollTop + rowAreaHeight - 1) / rowHeight))
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

        const entries = cache.entries
        const range = cache.range
        const visible: FileEntry[] = []
        for (let i = firstBackend; i <= lastBackend; i++) {
            if (i >= range.start && i < range.end) {
                visible.push(entries[i - range.start])
            }
        }

        const parentStats = isParentRowVisible ? cache.parentDirStats : null
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
            isSizeUpdating,
            showSizeMismatchWarning,
            sortBy,
            sizeFormatOpts,
            isRestricted,
            showExtensionInName,
        })
    })

    /** The entry at a UI index (`..` included). Called by the host pane. */
    export function getEntryAt(globalIndex: number): FileEntry | undefined {
        return cache.getEntryAt(globalIndex)
    }

    /** Updates index size fields on cached directory entries AND on the ".." row. */
    export function refreshIndexSizes(): void {
        cache.refreshIndexSizes()
    }

    /** Fetches the entries the current virtual window needs. */
    function fetchVisibleRange(force = false): void {
        void cache.fetch(virtualWindow.startIndex, virtualWindow.endIndex, force)
    }

    const visibleFiles = $derived.by(() => cache.windowRows(virtualWindow.startIndex, virtualWindow.endIndex))

    /**
     * `aria-activedescendant` may only name a row that's actually in the DOM.
     * The cursor can point at nothing rendered: an empty folder, or a row
     * scrolled outside the virtual window. A dangling reference isn't harmless
     * there, it's a critical a11y violation (axe `aria-valid-attr-value`) and
     * leaves screen readers announcing a stale row.
     */
    const activeDescendantId = $derived(
        visibleFiles.some((entry) => entry.globalIndex === cursorIndex) ? `file-${String(cursorIndex)}` : undefined,
    )

    function handleScroll(e: Event) {
        cancelClickToRename()
        const target = e.target as HTMLDivElement
        scrollTop = target.scrollTop
        fetchVisibleRange()
    }

    // Selects and initiates drag tracking. `planRowMouseDown` (full-list-mouse.ts)
    // owns the decision and the drag payload; this only performs it.
    function handleMouseDown(event: MouseEvent, index: number) {
        const plan = planRowMouseDown({
            event,
            index,
            cursorIndex,
            selectedIndices,
            getEntryAt: cache.getEntryAt,
            listingId,
            volumeId,
            includeHidden,
            hasParent,
            usingStaticEntries,
            isRenaming: renameState?.active ?? false,
            canStartRename: onStartRename !== undefined,
        })

        if (plan.kind === 'ignore') return
        if (plan.kind === 'select') {
            onSelect(index, event.shiftKey, event.metaKey)
            return
        }

        if (plan.startClickToRename && onStartRename) startClickToRename(event, onStartRename)
        else cancelClickToRename()

        if (plan.selectNow) {
            // Shift+click ranges, Cmd+click toggles; the drag then carries the
            // whole selection regardless of which row was pressed.
            onSelect(index, event.shiftKey, event.metaKey)
            startSelectionDragTracking(event, plan.context, { onDragInitiate })
            return
        }

        // No selection yet: hold the selection back until the drag threshold
        // decides whether this was a drag or a plain click.
        startSelectionDragTracking(event, plan.context, {
            onDragStart: () => {
                onSelect(index, event.shiftKey, event.metaKey)
            },
            onDragCancel: () => {
                onSelect(index, event.shiftKey, event.metaKey)
            },
            onDragInitiate,
        })
    }

    function handleDoubleClick(actualIndex: number) {
        cancelClickToRename()
        const entry = cache.getEntryAt(actualIndex)
        if (entry) onNavigate(entry)
    }

    // Exported for parent to call when arrow keys change cursor position
    export function scrollToIndex(index: number) {
        if (!scrollContainer) return
        // `getScrollToPosition` returns the spacer's required scroll offset in
        // row-area coords; the container's scrollTop is that plus the gutter (see
        // `spacerScrollTop` above).
        const spacerPos = getScrollToPosition(index, rowHeight, spacerScrollTop, rowAreaHeight)
        if (spacerPos !== undefined) {
            // Back to container coords. `0` stays `0` so scrolling to the first row shows
            // the top gutter instead of scrolling past it.
            const newScrollTop = spacerPos <= 0 ? 0 : spacerPos + GUTTER_PX
            scrollContainer.scrollTop = newScrollTop
            // Also update state directly to trigger reactive chain immediately
            // (scroll events may be batched or delayed by the browser)
            scrollTop = newScrollTop
            // Fetch entries for the new visible range
            fetchVisibleRange()
        }
    }

    export function autoScrollDuringDrag(
        position: DragAutoScrollPointer,
        elapsedMs: number,
    ): DragAutoScrollFrameResult {
        if (!scrollContainer) return { active: false, scrolled: false }
        const step = computeDragAutoScrollStep({
            axis: 'vertical',
            pointer: position,
            rect: scrollContainer.getBoundingClientRect(),
            scrollOffset: scrollContainer.scrollTop,
            maxScrollOffset: Math.max(0, scrollContainer.scrollHeight - scrollContainer.clientHeight),
            elapsedMs,
        })
        if (step.scrolled) {
            scrollContainer.scrollTop = step.nextScrollOffset
            scrollTop = step.nextScrollOffset
            fetchVisibleRange()
        }
        return { active: step.active, scrolled: step.scrolled }
    }

    // Static-entries sync: mirror the host pane's array into the cache so the same
    // rendering pipeline downstream works without backend round-trips.
    $effect(() => {
        cache.syncStaticEntries()
    })

    // Hard reset on cold context changes, soft refresh on diff bursts; the cache
    // owns the decision. A reset suppresses the grid-template-columns transition
    // for the first paint after a dir switch, else the header (which persists
    // across navs) slides from the previous dir's widths to the new ones.
    $effect(() => {
        const sync = cache.syncToProps(rowAreaHeight > 0)
        if (sync === 'idle') return
        if (sync === 'reset') {
            skipTransition = true
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    skipTransition = false
                })
            })
        }
        fetchVisibleRange(sync === 'refresh')
    })

    // Returns the number of visible items (for Page Up/Down navigation)
    export function getVisibleItemsCount(): number {
        return getVisibleItemsCountUtil(rowAreaHeight, rowHeight)
    }

    // Re-fetch icons when the icon cache is cleared (settings or theme change)
    $effect(() => {
        void $iconCacheCleared // Track the store value
        cache.refetchIcons()
    })

    // Fetch the current folder's recursive stats so the ".." row can show the total.
    // Re-runs when the directory changes; cleared when we're at a volume root.
    $effect(() => {
        // Re-run when cacheGeneration bumps (sort, refresh). `currentPath`,
        // `hasParent`, and `staticEntries` are subscribed inside the call.
        void cacheGeneration
        cache.syncParentDirStats()
    })

    // Report visible range to parent for MCP state sync
    $effect(() => {
        const startItem = virtualWindow.startIndex
        const endItem = virtualWindow.endIndex
        onVisibleRangeChange?.(startItem, endItem)
    })
</script>

<div class="full-list-container" class:is-focused={isFocused} class:is-compact={isCompact}>
    <!-- The column header sits ABOVE the scroll container, so the pane's vertical
         scrollbar starts below it the way a native macOS list does. The price is that
         the header no longer shares the scroller's content box, so it pays the
         scrollbar's width itself via `scrollbarWidth` (see above) to keep its columns
         over the rows'. ❌ Don't move it back inside to "simplify": that's what put
         the scrollbar alongside the column labels. -->
    <FullListHeader
        {gridTemplate}
        {isFocused}
        {sortBy}
        {sortOrder}
        {showExtensionInName}
        {gitColumnVisible}
        {skipTransition}
        {scrollbarWidth}
        {onSortChange}
    />
    <!-- Scrollable file list. `role="listbox"` lives on the inner rows wrapper
         because a listbox's children must be options/groups, and the scroller also
         holds the "empty folder" message. -->
    <div
        class="full-list"
        data-file-list-surface
        bind:this={scrollContainer}
        bind:clientHeight={rowAreaHeight}
        bind:clientWidth={listClientWidth}
        bind:offsetWidth={listOffsetWidth}
        onscroll={handleScroll}
        tabindex="-1"
    >
        <div
            class="listbox-region"
            role="listbox"
            aria-label={tString('fileExplorer.list.fileListAriaLabel')}
            aria-activedescendant={activeDescendantId}
            tabindex="-1"
        >
        <!-- Spacer div provides accurate scrollbar for full list size -->
        <div class="virtual-spacer" style="height: {virtualWindow.totalSize}px;">
            <!-- Visible window positioned with translateY -->
            <div class="virtual-window" style="transform: translateY({virtualWindow.offset}px);">
                {#each visibleFiles as { file, globalIndex } (file.path)}
                    {@const syncIcon = getSyncIconPath(syncStatusMap[file.path])}
                    {@const imageIndexBadge = file.isDirectory
                        ? getFolderCoverageBadge(folderCoverageMap[file.path], tString)
                        : getImageIndexBadge(indexStatusMap[file.path])}
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
                        <FileIcon {file} {syncIcon} {imageIndexBadge} />
                        {#if renameState?.active && shouldMountRenameEditor(renameState.target, { path: file.path })}
                            <div
                                class="col-rename"
                                class:has-git={gitColumnVisible}
                                class:no-ext-col={showExtensionInName}
                            >
                                <InlineRenameEditor
                                    value={renameState.currentName}
                                    severity={renameState.validation.severity}
                                    shaking={renameState.shaking}
                                    ariaLabel={`Rename ${renameState.target?.originalName ?? ''}`}
                                    ariaInvalid={renameState.validation.severity === 'error'}
                                    validationMessage={renameState.validation.message}
                                    focusTrigger={renameState.focusTrigger}
                                    sessionId={renameState.sessionId}
                                    onInput={(v: string) => onRenameInput?.(v)}
                                    onSubmit={() => onRenameSubmit?.()}
                                    onCancel={(sessionId: RenameSessionId) => onRenameCancel?.(sessionId)}
                                    onClickAway={() => onRenameClickAway?.()}
                                    onStep={(direction: RenameStepDirection, sessionId: RenameSessionId) =>
                                        onRenameStep?.(direction, sessionId)}
                                    onShakeEnd={() => onRenameShakeEnd?.()}
                                />
                            </div>
                        {:else}
                            <span class="col-name">
                                <span
                                    class="col-name-text"
                                    use:useShortenMiddle={{
                                        text: getNameColumnText(file.name, file.isDirectory, showExtensionInName),
                                        preferBreakAt: file.name.includes('/') ? '/' : '.',
                                        startRatio: 0.7,
                                        tooltipWhenTruncated: true,
                                    }}
                                ></span>{#if fileIsRestricted}<span
                                    class="restricted-indicator"
                                    aria-hidden="true"
                                    use:tooltip={RESTRICTED_FOLDER_TOOLTIP}
                                ><Icon name="info" size={12} /></span>{/if}{#if showTags}<TagDots tags={file.tags} />{/if}</span>
                            {#if gitColumnVisible}
                                {@const status = gitColumn.statusFor(file)}
                                <span
                                    class="col-git"
                                    class:has-status={status !== null}
                                    aria-label={status ? labelFor(status) : ''}
                                    title={status ? labelFor(status) : ''}
                                >
                                    {status ? glyphFor(status) : ''}
                                </span>
                            {/if}
                            {#if !showExtensionInName}
                                <span
                                    class="col-ext"
                                    use:useShortenMiddle={{
                                        text: getDisplayExtension(file.name, file.isDirectory),
                                        tooltipWhenTruncated: true,
                                    }}
                                ></span>
                            {/if}
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
                                        isSizeUpdating(file),
                                        formatByteSize,
                                        formatNumber,
                                        file.recursiveSizeComplete,
                                        file.recursiveSizeStale,
                                    )
                                  : buildFileSizeTooltip(file.size, file.physicalSize, formatByteSize)}
                        >
                            {#if sizeOverride.override !== undefined}
                                <span class="size-text">{sizeOverride.override}</span>
                            {:else if file.isDirectory}
                                {@const dirUpdating = isSizeUpdating(file)}
                                {@const dirSizeState = getDirSizeDisplayState(
                                    dirDisplaySize,
                                    file.recursiveSizeComplete,
                                    file.recursiveSizeStale,
                                    dirUpdating,
                                )}
                                {#if dirSizeState === 'dir' || dirSizeState === 'scanning'}
                                    <!-- Size unknown (not enriched yet, OR an incomplete subtree with
                                         nothing known below): the familiar `<dir>` placeholder, never a
                                         settled-looking value. Distinct from a genuinely-empty `0 bytes`.
                                         `'scanning'` adds the size-updating hourglass on top. -->
                                    <span class="size-dir">{tString('fileExplorer.dirSize.dirPlaceholder')}</span>
                                    {#if dirSizeState === 'scanning'}
                                        <span
                                            class="size-updating icon-indicator"
                                            role="img"
                                            aria-label={tString('fileExplorer.selectionInfo.sizeNotReadyAriaLabel')}
                                            use:tooltip={tString('fileExplorer.dirSize.scanProgressTooltip')}
                                            ><Icon name="hourglass" size={12} /></span
                                        >
                                    {/if}
                                {:else if dirDisplaySize != null}
                                    <span class="size-text"
                                        >{#if dirSizeState === 'lower-bound'}<span class="size-lower-bound-prefix">{LOWER_BOUND_GLYPH}</span
                                            >{/if}{#each formatSizeForDisplay(dirDisplaySize, sizeFormatOpts) as triad, i (i)}<span
                                                class={triad.tierClass}>{triad.value}</span
                                            >{/each}</span
                                    >
                                    {#if dirUpdating}
                                        <span class="size-updating icon-indicator" use:tooltip={tString('fileExplorer.dirSize.updatingIndexTooltip')}
                                            ><Icon name="hourglass" size={12} /></span
                                        >
                                    {/if}
                                    {#if showSizeMismatchWarning && hasSizeMismatch(file.recursiveSize, file.recursivePhysicalSize)}
                                        {@const dirTooltip = buildDirSizeTooltip(
                                            file.recursiveSize,
                                            file.recursivePhysicalSize,
                                            file.recursiveFileCount ?? 0,
                                            file.recursiveDirCount ?? 0,
                                            dirUpdating,
                                            formatByteSize,
                                            formatNumber,
                                            file.recursiveSizeComplete,
                                            file.recursiveSizeStale,
                                        )}
                                        {@const dirTooltipHtml =
                                            typeof dirTooltip === 'object' ? dirTooltip.html : dirTooltip}
                                        <span
                                            class="size-mismatch icon-indicator"
                                            use:tooltip={{
                                                html:
                                                    tString('fileExplorer.dirSize.mismatchTooltipPrefix') + '<br><br>' + dirTooltipHtml,
                                            }}
                                        >
                                            <Icon name="circle-alert" size={12} />
                                        </span>
                                    {/if}
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
                            {#each date.segments as seg, i (i)}{#if seg.ageClass}<span class={seg.ageClass}
                                        >{seg.text}</span
                                    >{:else}{seg.text}{/if}{/each}
                        </span>
                    </div>
                {/each}
            </div>
        </div>
        </div>
        <!-- Sibling of the listbox, not a child: the empty-state text is not an
             option, and a listbox holding a non-option child is an
             aria-required-children violation. An EMPTY listbox is fine. -->
        {#if (hasParent ? totalCount - 1 : totalCount) === 0}
            <div class="empty-folder-message">{tString('fileExplorer.list.empty')}</div>
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
        line-height: var(--font-line-height-flat);
        flex: 1;
        outline: none;
    }

    .virtual-spacer {
        position: relative;
    }

    /* Semantic wrapper for the listbox role: no background, no stacking context. It
       exists so the role + aria-activedescendant can sit on a child of the scroll
       container without violating aria-required-children (the "empty folder" message
       is a sibling, not a child, for the same reason). The pane bg lives on
       `.file-pane > .content` (see FilePane.svelte).

       The gutter runs on all four sides, keeping the cursor and selection fills off the
       pane edges. It sits on the ROWS region rather than on `.full-list` itself, whose
       `clientHeight` IS the row-area height the virtual-scroll math runs on: padding
       there would silently inflate it. `.header-row` doubles its own horizontal padding
       to match this gutter, so its labels line up with the rows while its background
       runs edge to edge. The block padding shifts the spacer inside the scroll content,
       which `spacerScrollTop` / `scrollToIndex` correct for. */
    .listbox-region {
        outline: none;
        padding: var(--spacing-xs);
    }

    .empty-folder-message {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: 1;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    /* No `will-change: transform` on `.virtual-window`: it force-promoted a
       permanent GPU layer that WebKit kept re-backing on every scroll/content
       change, ballooning compositor (IOAccelerator) memory to 1+ GB under heavy
       re-render. The translateY scroll still composites on demand. */

    .file-entry {
        display: grid;
        /* height and grid-template-columns set via inline style for reactivity */
        padding: var(--spacing-xxs) var(--spacing-sm);
        gap: var(--spacing-sm);
        align-items: center;
        /* Rows are transparent. The pane's base translucent layer lives on
           `.file-pane > .content` (see FilePane.svelte) — painting it once
           there is the single-source-of-truth approach: every pane pixel
           gets exactly one base layer, never zero (no flicker on state
           swap) and never two (no double-paint). Highlights (selection,
           cursor) keep their own bgs and sit on top as intentional tints. */
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

    /* `.restricted-indicator`'s own chrome, the stripe / selection / cursor
       fills, and the selected-row hairline are identical in `BriefList`, so
       they live in `src/app-file-list.css`. */

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

    /* The `--color-selection-fg` swap for a cursor-on-selected row lives in
       `src/app-file-list.css`; the size-tier collapse below is FullList-only
       (BriefList has no size column), so it stays here on the same selector. */
    .file-entry.is-selected.is-under-cursor {
        /* Size tiers are otherwise computed as `color-mix(secondary, selection-fg)`,
           so even with `--color-selection-fg` swapped to the cursor variant the
           size triads keep a grayer cast than the name/date. Collapse the
           gradient on the cursor row so every column reads as the same lighter
           red. */
        --color-size-bytes-selected: var(--color-selection-fg);
        --color-size-kb-selected: var(--color-selection-fg);
        --color-size-mb-selected: var(--color-selection-fg);
        --color-size-gb-selected: var(--color-selection-fg);
        --color-size-tb-selected: var(--color-selection-fg);
    }

    .col-name {
        display: inline-flex;
        align-items: center;
        overflow: hidden;
        white-space: nowrap;
        min-width: 0;
    }

    /* The truncating inner span lives inside `.col-name` so the optional restricted
       indicator icon can sit alongside without being wiped by the
       `useShortenMiddle` action's `textContent` writes. The inner span is the
       flex item that takes the remaining width. */
    .col-name-text {
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
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

    /* With the Ext column hidden (full name in the Name column), there's no
       Ext track to borrow, so the editor stays within the Name column and
       (when present) the Git column to avoid bleeding into the Size column. */
    .col-rename.no-ext-col {
        grid-column: 2 / span 1;
    }

    .col-rename.no-ext-col.has-git {
        grid-column: 2 / span 2;
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
        /* Equal-width digits so right-aligned sizes line up into columns even in
           our proportional system font. The measurer mirrors this by sizing the
           column to the widest digit (see `measure-column-widths.ts`). */
        font-variant-numeric: tabular-nums;
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

    /* In-flux hourglass wrapper (orthogonal to content state): the index has
       unsettled writes for this dir (`isDirSizeUpdating`). */
    .size-updating {
        display: inline-flex;
        align-items: center;
        cursor: help;
    }

    /* `≥` lower-bound prefix: secondary color so it reads as a qualifier on the
       number, not a digit. The number itself keeps its size-tier color. */
    .size-lower-bound-prefix {
        color: var(--color-text-secondary);
        margin-right: 1px;
    }

    .size-mismatch {
        display: inline-flex;
        align-items: center;
        cursor: help;
    }

    .col-date {
        overflow: hidden;
        text-overflow: ellipsis;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        white-space: nowrap;
        /* Equal-width digits so every row's date lines up into vertical columns
           (the slashes/colons stack) without a monospace font. Each token format
           emits a fixed character count, so with tabular figures every date is
           the same width and the times align with no split-cell trick. */
        font-variant-numeric: tabular-nums;
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

    .file-entry.is-selected .col-name,
    .file-entry.is-selected .col-ext,
    .file-entry.is-selected .col-date,
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
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-name,
    .full-list-container.is-focused .file-entry.is-under-cursor.is-selected .col-ext,
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

</style>
