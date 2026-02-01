<script lang="ts" module>
    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }

    function formatDate(timestamp: number): string {
        const date = new Date(timestamp * 1000)
        return date.toLocaleDateString(undefined, {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
        })
    }
</script>

<script lang="ts">
    /**
     * MtpBrowser - displays files and folders from an MTP device.
     * Rendered when user selects an MTP volume in the volume selector.
     * Auto-connects to the device if not yet connected.
     */
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { connect, getDevice } from './mtp-store.svelte'
    import { parseMtpPath, joinMtpPath, getMtpParentPath } from './mtp-path-utils'
    import PtpcameradDialog from './PtpcameradDialog.svelte'
    import { listMtpDirectory, onMtpTransferProgress, type UnlistenFn } from '$lib/tauri-commands'
    import type { FileEntry } from '$lib/file-explorer/types'
    import { handleNavigationShortcut } from '$lib/file-explorer/keyboard-shortcuts'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('mtp')

    /** Row height for file list */
    const ROW_HEIGHT = 20

    interface Props {
        /** Full MTP path: mtp://{deviceId}/{storageId}/{path} */
        path: string
        /** Device ID */
        deviceId: string
        /** Storage ID */
        storageId: number
        /** Whether this pane is focused */
        isFocused?: boolean
        /** Callback when user navigates to a folder */
        onNavigate?: (newPath: string, selectName?: string) => void
        /** Callback when an error occurs */
        onError?: (error: string) => void
    }

    const { path, deviceId, storageId, isFocused = false, onNavigate, onError }: Props = $props()

    // State
    let loading = $state(true)
    let connecting = $state(false)
    let error = $state<string | null>(null)
    let files = $state<FileEntry[]>([])
    let cursorIndex = $state(0)
    let showPtpcameradDialog = $state(false)
    let blockingProcess = $state<string | undefined>(undefined)

    // Selection state
    const selectedIndices: SvelteSet<number> = new SvelteSet()
    let selectionAnchorIndex = $state<number | null>(null)
    let selectionEndIndex = $state<number | null>(null)
    let isDeselecting = $state(false)

    // Container for scrolling
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)

    // Transfer progress listener
    let unlistenProgress: UnlistenFn | undefined

    // Get device state from store
    const deviceState = $derived(getDevice(deviceId))
    const isConnected = $derived(deviceState?.connectionState === 'connected')
    const isConnecting = $derived(deviceState?.connectionState === 'connecting')

    // Check if we're at the root of the storage
    const parsed = $derived(parseMtpPath(path))
    const isAtRoot = $derived(!parsed?.path || parsed.path === '')

    // Total count including ".." for parent navigation
    const totalCount = $derived(isAtRoot ? files.length : files.length + 1)

    /**
     * Attempts to connect to the MTP device.
     */
    async function connectToDevice() {
        connecting = true
        error = null

        try {
            await connect(deviceId)
            log.info('Connected to MTP device: {deviceId}', { deviceId })
            // After connection, load the directory
            await loadDirectory()
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Failed to connect to MTP device: {error}', { error: errorMessage })

            // Check if it's an exclusive access error (ptpcamerad)
            if (errorMessage.includes('exclusive access') || errorMessage.includes('ptpcamerad')) {
                // Extract blocking process name if available
                const match = errorMessage.match(/blocked by (.+)/i)
                blockingProcess = match ? match[1] : undefined
                showPtpcameradDialog = true
            } else {
                error = errorMessage
                onError?.(errorMessage)
            }
        } finally {
            connecting = false
        }
    }

    /**
     * Loads the current directory from the MTP device.
     */
    async function loadDirectory() {
        if (!isConnected) {
            return
        }

        loading = true
        error = null

        try {
            const innerPath = parsed?.path ?? ''
            const entries = await listMtpDirectory(deviceId, storageId, innerPath)
            files = entries
            cursorIndex = 0
            selectedIndices.clear()
            log.debug('Loaded {count} entries from MTP: {path}', { count: entries.length, path: innerPath || '/' })
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Failed to list MTP directory: {error}', { error: errorMessage })
            error = errorMessage
            files = []
            onError?.(errorMessage)
        } finally {
            loading = false
        }
    }

    /**
     * Handles navigation to a file or folder.
     */
    function handleItemNavigate(entry: FileEntry) {
        if (entry.name === '..') {
            // Navigate to parent
            const parentPath = getMtpParentPath(path)
            if (parentPath) {
                const currentFolderName = path.split('/').pop()
                onNavigate?.(parentPath, currentFolderName)
            }
        } else if (entry.isDirectory) {
            // Navigate into folder
            const newPath = joinMtpPath(path, entry.name)
            onNavigate?.(newPath)
        }
        // For files, we don't navigate - they can be downloaded via copy operation
    }

    /**
     * Scrolls to make the cursor visible.
     */
    function scrollToIndex(index: number) {
        if (!listContainer) return
        const targetTop = index * ROW_HEIGHT
        const targetBottom = targetTop + ROW_HEIGHT
        const scrollTop = listContainer.scrollTop
        const viewportBottom = scrollTop + containerHeight

        if (targetTop < scrollTop) {
            listContainer.scrollTop = targetTop
        } else if (targetBottom > viewportBottom) {
            listContainer.scrollTop = targetBottom - containerHeight
        }
    }

    // Selection helpers
    export function clearSelection() {
        selectedIndices.clear()
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
    }

    function toggleSelectionAt(index: number): boolean {
        // Can't select ".." entry
        if (!isAtRoot && index === 0) return false

        if (selectedIndices.has(index)) {
            selectedIndices.delete(index)
            return false
        } else {
            selectedIndices.add(index)
            return true
        }
    }

    function getIndicesInRange(a: number, b: number): number[] {
        const start = Math.min(a, b)
        const end = Math.max(a, b)
        const indices: number[] = []
        for (let i = start; i <= end; i++) {
            // Skip ".." entry
            if (!isAtRoot && i === 0) continue
            indices.push(i)
        }
        return indices
    }

    function applyRangeSelection(newEnd: number) {
        if (selectionAnchorIndex === null) return

        const rangeIsEmpty = newEnd === selectionAnchorIndex
        const newRange = rangeIsEmpty ? [] : getIndicesInRange(selectionAnchorIndex, newEnd)

        if (isDeselecting) {
            for (const i of newRange) {
                selectedIndices.delete(i)
            }
        } else {
            for (const i of newRange) {
                selectedIndices.add(i)
            }
        }

        if (selectionEndIndex !== null) {
            const oldRange =
                selectionEndIndex === selectionAnchorIndex
                    ? []
                    : getIndicesInRange(selectionAnchorIndex, selectionEndIndex)
            for (const i of oldRange) {
                if (!newRange.includes(i) && !isDeselecting) {
                    selectedIndices.delete(i)
                }
            }
        }

        selectionEndIndex = newEnd
    }

    function handleShiftNavigation(newIndex: number) {
        if (selectionAnchorIndex === null) {
            selectionAnchorIndex = cursorIndex
            isDeselecting = selectedIndices.has(cursorIndex)
        }
        applyRangeSelection(newIndex)
    }

    function clearRangeState() {
        selectionAnchorIndex = null
        selectionEndIndex = null
        isDeselecting = false
    }

    function selectAll() {
        selectedIndices.clear()
        const startIndex = isAtRoot ? 0 : 1 // Skip ".." entry
        for (let i = startIndex; i < totalCount; i++) {
            selectedIndices.add(i)
        }
        clearRangeState()
    }

    function deselectAll() {
        selectedIndices.clear()
        clearRangeState()
    }

    // Helper: Apply navigation result
    function applyNavigation(newIndex: number, shiftKey = false) {
        if (shiftKey) {
            handleShiftNavigation(newIndex)
        } else {
            clearRangeState()
        }
        cursorIndex = newIndex
        scrollToIndex(newIndex)
    }

    /**
     * Gets the file entry at the given display index.
     * Index 0 is ".." if not at root.
     */
    function getEntryAtIndex(index: number): FileEntry | null {
        if (!isAtRoot && index === 0) {
            // Return a fake ".." entry
            const parentPath = getMtpParentPath(path)
            return {
                name: '..',
                path: parentPath ?? path,
                isDirectory: true,
                isSymlink: false,
                permissions: 0o755,
                owner: '',
                group: '',
                iconId: 'dir',
                extendedMetadataLoaded: true,
            }
        }
        const fileIndex = isAtRoot ? index : index - 1
        return files[fileIndex] ?? null
    }

    // Helper: Handle selection keys (Space, Cmd+A)
    function handleSelectionKeys(e: KeyboardEvent): boolean {
        if (e.key === ' ') {
            e.preventDefault()
            toggleSelectionAt(cursorIndex)
            return true
        }

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

    // Helper: Handle arrow navigation
    function handleArrowKeys(e: KeyboardEvent): boolean {
        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                applyNavigation(Math.min(cursorIndex + 1, totalCount - 1), e.shiftKey)
                return true
            case 'ArrowUp':
                e.preventDefault()
                applyNavigation(Math.max(cursorIndex - 1, 0), e.shiftKey)
                return true
            case 'ArrowLeft':
                e.preventDefault()
                applyNavigation(0, e.shiftKey)
                return true
            case 'ArrowRight':
                e.preventDefault()
                applyNavigation(totalCount - 1, e.shiftKey)
                return true
        }
        return false
    }

    // Keyboard handler
    export function handleKeyDown(e: KeyboardEvent): boolean {
        // Handle Enter key - navigate into entry
        if (e.key === 'Enter') {
            const entry = getEntryAtIndex(cursorIndex)
            if (entry) {
                e.preventDefault()
                handleItemNavigate(entry)
                return true
            }
        }

        // Handle Backspace - go to parent
        if (e.key === 'Backspace' && !isAtRoot) {
            e.preventDefault()
            const entry = getEntryAtIndex(0) // ".." entry
            if (entry) handleItemNavigate(entry)
            return true
        }

        // Handle selection keys
        if (handleSelectionKeys(e)) return true

        // Try centralized navigation shortcuts
        const visibleItems = Math.max(1, Math.floor(containerHeight / ROW_HEIGHT))
        const navResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount,
            visibleItems,
        })
        if (navResult?.handled) {
            e.preventDefault()
            applyNavigation(navResult.newIndex, e.shiftKey)
            return true
        }

        // Arrow navigation
        return handleArrowKeys(e)
    }

    export function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            clearRangeState()
        }
    }

    // Export methods for external access
    export function getCursorIndex(): number {
        return cursorIndex
    }

    export function getSelectedIndices(): number[] {
        return Array.from(selectedIndices)
    }

    export function getSelectedFiles(): FileEntry[] {
        const selected: FileEntry[] = []
        for (const index of selectedIndices) {
            const entry = getEntryAtIndex(index)
            if (entry && entry.name !== '..') {
                selected.push(entry)
            }
        }
        return selected
    }

    export function getEntryUnderCursor(): FileEntry | null {
        return getEntryAtIndex(cursorIndex)
    }

    export function isLoading(): boolean {
        return loading || connecting
    }

    // Handlers for ptpcamerad dialog
    function handleDialogClose() {
        showPtpcameradDialog = false
    }

    function handleDialogRetry() {
        showPtpcameradDialog = false
        void connectToDevice()
    }

    // Row click handlers
    function handleRowClick(index: number) {
        cursorIndex = index
    }

    function handleRowDoubleClick(index: number) {
        const entry = getEntryAtIndex(index)
        if (entry) {
            handleItemNavigate(entry)
        }
    }

    // Lifecycle
    onMount(async () => {
        // Set up transfer progress listener
        unlistenProgress = await onMtpTransferProgress((progress) => {
            log.debug('Transfer progress: {done}/{total}', {
                done: progress.bytesDone,
                total: progress.bytesTotal,
            })
        })

        // Check if we need to connect
        if (!isConnected && !isConnecting) {
            await connectToDevice()
        } else if (isConnected) {
            await loadDirectory()
        }
    })

    onDestroy(() => {
        unlistenProgress?.()
    })

    // Reload directory when path changes
    $effect(() => {
        if (isConnected && path) {
            void loadDirectory()
        }
    })

    // Wait for connection to complete, then load
    $effect(() => {
        if (isConnected && files.length === 0 && !loading && !error) {
            void loadDirectory()
        }
    })
</script>

<div class="mtp-browser" class:is-focused={isFocused}>
    {#if showPtpcameradDialog}
        <PtpcameradDialog {blockingProcess} onClose={handleDialogClose} onRetry={handleDialogRetry} />
    {/if}

    {#if connecting || isConnecting}
        <div class="connecting-state">
            <span class="spinner"></span>
            <span class="status-text">Connecting to device...</span>
        </div>
    {:else if loading}
        <div class="loading-state">
            <span class="spinner"></span>
            <span class="status-text">Loading files...</span>
        </div>
    {:else if error}
        <div class="error-state">
            <div class="error-icon">❌</div>
            <div class="error-title">Couldn't load files</div>
            <div class="error-message">{error}</div>
            <button type="button" class="retry-button" onclick={() => void loadDirectory()}> Try again </button>
        </div>
    {:else}
        <div class="header-row">
            <span class="col-name">Name</span>
            <span class="col-size">Size</span>
            <span class="col-modified">Modified</span>
        </div>
        <div class="file-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
            {#if !isAtRoot}
                <!-- Parent directory entry -->
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                    class="file-row"
                    class:is-under-cursor={cursorIndex === 0}
                    class:is-focused-and-under-cursor={isFocused && cursorIndex === 0}
                    role="listitem"
                    onclick={() => {
                        handleRowClick(0)
                    }}
                    ondblclick={() => {
                        handleRowDoubleClick(0)
                    }}
                    onkeydown={() => {}}
                >
                    <span class="col-name">
                        <span class="file-icon">📁</span>
                        ..
                    </span>
                    <span class="col-size"></span>
                    <span class="col-modified"></span>
                </div>
            {/if}

            {#each files as file, index (file.path)}
                {@const displayIndex = isAtRoot ? index : index + 1}
                {@const isSelected = selectedIndices.has(displayIndex)}
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                    class="file-row"
                    class:is-under-cursor={cursorIndex === displayIndex}
                    class:is-focused-and-under-cursor={isFocused && cursorIndex === displayIndex}
                    class:is-selected={isSelected}
                    role="listitem"
                    onclick={() => {
                        handleRowClick(displayIndex)
                    }}
                    ondblclick={() => {
                        handleRowDoubleClick(displayIndex)
                    }}
                    onkeydown={() => {}}
                >
                    <span class="col-name">
                        <span class="file-icon">{file.isDirectory ? '📁' : '📄'}</span>
                        {file.name}
                    </span>
                    <span class="col-size">
                        {#if !file.isDirectory && file.size !== undefined}
                            {formatSize(file.size)}
                        {/if}
                    </span>
                    <span class="col-modified">
                        {#if file.modifiedAt}
                            {formatDate(file.modifiedAt)}
                        {/if}
                    </span>
                </div>
            {/each}

            {#if files.length === 0 && !loading}
                <div class="empty-state">This folder is empty.</div>
            {/if}
        </div>
    {/if}
</div>

<style>
    .mtp-browser {
        display: flex;
        flex-direction: column;
        height: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
    }

    .connecting-state,
    .loading-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .spinner {
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

    .status-text {
        font-size: var(--font-size-sm);
    }

    .error-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 24px;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .error-icon {
        font-size: 32px;
    }

    .error-title {
        font-size: 16px;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .error-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .retry-button {
        margin-top: 8px;
        padding: 8px 16px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .retry-button:hover {
        background-color: var(--color-bg-hover);
    }

    .header-row {
        display: flex;
        padding: 4px 8px;
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .file-list {
        flex: 1;
        overflow-y: auto;
    }

    .file-row {
        display: flex;
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        cursor: default;
    }

    .file-row.is-under-cursor {
        background-color: var(--color-cursor-unfocused-bg);
    }

    .file-row.is-focused-and-under-cursor {
        background-color: var(--color-cursor-focused-bg);
        color: var(--color-cursor-focused-fg);
    }

    .file-row.is-selected .col-name {
        color: var(--color-selection-fg);
    }

    .file-row.is-selected.is-focused-and-under-cursor .col-name {
        color: var(--color-selection-fg);
    }

    .col-name {
        flex: 3;
        display: flex;
        align-items: center;
        gap: 6px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-size {
        flex: 1;
        text-align: right;
        color: var(--color-text-secondary);
        padding-right: 16px;
    }

    .col-modified {
        flex: 1.5;
        color: var(--color-text-tertiary);
    }

    .file-icon {
        font-size: 14px;
    }

    .empty-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 48px 16px;
        color: var(--color-text-tertiary);
        font-style: italic;
    }
</style>
