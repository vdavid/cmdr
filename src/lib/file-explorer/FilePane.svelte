<script lang="ts">
    import { onMount } from 'svelte'
    import type { FileEntry } from './types'
    import type { FileService } from '$lib/file-service'
    import { defaultFileService } from '$lib/file-service'
    import { openFile } from '$lib/tauri-commands'
    import FileList from './FileList.svelte'

    interface Props {
        initialPath: string
        fileService?: FileService
        isFocused?: boolean
        onPathChange?: (path: string) => void
        onRequestFocus?: () => void
    }

    const {
        initialPath,
        fileService = defaultFileService,
        isFocused = false,
        onPathChange,
        onRequestFocus,
    }: Props = $props()

    let currentPath = $state(initialPath)
    let files = $state<FileEntry[]>([])
    let loading = $state(true)
    let error = $state<string | null>(null)
    let selectedIndex = $state(0)
    let fileListRef: FileList | undefined = $state()

    // Create ".." entry for parent navigation
    function createParentEntry(path: string): FileEntry | null {
        if (path === '/') return null
        const parentPath = path.substring(0, path.lastIndexOf('/')) || '/'
        return {
            name: '..',
            path: parentPath,
            isDirectory: true,
        }
    }

    async function loadDirectory(path: string, selectName?: string) {
        loading = true
        error = null
        try {
            const entries = await fileService.listDirectory(path)
            const parentEntry = createParentEntry(path)
            files = parentEntry ? [parentEntry, ...entries] : entries

            // If selectName is provided, find and select that entry
            if (selectName) {
                const targetIndex = files.findIndex((f) => f.name === selectName)
                selectedIndex = targetIndex >= 0 ? targetIndex : 0
            } else {
                selectedIndex = 0
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e)
            files = []
        } finally {
            loading = false
        }
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

    onMount(() => {
        void loadDirectory(currentPath)
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_click_events_have_key_events -->
<div class="file-pane" class:is-focused={isFocused} onclick={handlePaneClick} role="region" aria-label="File pane">
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
        font-family: var(--font-mono) monospace;
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
