<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import type { DirectoryDiff, FileEntry, SyncStatus } from './types'
    import {
        findFileIndex,
        getFileAt,
        getSyncStatus,
        getTotalCount,
        listDirectoryEnd,
        listDirectoryStart,
        listen,
        openFile,
        showFileContextMenu,
        type UnlistenFn,
        updateMenuContext,
    } from '$lib/tauri-commands'
    import type { ViewMode } from '$lib/app-status-store'
    import FullList from './FullList.svelte'
    import BriefList from './BriefList.svelte'
    import SelectionInfo from './SelectionInfo.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
    import PermissionDeniedPane from './PermissionDeniedPane.svelte'
    import * as benchmark from '$lib/benchmark'
    import { handleNavigationShortcut } from './keyboard-shortcuts'

    interface Props {
        initialPath: string
        volumeId?: string
        volumePath?: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        viewMode?: ViewMode
        onPathChange?: (path: string) => void
        onVolumeChange?: (volumeId: string, volumePath: string) => void
        onRequestFocus?: () => void
    }

    const {
        initialPath,
        volumeId = 'root',
        volumePath = '/',
        isFocused = false,
        showHiddenFiles = true,
        viewMode = 'brief',
        onPathChange,
        onVolumeChange,
        onRequestFocus,
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

    // Export method for keyboard shortcut
    export function toggleVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        volumeBreadcrumbRef?.toggle()
    }

    // Track the current load operation to cancel outdated ones
    let loadGeneration = 0
    // Track last sequence for file watcher diffs
    let lastSequence = 0
    let unlisten: UnlistenFn | undefined
    let unlistenMenuAction: UnlistenFn | undefined
    // Polling interval for sync status (visible files only)
    let syncPollInterval: ReturnType<typeof setInterval> | undefined
    const SYNC_POLL_INTERVAL_MS = 2000 // Poll every 2 seconds

    // Sync status map for visible files
    let syncStatusMap = $state<Record<string, SyncStatus>>({})

    // Derive includeHidden from showHiddenFiles prop
    const includeHidden = $derived(showHiddenFiles)

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

        // End previous listing when navigating away
        if (listingId) {
            void listDirectoryEnd(listingId)
            listingId = ''
            lastSequence = 0
        }

        // Set loading state BEFORE starting expensive IPC call
        // This ensures the UI shows the loading spinner immediately
        loading = true
        error = null
        syncStatusMap = {}
        totalCount = 0 // Reset to show empty list immediately
        selectedEntry = null // Clear old selection

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
            // Start listing - returns just listingId and totalCount (no entries!)
            benchmark.logEvent('IPC listDirectoryStart CALL')
            const result = await listDirectoryStart(path, includeHidden)
            benchmark.logEventValue('IPC listDirectoryStart RETURNED, totalCount', result.totalCount)

            // Check if this load was cancelled
            if (thisGeneration !== loadGeneration) {
                // Clean up abandoned listing
                void listDirectoryEnd(result.listingId)
                return
            }

            // Store listing info
            listingId = result.listingId
            totalCount = result.totalCount
            maxFilenameWidth = result.maxFilenameWidth
            lastSequence = 0

            // Determine initial selection
            if (selectName) {
                // Find the index of the folder we came from
                const foundIndex = await findFileIndex(listingId, selectName, includeHidden)
                // Account for ".." entry at index 0 if present
                const adjustedIndex = hasParent ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
                selectedIndex = adjustedIndex >= 0 ? adjustedIndex : 0
            } else {
                selectedIndex = 0
            }

            loading = false
            benchmark.logEvent('loading = false (UI can render)')

            // Fetch selected entry for SelectionInfo
            void fetchSelectedEntry()

            // Scroll to selection after DOM updates
            void tick().then(() => {
                const listRef = viewMode === 'brief' ? briefListRef : fullListRef
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                listRef?.scrollToIndex(selectedIndex)
            })
        } catch (e) {
            if (thisGeneration !== loadGeneration) return
            error = e instanceof Error ? e.message : String(e)
            listingId = ''
            totalCount = 0
            loading = false
        }
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
            onPathChange?.(entry.path)
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

        // Handle Backspace or ⌘↑ - go to parent directory
        if ((e.key === 'Backspace' || (e.key === 'ArrowUp' && e.metaKey)) && hasParent) {
            e.preventDefault()
            const parentEntry = createParentEntry(currentPath)
            if (parentEntry) {
                void handleNavigate(parentEntry)
            }
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
    $effect(() => {
        if (initialPath !== currentPath) {
            currentPath = initialPath
            void loadDirectory(initialPath)
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

            // For now, just refetch total count - the List components
            // will refetch their visible range on the next render
            void getTotalCount(listingId, includeHidden).then((count) => {
                totalCount = count
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
        void loadDirectory(currentPath)

        // Set up sync status polling for visible files
        syncPollInterval = setInterval(() => {
            // List components will call fetchSyncStatusForPaths with their visible entries
        }, SYNC_POLL_INTERVAL_MS)
    })

    onDestroy(() => {
        // Clean up listing
        if (listingId) {
            void listDirectoryEnd(listingId)
        }
        unlisten?.()
        unlistenMenuAction?.()
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
            onVolumeChange={(newVolumeId: string, newVolumePath: string) => {
                onVolumeChange?.(newVolumeId, newVolumePath)
            }}
        />
        <span class="path"
            >{currentPath.startsWith(volumePath) ? currentPath.slice(volumePath.length) || '/' : currentPath}</span
        >
    </div>
    <div class="content">
        {#if loading}
            <LoadingIcon />
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
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
                {hasParent}
                {maxFilenameWidth}
                parentPath={hasParent ? currentPath.substring(0, currentPath.lastIndexOf('/')) || '/' : ''}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={fetchSyncStatusForPaths}
            />
        {:else}
            <FullList
                bind:this={fullListRef}
                {listingId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
                {hasParent}
                parentPath={hasParent ? currentPath.substring(0, currentPath.lastIndexOf('/')) || '/' : ''}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={fetchSyncStatusForPaths}
            />
        {/if}
    </div>
    <!-- SelectionInfo shown in brief mode -->
    {#if viewMode === 'brief'}
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
</style>
