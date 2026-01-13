<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import type {
        DirectoryDiff,
        FileEntry,
        ListingCancelledEvent,
        ListingCompleteEvent,
        ListingErrorEvent,
        ListingProgressEvent,
        ListingReadCompleteEvent,
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
        getMaxFilenameWidth,
        getSyncStatus,
        getTotalCount,
        listDirectoryEnd,
        listDirectoryStartStreaming,
        listen,
        listVolumes,
        mountNetworkShare,
        openFile,
        showFileContextMenu,
        type UnlistenFn,
        updateMenuContext,
        updateLeftPaneState,
        updateRightPaneState,
        type PaneState,
        type PaneFileEntry,
    } from '$lib/tauri-commands'
    import type { ViewMode } from '$lib/app-status-store'
    import FullList from './FullList.svelte'
    import BriefList from './BriefList.svelte'
    import SelectionInfo from './SelectionInfo.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
    import PermissionDeniedPane from './PermissionDeniedPane.svelte'
    import NetworkBrowser from './NetworkBrowser.svelte'
    import ShareBrowser from './ShareBrowser.svelte'
    import * as benchmark from '$lib/benchmark'
    import { handleNavigationShortcut } from './keyboard-shortcuts'

    interface Props {
        initialPath: string
        paneId?: 'left' | 'right'
        volumeId?: string
        volumePath?: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        viewMode?: ViewMode
        sortBy?: SortColumn
        sortOrder?: SortOrder
        onPathChange?: (path: string) => void
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
        onSortChange?: (column: SortColumn) => void
        onRequestFocus?: () => void
        /** Called when network host selection changes (for history tracking) */
        onNetworkHostChange?: (host: NetworkHost | null) => void
        /** Called when user cancels loading (ESC key) - parent should navigate back */
        onCancelLoading?: () => void
    }

    const {
        initialPath,
        paneId,
        volumeId = 'root',
        volumePath = '/',
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
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // New architecture: store listingId and totalCount, not files
    let listingId = $state('')
    let totalCount = $state(0)
    let maxFilenameWidth = $state<number | undefined>(undefined)
    let loading = $state(true)
    let error = $state<string | null>(null)
    let selectedIndex = $state(0)

    // Selected entry fetched separately for SelectionInfo
    let selectedEntry = $state<FileEntry | null>(null)

    // Component refs for keyboard navigation
    let fullListRef: FullList | undefined = $state()
    let briefListRef: BriefList | undefined = $state()
    let volumeBreadcrumbRef: VolumeBreadcrumb | undefined = $state()
    let networkBrowserRef: NetworkBrowser | undefined = $state()
    let shareBrowserRef: ShareBrowser | undefined = $state()

    // Check if we're viewing the network (special virtual volume)
    const isNetworkView = $derived(volumeId === 'network')

    // Network browsing state - which host is selected (if any)
    let selectedNetworkHost = $state<NetworkHost | null>(null)

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

    // Get selected filename for cursor tracking during re-sort
    export function getSelectedFilename(): string | undefined {
        return selectedEntry?.name
    }

    // Set selected index directly (for cursor tracking after re-sort)
    export function setSelectedIndex(index: number): void {
        selectedIndex = index
        void fetchSelectedEntry()
    }

    // Cache generation counter - incremented to force list components to re-fetch
    let cacheGeneration = $state(0)

    // Force refresh the view by incrementing cache generation
    export function refreshView(): void {
        cacheGeneration++
    }

    // Set network host state (for history navigation)
    export function setNetworkHost(host: NetworkHost | null): void {
        selectedNetworkHost = host
        mountError = null
        lastMountAttempt = null
    }

    // Get current network host (for history tracking)
    export function getNetworkHost(): NetworkHost | null {
        return selectedNetworkHost
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
    let unlistenProgress: UnlistenFn | undefined
    let unlistenComplete: UnlistenFn | undefined
    let unlistenError: UnlistenFn | undefined
    let unlistenCancelled: UnlistenFn | undefined
    // Loading progress state for streaming
    let loadingCount = $state<number | undefined>(undefined)
    // Finalizing state (read_dir done, now sorting/caching)
    let finalizingCount = $state<number | undefined>(undefined)
    let unlistenReadComplete: UnlistenFn | undefined
    // Polling interval for sync status (visible files only)
    let syncPollInterval: ReturnType<typeof setInterval> | undefined
    const SYNC_POLL_INTERVAL_MS = 2000 // Poll every 2 seconds

    // Sync status map for visible files
    let syncStatusMap = $state<Record<string, SyncStatus>>({})

    // Derive includeHidden from showHiddenFiles prop
    const includeHidden = $derived(showHiddenFiles)

    /**
     * Sync pane state to Rust for MCP context tools.
     * Called when files load, selection changes, or view mode changes.
     */
    async function syncPaneStateToMcp() {
        if (!paneId) return // No pane ID, can't sync

        try {
            // Build file list from current state
            const files: PaneFileEntry[] = []

            // For network views, we don't sync files
            if (!isNetworkView && listingId && totalCount > 0) {
                // Get visible files - for now, just get first 100 for context
                const maxToFetch = Math.min(totalCount, 100)
                for (let i = 0; i < maxToFetch; i++) {
                    const backendIndex = hasParent ? i : i
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
            }

            const state: PaneState = {
                path: currentPath,
                volumeId,
                files,
                selectedIndex,
                viewMode,
            }

            if (paneId === 'left') {
                await updateLeftPaneState(state)
            } else {
                await updateRightPaneState(state)
            }
        } catch {
            // Silently ignore sync errors - MCP is optional
        }
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
    const hasParent = $derived(currentPath !== '/' && currentPath !== volumePath)

    // Effective total count includes ".." entry if not at root
    const effectiveTotalCount = $derived(hasParent ? totalCount + 1 : totalCount)

    async function loadDirectory(path: string, selectName?: string) {
        // Reset benchmark epoch for this navigation
        benchmark.resetEpoch()
        benchmark.logEventValue('loadDirectory CALLED', path)

        // Increment generation to cancel any in-flight requests
        const thisGeneration = ++loadGeneration

        // Cancel any abandoned listing from previous navigation
        if (listingId) {
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
            listingId = ''
            lastSequence = 0
        }

        // Clean up previous event listeners
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()

        // Set loading state BEFORE starting IPC call
        // This ensures the UI shows the loading spinner immediately
        loading = true
        loadingCount = undefined
        finalizingCount = undefined
        error = null
        syncStatusMap = {}
        totalCount = 0 // Reset to show empty list immediately
        selectedEntry = null // Clear old selection

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
                    loadingCount = undefined
                    finalizingCount = undefined
                }
            })

            // Subscribe to cancelled event
            unlistenCancelled = await listen<ListingCancelledEvent>('listing-cancelled', (event) => {
                if (event.payload.listingId === newListingId && thisGeneration === loadGeneration) {
                    // Cancellation handled by onCancelLoading callback
                    listingId = ''
                    loading = false
                    loadingCount = undefined
                    finalizingCount = undefined
                }
            })

            // Now start streaming listing - listeners are already set up
            benchmark.logEvent('IPC listDirectoryStartStreaming CALL')
            const result = await listDirectoryStartStreaming(path, includeHidden, sortBy, sortOrder, newListingId)
            benchmark.logEventValue('IPC listDirectoryStartStreaming RETURNED', result.listingId)

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

        // Determine initial selection
        if (loadSelectName) {
            const foundIndex = await findFileIndex(listingId, loadSelectName, includeHidden)
            const adjustedIndex = hasParent ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
            selectedIndex = adjustedIndex >= 0 ? adjustedIndex : 0
        } else {
            selectedIndex = 0
        }

        loading = false
        loadingCount = undefined
        finalizingCount = undefined
        benchmark.logEvent('loading = false (UI can render)')

        // NOW push to history (only on successful completion)
        onPathChange?.(loadPath)

        // Fetch selected entry for SelectionInfo
        void fetchSelectedEntry()

        // Sync state to MCP for context tools
        void syncPaneStateToMcp()

        // Scroll to selection after DOM updates
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(selectedIndex)
        })
    }

    // Handle cancellation during loading (called from DualPaneExplorer on ESC)
    export function handleCancelLoading() {
        if (!loading || !listingId) return

        // Cancel the Rust-side operation
        void cancelListing(listingId)

        // Navigate back or to home via callback
        onCancelLoading?.()
    }

    // Fetch the currently selected entry for SelectionInfo
    async function fetchSelectedEntry() {
        if (!listingId) {
            selectedEntry = null
            return
        }

        // Handle ".." entry specially
        if (hasParent && selectedIndex === 0) {
            selectedEntry = createParentEntry(currentPath)
            return
        }

        // Adjust index for ".." entry
        const backendIndex = hasParent ? selectedIndex - 1 : selectedIndex

        try {
            selectedEntry = await getFileAt(listingId, backendIndex, includeHidden)
        } catch {
            selectedEntry = null
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

    function handleSelect(index: number) {
        selectedIndex = index
        onRequestFocus?.()
        void fetchSelectedEntry()
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

        // Don't load directory for network virtual volume - NetworkBrowser handles its own data
        if (newVolumeId !== 'network') {
            void loadDirectory(targetPath)
        }
    }

    // Handle network host selection - show the ShareBrowser
    function handleNetworkHostSelect(host: NetworkHost) {
        selectedNetworkHost = host
        onNetworkHostChange?.(host)
    }

    // Handle going back from ShareBrowser to network host list
    function handleNetworkBack() {
        selectedNetworkHost = null
        mountError = null
        lastMountAttempt = null
        onNetworkHostChange?.(null)
    }

    // Handle going back from mount error to share list
    function handleMountErrorBack() {
        mountError = null
        // Stay on the share list (selectedNetworkHost remains set)
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
        if (!selectedNetworkHost) return

        // Store for retry
        lastMountAttempt = { share, credentials }

        isMounting = true
        mountError = null

        try {
            // Get server address - prefer IP, fall back to hostname
            const server = selectedNetworkHost.ipAddress ?? selectedNetworkHost.hostname ?? selectedNetworkHost.name

            // Use provided credentials if available
            const result = await mountNetworkShare(
                server,
                share.name,
                credentials?.username ?? null,
                credentials?.password ?? null,
            )

            // Navigate to the mounted share
            // Clear network host selection first
            selectedNetworkHost = null
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
            // eslint-disable-next-line no-console
            console.error('Mount failed:', mountError)
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
    // Helper: Handle navigation result by updating selection and scrolling
    function applyNavigation(newIndex: number, listRef: { scrollToIndex: (index: number) => void } | undefined) {
        selectedIndex = newIndex
        listRef?.scrollToIndex(newIndex)
        void fetchSelectedEntry()
    }

    // Helper: Handle brief mode key navigation
    function handleBriefModeKeys(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const newIndex: number | undefined = briefListRef?.handleKeyNavigation(e.key, e)
        if (newIndex !== undefined) {
            e.preventDefault()
            applyNavigation(newIndex, briefListRef)
            return true
        }
        return false
    }

    // Helper: Handle full mode key navigation
    function handleFullModeKeys(e: KeyboardEvent): boolean {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const visibleItems: number = fullListRef?.getVisibleItemsCount() ?? 20
        const shortcutResult = handleNavigationShortcut(e, {
            currentIndex: selectedIndex,
            totalCount: effectiveTotalCount,
            visibleItems,
        })
        if (shortcutResult) {
            e.preventDefault()
            applyNavigation(shortcutResult.newIndex, fullListRef)
            return true
        }

        // Handle arrow navigation
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            applyNavigation(Math.min(selectedIndex + 1, effectiveTotalCount - 1), fullListRef)
            return true
        }
        if (e.key === 'ArrowUp') {
            e.preventDefault()
            applyNavigation(Math.max(selectedIndex - 1, 0), fullListRef)
            return true
        }
        // Left/Right arrows jump to first/last (same as Brief mode at boundaries)
        if (e.key === 'ArrowLeft') {
            e.preventDefault()
            applyNavigation(0, fullListRef)
            return true
        }
        if (e.key === 'ArrowRight') {
            e.preventDefault()
            applyNavigation(effectiveTotalCount - 1, fullListRef)
            return true
        }
        return false
    }

    // Exported so DualPaneExplorer can forward keyboard events
    export function handleKeyDown(e: KeyboardEvent) {
        // Delegate to network components when in network view
        if (isNetworkView) {
            if (selectedNetworkHost) {
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                shareBrowserRef?.handleKeyDown(e)
            } else {
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                networkBrowserRef?.handleKeyDown(e)
            }
            return
        }

        // Handle Enter key - navigate into selected item
        // Use the list component's cached entry instead of selectedEntry to avoid race conditions
        // (selectedEntry is fetched asynchronously and may not be ready yet)
        if (e.key === 'Enter') {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-assignment
            const entry: FileEntry | undefined = listRef?.getEntryAt(selectedIndex)
            if (entry) {
                e.preventDefault()
                void handleNavigate(entry)
                return
            }
        }

        // Handle Backspace or ⌘↑ - go to parent directory (not available in network view)
        if ((e.key === 'Backspace' || (e.key === 'ArrowUp' && e.metaKey)) && hasParent) {
            e.preventDefault()
            void navigateToParent()
            return
        }

        // Delegate to view-mode-specific handler
        if (viewMode === 'brief') {
            handleBriefModeKeys(e)
        } else {
            handleFullModeKeys(e)
        }
    }

    // When includeHidden changes, refetch total count
    $effect(() => {
        if (listingId && !loading) {
            void getTotalCount(listingId, includeHidden).then((count) => {
                totalCount = count
                // Reset selection if out of bounds
                if (selectedIndex >= effectiveTotalCount) {
                    selectedIndex = 0
                    void fetchSelectedEntry()
                }
            })
        }
    })

    // Update path when initialPath prop changes (for persistence loading)
    // Skip for network view - NetworkBrowser handles its own data
    // Use untrack for currentPath so this effect only fires when initialPath changes,
    // not when the user navigates (which changes currentPath before onPathChange is called)
    $effect(() => {
        const newPath = initialPath // Track this
        const curPath = untrack(() => currentPath) // Don't track this
        if (!isNetworkView && newPath !== curPath) {
            currentPath = newPath
            void loadDirectory(newPath)
        }
    })

    // Update global menu context when selection or focus changes
    $effect(() => {
        if (!isFocused) return
        if (selectedEntry && selectedEntry.name !== '..') {
            void updateMenuContext(selectedEntry.path, selectedEntry.name)
        }
    })

    // Re-fetch selected entry when selectedIndex changes
    $effect(() => {
        void selectedIndex // Track
        if (listingId && !loading) {
            void fetchSelectedEntry()
        }
    })

    // Scroll selected item into view when view mode changes
    $effect(() => {
        void viewMode
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(selectedIndex)
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

            // Refetch total count and max filename width - the List components
            // will refetch their visible range on the next render
            void Promise.all([
                getTotalCount(listingId, includeHidden),
                getMaxFilenameWidth(listingId, includeHidden),
            ]).then(([count, newMaxWidth]) => {
                totalCount = count
                maxFilenameWidth = newMaxWidth
                // Re-fetch selected entry as it may have changed
                void fetchSelectedEntry()
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
                const entry: FileEntry | undefined = listRef?.getEntryAt(selectedIndex)
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

    onMount(() => {
        // Skip directory loading for network view - NetworkBrowser handles its own data
        if (!isNetworkView) {
            void loadDirectory(currentPath)
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
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()
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
            >{currentPath.startsWith(volumePath) ? currentPath.slice(volumePath.length) || '/' : currentPath}</span
        >
    </div>
    <div class="content">
        {#if isNetworkView}
            {#if isMounting}
                <div class="mounting-state">
                    <span class="spinner"></span>
                    <span class="mounting-text">Mounting {selectedNetworkHost?.name ?? 'share'}...</span>
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
            {:else if selectedNetworkHost}
                <ShareBrowser
                    bind:this={shareBrowserRef}
                    host={selectedNetworkHost}
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
        {:else if loading}
            <LoadingIcon loadedCount={loadingCount} {finalizingCount} showCancelHint={true} />
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
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
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
            />
        {:else}
            <FullList
                bind:this={fullListRef}
                {listingId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {cacheGeneration}
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
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
            />
        {/if}
    </div>
    <!-- SelectionInfo shown in brief mode (not in network view) -->
    {#if viewMode === 'brief' && !isNetworkView}
        <SelectionInfo entry={selectedEntry} currentDirModifiedAt={undefined} />
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
</style>
