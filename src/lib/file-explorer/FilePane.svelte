<script lang="ts">
    import { onMount, untrack } from 'svelte'
    import type { FileEntry } from './types'
    import { openFile } from '$lib/tauri-commands'
    import FileList from './FileList.svelte'
    import SelectionInfo from './SelectionInfo.svelte'

    /** Chunk size for loading large directories */
    const CHUNK_SIZE = 5000

    interface Props {
        initialPath: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        onPathChange?: (path: string) => void
        onRequestFocus?: () => void
    }

    const { initialPath, isFocused = false, showHiddenFiles = true, onPathChange, onRequestFocus }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // PERFORMANCE: Store files in plain JS (NOT reactive) to avoid 50k-item reactivity overhead
    // Use filesVersion to manually trigger re-renders when the list changes
    let allFilesRaw: FileEntry[] = []
    let filesVersion = $state(0)

    let loading = $state(true)
    let loadingMore = $state(false)
    let totalCount = $state(0)
    let error = $state<string | null>(null)
    let selectedIndex = $state(0)
    let fileListRef: FileList | undefined = $state()
    /** Metadata for the current directory (used for ".." entry in SelectionInfo) */
    const currentDirModifiedAt = $state<number | undefined>(undefined)

    // Track the current load operation to cancel outdated ones
    let loadGeneration = 0

    // Filter files based on showHiddenFiles setting
    // Always keep ".." visible for parent navigation
    function filterFiles(entries: FileEntry[], showHidden: boolean): FileEntry[] {
        if (showHidden) return entries
        return entries.filter((e) => !e.name.startsWith('.') || e.name === '..')
    }

    // Compute visible files based on showHiddenFiles prop
    // Note: filesVersion is read to trigger re-computation when allFilesRaw changes
    const files = $derived.by(() => {
        void filesVersion // Dependency trigger
        return filterFiles(allFilesRaw, showHiddenFiles)
    })

    // Currently selected entry for SelectionInfo (must be after files declaration)
    const selectedEntry = $derived(files[selectedIndex] ?? null)

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

        loading = true
        loadingMore = false
        error = null
        allFilesRaw = []
        filesVersion++ // Trigger reactivity
        totalCount = 0

        try {
            // Import session API functions
            const { listDirectoryStartSession, listDirectoryNextChunk, listDirectoryEndSession } =
                await import('$lib/tauri-commands')

            // Start session - reads directory ONCE, returns first chunk immediately
            const startResult = await listDirectoryStartSession(path, CHUNK_SIZE)

            // Check if this load was cancelled
            if (thisGeneration !== loadGeneration) {
                // Clean up abandoned session
                void listDirectoryEndSession(startResult.sessionId)
                return
            }

            const parentEntry = createParentEntry(path)
            const firstChunk = parentEntry ? [parentEntry, ...startResult.entries] : startResult.entries

            // +1 for parent entry if present
            totalCount = parentEntry ? startResult.totalCount + 1 : startResult.totalCount

            // Display first chunk immediately!
            allFilesRaw = firstChunk
            filesVersion++

            // Set selection
            if (selectName) {
                const visibleFiles = filterFiles(firstChunk, showHiddenFiles)
                const targetIndex = visibleFiles.findIndex((f) => f.name === selectName)
                selectedIndex = targetIndex >= 0 ? targetIndex : 0
            } else {
                selectedIndex = 0
            }

            loading = false

            // Start icon refresh for first chunk (non-blocking)
            void refreshIconsForCurrentDirectory(firstChunk.filter((e) => e.name !== '..'))

            // Load remaining chunks in background
            if (startResult.hasMore) {
                loadingMore = true
                void loadRemainingChunksFromSession(
                    startResult.sessionId,
                    firstChunk,
                    thisGeneration,
                    listDirectoryNextChunk,
                    listDirectoryEndSession,
                )
            }
        } catch (e) {
            if (thisGeneration !== loadGeneration) return
            error = e instanceof Error ? e.message : String(e)
            allFilesRaw = []
            filesVersion++ // Trigger reactivity
            totalCount = 0
            loading = false
        }
    }

    /**
     * Loads remaining chunks from a session in the background.
     * Uses requestAnimationFrame to avoid blocking the UI.
     */
    async function loadRemainingChunksFromSession(
        sessionId: string,
        initialEntries: FileEntry[],
        generation: number,
        nextChunk: (id: string, size: number) => Promise<{ entries: FileEntry[]; hasMore: boolean }>,
        endSession: (id: string) => Promise<void>,
    ) {
        let currentEntries = initialEntries
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

            // Append entries
            currentEntries = [...currentEntries, ...result.entries]
            allFilesRaw = currentEntries
            filesVersion++

            // Refresh icons for new entries
            void refreshIconsForCurrentDirectory(result.entries.filter((e) => e.name !== '..'))
        }

        loadingMore = false

        // Clean up session
        void endSession(sessionId)
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

    function handleSelect(index: number) {
        selectedIndex = index
        onRequestFocus?.()
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
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            const newIndex = Math.min(selectedIndex + 1, files.length - 1)
            selectedIndex = newIndex
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            fileListRef?.scrollToIndex(newIndex)
        } else if (e.key === 'ArrowUp') {
            e.preventDefault()
            const newIndex = Math.max(selectedIndex - 1, 0)
            selectedIndex = newIndex
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            fileListRef?.scrollToIndex(newIndex)
        } else if (e.key === 'Enter') {
            e.preventDefault()
            void handleNavigate(files[selectedIndex])
        }
        // Tab key bubbles up to DualPaneExplorer
    }

    // Update path when initialPath prop changes (for persistence loading)
    $effect(() => {
        if (initialPath !== currentPath) {
            currentPath = initialPath
            void loadDirectory(initialPath)
        }
    })

    // Reset selection when showHiddenFiles changes and current selection becomes invalid
    $effect(() => {
        // Re-run when files change (which depends on showHiddenFiles)
        if (selectedIndex >= files.length && files.length > 0) {
            selectedIndex = 0
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            fileListRef?.scrollToIndex(0)
        }
    })

    onMount(() => {
        void loadDirectory(currentPath)
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
            <div class="message">Loading...</div>
        {:else if error}
            <div class="error-message">{error}</div>
        {:else}
            <FileList
                bind:this={fileListRef}
                {files}
                {selectedIndex}
                {isFocused}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
            />
            {#if loadingMore}
                <div class="loading-more">
                    Loading {totalCount - allFilesRaw.length} more files...
                </div>
            {/if}
        {/if}
    </div>
    <SelectionInfo entry={selectedEntry} {currentDirModifiedAt} />
</div>

<style>
    .file-pane {
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--color-border-primary);
    }

    .file-pane.is-focused {
        border-color: var(--color-focus-border);
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

    .message,
    .error-message {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-text-secondary);
        font-size: var(--font-size-base);
    }

    .error-message {
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
