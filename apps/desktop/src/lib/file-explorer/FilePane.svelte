<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import type {
        DirectoryDiff,
        FileEntry,
        ListingCancelledEvent,
        ListingCompleteEvent,
        ListingErrorEvent,
        ListingOpeningEvent,
        ListingProgressEvent,
        ListingReadCompleteEvent,
        ListingStats,
        MountError,
        NetworkHost,
        ShareInfo,
        SortColumn,
        SortOrder,
        SyncStatus,
    } from './types'
    import {
        cancelListing,
        findContainingVolume,
        findFileIndex,
        getFileAt,
        getListingStats,
        getMaxFilenameWidth,
        getSyncStatus,
        getTotalCount,
        listDirectoryEnd,
        listDirectoryStartStreaming,
        listen,
        listVolumes,
        mountNetworkShare,
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
    import { getMountTimeoutMs } from '$lib/settings/network-settings'
    import FullList from './FullList.svelte'
    import BriefList from './BriefList.svelte'
    import SelectionInfo from './SelectionInfo.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
    import PermissionDeniedPane from './PermissionDeniedPane.svelte'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('fileExplorer')
    import NetworkBrowser from './NetworkBrowser.svelte'
    import ShareBrowser from './ShareBrowser.svelte'
    import { isMtpVolumeId, getMtpDisplayPath, constructMtpPath } from '$lib/mtp'
    import { connect as connectMtpDevice } from '$lib/mtp/mtp-store.svelte'
    import * as benchmark from '$lib/benchmark'
    import { handleNavigationShortcut } from './keyboard-shortcuts'

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

    // Selection state
    // SAFETY CONTRACT: selectedIndices is the single source of truth for what files are selected.
    // Both the UI (via props to BriefList/FullList) and file operations (via getSelectedIndices())
    // read from this same Set. This ensures what the user sees is what operations act on.
    //
    // CRITICAL: Always use mutations (.add(), .delete(), .clear()) - never reassign this variable.
    // SvelteSet only tracks mutations for reactivity. Reassignment breaks UI updates, which could
    // cause users to see stale selection while operations act on different data.
    // See: "Selection state consistency" tests in integration.test.ts
    const selectedIndices: SvelteSet<number> = new SvelteSet()
    let selectionAnchorIndex = $state<number | null>(null)
    let selectionEndIndex = $state<number | null>(null)
    let isDeselecting = $state(false)

    // File under the cursor fetched separately for SelectionInfo
    let entryUnderCursor = $state<FileEntry | null>(null)

    // Listing stats for SelectionInfo (selection summary in Full mode, totals display)
    let listingStats = $state<ListingStats | null>(null)

    // Volume root path from listing-complete event (accurate for MTP and all volume types)
    let volumeRootFromEvent = $state<string | undefined>(undefined)

    // Component refs for keyboard navigation
    let fullListRef: FullList | undefined = $state()
    let briefListRef: BriefList | undefined = $state()
    let volumeBreadcrumbRef: VolumeBreadcrumb | undefined = $state()
    let networkBrowserRef: NetworkBrowser | undefined = $state()
    let shareBrowserRef: ShareBrowser | undefined = $state()

    // Check if we're viewing the network (special virtual volume)
    const isNetworkView = $derived(volumeId === 'network')

    // Check if we're viewing an MTP device
    const isMtpView = $derived(isMtpVolumeId(volumeId))

    // Check if this is a device-only MTP ID (needs connection)
    // Device-only IDs start with "mtp-" but don't contain ":" (no storage ID)
    const isMtpDeviceOnly = $derived(isMtpView && volumeId.startsWith('mtp-') && !volumeId.includes(':'))

    // MTP connection state for device-only IDs
    let mtpConnecting = $state(false)
    let mtpConnectionError = $state<string | null>(null)
    // Track the device ID we've successfully connected to, to prevent re-triggering auto-connect
    // while waiting for the parent to update volumeId after onVolumeChange
    let mtpConnectedDeviceId = $state<string | null>(null)

    // Effect: Reset connected device ID when we're no longer on a device-only MTP volume
    // This runs when the volume change completes and we switch to a storage-specific ID
    $effect(() => {
        if (!isMtpDeviceOnly) {
            mtpConnectedDeviceId = null
        }
    })

    // Effect: Auto-connect when a device-only MTP ID is selected
    $effect(() => {
        // Log all conditions for debugging reconnection issues
        log.debug(
            'MTP auto-connect effect evaluated: isMtpDeviceOnly={isMtpDeviceOnly}, mtpConnecting={mtpConnecting}, mtpConnectionError={mtpConnectionError}, mtpConnectedDeviceId={mtpConnectedDeviceId}, volumeId={volumeId}',
            {
                isMtpDeviceOnly,
                mtpConnecting,
                mtpConnectionError,
                mtpConnectedDeviceId,
                volumeId,
            },
        )

        // Skip if we've already successfully connected to this device (waiting for volume change)
        if (isMtpDeviceOnly && !mtpConnecting && !mtpConnectionError && mtpConnectedDeviceId !== volumeId) {
            // Extract device ID from "mtp-{deviceId}" format
            const deviceId = volumeId // The whole thing is the device ID for device-only format

            log.info('MTP auto-connect conditions met, starting connection to device: {deviceId}', { deviceId })
            mtpConnecting = true
            mtpConnectionError = null

            log.info('Auto-connecting to MTP device: {deviceId}', { deviceId })

            void connectMtpDevice(deviceId)
                .then((result) => {
                    log.info('MTP connection result: {result}', { result: JSON.stringify(result) })
                    if (result && result.storages.length > 0) {
                        // Connection successful, switch to first storage
                        const storage = result.storages[0]
                        const newVolumeId = `${deviceId}:${String(storage.id)}`
                        const newPath = constructMtpPath(deviceId, storage.id)
                        log.info(
                            'MTP connected, switching to storage: {storageId}, newVolumeId: {newVolumeId}, hasOnVolumeChange: {hasCallback}',
                            {
                                storageId: storage.id,
                                newVolumeId,
                                hasCallback: !!onVolumeChange,
                            },
                        )
                        // Mark device as connected to prevent auto-connect re-triggering
                        // while waiting for the parent to update volumeId
                        mtpConnectedDeviceId = deviceId
                        if (onVolumeChange) {
                            onVolumeChange(newVolumeId, newPath, newPath)
                            log.info('onVolumeChange called successfully')
                        } else {
                            log.warn('onVolumeChange callback not provided!')
                        }
                    } else {
                        mtpConnectionError = 'Device has no accessible storage'
                        log.warn('Device has no storages')
                    }
                })
                .catch((err: unknown) => {
                    // Handle various error formats from Tauri
                    let msg: string

                    // Helper to convert error type to user-friendly message
                    // Note: Rust serde uses camelCase for enum variants (e.g., "timeout" not "Timeout")
                    const getMessageForType = (errType: string | undefined): string | undefined => {
                        switch (errType) {
                            case 'timeout':
                                return 'Connection timed out. The device may be slow or unresponsive.'
                            case 'exclusiveAccess':
                                return 'Another app is using this device. Run the ptpcamerad workaround.'
                            case 'deviceNotFound':
                                return 'Device not found. It may have been unplugged.'
                            case 'disconnected':
                                return 'Device was disconnected.'
                            case 'deviceBusy':
                                return 'Device is busy. Please try again.'
                            default:
                                return undefined
                        }
                    }

                    if (err instanceof Error) {
                        msg = err.message
                    } else if (typeof err === 'string') {
                        // Error might be a JSON string - try to parse it
                        try {
                            const parsed = JSON.parse(err) as Record<string, unknown>
                            const typeMsg = getMessageForType(parsed.type as string | undefined)
                            msg = typeMsg || (parsed.userMessage as string) || (parsed.message as string) || err
                        } catch {
                            msg = err
                        }
                    } else if (typeof err === 'object' && err !== null) {
                        // Tauri MTP errors come as objects with type field
                        const errObj = err as Record<string, unknown>
                        const typeMsg = getMessageForType(errObj.type as string | undefined)
                        msg =
                            typeMsg ||
                            (errObj.userMessage as string) ||
                            (errObj.message as string) ||
                            JSON.stringify(err)
                    } else {
                        msg = String(err)
                    }
                    log.error('MTP connection failed: {error}', { error: msg })
                    mtpConnectionError = msg
                })
                .finally(() => {
                    log.info('MTP connection finally block, setting mtpConnecting=false')
                    mtpConnecting = false
                })
        }
    })

    // Network browsing state - which host is currently active (if any)
    let currentNetworkHost = $state<NetworkHost | null>(null)

    // Export method for keyboard shortcut
    export function toggleVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.toggle()
    }

    // Check if volume chooser is open (for event routing)
    export function isVolumeChooserOpen(): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-call
        return volumeBreadcrumbRef?.getIsOpen() ?? false
    }

    // Close the volume chooser dropdown (for coordination between panes)
    export function closeVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.close()
    }

    // Open the volume chooser dropdown (for MCP)
    export function openVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.open()
    }

    // Forward keyboard events to volume chooser when open
    export function handleVolumeChooserKeyDown(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-call
        return volumeBreadcrumbRef?.handleKeyDown(e) ?? false
    }

    // Get current listing ID for re-sorting
    export function getListingId(): string {
        return listingId
    }

    // Check if the pane is currently loading
    export function isLoading(): boolean {
        return loading
    }

    // Get the filename of the file under the cursor for cursor tracking during re-sort
    export function getFilenameUnderCursor(): string | undefined {
        return entryUnderCursor?.name
    }

    // Set cursor index directly (for cursor tracking after re-sort)
    // Also scrolls the view to make the cursor visible, then syncs to MCP
    export async function setCursorIndex(index: number): Promise<void> {
        cursorIndex = index
        void fetchEntryUnderCursor()
        // Scroll to make cursor visible
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        listRef?.scrollToIndex(index)
        // Wait for scroll effects to complete before syncing to MCP
        await tick()
        void syncPaneStateToMcp()
    }

    // Get current cursor index
    export function getCursorIndex(): number {
        return cursorIndex
    }

    // Get selected indices (for selection preservation during re-sort)
    export function getSelectedIndices(): number[] {
        return Array.from(selectedIndices)
    }

    // Check if the ".." entry is shown (needed for index adjustment in copy/move operations)
    export function hasParentEntry(): boolean {
        return hasParent
    }

    // Check if all files are selected (optimization for resort)
    export function isAllSelected(): boolean {
        const selectableCount = hasParent ? effectiveTotalCount - 1 : effectiveTotalCount
        return selectedIndices.size === selectableCount && selectableCount > 0
    }

    // Set selected indices directly (for selection preservation after re-sort)
    export function setSelectedIndices(indices: number[]): void {
        selectedIndices.clear()
        for (const i of indices) {
            selectedIndices.add(i)
        }
        clearRangeState()
        void syncPaneStateToMcp()
    }

    // Export clearSelection for MCP
    export { clearSelection }

    // Export selectAll for MCP (wrapper to use the local helper)
    export { selectAll }

    // Export toggle selection at cursor for MCP
    export function toggleSelectionAtCursor(): void {
        toggleSelectionAt(cursorIndex)
    }

    // Export select range for MCP
    export function selectRange(startIndex: number, endIndex: number): void {
        const indices = getIndicesInRange(startIndex, endIndex)
        for (const i of indices) {
            selectedIndices.add(i)
        }
        clearRangeState()
    }

    // Cache generation counter - incremented to force list components to re-fetch
    let cacheGeneration = $state(0)

    // Force refresh the view by incrementing cache generation
    export function refreshView(): void {
        cacheGeneration++
    }

    // Check if this pane is showing an MTP volume
    export function isMtp(): boolean {
        return isMtpView
    }

    // Get the current volume ID
    export function getVolumeId(): string {
        return volumeId
    }

    // Get the current path
    export function getCurrentPath(): string {
        return currentPath
    }

    // Get selected files from MTP browser - DEPRECATED, use standard selection
    // For MTP views, this now returns files from the standard listing
    export async function getMtpSelectedFiles(): Promise<FileEntry[]> {
        if (!isMtpView || !listingId) return []
        const files: FileEntry[] = []
        for (const index of selectedIndices) {
            const backendIndex = hasParent ? index - 1 : index
            if (backendIndex >= 0) {
                const entry = await getFileAt(listingId, backendIndex, includeHidden)
                if (entry) files.push(entry)
            }
        }
        return files
    }

    // Get file under cursor from MTP browser - DEPRECATED, use standard cursor
    // For MTP views, this now returns the file from the standard listing
    export function getMtpEntryUnderCursor(): FileEntry | null {
        if (!isMtpView) return null
        return entryUnderCursor
    }

    // Set network host state (for history navigation)
    export function setNetworkHost(host: NetworkHost | null): void {
        currentNetworkHost = host
        mountError = null
        lastMountAttempt = null
    }

    // Navigate to parent directory, selecting the folder we came from
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
    let unlisten: UnlistenFn | undefined
    let unlistenMenuAction: UnlistenFn | undefined
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
    // MTP device removal listener
    let unlistenMtpRemoved: UnlistenFn | undefined
    // Polling interval for sync status (visible files only)
    let syncPollInterval: ReturnType<typeof setInterval> | undefined
    const SYNC_POLL_INTERVAL_MS = 2000 // Poll every 2 seconds

    // Sync status map for visible files
    let syncStatusMap = $state<Record<string, SyncStatus>>({})

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
                selectedIndices: Array.from(selectedIndices),
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

    /** Handle visible range change from list components */
    function handleVisibleRangeChange(start: number, end: number) {
        visibleRangeStart = start
        visibleRangeEnd = end
        void syncPaneStateToMcp()
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

    // Helper: Clear all selection state
    function clearSelection() {
        selectedIndices.clear()
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
        void syncPaneStateToMcp()
    }

    // Helper: Toggle selection at a given index (returns true if now selected)
    function toggleSelectionAt(index: number): boolean {
        // Can't select ".." entry
        if (hasParent && index === 0) return false

        if (selectedIndices.has(index)) {
            selectedIndices.delete(index)
            return false
        } else {
            selectedIndices.add(index)
            return true
        }
    }

    // Helper: Get indices in range [a, b] inclusive, skipping ".." entry (index 0 when hasParent)
    function getIndicesInRange(a: number, b: number): number[] {
        const start = Math.min(a, b)
        const end = Math.max(a, b)
        const indices: number[] = []
        for (let i = start; i <= end; i++) {
            // Skip ".." entry
            if (hasParent && i === 0) continue
            indices.push(i)
        }
        return indices
    }

    // Helper: Apply range selection from anchor to end
    // Handles both selection and deselection modes, including range shrinking
    // When cursor returns to anchor (newEnd === anchor), nothing is selected
    function applyRangeSelection(newEnd: number) {
        if (selectionAnchorIndex === null) return

        // When cursor returns to anchor, range is empty (nothing selected)
        const rangeIsEmpty = newEnd === selectionAnchorIndex
        const newRange = rangeIsEmpty ? [] : getIndicesInRange(selectionAnchorIndex, newEnd)

        if (isDeselecting) {
            // Deselection mode: remove items in range
            for (const i of newRange) {
                selectedIndices.delete(i)
            }
        } else {
            // Selection mode: add items in range
            for (const i of newRange) {
                selectedIndices.add(i)
            }
        }

        // Handle range shrinking: if old range was larger, clear the difference
        if (selectionEndIndex !== null) {
            const oldRange =
                selectionEndIndex === selectionAnchorIndex
                    ? []
                    : getIndicesInRange(selectionAnchorIndex, selectionEndIndex)
            for (const i of oldRange) {
                if (!newRange.includes(i)) {
                    if (isDeselecting) {
                        // In deselect mode, shrinking means we stop deselecting those items
                        // They stay in whatever state they were before this selection action
                        // Since we track from start, we need to re-add them if they were selected
                        // For simplicity, in deselect mode we just keep them deselected
                    } else {
                        // In select mode, shrinking means we deselect the items no longer in range
                        selectedIndices.delete(i)
                    }
                }
            }
        }

        selectionEndIndex = newEnd
    }

    // Helper: Start or continue range selection
    function handleShiftNavigation(newIndex: number) {
        // Set anchor if not already set (use current cursor position before moving)
        if (selectionAnchorIndex === null) {
            selectionAnchorIndex = cursorIndex
            // Determine if we're in deselect mode (anchor was already selected)
            isDeselecting = selectedIndices.has(cursorIndex)
        }

        // Apply the range selection
        applyRangeSelection(newIndex)
    }

    // Helper: Clear anchor/end on non-shift navigation (selection remains)
    function clearRangeState() {
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
    }

    // Helper: Select all files (excluding ".." entry)
    function selectAll() {
        selectedIndices.clear()
        const startIndex = hasParent ? 1 : 0 // Skip ".." entry
        for (let i = startIndex; i < effectiveTotalCount; i++) {
            selectedIndices.add(i)
        }
        clearRangeState()
        void syncPaneStateToMcp()
    }

    // Helper: Deselect all files
    function deselectAll() {
        selectedIndices.clear()
        clearRangeState()
        void syncPaneStateToMcp()
    }

    // Effective total count includes ".." entry if not at root
    const effectiveTotalCount = $derived(hasParent ? totalCount + 1 : totalCount)

    // Track the visible range for MCP state sync
    // This is updated by the list components when they scroll
    let visibleRangeStart = $state(0)
    let visibleRangeEnd = $state(100)

    async function loadDirectory(path: string, selectName?: string) {
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
        clearSelection()
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
                    error = event.payload.message
                    listingId = ''
                    totalCount = 0
                    loading = false
                    openingFolder = false
                    loadingCount = undefined
                    finalizingCount = undefined

                    // For MTP volumes, trigger fallback on error (device likely disconnected)
                    if (isMtpView) {
                        log.warn('MTP listing error, triggering fallback: {error}', {
                            error: event.payload.message,
                        })
                        onMtpFatalError?.(event.payload.message)
                    }
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
            benchmark.logEvent('IPC listDirectoryStartStreaming CALL')
            log.debug(
                '[FilePane] calling listDirectoryStartStreaming: volumeId={volumeId}, path={loadPath}, listingId={listingId}',
                { volumeId, loadPath, listingId: newListingId },
            )
            const result = await listDirectoryStartStreaming(
                volumeId,
                path,
                includeHidden,
                sortBy,
                sortOrder,
                newListingId,
            )
            benchmark.logEventValue('IPC listDirectoryStartStreaming RETURNED', result.listingId)
            log.debug('[FilePane] listDirectoryStartStreaming returned: status={status}', {
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
        void syncPaneStateToMcp()

        // Scroll to cursor after DOM updates
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(cursorIndex)
        })
    }

    // Handle cancellation during loading (called from DualPaneExplorer on ESC)
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
                selectedIndices.size > 0 ? Array.from(selectedIndices).map((i) => (hasParent ? i - 1 : i)) : undefined

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
            handleShiftNavigation(index)
        } else {
            clearRangeState()
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
        }
    }

    // Handle MTP navigation (selectName is for future use when we implement cursor restoration)
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function handleMtpNavigate(newPath: string, selectName?: string) {
        currentPath = newPath
        onPathChange?.(newPath)
    }

    // Handle network host switching - show the ShareBrowser
    function handleNetworkHostSelect(host: NetworkHost) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

    // Handle going back from ShareBrowser to network host list
    function handleNetworkBack() {
        currentNetworkHost = null
        mountError = null
        lastMountAttempt = null
        onNetworkHostChange?.(null)
    }

    // Handle going back from mount error to share list
    function handleMountErrorBack() {
        mountError = null
        // Stay on the share list (currentNetworkHost remains set)
    }

    // Mounting state
    let isMounting = $state(false)
    let mountError = $state<MountError | null>(null)

    // Track last mount attempt for retry
    let lastMountAttempt = $state<{
        share: ShareInfo
        credentials: { username: string; password: string } | null
    } | null>(null)

    // Handle share selection from ShareBrowser - mount and navigate
    async function handleShareSelect(share: ShareInfo, credentials: { username: string; password: string } | null) {
        if (!currentNetworkHost) return

        // Store for retry
        lastMountAttempt = { share, credentials }

        isMounting = true
        mountError = null

        try {
            // Get server address - prefer IP, fall back to hostname
            const server = currentNetworkHost.ipAddress ?? currentNetworkHost.hostname ?? currentNetworkHost.name

            // Use provided credentials if available
            const result = await mountNetworkShare(
                server,
                share.name,
                credentials?.username ?? null,
                credentials?.password ?? null,
                getMountTimeoutMs(),
            )

            // Navigate to the mounted share
            // Clear current network host first
            currentNetworkHost = null
            lastMountAttempt = null

            // The mount path is typically /Volumes/<ShareName>
            const mountPath = result.mountPath

            // Refresh the volume list first - the new mount needs to be recognized
            await listVolumes()

            // Find the actual volume for the mounted path
            // This ensures proper breadcrumb display and volume context
            const mountedVolume = await findContainingVolume(mountPath)

            if (mountedVolume) {
                // Use the real volume ID and path from the system
                onVolumeChange?.(mountedVolume.id, mountedVolume.path, mountPath)
            } else {
                // Fallback: use mount path as both volume path and target
                // This can happen if the volume list hasn't refreshed yet
                onVolumeChange?.(mountPath, mountPath, mountPath)
            }
        } catch (e) {
            mountError = e as MountError
            log.error('Mount failed: {error}', { error: mountError })
        } finally {
            isMounting = false
        }
    }

    // Retry last mount attempt
    function handleMountRetry() {
        if (lastMountAttempt) {
            void handleShareSelect(lastMountAttempt.share, lastMountAttempt.credentials)
        }
    }
    // Helper: Handle navigation result by updating cursor index and scrolling
    // If shiftKey is true, handles range selection; otherwise clears range state
    function applyNavigation(
        newIndex: number,
        listRef: { scrollToIndex: (index: number) => void } | undefined,
        shiftKey = false,
    ) {
        if (shiftKey) {
            handleShiftNavigation(newIndex)
        } else {
            clearRangeState()
        }
        cursorIndex = newIndex
        listRef?.scrollToIndex(newIndex)
        void fetchEntryUnderCursor()
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
            toggleSelectionAt(cursorIndex)
            return true
        }
        // Cmd+A - select all (Cmd+Shift+A - deselect all)
        if (e.key === 'a' && e.metaKey) {
            e.preventDefault()
            if (e.shiftKey) {
                deselectAll()
            } else {
                selectAll()
            }
            return true
        }
        return false
    }

    // Helper: Delegate to network components when in network view
    function handleNetworkKeyDown(e: KeyboardEvent): void {
        if (currentNetworkHost) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            shareBrowserRef?.handleKeyDown(e)
        } else {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            networkBrowserRef?.handleKeyDown(e)
        }
    }

    /** Gets the file entry under the cursor from the current list view */
    function getEntryUnderCursor(): FileEntry | undefined {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-return
        return listRef?.getEntryAt(cursorIndex)
    }

    // Exported so DualPaneExplorer can forward keyboard events
    export function handleKeyDown(e: KeyboardEvent) {
        if (isNetworkView) {
            handleNetworkKeyDown(e)
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
    export function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            clearRangeState()
        }
    }

    // When includeHidden changes, refetch total count
    $effect(() => {
        if (listingId && !loading) {
            void getTotalCount(listingId, includeHidden).then((count) => {
                totalCount = count
                // Reset cursor index if out of bounds
                if (cursorIndex >= effectiveTotalCount) {
                    cursorIndex = 0
                    void fetchEntryUnderCursor()
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

    // Update global menu context when cursor position or focus changes
    $effect(() => {
        if (!isFocused) return
        if (entryUnderCursor && entryUnderCursor.name !== '..') {
            void updateMenuContext(entryUnderCursor.path, entryUnderCursor.name)
        }
    })

    // Re-fetch entry under the cursor when cursorIndex changes
    $effect(() => {
        void cursorIndex // Track
        if (listingId && !loading) {
            void fetchEntryUnderCursor()
        }
    })

    // Re-fetch listing stats when selection changes
    $effect(() => {
        void selectedIndices.size // Track selection changes
        if (listingId && !loading) {
            void fetchListingStats()
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
        void listen<DirectoryDiff>('directory-diff', (event) => {
            const diff = event.payload
            // Only process diffs for our current listing
            if (diff.listingId !== listingId) return

            // Ignore out-of-order events
            if (diff.sequence <= lastSequence) return
            lastSequence = diff.sequence

            // Refetch total count and max filename width, then force the List
            // components to re-fetch their visible range. We always bump
            // cacheGeneration because renames don't change totalCount.
            void Promise.all([
                getTotalCount(listingId, includeHidden),
                getMaxFilenameWidth(listingId, includeHidden),
            ]).then(([count, newMaxWidth]) => {
                totalCount = count
                maxFilenameWidth = newMaxWidth
                cacheGeneration++
                void fetchEntryUnderCursor()
            })
        })
            .then((unsub) => {
                unlisten = unsub
            })
            .catch(() => {
                // Ignore - file watching is optional enhancement
            })

        return () => {
            unlisten?.()
        }
    })

    // Listen for menu action events
    $effect(() => {
        void listen<string>('menu-action', (event) => {
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
            .then((unsub) => {
                unlistenMenuAction = unsub
            })
            .catch(() => {})

        return () => {
            unlistenMenuAction?.()
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

        // Extract device ID from volume ID (e.g., "mtp-2097152:65537" -> "mtp-2097152")
        const deviceIdFromVolume =
            currentIsMtpView && currentVolumeId.includes(':') ? currentVolumeId.split(':')[0] : null

        // Only set up listener if we're viewing an MTP volume with a storage ID
        if (!currentIsMtpView || !deviceIdFromVolume) {
            return
        }

        void onMtpDeviceRemoved((event) => {
            // Check if the removed device matches our current MTP volume
            if (event.deviceId === deviceIdFromVolume) {
                log.warn('MTP device disconnected while viewing: {deviceId}, triggering fallback', {
                    deviceId: event.deviceId,
                })
                onMtpFatalError?.('Device disconnected')
            }
        })
            .then((unsub) => {
                unlistenMtpRemoved = unsub
            })
            .catch(() => {})

        return () => {
            unlistenMtpRemoved?.()
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
        } else {
            log.debug('[FilePane] onMount: SKIPPING loadDirectory for paneId={paneId}', { paneId })
        }

        // Set up sync status polling for visible files
        syncPollInterval = setInterval(() => {
            // List components will call fetchSyncStatusForPaths with their visible entries
        }, SYNC_POLL_INTERVAL_MS)
    })

    onDestroy(() => {
        // Clean up listing
        if (listingId) {
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
        }
        unlisten?.()
        unlistenMenuAction?.()
        unlistenOpening?.()
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()
        unlistenMtpRemoved?.()
        if (syncPollInterval) {
            clearInterval(syncPollInterval)
        }
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
            {#if isMounting}
                <div class="mounting-state">
                    <span class="spinner"></span>
                    <span class="mounting-text">Mounting {currentNetworkHost?.name ?? 'share'}...</span>
                </div>
            {:else if mountError}
                <div class="mount-error-state">
                    <div class="error-icon">❌</div>
                    <div class="error-title">Couldn't mount share</div>
                    <div class="error-message">{mountError.message}</div>
                    <div class="error-actions">
                        <button type="button" class="btn" onclick={handleMountRetry}>Try again</button>
                        <button type="button" class="btn" onclick={handleMountErrorBack}>Back</button>
                    </div>
                </div>
            {:else if currentNetworkHost}
                <ShareBrowser
                    bind:this={shareBrowserRef}
                    host={currentNetworkHost}
                    {isFocused}
                    onShareSelect={handleShareSelect}
                    onBack={handleNetworkBack}
                />
            {:else}
                <NetworkBrowser
                    bind:this={networkBrowserRef}
                    {paneId}
                    {isFocused}
                    onHostSelect={handleNetworkHostSelect}
                />
            {/if}
        {:else if isMtpDeviceOnly}
            <!-- MTP device selected but not yet connected -->
            <div class="mtp-connecting">
                {#if mtpConnecting}
                    <div class="connecting-spinner">
                        <div class="spinner"></div>
                        <span>Connecting to device...</span>
                    </div>
                {:else if mtpConnectionError}
                    <div class="mtp-error">
                        <span class="error-icon">⚠</span>
                        <span class="error-message">{mtpConnectionError}</span>
                        <button
                            type="button"
                            class="btn"
                            onclick={() => {
                                log.info(
                                    'MTP "Try again" clicked. Clearing error to trigger auto-connect. volumeId={volumeId}, isMtpDeviceOnly={isMtpDeviceOnly}, mtpConnectedDeviceId={mtpConnectedDeviceId}',
                                    { volumeId, isMtpDeviceOnly, mtpConnectedDeviceId },
                                )
                                mtpConnectionError = null
                                // Also reset mtpConnectedDeviceId to allow re-triggering auto-connect
                                // even if we previously "connected" to this device
                                mtpConnectedDeviceId = null
                            }}>Try again</button
                        >
                    </div>
                {/if}
            </div>
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
                {selectedIndices}
                {hasParent}
                {maxFilenameWidth}
                {sortBy}
                {sortOrder}
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
                {selectedIndices}
                {hasParent}
                {sortBy}
                {sortOrder}
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
            />
        {/if}
    </div>
    <!-- SelectionInfo shown in both modes (not in network view or MTP connecting state) -->
    {#if !isNetworkView && !isMtpDeviceOnly}
        <SelectionInfo
            {viewMode}
            entry={entryUnderCursor}
            currentDirModifiedAt={undefined}
            stats={listingStats}
            selectedCount={selectedIndices.size}
        />
    {/if}
</div>

<style>
    .file-pane {
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--color-border-primary);
    }

    .header {
        padding: 2px var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        font-size: var(--font-size-xs);
        white-space: nowrap;
        display: flex;
        align-items: center;
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
        padding: var(--spacing-md);
    }

    .mounting-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .mounting-state .spinner {
        width: 24px;
        height: 24px;
        border: 3px solid var(--color-border-primary);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 1s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .mounting-text {
        font-size: var(--font-size-sm);
    }

    .mount-error-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 24px;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .mount-error-state .error-icon {
        font-size: 32px;
    }

    .mount-error-state .error-title {
        font-size: 16px;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .mount-error-state .error-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
        height: auto;
        padding: 0;
    }

    .mount-error-state .error-actions {
        display: flex;
        gap: 8px;
        margin-top: 8px;
    }

    .mount-error-state .btn {
        padding: 8px 16px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .mount-error-state .btn:hover {
        background-color: var(--color-bg-hover);
    }

    /* MTP connecting state */
    .mtp-connecting {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        flex: 1;
        gap: 12px;
        padding: 24px;
    }

    .connecting-spinner {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 12px;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .connecting-spinner .spinner {
        width: 24px;
        height: 24px;
        border: 2px solid var(--color-border-primary);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .mtp-error {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 12px;
        text-align: center;
    }

    .mtp-error .error-icon {
        font-size: 32px;
    }

    .mtp-error .error-message {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        height: auto;
        padding: 0;
    }

    .mtp-error .btn {
        padding: 8px 16px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .mtp-error .btn:hover {
        background-color: var(--color-bg-hover);
    }
</style>
