<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import FilePane from './FilePane.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import {
        loadAppStatus,
        saveAppStatus,
        getLastUsedPathForVolume,
        saveLastUsedPathForVolume,
        getColumnSortOrder,
        saveColumnSortOrder,
        type ViewMode,
    } from '$lib/app-status-store'
    import { loadSettings, saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import {
        pathExists,
        listen,
        listVolumes,
        getDefaultVolumeId,
        findContainingVolume,
        resortListing,
        DEFAULT_VOLUME_ID,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { VolumeInfo, SortColumn, SortOrder, NetworkHost } from './types'
    import { defaultSortOrders, DEFAULT_SORT_BY } from './types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'
    import {
        createHistory,
        push,
        pushPath,
        back,
        forward,
        getCurrentEntry,
        canGoBack,
        canGoForward,
        type NavigationHistory,
    } from './navigation-history'
    import { initNetworkDiscovery, cleanupNetworkDiscovery } from '$lib/network-store.svelte'

    let leftPath = $state('~')
    let rightPath = $state('~')
    let focusedPane = $state<'left' | 'right'>('left')
    let showHiddenFiles = $state(true)
    let leftViewMode = $state<ViewMode>('brief')
    let rightViewMode = $state<ViewMode>('brief')
    let leftVolumeId = $state(DEFAULT_VOLUME_ID)
    let rightVolumeId = $state(DEFAULT_VOLUME_ID)
    let volumes = $state<VolumeInfo[]>([])
    let initialized = $state(false)

    // Sorting state - per-pane
    let leftSortBy = $state<SortColumn>(DEFAULT_SORT_BY)
    let rightSortBy = $state<SortColumn>(DEFAULT_SORT_BY)
    let leftSortOrder = $state<SortOrder>(defaultSortOrders[DEFAULT_SORT_BY])
    let rightSortOrder = $state<SortOrder>(defaultSortOrders[DEFAULT_SORT_BY])

    let containerElement: HTMLDivElement | undefined = $state()
    let leftPaneRef: FilePane | undefined = $state()
    let rightPaneRef: FilePane | undefined = $state()
    let unlistenSettings: UnlistenFn | undefined
    let unlistenViewMode: UnlistenFn | undefined
    let unlistenVolumeMount: UnlistenFn | undefined
    let unlistenVolumeUnmount: UnlistenFn | undefined
    let unlistenNavigation: UnlistenFn | undefined

    // Navigation history for each pane (per-pane, session-only)
    // Initialize with default volume - will be updated on mount with actual state
    let leftHistory = $state<NavigationHistory>(createHistory(DEFAULT_VOLUME_ID, '~'))
    let rightHistory = $state<NavigationHistory>(createHistory(DEFAULT_VOLUME_ID, '~'))

    // Derived volume paths - handle 'network' virtual volume specially
    const leftVolumePath = $derived(
        leftVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === leftVolumeId)?.path ?? '/'),
    )
    const rightVolumePath = $derived(
        rightVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === rightVolumeId)?.path ?? '/'),
    )

    function handleLeftPathChange(path: string) {
        leftPath = path
        // Use pushPath to keep current volumeId (directory navigation within volume)
        leftHistory = pushPath(leftHistory, path)
        void saveAppStatus({ leftPath: path })
        void saveLastUsedPathForVolume(leftVolumeId, path)
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    function handleRightPathChange(path: string) {
        rightPath = path
        // Use pushPath to keep current volumeId (directory navigation within volume)
        rightHistory = pushPath(rightHistory, path)
        void saveAppStatus({ rightPath: path })
        void saveLastUsedPathForVolume(rightVolumeId, path)
        // Re-focus to maintain keyboard handling after navigation
        containerElement?.focus()
    }

    // Handle network host selection changes (for history tracking)
    function handleLeftNetworkHostChange(host: NetworkHost | null) {
        // Push to history with network host state
        leftHistory = push(leftHistory, {
            volumeId: 'network',
            path: 'smb://',
            networkHost: host ?? undefined,
        })
        containerElement?.focus()
    }

    function handleRightNetworkHostChange(host: NetworkHost | null) {
        // Push to history with network host state
        rightHistory = push(rightHistory, {
            volumeId: 'network',
            path: 'smb://',
            networkHost: host ?? undefined,
        })
        containerElement?.focus()
    }

    /**
     * Handles sorting column click for left pane.
     * If clicking the same column, toggles order. Otherwise, switches to new column with its default order.
     */
    async function handleLeftSortChange(newColumn: SortColumn) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = leftPaneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        let newOrder: SortOrder
        if (newColumn === leftSortBy) {
            // Toggle order
            newOrder = leftSortOrder === 'ascending' ? 'descending' : 'ascending'
        } else {
            // New column - use remembered or default order
            newOrder = await getColumnSortOrder(newColumn)
        }

        // Get current cursor filename to track position
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const cursorFilename = leftPaneRef?.getSelectedFilename?.() as string | undefined

        // Re-sort the backend listing
        const result = await resortListing(listingId, newColumn, newOrder, cursorFilename, showHiddenFiles)

        // Update state
        leftSortBy = newColumn
        leftSortOrder = newOrder

        // Persist
        void saveAppStatus({ leftSortBy: newColumn })
        void saveColumnSortOrder(newColumn, newOrder)

        // Update cursor position after re-sort
        if (result.newCursorIndex !== undefined) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            leftPaneRef?.setSelectedIndex?.(result.newCursorIndex)
        }

        // Refresh the view
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        leftPaneRef?.refreshView?.()
    }

    /**
     * Handles sorting column click for right pane.
     */
    async function handleRightSortChange(newColumn: SortColumn) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = rightPaneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        let newOrder: SortOrder
        if (newColumn === rightSortBy) {
            // Toggle order
            newOrder = rightSortOrder === 'ascending' ? 'descending' : 'ascending'
        } else {
            // New column - use remembered or default order
            newOrder = await getColumnSortOrder(newColumn)
        }

        // Get current cursor filename to track position
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const cursorFilename = rightPaneRef?.getSelectedFilename?.() as string | undefined

        // Re-sort the backend listing
        const result = await resortListing(listingId, newColumn, newOrder, cursorFilename, showHiddenFiles)

        // Update state
        rightSortBy = newColumn
        rightSortOrder = newOrder

        // Persist
        void saveAppStatus({ rightSortBy: newColumn })
        void saveColumnSortOrder(newColumn, newOrder)

        // Update cursor position after re-sort
        if (result.newCursorIndex !== undefined) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            rightPaneRef?.setSelectedIndex?.(result.newCursorIndex)
        }

        // Refresh the view
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        rightPaneRef?.refreshView?.()
    }

    async function handleLeftVolumeChange(volumeId: string, volumePath: string, targetPath: string) {
        // Save the current path for the old volume before switching
        void saveLastUsedPathForVolume(leftVolumeId, leftPath)

        // If this is a new volume (e.g., freshly mounted network share), refresh volume list first
        const found = volumes.find((v) => v.id === volumeId)
        if (!found) {
            volumes = await listVolumes()
        }

        // Pass the right pane's state so we can copy its path if it's on the same volume
        const pathToNavigate = await determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: rightVolumeId,
            otherPanePath: rightPath,
        })

        leftVolumeId = volumeId
        leftPath = pathToNavigate

        // Push volume change to history (this enables back/forward across volumes)
        leftHistory = push(leftHistory, { volumeId, path: pathToNavigate })

        // Focus the left pane after successful volume selection
        focusedPane = 'left'
        void saveAppStatus({ leftVolumeId: volumeId, leftPath: pathToNavigate, focusedPane: 'left' })
    }

    async function handleRightVolumeChange(volumeId: string, volumePath: string, targetPath: string) {
        // Save the current path for the old volume before switching
        void saveLastUsedPathForVolume(rightVolumeId, rightPath)

        // If this is a new volume (e.g., freshly mounted network share), refresh volume list first
        if (!volumes.find((v) => v.id === volumeId)) {
            volumes = await listVolumes()
        }

        // Pass the left pane's state so we can copy its path if it's on the same volume
        const pathToNavigate = await determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: leftVolumeId,
            otherPanePath: leftPath,
        })

        rightVolumeId = volumeId
        rightPath = pathToNavigate

        // Push volume change to history (this enables back/forward across volumes)
        rightHistory = push(rightHistory, { volumeId, path: pathToNavigate })

        // Focus the right pane after successful volume selection
        focusedPane = 'right'
        void saveAppStatus({ rightVolumeId: volumeId, rightPath: pathToNavigate, focusedPane: 'right' })
    }

    interface OtherPaneState {
        otherPaneVolumeId: string
        otherPanePath: string
    }

    /**
     * Determines which path to navigate to when switching volumes.
     * Priority order:
     * 1. Favorite path (if targetPath !== volumePath)
     * 2. Other pane's path (if the other pane is on the same volume)
     * 3. Stored lastUsedPath for this volume
     * 4. Default: ~ for main volume, volume root for others
     */
    async function determineNavigationPath(
        volumeId: string,
        volumePath: string,
        targetPath: string,
        otherPane: OtherPaneState,
    ): Promise<string> {
        // User selected a favorite - go to the favorite's path directly
        if (targetPath !== volumePath) {
            return targetPath
        }

        // If the other pane is on the same volume, use its path (allows copying paths between panes)
        if (otherPane.otherPaneVolumeId === volumeId && (await pathExists(otherPane.otherPanePath))) {
            return otherPane.otherPanePath
        }

        // Look up the last used path for this volume
        const lastUsedPath = await getLastUsedPathForVolume(volumeId)
        if (lastUsedPath && (await pathExists(lastUsedPath))) {
            return lastUsedPath
        }

        // Default: ~ for main volume (root), volume path for others
        if (volumeId === DEFAULT_VOLUME_ID) {
            return '~'
        }
        return volumePath
    }

    function handleLeftFocus() {
        if (focusedPane !== 'left') {
            focusedPane = 'left'
            void saveAppStatus({ focusedPane: 'left' })
        }
    }

    function handleRightFocus() {
        if (focusedPane !== 'right') {
            focusedPane = 'right'
            void saveAppStatus({ focusedPane: 'right' })
        }
    }
    // Helper: Route key event to any open volume chooser
    // Returns true if the event was handled by a volume chooser
    function routeToVolumeChooser(e: KeyboardEvent): boolean {
        // Check if EITHER pane has a volume chooser open - if so, route events there
        // This is important because F1/F2 can open a volume chooser on the non-focused pane
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (leftPaneRef?.isVolumeChooserOpen?.()) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            if (leftPaneRef.handleVolumeChooserKeyDown?.(e)) {
                return true
            }
        }
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (rightPaneRef?.isVolumeChooserOpen?.()) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            if (rightPaneRef.handleVolumeChooserKeyDown?.(e)) {
                return true
            }
        }
        return false
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Tab') {
            e.preventDefault()
            const newFocus = focusedPane === 'left' ? 'right' : 'left'
            focusedPane = newFocus
            void saveAppStatus({ focusedPane: newFocus })
            return
        }

        // F1 or ⌥F1 - Open left pane volume chooser
        if (e.key === 'F1') {
            e.preventDefault()
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            leftPaneRef?.toggleVolumeChooser()
            return
        }

        // F2 or ⌥F2 - Open right pane volume chooser
        if (e.key === 'F2') {
            e.preventDefault()
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            rightPaneRef?.toggleVolumeChooser()
            return
        }

        // Route to volume chooser if one is open
        if (routeToVolumeChooser(e)) {
            return
        }

        // Forward arrow keys and Enter to the focused pane
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion -- TypeScript thinks FilePane methods are unused without this
        const activePaneRef = (focusedPane === 'left' ? leftPaneRef : rightPaneRef) as FilePane | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        activePaneRef?.handleKeyDown(e)
    }

    onMount(async () => {
        // Start font metrics measurement in background (non-blocking)
        void ensureFontMetricsLoaded()

        // Start network discovery in background (non-blocking)
        void initNetworkDiscovery()

        // Load volumes first
        volumes = await listVolumes()

        // Load persisted state and settings in parallel
        const [status, settings] = await Promise.all([loadAppStatus(pathExists), loadSettings()])

        leftPath = status.leftPath
        rightPath = status.rightPath
        focusedPane = status.focusedPane
        showHiddenFiles = settings.showHiddenFiles
        leftViewMode = status.leftViewMode
        rightViewMode = status.rightViewMode

        // Load sort state
        leftSortBy = status.leftSortBy
        rightSortBy = status.rightSortBy
        // Load remembered sort orders for each column
        leftSortOrder = await getColumnSortOrder(leftSortBy)
        rightSortOrder = await getColumnSortOrder(rightSortBy)

        // Determine the correct volume IDs by finding which volume contains each path
        // This is more reliable than trusting the stored volumeId, which may be stale
        // Exception: 'network' is a virtual volume, trust the stored ID for that
        const defaultId = await getDefaultVolumeId()

        if (status.leftVolumeId === 'network') {
            leftVolumeId = 'network'
        } else {
            const leftContaining = await findContainingVolume(status.leftPath)
            leftVolumeId = leftContaining?.id ?? defaultId
        }

        if (status.rightVolumeId === 'network') {
            rightVolumeId = 'network'
        } else {
            const rightContaining = await findContainingVolume(status.rightPath)
            rightVolumeId = rightContaining?.id ?? defaultId
        }

        // Initialize history with loaded paths and their volumes
        leftHistory = createHistory(leftVolumeId, status.leftPath)
        rightHistory = createHistory(rightVolumeId, status.rightPath)

        initialized = true

        // Subscribe to settings changes from the backend menu
        unlistenSettings = await subscribeToSettingsChanges((newSettings) => {
            if (newSettings.showHiddenFiles !== undefined) {
                showHiddenFiles = newSettings.showHiddenFiles
                // Persist to settings store
                void saveSettings({ showHiddenFiles: newSettings.showHiddenFiles })
            }
        })

        // Subscribe to view mode changes from the backend menu
        unlistenViewMode = await listen<{ mode: ViewMode }>('view-mode-changed', (event) => {
            const newMode = event.payload.mode
            // Apply to the focused pane
            if (focusedPane === 'left') {
                leftViewMode = newMode
                void saveAppStatus({ leftViewMode: newMode })
            } else {
                rightViewMode = newMode
                void saveAppStatus({ rightViewMode: newMode })
            }
        })

        // Subscribe to volume mount events (refresh volume list when new volumes appear)
        unlistenVolumeMount = await listen<{ volumePath: string }>('volume-mounted', () => {
            void (async () => {
                volumes = await listVolumes()
            })()
        })

        // Subscribe to volume unmount events
        unlistenVolumeUnmount = await listen<{ volumePath: string }>('volume-unmounted', (event) => {
            void (async () => {
                // Find the volume ID from the path
                const volume = volumes.find((v) => v.path === event.payload.volumePath)
                if (volume) {
                    void handleVolumeUnmount(volume.id)
                } else {
                    // Volume already gone, just refresh the list
                    volumes = await listVolumes()
                }
            })()
        })

        // Subscribe to navigation actions from Go menu
        unlistenNavigation = await listen<{ action: string }>('navigation-action', (event) => {
            void handleNavigationAction(event.payload.action)
        })
    })

    async function handleVolumeUnmount(unmountedId: string) {
        const defaultVolumeId = await getDefaultVolumeId()
        const defaultVolume = volumes.find((v) => v.id === defaultVolumeId)
        const defaultPath = defaultVolume?.path ?? '/'

        // Switch affected panes to default volume
        if (leftVolumeId === unmountedId) {
            leftVolumeId = defaultVolumeId
            leftPath = defaultPath
            void saveAppStatus({ leftVolumeId: defaultVolumeId, leftPath: defaultPath })
        }
        if (rightVolumeId === unmountedId) {
            rightVolumeId = defaultVolumeId
            rightPath = defaultPath
            void saveAppStatus({ rightVolumeId: defaultVolumeId, rightPath: defaultPath })
        }

        // Refresh volume list
        volumes = await listVolumes()
    }

    /**
     * Resolves a path to a valid existing path by walking up the parent tree.
     * Returns null if even the root doesn't exist (volume unmounted).
     */
    async function resolveValidPath(targetPath: string): Promise<string | null> {
        let path = targetPath
        while (path !== '/' && path !== '') {
            if (await pathExists(path)) {
                return path
            }
            // Go to parent
            const lastSlash = path.lastIndexOf('/')
            path = lastSlash > 0 ? path.substring(0, lastSlash) : '/'
        }
        // Check root
        if (await pathExists('/')) {
            return '/'
        }
        return null
    }

    /**
     * Updates pane state after navigating back/forward (restores full state from history entry).
     * This includes both path AND volumeId changes - enabling back/forward across volumes.
     */
    function updatePaneAfterHistoryNavigation(isLeft: boolean, newHistory: NavigationHistory, targetPath: string) {
        const entry = getCurrentEntry(newHistory)
        const paneRef = isLeft ? leftPaneRef : rightPaneRef

        if (isLeft) {
            leftHistory = newHistory
            leftPath = targetPath
            // Restore volume context if it changed
            if (entry.volumeId !== leftVolumeId) {
                leftVolumeId = entry.volumeId
                void saveAppStatus({ leftVolumeId: entry.volumeId, leftPath: targetPath })
            } else {
                void saveAppStatus({ leftPath: targetPath })
            }
            void saveLastUsedPathForVolume(entry.volumeId, targetPath)
        } else {
            rightHistory = newHistory
            rightPath = targetPath
            // Restore volume context if it changed
            if (entry.volumeId !== rightVolumeId) {
                rightVolumeId = entry.volumeId
                void saveAppStatus({ rightVolumeId: entry.volumeId, rightPath: targetPath })
            } else {
                void saveAppStatus({ rightPath: targetPath })
            }
            void saveLastUsedPathForVolume(entry.volumeId, targetPath)
        }

        // Restore network host state if navigating within network volume
        if (entry.volumeId === 'network') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef?.setNetworkHost?.(entry.networkHost ?? null)
        }

        containerElement?.focus()
    }

    /**
     * Handles navigation actions from the Go menu (back/forward/parent).
     */
    async function handleNavigationAction(action: string) {
        const isLeft = focusedPane === 'left'
        const paneRef = isLeft ? leftPaneRef : rightPaneRef

        if (action === 'parent') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            await paneRef?.navigateToParent()
            return
        }

        const history = isLeft ? leftHistory : rightHistory
        let newHistory: NavigationHistory

        if (action === 'back' && canGoBack(history)) {
            newHistory = back(history)
        } else if (action === 'forward' && canGoForward(history)) {
            newHistory = forward(history)
        } else {
            return
        }

        // Get the target entry (includes volumeId, path, and network state)
        const targetEntry = getCurrentEntry(newHistory)

        // For network virtual volume, path resolution doesn't apply
        // (network browser handles its own state)
        if (targetEntry.volumeId === 'network') {
            updatePaneAfterHistoryNavigation(isLeft, newHistory, targetEntry.path)
            return
        }

        // For real volumes, resolve path to handle deleted folders
        const resolvedPath = await resolveValidPath(targetEntry.path)
        if (resolvedPath !== null) {
            updatePaneAfterHistoryNavigation(isLeft, newHistory, resolvedPath)
        }
    }

    onDestroy(() => {
        unlistenSettings?.()
        unlistenViewMode?.()
        unlistenVolumeMount?.()
        unlistenVolumeUnmount?.()
        unlistenNavigation?.()
        cleanupNetworkDiscovery()
    })

    // Focus the container after initialization so keyboard events work
    $effect(() => {
        if (initialized) {
            containerElement?.focus()
        }
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex,a11y_no_noninteractive_element_interactions -->
<div
    class="dual-pane-explorer"
    bind:this={containerElement}
    onkeydown={handleKeyDown}
    tabindex="0"
    role="application"
    aria-label="File explorer"
>
    {#if initialized}
        <FilePane
            bind:this={leftPaneRef}
            initialPath={leftPath}
            volumeId={leftVolumeId}
            volumePath={leftVolumePath}
            isFocused={focusedPane === 'left'}
            {showHiddenFiles}
            viewMode={leftViewMode}
            sortBy={leftSortBy}
            sortOrder={leftSortOrder}
            onPathChange={handleLeftPathChange}
            onVolumeChange={handleLeftVolumeChange}
            onRequestFocus={handleLeftFocus}
            onSortChange={handleLeftSortChange}
            onNetworkHostChange={handleLeftNetworkHostChange}
        />
        <FilePane
            bind:this={rightPaneRef}
            initialPath={rightPath}
            volumeId={rightVolumeId}
            volumePath={rightVolumePath}
            isFocused={focusedPane === 'right'}
            {showHiddenFiles}
            viewMode={rightViewMode}
            sortBy={rightSortBy}
            sortOrder={rightSortOrder}
            onPathChange={handleRightPathChange}
            onVolumeChange={handleRightVolumeChange}
            onRequestFocus={handleRightFocus}
            onSortChange={handleRightSortChange}
            onNetworkHostChange={handleRightNetworkHostChange}
        />
    {:else}
        <LoadingIcon />
    {/if}
</div>

<style>
    .dual-pane-explorer {
        display: flex;
        width: 100%;
        height: 100%;
        gap: 0;
        outline: none;
    }
</style>
