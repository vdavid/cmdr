<script lang="ts">
    import { onMount, untrack } from 'svelte'
    import type { FileEntry } from './types'
    import type { FileService } from '$lib/file-service'
    import { defaultFileService } from '$lib/file-service'
    import { openFile } from '$lib/tauri-commands'
    import FileList from './FileList.svelte'

    interface Props {
        initialPath: string
        fileService?: FileService
        isFocused?: boolean
        showHiddenFiles?: boolean
        onPathChange?: (path: string) => void
        onRequestFocus?: () => void
    }

    const {
        initialPath,
        fileService = defaultFileService,
        isFocused = false,
        showHiddenFiles = true,
        onPathChange,
        onRequestFocus,
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))
    let allFiles = $state<FileEntry[]>([])
    let loading = $state(true)
    let error = $state<string | null>(null)
    let selectedIndex = $state(0)
    let fileListRef: FileList | undefined = $state()

    // Filter files based on showHiddenFiles setting
    // Always keep ".." visible for parent navigation
    function filterFiles(entries: FileEntry[], showHidden: boolean): FileEntry[] {
        if (showHidden) return entries
        return entries.filter((e) => !e.name.startsWith('.') || e.name === '..')
    }

    // Compute visible files based on showHiddenFiles prop
    const files = $derived(filterFiles(allFiles, showHiddenFiles))

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
        loading = true
        error = null
        try {
            const entries = await fileService.listDirectory(path)
            const parentEntry = createParentEntry(path)
            allFiles = parentEntry ? [parentEntry, ...entries] : entries

            // If selectName is provided, find and select that entry
            // But only if it's visible (not filtered out)
            if (selectName) {
                const visibleFiles = filterFiles(allFiles, showHiddenFiles)
                const targetIndex = visibleFiles.findIndex((f) => f.name === selectName)
                // If target is hidden (e.g., navigating up from .config with hidden files off),
                // fall back to index 0
                selectedIndex = targetIndex >= 0 ? targetIndex : 0
            } else {
                selectedIndex = 0
            }

            // Refresh icons for directories and extensions in the background
            void refreshIconsForCurrentDirectory(entries)
        } catch (e) {
            error = e instanceof Error ? e.message : String(e)
            allFiles = []
        } finally {
            loading = false
        }
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
        {/if}
    </div>
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
</style>
