<script lang="ts">
    import { onMount, tick, untrack, onDestroy } from 'svelte'
    import type { FileEntry, DirectoryDiff, SyncStatus } from './types'
    import {
        listen,
        openFile,
        showFileContextMenu,
        updateMenuContext,
        listDirectoryStartSession,
        listDirectoryNextChunk,
        listDirectoryEndSession,
        getSyncStatus,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { ViewMode } from '$lib/app-status-store'
    import FullList from './FullList.svelte'
    import BriefList from './BriefList.svelte'
    import SelectionInfo from './SelectionInfo.svelte'
    import LoadingIcon from '../LoadingIcon.svelte'
    import { createFileDataStore } from './FileDataStore'

    /** Chunk size for loading large directories */
    const CHUNK_SIZE = 5000

    interface Props {
        initialPath: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        viewMode?: ViewMode
        onPathChange?: (path: string) => void
        onRequestFocus?: () => void
    }

    const {
        initialPath,
        isFocused = false,
        showHiddenFiles = true,
        viewMode = 'brief',
        onPathChange,
        onRequestFocus,
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // PERFORMANCE: FileDataStore keeps files outside Svelte's reactivity system
    // to avoid O(n) reactivity costs for large directories (20k+ files).
    // Only storeVersion is reactive - when it changes, components re-read from store.
    const fileStore = createFileDataStore()
    let storeVersion = $state(0)

    // Subscribe to store updates
    $effect(() => {
        return fileStore.onUpdate(() => {
            storeVersion++
        })
    })

    let loading = $state(true)
    let loadingMore = $state(false)
    let error = $state<string | null>(null)
    let selectedIndex = $state(0)
    let fullListRef: FullList | undefined = $state()
    let briefListRef: BriefList | undefined = $state()
    /** Metadata for the current directory (used for ".." entry in SelectionInfo) */
    const currentDirModifiedAt = $state<number | undefined>(undefined)

    // Track the current load operation to cancel outdated ones
    let loadGeneration = 0
    // Track current session for file watching
    let currentSessionId = ''
    let lastSequence = 0
    let unlisten: UnlistenFn | undefined
    let unlistenMenuAction: UnlistenFn | undefined
    // Polling interval for sync status (visible files only)
    let syncPollInterval: ReturnType<typeof setInterval> | undefined
    const SYNC_POLL_INTERVAL_MS = 2000 // Poll every 2 seconds

    // Note: totalCount and maxFilenameWidth are available via fileStore.totalCount
    // and fileStore.maxFilenameWidth - they'll be passed to List components in Phase 2

    // Derive syncStatusMap from store (reactive via storeVersion)
    const syncStatusMap = $derived.by(() => {
        void storeVersion
        return fileStore.syncStatusMap as Record<string, SyncStatus>
    })

    // Get all visible files from store
    // Note: This is used for operations that need the full list (keyboard nav, context menu)
    // Virtual scroll components should use getRange() instead
    const files = $derived.by(() => {
        void storeVersion
        return fileStore.getAllFiltered()
    })

    // Currently selected entry for SelectionInfo (must be after files declaration)
    const selectedEntry = $derived(files[selectedIndex])

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
        }
    }

    async function loadDirectory(path: string, selectName?: string) {
        // Increment generation to cancel any in-flight requests
        const thisGeneration = ++loadGeneration

        // End previous session (and watcher) when navigating away
        if (currentSessionId) {
            void listDirectoryEndSession(currentSessionId)
            currentSessionId = ''
            lastSequence = 0
        }

        loading = true
        loadingMore = false
        error = null
        fileStore.clear() // Reset store on directory change

        try {
            // Start session - reads directory ONCE, returns first chunk immediately
            const startResult = await listDirectoryStartSession(path, CHUNK_SIZE)

            // Check if this load was cancelled
            if (thisGeneration !== loadGeneration) {
                // Clean up abandoned session
                void listDirectoryEndSession(startResult.sessionId)
                return
            }

            // Store session ID for file watching events
            currentSessionId = startResult.sessionId
            lastSequence = 0

            const parentEntry = createParentEntry(path)
            const firstChunk = parentEntry ? [parentEntry, ...startResult.entries] : startResult.entries

            // Display first chunk immediately via FileDataStore
            fileStore.setFiles(firstChunk)

            // Set selection
            if (selectName) {
                const targetIndex = fileStore.findIndex(selectName)
                selectedIndex = targetIndex >= 0 ? targetIndex : 0

                // Scroll the selected folder into view (after DOM updates)
                void tick().then(() => {
                    const listRef = viewMode === 'brief' ? briefListRef : fullListRef
                    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                    listRef?.scrollToIndex(selectedIndex)
                })
            } else {
                selectedIndex = 0
            }

            loading = false

            // Start icon refresh for first chunk (non-blocking)
            void refreshIconsForCurrentDirectory(firstChunk.filter((e) => e.name !== '..'))

            // Fetch sync status for visible files (non-blocking)
            void fetchSyncStatusForEntries(firstChunk)

            // Load remaining chunks in background
            if (startResult.hasMore) {
                loadingMore = true
                void loadRemainingChunksFromSession(
                    startResult.sessionId,
                    thisGeneration,
                    listDirectoryNextChunk,
                    listDirectoryEndSession,
                )
            }
        } catch (e) {
            if (thisGeneration !== loadGeneration) return
            error = e instanceof Error ? e.message : String(e)
            fileStore.clear()
            loading = false
            currentSessionId = ''
            lastSequence = 0
        }
    }

    /**
     * Apply a diff from the file watcher to the file list.
     * Uses FileDataStore's applyDiff method which handles internal state updates.
     */
    function applyDiffToList(changes: { type: 'add' | 'remove' | 'modify'; entry: FileEntry }[]) {
        selectedIndex = fileStore.applyDiff(changes, selectedIndex)
    }

    /**
     * Loads remaining chunks from a session in the background.
     * Uses requestAnimationFrame to avoid blocking the UI.
     */
    async function loadRemainingChunksFromSession(
        sessionId: string,
        generation: number,
        nextChunk: (id: string, size: number) => Promise<{ entries: FileEntry[]; hasMore: boolean }>,
        endSession: (id: string) => Promise<void>,
    ) {
        let hasMore = true

        while (hasMore) {
            // Check if cancelled
            if (generation !== loadGeneration) {
                void endSession(sessionId)
                return
            }

            // Wait for next animation frame to keep UI responsive
            await new Promise((resolve) => requestAnimationFrame(resolve))

            // Check again after await
            if (generation !== loadGeneration) {
                void endSession(sessionId)
                return
            }

            // Fetch next chunk from cache (fast!)
            const result = await nextChunk(sessionId, CHUNK_SIZE)
            hasMore = result.hasMore

            // Append entries to store
            fileStore.appendFiles(result.entries)

            // Refresh icons for new entries
            void refreshIconsForCurrentDirectory(result.entries.filter((e) => e.name !== '..'))
        }

        loadingMore = false
        // Session stays active for file watching - only end when navigating away
    }

    // Refresh icons for directories (custom folder icons) and extensions (file association changes)
    async function refreshIconsForCurrentDirectory(entries: FileEntry[]) {
        // Use static import since knip doesn't detect dynamic imports
        const { refreshDirectoryIcons } = await import('$lib/icon-cache')

        // Collect all directory paths (for custom folder icons)
        const directoryPaths = entries.filter((e) => e.isDirectory).map((e) => e.path)

        // Collect all unique extensions (for file association changes)
        // eslint-disable-next-line svelte/prefer-svelte-reactivity
        const extensionSet = new Set<string>()
        for (const entry of entries) {
            if (!entry.isDirectory && entry.name.includes('.')) {
                const ext = entry.name.split('.').pop()
                if (ext) extensionSet.add(ext.toLowerCase())
            }
        }

        await refreshDirectoryIcons(directoryPaths, [...extensionSet])
    }

    /**
     * Fetch sync status for entries in the current directory.
     * Called lazily after directory loads to avoid blocking the UI.
     */
    async function fetchSyncStatusForEntries(entries: FileEntry[]) {
        // Fetch for both files and directories (but not "..")
        const paths = entries.filter((e) => e.name !== '..').map((e) => e.path)

        if (paths.length === 0) return

        try {
            const statuses = await getSyncStatus(paths)
            // Merge with existing map in store
            fileStore.setSyncStatusMap({ ...fileStore.syncStatusMap, ...statuses })
        } catch {
            // Silently ignore - sync status is optional
        }
    }

    function handleSelect(index: number) {
        selectedIndex = index
        onRequestFocus?.()
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

    // Exported so DualPaneExplorer can forward keyboard events
    export function handleKeyDown(e: KeyboardEvent) {
        // Handle navigation keys (Enter, Backspace) the same for both modes
        if (e.key === 'Enter') {
            e.preventDefault()
            void handleNavigate(files[selectedIndex])
            return
        }
        if (e.key === 'Backspace') {
            e.preventDefault()
            const parentEntry = files.find((f) => f.name === '..')
            if (parentEntry) {
                void handleNavigate(parentEntry)
            }
            return
        }

        // Handle arrow keys based on view mode
        if (viewMode === 'brief') {
            // BriefList handles all arrow keys (Up/Down for prev/next, Left/Right for columns)
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
            const newIndex: number | undefined = briefListRef?.handleKeyNavigation(e.key)
            if (newIndex !== undefined) {
                e.preventDefault()
                selectedIndex = newIndex
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                briefListRef?.scrollToIndex(newIndex)
            }
        } else {
            // Full mode: only Up/Down navigate
            if (e.key === 'ArrowDown') {
                e.preventDefault()
                const newIndex = Math.min(selectedIndex + 1, files.length - 1)
                selectedIndex = newIndex
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                fullListRef?.scrollToIndex(newIndex)
            } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                const newIndex = Math.max(selectedIndex - 1, 0)
                selectedIndex = newIndex
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                fullListRef?.scrollToIndex(newIndex)
            }
        }
        // Tab key bubbles up to DualPaneExplorer
    }

    // Sync showHiddenFiles prop with store
    $effect(() => {
        fileStore.setShowHiddenFiles(showHiddenFiles)
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

        const entry = files[selectedIndex] as FileEntry | undefined
        if (entry && entry.name !== '..') {
            void updateMenuContext(entry.path, entry.name)
        }
    })

    // Reset selection when showHiddenFiles changes and current selection becomes invalid
    $effect(() => {
        // Re-run when files change (which depends on showHiddenFiles)
        if (selectedIndex >= files.length && files.length > 0) {
            selectedIndex = 0

            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(0)
        }
    })

    // Scroll selected item into view when view mode changes
    $effect(() => {
        // Track viewMode to trigger on change
        void viewMode
        // Wait for the new list component to mount and render
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            listRef?.scrollToIndex(selectedIndex)
        })
    })

    onMount(async () => {
        // Listen for directory diff events from the file watcher
        // Wrapped in try-catch to handle test environments where Tauri isn't available
        try {
            unlisten = await listen<DirectoryDiff>('directory-diff', (event) => {
                const diff = event.payload
                // Only process diffs for our current session
                if (diff.sessionId !== currentSessionId) return

                // Check sequence - if we missed events, do full reload
                if (diff.sequence !== lastSequence + 1) {
                    // eslint-disable-next-line no-console
                    console.warn('[FilePane] Sequence gap detected, reloading directory')
                    void loadDirectory(currentPath)
                    return
                }

                // Apply the diff
                lastSequence = diff.sequence
                applyDiffToList(diff.changes)

                // Get all added and modified entries for icon and sync status refresh
                const changedEntries = diff.changes
                    .filter((c) => c.type === 'add' || c.type === 'modify')
                    .map((c) => c.entry)

                if (changedEntries.length > 0) {
                    // Refresh icons for new/modified entries
                    void refreshIconsForCurrentDirectory(changedEntries)
                    // Refresh sync status for changed entries
                    void fetchSyncStatusForEntries(changedEntries)
                }
            })
        } catch {
            // In test environment, Tauri API may not be available
        }

        // Listen for menu actions from Rust
        try {
            unlistenMenuAction = await listen<{ action: string; path: string }>('menu-action', (event) => {
                if (isFocused && event.payload.action === 'get-info') {
                    if (selectedEntry.path === event.payload.path) {
                        // Show info in a simple way for now
                        const size = selectedEntry.size !== undefined ? selectedEntry.size.toString() : 'N/A'
                        const perms = selectedEntry.permissions.toString(8)
                        alert(
                            `Get info for: ${selectedEntry.name}\nPath: ${selectedEntry.path}\nSize: ${size} bytes\nOwner: ${selectedEntry.owner}\nPermissions: ${perms}`,
                        )
                    }
                }
            })
        } catch {
            // In test environment, Tauri API may not be available
        }

        void loadDirectory(currentPath)

        // Start polling visible files for sync status changes
        // Always poll - both panes are visible even when not focused
        syncPollInterval = setInterval(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-assignment
            const visiblePaths: string[] = listRef?.getVisiblePaths?.() ?? []
            if (visiblePaths.length > 0) {
                void getSyncStatus(visiblePaths).then((statuses) => {
                    // Only update if statuses actually changed
                    const currentMap = fileStore.syncStatusMap
                    let changed = false
                    for (const [path, status] of Object.entries(statuses)) {
                        if (currentMap[path] !== status) {
                            changed = true
                            break
                        }
                    }
                    if (changed) {
                        fileStore.setSyncStatusMap({ ...currentMap, ...statuses })
                    }
                })
            }
        }, SYNC_POLL_INTERVAL_MS)
    })

    onDestroy(() => {
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
        <span class="path">{currentPath}</span>
    </div>
    <div class="content">
        {#if loading}
            <LoadingIcon />
        {:else if error}
            <div class="error-message">{error}</div>
        {:else if viewMode === 'brief'}
            <BriefList
                bind:this={briefListRef}
                {files}
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
            />
        {:else}
            <FullList
                bind:this={fullListRef}
                {files}
                {selectedIndex}
                {isFocused}
                {syncStatusMap}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
            />
        {/if}
        {#if loadingMore}
            <div class="loading-more">Loading more files...</div>
        {/if}
    </div>
    <!-- SelectionInfo shown in brief mode (full mode will have inline metadata in the future) -->
    {#if viewMode === 'brief'}
        <SelectionInfo entry={selectedEntry} {currentDirModifiedAt} />
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
        padding: var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        font-size: var(--font-size-xs);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .path {
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-secondary);
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

    .loading-more {
        padding: var(--spacing-sm);
        text-align: center;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background-color: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-primary);
    }
</style>
