<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import type {
        DirectoryDeletedEvent,
        DirectoryDiff,
        FileEntry,
        ListingCancelledEvent,
        ListingCompleteEvent,
        ListingErrorEvent,
        ListingOpeningEvent,
        ListingProgressEvent,
        ListingReadCompleteEvent,
        ListingStats,
        NetworkHost,
        SortColumn,
        SortOrder,
        SyncStatus,
    } from '../types'
    import {
        cancelListing,
        findFileIndex,
        pathExists,
        getFileAt,
        getListingStats,
        getMaxFilenameWidth,
        getSyncStatus,
        getTotalCount,
        listDirectoryEnd,
        listDirectoryStart,
        listen,
        onMtpDeviceRemoved,
        openFile,
        openInEditor,
        showFileContextMenu,
        type UnlistenFn,
        updateMenuContext,
        updateLeftPaneState,
        updateRightPaneState,
        type PaneState,
        type PaneFileEntry,
    } from '$lib/tauri-commands'
    import type { ViewMode } from '$lib/app-status-store'
    import { tooltip } from '$lib/tooltip/tooltip'

    /** State snapshot for swapping panes without backend calls. */
    export interface SwapState {
        currentPath: string
        listingId: string
        totalCount: number
        maxFilenameWidth: number | undefined
        cursorIndex: number
        selectedIndices: number[]
        lastSequence: number
    }
    import FullList from '../views/FullList.svelte'
    import BriefList from '../views/BriefList.svelte'
    import SelectionInfo from '../selection/SelectionInfo.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import VolumeBreadcrumb from '../navigation/VolumeBreadcrumb.svelte'
    import PermissionDeniedPane from './PermissionDeniedPane.svelte'
    import NetworkMountView from './NetworkMountView.svelte'
    import MtpConnectionView from './MtpConnectionView.svelte'
    import { createSelectionState } from './selection-state.svelte'
    import { createRenameState } from '../rename/rename-state.svelte'
    import { cancelClickToRename } from '../rename/rename-activation'
    import { type DirectorySortMode } from '$lib/settings'
    import { dismissTransientToasts } from '$lib/ui/toast'
    import { createRenameFlow } from './rename-flow.svelte'
    import ExtensionChangeDialog from '../rename/ExtensionChangeDialog.svelte'
    import RenameConflictDialog from '../rename/RenameConflictDialog.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { createDebounce, createThrottle } from '$lib/utils/timing'

    const log = getAppLogger('fileExplorer')
    import { isMtpVolumeId, getMtpDisplayPath } from '$lib/mtp'
    import * as benchmark from '$lib/benchmark'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { resolveValidPath } from '../navigation/path-navigation'
    import { prioritizeDir } from '$lib/indexing'
    import { getVolumeSpace, type VolumeSpaceInfo } from '$lib/tauri-commands/storage'
    import { getDiskUsageLevel, getUsedPercent, formatBarTooltip } from '../disk-space-utils'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        initialPath: string
        paneId?: 'left' | 'right'
        volumeId?: string
        volumePath?: string
        volumeName?: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        viewMode?: ViewMode
        sortBy?: SortColumn
        sortOrder?: SortOrder
        directorySortMode?: DirectorySortMode
        onPathChange?: (path: string) => void
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
        onSortChange?: (column: SortColumn) => void
        onRequestFocus?: () => void
        /** Called when active network host changes (for history tracking) */
        onNetworkHostChange?: (host: NetworkHost | null) => void
        /** Called when user cancels loading (ESC key) - parent should reload previous folder, optionally selecting the folder we tried to enter */
        onCancelLoading?: (selectName?: string) => void
        /** Called when MTP connection fails fatally (device disconnected, timeout) - parent should fall back to previous volume */
        onMtpFatalError?: (error: string) => void
    }

    const {
        initialPath,
        paneId,
        volumeId = 'root',
        volumePath = '/',
        volumeName,
        isFocused = false,
        showHiddenFiles = true,
        viewMode = 'brief',
        sortBy = 'name',
        sortOrder = 'ascending',
        directorySortMode = 'likeFiles',
        onPathChange,
        onVolumeChange,
        onSortChange,
        onRequestFocus,
        onNetworkHostChange,
        onCancelLoading,
        onMtpFatalError,
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // New architecture: store listingId and totalCount, not files
    let listingId = $state('')
    let totalCount = $state(0)
    let maxFilenameWidth = $state<number | undefined>(undefined)
    let loading = $state(true)
    let error = $state<string | null>(null)
    let cursorIndex = $state(0)

    // Selection state (extracted to selection-state.svelte.ts)
    const selection = createSelectionState({
        onChanged: () => {
            debouncedSyncMcp.call()
        },
    })

    // Rename state (inline rename editor)
    const rename = createRenameState()

    // File under the cursor fetched separately for SelectionInfo
    let entryUnderCursor = $state<FileEntry | null>(null)

    // Listing stats for SelectionInfo (selection summary in Full mode, totals display)
    let listingStats = $state<ListingStats | null>(null)

    // Volume root path from listing-complete event (accurate for MTP and all volume types)
    let volumeRootFromEvent = $state<string | undefined>(undefined)

    // Disk space info for the current volume (fetched on mount, volume change, and after file ops)
    let volumeSpace: VolumeSpaceInfo | null = $state(null)

    // Component refs for keyboard navigation
    let fullListRef: FullList | undefined = $state()
    let briefListRef: BriefList | undefined = $state()
    let volumeBreadcrumbRef: VolumeBreadcrumb | undefined = $state()
    let networkMountViewRef: NetworkMountView | undefined = $state()

    // Check if we're viewing the network (special virtual volume)
    const isNetworkView = $derived(volumeId === 'network')

    // Check if we're viewing an MTP device
    const isMtpView = $derived(isMtpVolumeId(volumeId))

    // Check if this is a device-only MTP ID (needs connection)
    // Device-only IDs start with "mtp-" but don't contain ":" (no storage ID)
    const isMtpDeviceOnly = $derived(isMtpView && volumeId.startsWith('mtp-') && !volumeId.includes(':'))

    // Network browsing state - tracked here for history navigation integration
    let currentNetworkHost = $state<NetworkHost | null>(null)

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function toggleVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.toggle()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isVolumeChooserOpen(): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-call
        return volumeBreadcrumbRef?.getIsOpen() ?? false
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function closeVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.close()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function openVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.open()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleVolumeChooserKeyDown(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-call
        return volumeBreadcrumbRef?.handleKeyDown(e) ?? false
    }

    export function getListingId(): string {
        return listingId
    }

    export function isLoading(): boolean {
        return loading
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getFilenameUnderCursor(): string | undefined {
        return entryUnderCursor?.name
    }

    /** Also scrolls to make the cursor visible and syncs state to MCP. */
    export async function setCursorIndex(index: number): Promise<void> {
        if (isNetworkView) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
            networkMountViewRef?.setCursorIndex(index)
            return
        }
        cursorIndex = index
        // fetchEntryUnderCursor is handled by the $effect tracking cursorIndex
        // Scroll to make cursor visible
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        listRef?.scrollToIndex(index)
        // Wait for scroll effects to complete before syncing to MCP
        await tick()
        debouncedSyncMcp.call()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getCursorIndex(): number {
        return cursorIndex
    }

    /** Find an item by name in network views. Returns index or -1. */
    export function findNetworkItemIndex(name: string): number {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
        return (networkMountViewRef?.findItemIndex(name) as number | undefined) ?? -1
    }

    export function isInNetworkView(): boolean {
        return isNetworkView
    }

    /** Refresh network hosts (used by ⌘R shortcut). */
    export function refreshNetworkHosts(): void {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        networkMountViewRef?.refreshNetworkHosts()
    }

    export function getSelectedIndices(): number[] {
        return selection.getSelectedIndices()
    }

    /** Whether ".." is shown — needed for index adjustment in copy/move. */
    export function hasParentEntry(): boolean {
        return hasParent
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isAllSelected(): boolean {
        return selection.isAllSelected(hasParent, effectiveTotalCount)
    }

    export function setSelectedIndices(indices: number[]): void {
        selection.setSelectedIndices(indices)
    }

    export function clearSelection(): void {
        selection.clearSelection()
    }

    export function selectAll(): void {
        selection.selectAll(hasParent, effectiveTotalCount)
    }

    export function toggleSelectionAtCursor(): void {
        selection.toggleAt(cursorIndex, hasParent)
    }

    export function selectRange(startIndex: number, endIndex: number): void {
        selection.selectRange(startIndex, endIndex, hasParent)
    }

    // ==== Rename flow (logic in rename-flow.svelte.ts) ====

    const renameFlow = createRenameFlow({
        rename,
        getListingId: () => listingId,
        getTotalCount: () => totalCount,
        getIncludeHidden: () => includeHidden,
        getCurrentPath: () => currentPath,
        getCursorIndex: () => cursorIndex,
        getShowHiddenFiles: () => showHiddenFiles,
        getEntryUnderCursor,
        onRequestFocus: () => onRequestFocus?.(),
    })

    // Destructure handlers — factory methods don't use `this`, safe to destructure
    /* eslint-disable @typescript-eslint/unbound-method -- factory return, no `this` */
    const {
        handleRenameInput,
        handleRenameSubmit,
        handleRenameCancel,
        handleRenameShakeEnd,
        handleExtensionKeepOld,
        handleExtensionUseNew,
        handleConflictResolve,
    } = renameFlow
    /* eslint-enable @typescript-eslint/unbound-method */

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isRenaming(): boolean {
        return rename.active
    }

    export function startRename(): void {
        renameFlow.startRename()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function cancelRename(): void {
        renameFlow.cancelRename()
    }

    // Cache generation counter - incremented to force list components to re-fetch
    let cacheGeneration = $state(0)

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function refreshView(): void {
        cacheGeneration++
    }

    export async function refreshVolumeSpace(): Promise<void> {
        volumeSpace = await getVolumeSpace(currentPath)
    }

    /** Re-fetches index sizes (recursive_size, etc.) without a full list rebuild. */
    export function refreshIndexSizes(): void {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        listRef?.refreshIndexSizes?.()
    }

    export function getSwapState(): SwapState {
        return {
            currentPath,
            listingId,
            totalCount,
            maxFilenameWidth,
            cursorIndex,
            selectedIndices: selection.getSelectedIndices(),
            lastSequence,
        }
    }

    export function adoptListing(state: SwapState): void {
        // Cancel any in-flight loads
        loadGeneration++

        // Set currentPath first so the initialPath $effect sees newPath === curPath and skips reload
        currentPath = state.currentPath

        // Adopt the listing identity
        listingId = state.listingId
        totalCount = state.totalCount
        maxFilenameWidth = state.maxFilenameWidth
        lastSequence = state.lastSequence

        // Restore cursor and selection
        cursorIndex = state.cursorIndex
        selection.setSelectedIndices(state.selectedIndices)

        // Force virtual list to re-fetch visible range from (now-swapped) cache
        cacheGeneration++

        // Clear loading/error state
        loading = false
        error = null

        // Re-fetch entry under cursor and listing stats for SelectionInfo
        void fetchEntryUnderCursor()
        void fetchListingStats()

        // Sync state to MCP
        debouncedSyncMcp.call()

        // Scroll to cursor position
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(cursorIndex)
        })
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isMtp(): boolean {
        return isMtpView
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getVolumeId(): string {
        return volumeId
    }

    export function getCurrentPath(): string {
        return currentPath
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function setNetworkHost(host: NetworkHost | null): void {
        currentNetworkHost = host
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        networkMountViewRef?.setNetworkHost(host)
    }

    /** Navigates up and selects the folder we came from. Returns false if already at root. */
    export async function navigateToParent(): Promise<boolean> {
        if (currentPath === '/' || currentPath === volumePath) {
            return false // Already at root
        }
        const currentFolderName = currentPath.split('/').pop()
        const lastSlash = currentPath.lastIndexOf('/')
        const parentPath = lastSlash > 0 ? currentPath.substring(0, lastSlash) : '/'

        currentPath = parentPath
        // Note: onPathChange is called in listing-complete handler after successful load
        await loadDirectory(parentPath, currentFolderName)
        return true
    }

    // Track the current load operation to cancel outdated ones
    let loadGeneration = 0
    // Track last sequence for file watcher diffs
    let lastSequence = 0
    // Streaming event listeners
    let unlistenOpening: UnlistenFn | undefined
    let unlistenProgress: UnlistenFn | undefined
    let unlistenComplete: UnlistenFn | undefined
    let unlistenError: UnlistenFn | undefined
    let unlistenCancelled: UnlistenFn | undefined
    // Opening folder state (before read_dir starts - slow for network folders)
    let openingFolder = $state(false)
    // Loading progress state for streaming
    let loadingCount = $state<number | undefined>(undefined)
    // Finalizing state (read_dir done, now sorting/caching)
    let finalizingCount = $state<number | undefined>(undefined)
    let unlistenReadComplete: UnlistenFn | undefined
    // Sync status map for visible files
    let syncStatusMap = $state<Record<string, SyncStatus>>({})
    const syncPollIntervalMs = 3000
    let syncPollInterval: ReturnType<typeof setInterval>
    // Poll to detect when the current directory is deleted externally (FSEvents doesn't notify)
    const dirExistsPollMs = 2000
    let dirExistsPollInterval: ReturnType<typeof setInterval>

    // Derive includeHidden from showHiddenFiles prop
    const includeHidden = $derived(showHiddenFiles)

    // Map sort column names to MCP format (constant, no need to recreate)
    const sortFieldMap: Record<string, string> = {
        name: 'name',
        extension: 'ext',
        size: 'size',
        modified: 'modified',
        created: 'created',
    }

    /** Build file list for MCP state sync */
    async function buildMcpFileList(): Promise<PaneFileEntry[]> {
        const files: PaneFileEntry[] = []

        // For network views, we don't sync files
        if (isNetworkView || !listingId || totalCount === 0) return files

        // Calculate backend indices from visible range (frontend indices include "..")
        const backendStart = hasParent ? Math.max(0, visibleRangeStart - 1) : visibleRangeStart
        const backendEnd = hasParent ? Math.max(0, visibleRangeEnd - 1) : visibleRangeEnd

        // Include ".." entry if it's in the visible range
        if (hasParent && visibleRangeStart === 0) {
            const parentPath = currentPath.substring(0, currentPath.lastIndexOf('/')) || '/'
            files.push({ name: '..', path: parentPath, isDirectory: true })
        }

        // Limit to 100 files max for performance
        const maxToFetch = Math.min(backendEnd - backendStart, 100)
        for (let i = 0; i < maxToFetch; i++) {
            const backendIndex = backendStart + i
            if (backendIndex >= totalCount) break
            const entry = await getFileAt(listingId, backendIndex, includeHidden)
            if (entry) {
                files.push({
                    name: entry.name,
                    path: entry.path,
                    isDirectory: entry.isDirectory,
                    size: entry.size,
                    modified: entry.modifiedAt ? new Date(entry.modifiedAt * 1000).toISOString() : undefined,
                })
            }
        }
        return files
    }

    /**
     * Sync pane state to Rust for MCP context tools.
     * Called when files load, cursor position changes, or view mode changes.
     */
    async function syncPaneStateToMcp() {
        if (!paneId) return

        try {
            const files = await buildMcpFileList()
            const effectiveTotal = hasParent ? totalCount + 1 : totalCount
            // Use actual visible range, clamped to valid bounds
            const loadedStart = Math.max(0, visibleRangeStart)
            const loadedEnd = Math.min(effectiveTotal, visibleRangeEnd)
            const state: PaneState = {
                path: currentPath,
                volumeId,
                volumeName,
                files,
                cursorIndex,
                viewMode,
                selectedIndices: selection.getSelectedIndices(),
                sortField: sortFieldMap[sortBy] ?? 'name',
                sortOrder: sortOrder === 'ascending' ? 'asc' : 'desc',
                totalFiles: effectiveTotal,
                loadedStart,
                loadedEnd,
                showHidden: showHiddenFiles,
            }

            const updateFn = paneId === 'left' ? updateLeftPaneState : updateRightPaneState
            await updateFn(state)
        } catch {
            // Silently ignore sync errors - MCP is optional
        }
    }

    // Debounced/throttled IPC wrappers to avoid flooding the backend during rapid navigation.
    // The virtual scroll (cursorIndex → scrollToIndex → DOM) is fully synchronous and unaffected.
    const debouncedFetchEntry = createDebounce(() => void fetchEntryUnderCursor(), 16)
    const throttledFetchStats = createThrottle(() => void fetchListingStats(), 150)
    const debouncedMenuContext = createDebounce(() => {
        if (entryUnderCursor && entryUnderCursor.name !== '..') {
            void updateMenuContext(entryUnderCursor.path, entryUnderCursor.name)
        }
    }, 100)
    const debouncedSyncMcp = createDebounce(() => void syncPaneStateToMcp(), 300)

    /** Handle visible range change from list components */
    function handleVisibleRangeChange(start: number, end: number) {
        visibleRangeStart = start
        visibleRangeEnd = end
        debouncedSyncMcp.call()
    }

    // Check if error is a permission denied error
    const isPermissionDenied = $derived(
        error !== null && (error.includes('Permission denied') || error.includes('os error 13')),
    )

    // Create ".." entry for parent navigation
    function createParentEntry(path: string): FileEntry | null {
        if (path === '/') return null
        const parentPath = path.substring(0, path.lastIndexOf('/')) || '/'
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

    // Check if current directory has a parent (not at filesystem root AND not at volume root)
    // Prefer volumeRoot from the listing event (accurate for MTP), fall back to prop (for initial state)
    const effectiveVolumeRoot = $derived(volumeRootFromEvent ?? volumePath)
    const hasParent = $derived(currentPath !== '/' && currentPath !== effectiveVolumeRoot)

    // Effective total count includes ".." entry if not at root
    const effectiveTotalCount = $derived(hasParent ? totalCount + 1 : totalCount)

    // Track the visible range for MCP state sync
    // This is updated by the list components when they scroll
    let visibleRangeStart = $state(0)
    let visibleRangeEnd = $state(100)

    async function loadDirectory(path: string, selectName?: string) {
        // Cancel any active rename when navigating
        rename.cancel()
        cancelClickToRename()
        dismissTransientToasts()

        // Reset benchmark epoch for this navigation
        benchmark.resetEpoch()
        benchmark.logEventValue('loadDirectory CALLED', path)

        // Debug logging for diagnosing concurrent list_directory calls
        log.debug(
            '[FilePane] loadDirectory called: paneId={paneId}, volumeId={volumeId}, path={path}, selectName={selectName}, currentLoading={loading}, currentListingId={listingId}',
            { paneId, volumeId, path, selectName: selectName ?? 'none', loading, listingId },
        )

        // Increment generation to cancel any in-flight requests
        const thisGeneration = ++loadGeneration
        log.debug('[FilePane] loadDirectory: generation={generation}', { generation: thisGeneration })

        // Cancel any abandoned listing from previous navigation
        if (listingId) {
            log.debug('[FilePane] loadDirectory: cancelling previous listing {listingId}', { listingId })
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
            listingId = ''
            lastSequence = 0
        }

        // Clean up previous event listeners
        unlistenOpening?.()
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()

        // Set loading state BEFORE starting IPC call
        // This ensures the UI shows the loading spinner immediately
        loading = true
        openingFolder = false
        loadingCount = undefined
        finalizingCount = undefined
        error = null
        syncStatusMap = {}
        selection.clearSelection()
        totalCount = 0 // Reset to show empty list immediately
        entryUnderCursor = null // Clear old under-the-cursor entry info

        // Store path and selectName for use in event handlers
        const loadPath = path
        const loadSelectName = selectName

        // CRITICAL: Wait for browser to actually PAINT the loading state before IPC call
        // tick() only flushes Svelte render, requestAnimationFrame waits for paint
        // Double-RAF ensures we wait for both the render AND the paint to complete
        await new Promise<void>((resolve) => {
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    resolve()
                })
            })
        })

        try {
            // Generate listingId first and set up listeners BEFORE starting the streaming
            // This prevents a race condition where fast folders complete before listeners are ready
            const newListingId = crypto.randomUUID()
            listingId = newListingId
            lastSequence = 0

            // Subscribe to opening event (emitted before read_dir - slow for network folders)
            unlistenOpening = await listen<ListingOpeningEvent>('listing-opening', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    openingFolder = true
                }
            })

            // Subscribe to progress events
            unlistenProgress = await listen<ListingProgressEvent>('listing-progress', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    loadingCount = event.payload.loadedCount
                }
            })

            // Subscribe to read-complete event (read_dir finished, now sorting/caching)
            unlistenReadComplete = await listen<ListingReadCompleteEvent>('listing-read-complete', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    finalizingCount = event.payload.totalCount
                }
            })

            // Subscribe to completion event
            unlistenComplete = await listen<ListingCompleteEvent>('listing-complete', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    void handleListingComplete(event.payload, loadPath, loadSelectName)
                }
            })

            // Subscribe to error event
            unlistenError = await listen<ListingErrorEvent>('listing-error', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    // For MTP volumes, trigger fallback on error (device likely disconnected)
                    if (isMtpView) {
                        error = event.payload.message
                        listingId = ''
                        totalCount = 0
                        loading = false
                        openingFolder = false
                        loadingCount = undefined
                        finalizingCount = undefined
                        log.warn('MTP listing error, triggering fallback: {error}', {
                            error: event.payload.message,
                        })
                        onMtpFatalError?.(event.payload.message)
                        return
                    }

                    // For local volumes, check if the path was deleted
                    void pathExists(loadPath).then((exists) => {
                        if (!exists) {
                            // Path is gone — auto-navigate to nearest valid parent
                            log.info('Listing error for deleted path, navigating to valid parent: {path}', {
                                path: loadPath,
                            })
                            void resolveValidPath(loadPath).then((validPath) => {
                                const target = validPath ?? volumePath
                                currentPath = target
                                void loadDirectory(target)
                            })
                        } else {
                            // Path exists but has another error (permission denied, etc.)
                            error = event.payload.message
                            listingId = ''
                            totalCount = 0
                            loading = false
                            openingFolder = false
                            loadingCount = undefined
                            finalizingCount = undefined
                        }
                    })
                }
            })

            // Subscribe to cancelled event
            unlistenCancelled = await listen<ListingCancelledEvent>('listing-cancelled', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    // Cancellation handled by onCancelLoading callback
                    listingId = ''
                    loading = false
                    openingFolder = false
                    loadingCount = undefined
                    finalizingCount = undefined
                }
            })

            // Now start streaming listing - listeners are already set up
            benchmark.logEvent('IPC listDirectoryStart CALL')
            log.debug(
                '[FilePane] calling listDirectoryStart: volumeId={volumeId}, path={loadPath}, listingId={listingId}',
                { volumeId, loadPath, listingId: newListingId },
            )
            const result = await listDirectoryStart(
                volumeId,
                path,
                includeHidden,
                sortBy,
                sortOrder,
                newListingId,
                directorySortMode,
            )
            benchmark.logEventValue('IPC listDirectoryStart RETURNED', result.listingId)
            log.debug('[FilePane] listDirectoryStart returned: status={status}', {
                status: JSON.stringify(result.status),
            })

            // Check if this load was cancelled while we were starting
            if (thisGeneration !== loadGeneration) {
                // Cancel the abandoned listing
                void cancelListing(newListingId)
                return
            }
        } catch (e) {
            if (thisGeneration !== loadGeneration) return
            error = e instanceof Error ? e.message : String(e)
            listingId = ''
            totalCount = 0
            loading = false
            openingFolder = false
            loadingCount = undefined
            finalizingCount = undefined
        }
    }

    // Handle listing completion event
    async function handleListingComplete(
        payload: ListingCompleteEvent,
        loadPath: string,
        loadSelectName: string | undefined,
    ) {
        benchmark.logEventValue('listing-complete received, totalCount', payload.totalCount)
        totalCount = payload.totalCount
        maxFilenameWidth = payload.maxFilenameWidth
        volumeRootFromEvent = payload.volumeRoot

        // Determine initial cursor position
        if (loadSelectName) {
            const foundIndex = await findFileIndex(listingId, loadSelectName, includeHidden)
            const adjustedIndex = hasParent ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
            cursorIndex = adjustedIndex >= 0 ? adjustedIndex : 0
        } else {
            cursorIndex = 0
        }

        loading = false
        openingFolder = false
        loadingCount = undefined
        finalizingCount = undefined
        benchmark.logEvent('loading = false (UI can render)')

        // NOW push to history (only on successful completion)
        onPathChange?.(loadPath)

        // Fetch entry under the cursor for SelectionInfo
        void fetchEntryUnderCursor()

        // Fetch listing stats for SelectionInfo
        void fetchListingStats()

        // Sync state to MCP for context tools
        debouncedSyncMcp.call()

        // Scroll to cursor after DOM updates
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(cursorIndex)
        })
    }

    // Handle cancellation during loading (called from DualPaneExplorer on ESC)
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleCancelLoading() {
        if (!loading || !listingId) return

        // Cancel the Rust-side operation
        void cancelListing(listingId)

        // Extract the folder name we were trying to enter, so parent can select it when reloading
        const folderName = currentPath.split('/').pop()

        // Reload previous folder via callback (parent will set the path, triggering our effect)
        onCancelLoading?.(folderName)
    }

    // Navigate to a specific path with optional item selection (used when cancelling navigation)
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function navigateToPath(path: string, selectName?: string) {
        currentPath = path
        void loadDirectory(path, selectName)
    }

    // Fetch the entry currently under the cursor for SelectionInfo
    async function fetchEntryUnderCursor() {
        if (!listingId) {
            entryUnderCursor = null
            return
        }

        // Handle ".." entry specially
        if (hasParent && cursorIndex === 0) {
            entryUnderCursor = createParentEntry(currentPath)
            return
        }

        // Adjust index for ".." entry
        const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex

        try {
            entryUnderCursor = await getFileAt(listingId, backendIndex, includeHidden)
        } catch {
            entryUnderCursor = null
        }
    }

    // Fetch listing stats for SelectionInfo (totals and selection stats)
    async function fetchListingStats() {
        if (!listingId) {
            listingStats = null
            return
        }

        try {
            // Convert selected indices to backend indices (adjust for ".." entry)
            const backendIndices =
                selection.selectedIndices.size > 0
                    ? Array.from(selection.selectedIndices).map((i) => (hasParent ? i - 1 : i))
                    : undefined

            listingStats = await getListingStats(listingId, includeHidden, backendIndices)
        } catch {
            listingStats = null
        }
    }

    // Fetch sync status for visible entries (called by List components)
    async function fetchSyncStatusForPaths(paths: string[]) {
        if (paths.length === 0) return

        try {
            const statuses = await getSyncStatus(paths)
            syncStatusMap = { ...syncStatusMap, ...statuses }
        } catch {
            // Silently ignore - sync status is optional
        }
    }

    function handleSelect(index: number, shiftKey = false) {
        if (shiftKey) {
            selection.handleShiftNavigation(index, cursorIndex, hasParent)
        } else {
            selection.clearRangeState()
        }
        cursorIndex = index
        onRequestFocus?.()
        void fetchEntryUnderCursor()
    }

    async function handleContextMenu(entry: FileEntry) {
        if (entry.name === '..') return // No context menu for parent entry
        await showFileContextMenu(entry.path, entry.name, entry.isDirectory)
    }

    async function handleNavigate(entry: FileEntry) {
        if (entry.isDirectory) {
            // When navigating to parent (..), remember current folder name to select it
            const isGoingUp = entry.name === '..'
            const currentFolderName = isGoingUp ? currentPath.split('/').pop() : undefined

            currentPath = entry.path
            // Note: onPathChange is called in listing-complete handler after successful load
            await loadDirectory(entry.path, currentFolderName)
        } else {
            // Open file with default application
            try {
                await openFile(entry.path)
            } catch {
                // Silently fail - file open errors are expected sometimes
            }
        }
    }

    function handlePaneClick() {
        onRequestFocus?.()
    }

    function handleVolumeChangeFromBreadcrumb(newVolumeId: string, newVolumePath: string, targetPath: string) {
        // Navigate to the target path (may differ from volume root for favorites)
        // Note: We intentionally don't call onPathChange here - the volume change handler
        // in DualPaneExplorer takes care of saving both the old volume's path and the new path.
        // Calling onPathChange would save the new path under the OLD volume ID (race condition).
        currentPath = targetPath
        onVolumeChange?.(newVolumeId, newVolumePath, targetPath)

        // Don't load directory for network views (they handle their own data)
        // or device-only MTP views (they need connection first via auto-connect effect)
        // But DO load for connected MTP views (storage-specific volume ID contains ":")
        const isDeviceOnlyMtp = isMtpVolumeId(newVolumeId) && !newVolumeId.includes(':')
        if (newVolumeId !== 'network' && !isDeviceOnlyMtp) {
            void loadDirectory(targetPath)
            void refreshVolumeSpace()
        }
    }

    // Handle network host change from NetworkMountView
    function handleNetworkHostChange(host: NetworkHost | null) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

    // Helper: Handle navigation result by updating cursor index and scrolling
    // If shiftKey is true, handles range selection; otherwise clears range state
    function applyNavigation(
        newIndex: number,
        listRef: { scrollToIndex: (index: number) => void } | undefined,
        shiftKey = false,
    ) {
        if (shiftKey) {
            selection.handleShiftNavigation(newIndex, cursorIndex, hasParent)
        } else {
            selection.clearRangeState()
        }
        cursorIndex = newIndex
        listRef?.scrollToIndex(newIndex)
        // fetchEntryUnderCursor is handled by the $effect tracking cursorIndex
    }

    // Helper: Handle brief mode key navigation
    function handleBriefModeKeys(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const newIndex: number | undefined = briefListRef?.handleKeyNavigation(e.key, e)
        if (newIndex !== undefined) {
            e.preventDefault()
            applyNavigation(newIndex, briefListRef, e.shiftKey)
            return true
        }
        return false
    }

    // Helper: Handle full mode key navigation
    function handleFullModeKeys(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const visibleItems: number = fullListRef?.getVisibleItemsCount() ?? 20
        const shortcutResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount: effectiveTotalCount,
            visibleItems,
        })
        if (shortcutResult) {
            e.preventDefault()
            applyNavigation(shortcutResult.newIndex, fullListRef, e.shiftKey)
            return true
        }

        // Handle arrow navigation
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            applyNavigation(Math.min(cursorIndex + 1, effectiveTotalCount - 1), fullListRef, e.shiftKey)
            return true
        }
        if (e.key === 'ArrowUp') {
            e.preventDefault()
            applyNavigation(Math.max(cursorIndex - 1, 0), fullListRef, e.shiftKey)
            return true
        }
        // Left/Right arrows jump to first/last (same as Brief mode at boundaries)
        if (e.key === 'ArrowLeft') {
            e.preventDefault()
            applyNavigation(0, fullListRef, e.shiftKey)
            return true
        }
        if (e.key === 'ArrowRight') {
            e.preventDefault()
            applyNavigation(effectiveTotalCount - 1, fullListRef, e.shiftKey)
            return true
        }
        return false
    }

    // Helper: Handle selection-related key events
    function handleSelectionKeys(e: KeyboardEvent): boolean {
        // Space - toggle selection at cursor
        if (e.key === ' ') {
            e.preventDefault()
            selection.toggleAt(cursorIndex, hasParent)

            // Request index priority scan when selecting a directory
            const entry = getEntryUnderCursor()
            if (entry?.isDirectory && entry.name !== '..') {
                void prioritizeDir(entry.path, 'user_selected')
            }

            return true
        }
        // Cmd+A - select all (Cmd+Shift+A - deselect all)
        if (e.key === 'a' && e.metaKey) {
            e.preventDefault()
            if (e.shiftKey) {
                selection.deselectAll()
            } else {
                selection.selectAll(hasParent, effectiveTotalCount)
            }
            return true
        }
        return false
    }

    /** Gets the file entry under the cursor from the current list view */
    function getEntryUnderCursor(): FileEntry | undefined {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-return
        return listRef?.getEntryAt(cursorIndex)
    }

    // Exported so DualPaneExplorer can forward keyboard events
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyDown(e: KeyboardEvent) {
        // When rename is active, suppress all app-level shortcuts.
        // The InlineRenameEditor handles its own keyboard events via stopPropagation.
        // This guard handles any edge cases where events still bubble.
        if (rename.active) return

        // Any keyboard action cancels a pending click-to-rename timer
        cancelClickToRename()

        if (isNetworkView) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            networkMountViewRef?.handleKeyDown(e)
            return
        }

        // Handle Enter key - navigate into the entry under the cursor
        if (e.key === 'Enter') {
            const entry = getEntryUnderCursor()
            if (entry) {
                e.preventDefault()
                void handleNavigate(entry)
                return
            }
        }

        // Handle F4 key - open file in default text editor
        if (e.key === 'F4') {
            const entry = getEntryUnderCursor()
            if (entry && !entry.isDirectory) {
                e.preventDefault()
                void openInEditor(entry.path)
                return
            }
        }

        // Handle Backspace or ⌘↑ - go to parent directory
        if ((e.key === 'Backspace' || (e.key === 'ArrowUp' && e.metaKey)) && hasParent) {
            e.preventDefault()
            void navigateToParent()
            return
        }

        // Handle selection keys
        if (handleSelectionKeys(e)) return

        // Delegate to view-mode-specific handler
        if (viewMode === 'brief') {
            handleBriefModeKeys(e)
        } else {
            handleFullModeKeys(e)
        }
    }

    // Handle key release - clear range state when Shift is released
    // This ensures a new Shift+navigation starts fresh selection from current cursor
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            selection.clearRangeState()
        }
    }

    // When includeHidden changes, cancel rename and refetch total count
    $effect(() => {
        if (listingId && !loading) {
            // Cancel rename on hidden files toggle (spec: sort change / toggle hidden = cancel)
            untrack(() => {
                rename.cancel()
            })
            // Read cursor state without tracking to avoid infinite re-triggers
            const nameToFollow = untrack(() => entryUnderCursor?.name)
            const currentCursor = untrack(() => cursorIndex)
            void getTotalCount(listingId, includeHidden).then(async (count) => {
                totalCount = count
                const total = hasParent ? count + 1 : count
                // Try to keep cursor on the same file
                if (nameToFollow) {
                    const foundIndex = await findFileIndex(listingId, nameToFollow, includeHidden)
                    if (foundIndex !== null) {
                        const adjustedIndex = hasParent ? foundIndex + 1 : foundIndex
                        await setCursorIndex(adjustedIndex)
                        return
                    }
                }
                // File not found (was hidden) or no file — clamp cursor
                if (currentCursor >= total) {
                    await setCursorIndex(Math.max(0, total - 1))
                }
            })
        }
    })

    // Track the previous volumeId to detect MTP connection completion
    let prevVolumeId = $state(volumeId)

    // Update path when initialPath prop changes (for persistence loading)
    // Skip for network views and device-only MTP views (not yet connected)
    // Use untrack for currentPath so this effect only fires when initialPath changes,
    // not when the user navigates (which changes currentPath before onPathChange is called)
    $effect(() => {
        const newPath = initialPath // Track this
        const curPath = untrack(() => currentPath) // Don't track this
        // Load for local volumes and connected MTP views (not device-only)
        if (!isNetworkView && !isMtpDeviceOnly && newPath !== curPath) {
            log.debug(
                '[FilePane] initialPath effect: triggering loadDirectory, paneId={paneId}, newPath={newPath}, curPath={curPath}',
                { paneId, newPath, curPath },
            )
            currentPath = newPath
            void loadDirectory(newPath)
        }
        // For device-only MTP views, just update the path (auto-connect will handle switching to storage)
        if (isMtpDeviceOnly && newPath !== curPath) {
            log.debug('[FilePane] initialPath effect (MTP device-only): updating path only, paneId={paneId}', {
                paneId,
            })
            currentPath = newPath
        }
    })

    // Detect when MTP volume transitions from device-only to connected (has storage ID)
    // This triggers loading after auto-connect completes
    $effect(() => {
        const wasDeviceOnly = isMtpVolumeId(prevVolumeId) && !prevVolumeId.includes(':')
        const isNowConnected = isMtpVolumeId(volumeId) && volumeId.includes(':')

        if (wasDeviceOnly && isNowConnected) {
            log.info('MTP volume connected, loading directory: {path}', { path: initialPath })
            log.debug(
                '[FilePane] MTP volume transition effect: triggering loadDirectory, paneId={paneId}, prevVolumeId={prevVolumeId}, volumeId={volumeId}, initialPath={initialPath}',
                { paneId, prevVolumeId, volumeId, initialPath },
            )
            currentPath = initialPath
            void loadDirectory(initialPath)
        }

        prevVolumeId = volumeId
    })

    // Update global menu context when cursor position or focus changes (debounced — only matters for right-click)
    $effect(() => {
        if (!isFocused) return
        if (entryUnderCursor && entryUnderCursor.name !== '..') {
            debouncedMenuContext.call()
        }
    })

    // Re-fetch entry under the cursor when cursorIndex changes (debounced — status bar info can lag one frame)
    $effect(() => {
        void cursorIndex // Track
        if (listingId && !loading) {
            debouncedFetchEntry.call()
        }
    })

    // Re-fetch listing stats when selection changes (throttled — shows live count at steady cadence)
    $effect(() => {
        void selection.selectedIndices.size // Track selection changes
        if (listingId && !loading) {
            throttledFetchStats.call()
        }
    })

    // Scroll the entry under the cursor into view when view mode changes
    $effect(() => {
        void viewMode
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(cursorIndex)
        })
    })

    // Listen for file watcher diff events
    $effect(() => {
        const listenerPromise = listen<DirectoryDiff>('directory-diff', (event) => {
            const diff = event.payload
            // Only process diffs for our current listing
            if (diff.listingId !== listingId) return

            // Ignore out-of-order events
            if (diff.sequence <= lastSequence) return
            lastSequence = diff.sequence

            // If a rename is active and the file being renamed was removed
            // externally, cancel the rename gracefully
            if (rename.active && rename.target) {
                const targetName = rename.target.originalName
                const wasRemoved = diff.changes.some((c) => c.type === 'remove' && c.entry.name === targetName)
                if (wasRemoved) {
                    rename.cancel()
                    onRequestFocus?.()
                }
            }

            // Refetch total count and max filename width, then force the List
            // components to re-fetch their visible range. We always bump
            // cacheGeneration because renames don't change totalCount.
            void Promise.all([
                getTotalCount(listingId, includeHidden),
                getMaxFilenameWidth(listingId, includeHidden),
            ]).then(async ([count, newMaxWidth]) => {
                totalCount = count
                maxFilenameWidth = newMaxWidth
                cacheGeneration++

                // Post-rename cursor tracking: move cursor to the renamed file
                const nameToFind = renameFlow.pendingCursorName
                if (nameToFind) {
                    renameFlow.pendingCursorName = null
                    const foundIndex = await findFileIndex(listingId, nameToFind, includeHidden)
                    if (foundIndex !== null) {
                        const adjustedIndex = hasParent ? foundIndex + 1 : foundIndex
                        await setCursorIndex(adjustedIndex)
                        return
                    }
                }

                void fetchEntryUnderCursor()
            })
        })

        return () => {
            void listenerPromise
                .then((unsub) => {
                    unsub()
                })
                .catch(() => {})
        }
    })

    // Listen for directory-deleted events (watched directory was removed externally)
    $effect(() => {
        const listenerPromise = listen<DirectoryDeletedEvent>('directory-deleted', (event) => {
            if (event.payload.listingId !== listingId) return

            log.info('Directory deleted externally, navigating to nearest valid parent: {path}', {
                path: event.payload.path,
            })

            void resolveValidPath(currentPath).then((validPath) => {
                const target = validPath ?? volumePath
                currentPath = target
                // loadDirectory handles onPathChange via handleListingComplete
                void loadDirectory(target)
            })
        })

        return () => {
            void listenerPromise
                .then((unsub) => {
                    unsub()
                })
                .catch(() => {})
        }
    })

    // Listen for menu action events
    $effect(() => {
        const listenerPromise = listen<string>('menu-action', (event) => {
            const action = event.payload
            if (action === 'open') {
                // Use the list component's cached entry for consistency
                const listRef = viewMode === 'brief' ? briefListRef : fullListRef
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-assignment
                const entry: FileEntry | undefined = listRef?.getEntryAt(cursorIndex)
                if (entry) {
                    void handleNavigate(entry)
                }
            }
        })

        return () => {
            void listenerPromise
                .then((unsub) => {
                    unsub()
                })
                .catch(() => {})
        }
    })

    // Listen for MTP device removal events
    // When the device is disconnected, trigger fallback to previous volume
    //
    // IMPORTANT: We capture reactive values (volumeId, isMtpView) in the effect body
    // so Svelte tracks them as dependencies. This ensures the listener is re-created
    // when volumeId changes, avoiding stale closures in the callback.
    $effect(() => {
        // Capture current values - this makes Svelte track volumeId as a dependency
        const currentVolumeId = volumeId
        const currentIsMtpView = isMtpView

        // Extract device ID from volume ID (like "mtp-2097152:65537" -> "mtp-2097152")
        const deviceIdFromVolume =
            currentIsMtpView && currentVolumeId.includes(':') ? currentVolumeId.split(':')[0] : null

        // Only set up listener if we're viewing an MTP volume with a storage ID
        if (!currentIsMtpView || !deviceIdFromVolume) {
            return
        }

        const listenerPromise = onMtpDeviceRemoved((event) => {
            // Check if the removed device matches our current MTP volume
            if (event.deviceId === deviceIdFromVolume) {
                log.warn('MTP device disconnected while viewing: {deviceId}, triggering fallback', {
                    deviceId: event.deviceId,
                })
                onMtpFatalError?.('Device disconnected')
            }
        })

        return () => {
            void listenerPromise
                .then((unsub) => {
                    unsub()
                })
                .catch(() => {})
        }
    })

    // NOTE: MTP file watching now uses the unified directory-diff event system
    // (same as local volumes). The existing directory-diff listener above handles
    // both local and MTP changes, providing smooth incremental updates.

    onMount(() => {
        // Skip directory loading for:
        // - Network views (they handle their own data via NetworkBrowser/ShareBrowser)
        // - Device-only MTP views (they need connection first, handled by auto-connect effect)
        // But DO load for connected MTP views (storage-specific volume ID)
        log.debug(
            '[FilePane] onMount: paneId={paneId}, volumeId={volumeId}, currentPath={currentPath}, isNetworkView={isNetworkView}, isMtpDeviceOnly={isMtpDeviceOnly}',
            { paneId, volumeId, currentPath, isNetworkView, isMtpDeviceOnly },
        )
        if (!isNetworkView && !isMtpDeviceOnly) {
            log.debug('[FilePane] onMount: triggering loadDirectory for paneId={paneId}', { paneId })
            void loadDirectory(currentPath)
            void refreshVolumeSpace()
        } else {
            log.debug('[FilePane] onMount: SKIPPING loadDirectory for paneId={paneId}', { paneId })
        }

        // Poll sync status so iCloud/Dropbox icons update while idle
        syncPollInterval = setInterval(() => {
            const paths = Object.keys(syncStatusMap)
            if (!listingId || paths.length === 0) return
            void fetchSyncStatusForPaths(paths)
        }, syncPollIntervalMs)

        // Poll to detect externally deleted directories (macOS FSEvents doesn't notify)
        dirExistsPollInterval = setInterval(() => {
            if (!listingId || loading || isNetworkView || isMtpView) return
            void pathExists(currentPath).then((exists) => {
                if (exists) return

                // If on an external volume, check whether the volume root itself is gone.
                // If so, skip — the volume unmount handler will manage the transition.
                if (volumePath !== '/') {
                    void pathExists(volumePath).then((volumeExists) => {
                        if (!volumeExists) return
                        log.info('Directory no longer exists, navigating to valid parent: {path}', {
                            path: currentPath,
                        })
                        void resolveValidPath(currentPath).then((validPath) => {
                            const target = validPath ?? volumePath
                            currentPath = target
                            void loadDirectory(target)
                        })
                    })
                } else {
                    log.info('Directory no longer exists, navigating to valid parent: {path}', {
                        path: currentPath,
                    })
                    void resolveValidPath(currentPath).then((validPath) => {
                        const target = validPath ?? volumePath
                        currentPath = target
                        void loadDirectory(target)
                    })
                }
            })
        }, dirExistsPollMs)
    })

    onDestroy(() => {
        // Clean up listing
        if (listingId) {
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
        }
        clearInterval(syncPollInterval)
        clearInterval(dirExistsPollInterval)
        debouncedFetchEntry.cancel()
        throttledFetchStats.cancel()
        debouncedMenuContext.cancel()
        debouncedSyncMcp.cancel()
        unlistenOpening?.()
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    class="file-pane"
    class:is-focused={isFocused}
    onclick={handlePaneClick}
    onkeydown={() => {}}
    role="region"
    aria-label="File pane"
>
    <div class="header">
        <VolumeBreadcrumb
            bind:this={volumeBreadcrumbRef}
            {volumeId}
            {currentPath}
            onVolumeChange={handleVolumeChangeFromBreadcrumb}
        />
        <span class="path"
            >{isMtpView
                ? getMtpDisplayPath(currentPath)
                : currentPath.startsWith(volumePath)
                  ? currentPath.slice(volumePath.length) || '/'
                  : currentPath}</span
        >
    </div>
    <div class="content">
        {#if isNetworkView}
            <NetworkMountView
                bind:this={networkMountViewRef}
                {paneId}
                {isFocused}
                initialNetworkHost={currentNetworkHost}
                {onVolumeChange}
                onNetworkHostChange={handleNetworkHostChange}
            />
        {:else if isMtpDeviceOnly}
            <MtpConnectionView {volumeId} {onVolumeChange} />
        {:else if loading}
            <LoadingIcon {openingFolder} loadedCount={loadingCount} {finalizingCount} showCancelHint={true} />
        {:else if isPermissionDenied}
            <PermissionDeniedPane folderPath={currentPath} />
        {:else if error}
            <div class="error-message">{error}</div>
        {:else if viewMode === 'brief'}
            <BriefList
                bind:this={briefListRef}
                {listingId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {cacheGeneration}
                {cursorIndex}
                {isFocused}
                {syncStatusMap}
                selectedIndices={selection.selectedIndices}
                {hasParent}
                {maxFilenameWidth}
                {sortBy}
                {sortOrder}
                renameState={rename.active ? rename : null}
                parentPath={hasParent ? currentPath.substring(0, currentPath.lastIndexOf('/')) || '/' : ''}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={fetchSyncStatusForPaths}
                onSortChange={onSortChange
                    ? (column: SortColumn) => {
                          onSortChange(column)
                      }
                    : undefined}
                onVisibleRangeChange={handleVisibleRangeChange}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
                onRenameShakeEnd={handleRenameShakeEnd}
                onStartRename={startRename}
            />
        {:else}
            <FullList
                bind:this={fullListRef}
                {listingId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {cacheGeneration}
                {cursorIndex}
                {isFocused}
                {syncStatusMap}
                selectedIndices={selection.selectedIndices}
                {hasParent}
                {sortBy}
                {sortOrder}
                renameState={rename.active ? rename : null}
                parentPath={hasParent ? currentPath.substring(0, currentPath.lastIndexOf('/')) || '/' : ''}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={fetchSyncStatusForPaths}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
                onRenameShakeEnd={handleRenameShakeEnd}
                onStartRename={startRename}
                onSortChange={onSortChange
                    ? (column: SortColumn) => {
                          onSortChange(column)
                      }
                    : undefined}
                onVisibleRangeChange={handleVisibleRangeChange}
            />
        {/if}
    </div>
    <!-- SelectionInfo shown in both modes (not in network view, MTP connecting state, or error states) -->
    {#if !isNetworkView && !isMtpDeviceOnly && !isPermissionDenied && !error}
        <SelectionInfo
            {viewMode}
            entry={entryUnderCursor}
            currentDirModifiedAt={undefined}
            stats={listingStats}
            selectedCount={selection.selectedIndices.size}
            {volumeSpace}
        />
        <!--suppress HtmlWrongAttributeValue -- We know this is not a valid ARIA role, it's fine -->
        <div
            class="disk-usage-bar-wrapper"
            use:tooltip={volumeSpace ? formatBarTooltip(volumeSpace, formatFileSize) : ''}
        >
            <div
                class="disk-usage-bar"
                role="meter"
                aria-label="Disk usage"
                aria-valuenow={volumeSpace ? getUsedPercent(volumeSpace) : 0}
                aria-valuemin={0}
                aria-valuemax={100}
            >
                {#if volumeSpace}
                    <div
                        class="disk-usage-fill"
                        style:width="{getUsedPercent(volumeSpace)}%"
                        style:background-color="var({getDiskUsageLevel(getUsedPercent(volumeSpace)).cssVar})"
                    ></div>
                {/if}
            </div>
        </div>
    {/if}
</div>

{#if renameFlow.extensionDialogState}
    <ExtensionChangeDialog
        oldExtension={renameFlow.extensionDialogState.oldExtension}
        newExtension={renameFlow.extensionDialogState.newExtension}
        onKeepOld={handleExtensionKeepOld}
        onUseNew={handleExtensionUseNew}
    />
{/if}

{#if renameFlow.conflictDialogState?.validity.conflict}
    <RenameConflictDialog
        renamedFile={{
            name: rename.target?.originalName ?? '',
            size: entryUnderCursor?.size ?? 0,
            modifiedAt: entryUnderCursor?.modifiedAt,
        }}
        existingFile={{
            name: renameFlow.conflictDialogState.validity.conflict.name,
            size: renameFlow.conflictDialogState.validity.conflict.size,
            modifiedAt: renameFlow.conflictDialogState.validity.conflict.modified ?? undefined,
        }}
        onResolve={handleConflictResolve}
    />
{/if}

<style>
    .file-pane {
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--color-border-strong);
    }

    .header {
        padding: 2px var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        font-size: var(--font-size-sm);
        white-space: nowrap;
        display: flex;
        align-items: center;
    }

    .disk-usage-bar-wrapper {
        flex-shrink: 0;
    }

    .disk-usage-bar {
        height: 2px;
        background-color: var(--color-disk-track);
    }

    .disk-usage-fill {
        height: 100%;
        transition: none;
        pointer-events: none;
    }

    .path {
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        flex: 1;
        min-width: 0;
    }

    .content {
        flex: 1;
        overflow: hidden;
        display: flex;
        flex-direction: column;
    }

    .error-message {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-error);
        text-align: center;
        padding: var(--spacing-lg);
    }
</style>
