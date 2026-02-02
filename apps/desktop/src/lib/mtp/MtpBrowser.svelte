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
     * Supports file operations: delete, new folder, rename.
     */
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { connect, getDevice } from './mtp-store.svelte'
    import { parseMtpPath, joinMtpPath, getMtpParentPath } from './mtp-path-utils'
    import PtpcameradDialog from './PtpcameradDialog.svelte'
    import MtpDeleteDialog from './MtpDeleteDialog.svelte'
    import MtpNewFolderDialog from './MtpNewFolderDialog.svelte'
    import MtpRenameDialog from './MtpRenameDialog.svelte'
    import {
        listMtpDirectory,
        onMtpTransferProgress,
        deleteMtpObject,
        createMtpFolder,
        renameMtpObject,
        downloadMtpFile,
        uploadToMtp,
        type UnlistenFn,
        type MtpTransferProgress,
    } from '$lib/tauri-commands'
    import type { FileEntry } from '$lib/file-explorer/types'
    import { handleNavigationShortcut } from '$lib/file-explorer/keyboard-shortcuts'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('mtp')

    /** Row height for file list */
    const ROW_HEIGHT = 20

    /**
     * Extracts a user-friendly error message from various error formats.
     * Handles Error objects, Tauri error objects (including MTP errors), and plain strings.
     */
    function extractErrorMessage(e: unknown): string {
        if (e instanceof Error) {
            return e.message
        }
        if (typeof e === 'string') {
            // Could be a JSON string from Tauri
            try {
                const parsed = JSON.parse(e) as Record<string, unknown>
                return extractFromParsedError(parsed) || e
            } catch {
                return e
            }
        }
        if (typeof e === 'object' && e !== null) {
            const errObj = e as Record<string, unknown>
            return extractFromParsedError(errObj) || 'Unknown error'
        }
        return String(e)
    }

    /**
     * Checks if an error indicates the MTP connection is unrecoverable.
     * Fatal errors mean the device is disconnected or inaccessible.
     */
    function isFatalMtpError(e: unknown): boolean {
        // Check parsed error type
        const errType = getErrorType(e)
        if (!errType) return false

        // These error types indicate the device is no longer available
        const fatalTypes = ['notConnected', 'deviceNotFound', 'disconnected', 'timeout']
        return fatalTypes.includes(errType)
    }

    /**
     * Extracts the error type from various error formats.
     */
    function getErrorType(e: unknown): string | null {
        if (typeof e === 'string') {
            try {
                const parsed = JSON.parse(e) as Record<string, unknown>
                return (parsed.type as string) || null
            } catch {
                return null
            }
        }
        if (typeof e === 'object' && e !== null) {
            const errObj = e as Record<string, unknown>
            return (errObj.type as string) || null
        }
        return null
    }

    /**
     * Extracts message from a parsed error object.
     * Handles MTP error types with 'type' field and standard error formats.
     */
    function extractFromParsedError(errObj: Record<string, unknown>): string | null {
        // Standard error fields
        if (errObj.userMessage) return String(errObj.userMessage)
        if (errObj.message) return String(errObj.message)

        // MTP errors have a 'type' field (from Rust enum with serde tag="type")
        if (errObj.type && typeof errObj.type === 'string') {
            const deviceId = (errObj.deviceId as string) || (errObj.device_id as string) || 'device'
            const errType = errObj.type as string

            switch (errType) {
                case 'timeout':
                    return `Connection timed out. The device (${deviceId}) may be slow or unresponsive.`
                case 'notConnected':
                    return 'Device not connected. Please reconnect from the volume picker.'
                case 'deviceNotFound':
                    return 'Device not found. It may have been unplugged.'
                case 'alreadyConnected':
                    return 'Device is already connected.'
                case 'exclusiveAccess':
                    return 'Another app is using this device. Close it and try again.'
                case 'disconnected':
                    return 'Device was disconnected.'
                case 'deviceBusy':
                    return 'Device is busy. Please wait and try again.'
                case 'storageFull':
                    return 'Device storage is full.'
                case 'objectNotFound':
                    return `File or folder not found: ${(errObj.path as string) || 'unknown'}`
                case 'protocol':
                    return `Device error: ${(errObj.message as string) || 'protocol error'}`
                case 'other':
                    return (errObj.message as string) || 'An error occurred'
                default:
                    // Unknown type, try to provide useful info
                    return `Error (${errType}): ${JSON.stringify(errObj)}`
            }
        }

        // Fallback: try to stringify
        try {
            const json = JSON.stringify(errObj)
            // Avoid returning unhelpful empty object
            if (json === '{}') return null
            return json
        } catch {
            return null
        }
    }

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
        /** Callback when a fatal error occurs (device disconnected, unrecoverable timeout) - parent should fall back to previous volume */
        onFatalError?: (error: string) => void
    }

    const { path, deviceId, storageId, isFocused = false, onNavigate, onError, onFatalError }: Props = $props()

    // State
    let loading = $state(true)
    let connecting = $state(false)
    let error = $state<string | null>(null)
    // Guard to prevent concurrent loadDirectory calls
    let loadInProgress = $state(false)
    // Track which path was last loaded to prevent redundant loads
    let lastLoadedPath = $state<string | null>(null)
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

    // Operation dialog states
    let showDeleteDialog = $state(false)
    let deleteDialogProps = $state<{
        itemNames: string[]
        fileCount: number
        folderCount: number
        entries: FileEntry[]
    } | null>(null)

    let showNewFolderDialog = $state(false)

    let showRenameDialog = $state(false)
    let renameDialogProps = $state<{
        entry: FileEntry
    } | null>(null)

    // Operation in progress state
    let operationInProgress = $state(false)
    let operationError = $state<string | null>(null)

    // Auto-dismiss operation errors after 5 seconds
    $effect(() => {
        if (operationError) {
            const timer = setTimeout(() => {
                operationError = null
            }, 5000)
            return () => {
                clearTimeout(timer)
            }
        }
    })

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
        log.debug('connectToDevice: starting connection to {deviceId}', { deviceId })
        connecting = true
        error = null

        try {
            log.debug('connectToDevice: calling connect()...')
            await connect(deviceId)
            log.info('Connected to MTP device: {deviceId}', { deviceId })
            log.debug('connectToDevice: connection complete, isConnected={isConnected}', { isConnected })
            // After connection, load the directory
            log.debug('connectToDevice: calling loadDirectory()...')
            await loadDirectory()
            log.debug('connectToDevice: loadDirectory() complete')
        } catch (e) {
            const errorMessage = extractErrorMessage(e)
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

                // Connection failures (except exclusive access) are fatal - trigger fallback
                if (isFatalMtpError(e)) {
                    log.warn('Fatal MTP connection error, triggering fallback: {error}', { error: errorMessage })
                    onFatalError?.(errorMessage)
                }
            }
        } finally {
            connecting = false
        }
    }

    /**
     * Loads the current directory from the MTP device.
     * @param force - If true, ignores lastLoadedPath check and forces a reload
     */
    async function loadDirectory(force = false) {
        log.debug('loadDirectory called, isConnected={isConnected}, loadInProgress={loadInProgress}', {
            isConnected,
            loadInProgress,
        })
        if (!isConnected) {
            log.debug('loadDirectory: not connected, returning early')
            return
        }

        // Prevent concurrent or redundant calls
        if (loadInProgress) {
            log.debug('loadDirectory: already in progress, skipping')
            return
        }

        const currentPath = path
        if (!force && lastLoadedPath === currentPath) {
            log.debug('loadDirectory: path unchanged ({path}), skipping', { path: currentPath })
            return
        }

        loadInProgress = true
        loading = true
        error = null

        try {
            const innerPath = parsed?.path ?? ''
            log.debug('loadDirectory: calling listMtpDirectory({deviceId}, {storageId}, {path})', {
                deviceId,
                storageId,
                path: innerPath || '/',
            })
            const entries = await listMtpDirectory(deviceId, storageId, innerPath)
            log.debug('loadDirectory: got {count} entries', { count: entries.length })
            files = entries
            cursorIndex = 0
            selectedIndices.clear()
            lastLoadedPath = currentPath
            log.debug('Loaded {count} entries from MTP: {path}', { count: entries.length, path: innerPath || '/' })
        } catch (e) {
            const errorMessage = extractErrorMessage(e)
            log.error('Failed to list MTP directory: {error}', { error: errorMessage })
            error = errorMessage
            files = []
            onError?.(errorMessage)

            // Check if this is a fatal error that requires falling back to another volume
            if (isFatalMtpError(e)) {
                log.warn('Fatal MTP error detected, triggering fallback: {error}', { error: errorMessage })
                onFatalError?.(errorMessage)
            }
        } finally {
            loading = false
            loadInProgress = false
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

    // Helper: Handle Enter key (navigate or rename)
    function handleEnterKey(e: KeyboardEvent): boolean {
        const entry = getEntryAtIndex(cursorIndex)
        if (!entry) return false

        if (entry.isDirectory) {
            e.preventDefault()
            handleItemNavigate(entry)
            return true
        }
        if (entry.name !== '..') {
            // For files, Enter opens rename dialog (like Finder)
            e.preventDefault()
            openRenameDialog()
            return true
        }
        return false
    }

    // Helper: Handle action keys (F2, F7, Delete, Backspace)
    function handleActionKeys(e: KeyboardEvent): boolean {
        // F2 - rename
        if (e.key === 'F2') {
            const entry = getEntryAtIndex(cursorIndex)
            if (entry && entry.name !== '..') {
                e.preventDefault()
                openRenameDialog()
                return true
            }
        }
        // Delete or Cmd+Backspace - delete
        if (e.key === 'Delete' || (e.key === 'Backspace' && e.metaKey)) {
            e.preventDefault()
            openDeleteDialog()
            return true
        }
        // Backspace (without Cmd) - go to parent
        if (e.key === 'Backspace' && !e.metaKey && !isAtRoot) {
            e.preventDefault()
            const entry = getEntryAtIndex(0) // ".." entry
            if (entry) handleItemNavigate(entry)
            return true
        }
        // F7 - new folder
        if (e.key === 'F7') {
            e.preventDefault()
            openNewFolderDialog()
            return true
        }
        return false
    }

    // Keyboard handler
    export function handleKeyDown(e: KeyboardEvent): boolean {
        // Don't handle keys while operation in progress
        if (operationInProgress) return false

        // Handle Enter key
        if (e.key === 'Enter' && handleEnterKey(e)) return true

        // Handle action keys (F2, F7, Delete, Backspace)
        if (handleActionKeys(e)) return true

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

    /**
     * Gets the MTP object path for a given display index.
     * Returns the path within the storage (for example, "/DCIM/photo.jpg").
     */
    function getObjectPath(entry: FileEntry): string {
        // Extract the path from the full MTP path
        // Entry paths from listing are already inner paths like "/DCIM/photo.jpg"
        // But we need to handle the case where they might be prefixed differently
        const entryName = entry.name
        const innerPath = parsed?.path ?? ''
        return innerPath ? `${innerPath}/${entryName}` : entryName
    }

    // ============================================================================
    // Delete operation
    // ============================================================================

    /**
     * Opens the delete confirmation dialog for selected files or file under cursor.
     */
    export function openDeleteDialog() {
        const entriesToDelete = getFilesToOperate()
        if (entriesToDelete.length === 0) return

        const fileCount = entriesToDelete.filter((e) => !e.isDirectory).length
        const folderCount = entriesToDelete.filter((e) => e.isDirectory).length

        deleteDialogProps = {
            itemNames: entriesToDelete.map((e) => e.name),
            fileCount,
            folderCount,
            entries: entriesToDelete,
        }
        showDeleteDialog = true
    }

    /**
     * Performs the delete operation on the selected items.
     */
    async function performDelete() {
        if (!deleteDialogProps) return
        const entries = deleteDialogProps.entries

        showDeleteDialog = false
        operationInProgress = true
        operationError = null

        try {
            for (const entry of entries) {
                const objPath = getObjectPath(entry)
                await deleteMtpObject(deviceId, storageId, objPath)
                log.info('Deleted MTP object: {path}', { path: objPath })
            }
            // Refresh the directory listing
            await loadDirectory()
            clearSelection()
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Delete failed: {error}', { error: errorMessage })
            operationError = errorMessage
            onError?.(errorMessage)
        } finally {
            operationInProgress = false
            deleteDialogProps = null
        }
    }

    function handleDeleteCancel() {
        showDeleteDialog = false
        deleteDialogProps = null
    }

    // ============================================================================
    // New folder operation
    // ============================================================================

    /**
     * Opens the new folder dialog.
     */
    export function openNewFolderDialog() {
        showNewFolderDialog = true
    }

    /**
     * Creates a new folder on the MTP device.
     */
    async function performCreateFolder(folderName: string) {
        showNewFolderDialog = false
        operationInProgress = true
        operationError = null

        try {
            const parentPath = parsed?.path ?? ''
            await createMtpFolder(deviceId, storageId, parentPath, folderName)
            log.info('Created MTP folder: {name} in {path}', { name: folderName, path: parentPath || '/' })

            // Refresh and select the new folder
            await loadDirectory()
            clearSelection()

            // Find and select the new folder
            const newFolderIndex = files.findIndex((f) => f.name === folderName)
            if (newFolderIndex >= 0) {
                const displayIndex = isAtRoot ? newFolderIndex : newFolderIndex + 1
                cursorIndex = displayIndex
                scrollToIndex(displayIndex)
            }
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Create folder failed: {error}', { error: errorMessage })
            operationError = errorMessage
            onError?.(errorMessage)
        } finally {
            operationInProgress = false
        }
    }

    function handleNewFolderCancel() {
        showNewFolderDialog = false
    }

    // ============================================================================
    // Rename operation
    // ============================================================================

    /**
     * Opens the rename dialog for the file under cursor.
     */
    export function openRenameDialog() {
        const entry = getEntryAtIndex(cursorIndex)
        if (!entry || entry.name === '..') return

        renameDialogProps = { entry }
        showRenameDialog = true
    }

    /**
     * Renames a file or folder on the MTP device.
     */
    async function performRename(newName: string) {
        if (!renameDialogProps) return
        const entry = renameDialogProps.entry

        showRenameDialog = false
        operationInProgress = true
        operationError = null

        try {
            const objPath = getObjectPath(entry)
            await renameMtpObject(deviceId, storageId, objPath, newName)
            log.info('Renamed MTP object: {oldName} -> {newName}', { oldName: entry.name, newName })

            // Refresh and select the renamed item
            await loadDirectory()
            clearSelection()

            // Find and select the renamed item
            const renamedIndex = files.findIndex((f) => f.name === newName)
            if (renamedIndex >= 0) {
                const displayIndex = isAtRoot ? renamedIndex : renamedIndex + 1
                cursorIndex = displayIndex
                scrollToIndex(displayIndex)
            }
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Rename failed: {error}', { error: errorMessage })
            operationError = errorMessage
            onError?.(errorMessage)
        } finally {
            operationInProgress = false
            renameDialogProps = null
        }
    }

    function handleRenameCancel() {
        showRenameDialog = false
        renameDialogProps = null
    }

    // ============================================================================
    // Helpers for operations
    // ============================================================================

    /**
     * Gets the files to operate on: selected files, or file under cursor if no selection.
     * Excludes the ".." entry.
     */
    function getFilesToOperate(): FileEntry[] {
        if (selectedIndices.size > 0) {
            return getSelectedFiles()
        }
        const entry = getEntryAtIndex(cursorIndex)
        if (entry && entry.name !== '..') {
            return [entry]
        }
        return []
    }

    /**
     * Gets existing file names in the current folder (for conflict checking).
     */
    function getExistingNames(): string[] {
        return files.map((f) => f.name)
    }

    /**
     * Gets the current folder name for display in dialogs.
     */
    function getCurrentFolderName(): string {
        if (!parsed?.path) return 'Root'
        return parsed.path.split('/').pop() || 'Root'
    }

    // ============================================================================
    // Copy operations (download from MTP / upload to MTP)
    // ============================================================================

    /** Transfer progress callback type */
    export type TransferProgressCallback = (progress: MtpTransferProgress) => void

    /** Result from a download/upload operation */
    export interface TransferResult {
        success: boolean
        filesProcessed: number
        bytesTransferred: number
        error?: string
    }

    /**
     * Downloads files from MTP device to a local folder.
     * @param entries - The file entries to download
     * @param localDestination - The local folder path to download to
     * @param onProgress - Optional callback for progress updates
     */
    export async function downloadFiles(
        entries: FileEntry[],
        localDestination: string,
        onProgress?: TransferProgressCallback,
    ): Promise<TransferResult> {
        operationInProgress = true
        operationError = null

        let filesProcessed = 0
        let bytesTransferred = 0
        let progressUnlisten: UnlistenFn | undefined

        try {
            // Set up progress listener if callback provided
            if (onProgress) {
                progressUnlisten = await onMtpTransferProgress(onProgress)
            }

            for (const entry of entries) {
                const objPath = getObjectPath(entry)
                const operationId = crypto.randomUUID()
                const localPath = `${localDestination}/${entry.name}`

                const result = await downloadMtpFile(deviceId, storageId, objPath, localPath, operationId)
                filesProcessed += result.filesProcessed
                bytesTransferred += result.bytesTransferred
                log.info('Downloaded MTP file: {path} -> {local}', { path: objPath, local: localPath })
            }

            return {
                success: true,
                filesProcessed,
                bytesTransferred,
            }
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Download failed: {error}', { error: errorMessage })
            operationError = errorMessage
            onError?.(errorMessage)
            return {
                success: false,
                filesProcessed,
                bytesTransferred,
                error: errorMessage,
            }
        } finally {
            progressUnlisten?.()
            operationInProgress = false
        }
    }

    /**
     * Uploads files from local filesystem to the current MTP folder.
     * @param localPaths - Array of local file paths to upload
     * @param onProgress - Optional callback for progress updates
     */
    export async function uploadFiles(
        localPaths: string[],
        onProgress?: TransferProgressCallback,
    ): Promise<TransferResult> {
        operationInProgress = true
        operationError = null

        let filesProcessed = 0
        const bytesTransferred = 0
        let progressUnlisten: UnlistenFn | undefined

        try {
            // Set up progress listener if callback provided
            if (onProgress) {
                progressUnlisten = await onMtpTransferProgress(onProgress)
            }

            const destFolder = parsed?.path ?? ''

            for (const localPath of localPaths) {
                const operationId = crypto.randomUUID()
                await uploadToMtp(deviceId, storageId, localPath, destFolder, operationId)
                filesProcessed++
                log.info('Uploaded file to MTP: {local} -> {dest}', { local: localPath, dest: destFolder })
            }

            // Refresh the directory listing to show new files
            await loadDirectory()

            return {
                success: true,
                filesProcessed,
                bytesTransferred,
            }
        } catch (e) {
            const errorMessage = e instanceof Error ? e.message : String(e)
            log.error('Upload failed: {error}', { error: errorMessage })
            operationError = errorMessage
            onError?.(errorMessage)
            return {
                success: false,
                filesProcessed,
                bytesTransferred,
                error: errorMessage,
            }
        } finally {
            progressUnlisten?.()
            operationInProgress = false
        }
    }

    /**
     * Gets the MTP volume info for external use.
     */
    export function getMtpInfo(): { deviceId: string; storageId: number; currentPath: string } {
        return {
            deviceId,
            storageId,
            currentPath: parsed?.path ?? '',
        }
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

    {#if showDeleteDialog && deleteDialogProps}
        <MtpDeleteDialog
            itemNames={deleteDialogProps.itemNames}
            fileCount={deleteDialogProps.fileCount}
            folderCount={deleteDialogProps.folderCount}
            onConfirm={() => void performDelete()}
            onCancel={handleDeleteCancel}
        />
    {/if}

    {#if showNewFolderDialog}
        <MtpNewFolderDialog
            currentFolderName={getCurrentFolderName()}
            existingNames={getExistingNames()}
            onConfirm={(name: string) => void performCreateFolder(name)}
            onCancel={handleNewFolderCancel}
        />
    {/if}

    {#if showRenameDialog && renameDialogProps}
        <MtpRenameDialog
            originalName={renameDialogProps.entry.name}
            isDirectory={renameDialogProps.entry.isDirectory}
            existingNames={getExistingNames()}
            onConfirm={(newName: string) => void performRename(newName)}
            onCancel={handleRenameCancel}
        />
    {/if}

    {#if operationError}
        <div class="error-toast" role="alert">
            <span class="error-toast-message">{operationError}</span>
            <button
                type="button"
                class="error-toast-dismiss"
                aria-label="Dismiss"
                onclick={() => {
                    operationError = null
                }}>×</button
            >
        </div>
    {/if}

    {#if operationInProgress}
        <div class="operation-overlay">
            <span class="spinner"></span>
            <span class="status-text">Working...</span>
        </div>
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
                        <span class="file-icon">
                            <svg
                                width="16"
                                height="16"
                                viewBox="0 0 16 16"
                                fill="none"
                                xmlns="http://www.w3.org/2000/svg"
                            >
                                <path
                                    d="M1 3.5C1 2.67 1.67 2 2.5 2H6L7.5 4H13.5C14.33 4 15 4.67 15 5.5V12.5C15 13.33 14.33 14 13.5 14H2.5C1.67 14 1 13.33 1 12.5V3.5Z"
                                    fill="currentColor"
                                    opacity="0.7"
                                />
                            </svg>
                        </span>
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
                        <span class="file-icon">
                            {#if file.isDirectory}
                                <svg
                                    width="16"
                                    height="16"
                                    viewBox="0 0 16 16"
                                    fill="none"
                                    xmlns="http://www.w3.org/2000/svg"
                                >
                                    <path
                                        d="M1 3.5C1 2.67 1.67 2 2.5 2H6L7.5 4H13.5C14.33 4 15 4.67 15 5.5V12.5C15 13.33 14.33 14 13.5 14H2.5C1.67 14 1 13.33 1 12.5V3.5Z"
                                        fill="currentColor"
                                        opacity="0.7"
                                    />
                                </svg>
                            {:else}
                                <svg
                                    width="16"
                                    height="16"
                                    viewBox="0 0 16 16"
                                    fill="none"
                                    xmlns="http://www.w3.org/2000/svg"
                                >
                                    <path
                                        d="M3 1.5C3 0.67 3.67 0 4.5 0H9L13 4V14.5C13 15.33 12.33 16 11.5 16H4.5C3.67 16 3 15.33 3 14.5V1.5Z"
                                        fill="currentColor"
                                        opacity="0.5"
                                    />
                                    <path d="M9 0L13 4H10C9.45 4 9 3.55 9 3V0Z" fill="currentColor" opacity="0.3" />
                                </svg>
                            {/if}
                        </span>
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
        position: relative;
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
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
        color: var(--color-text-secondary);
    }

    .file-icon svg {
        width: 16px;
        height: 16px;
    }

    .empty-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 48px 16px;
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .operation-overlay {
        position: absolute;
        inset: 0;
        background: rgba(0, 0, 0, 0.5);
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 12px;
        z-index: 100;
    }

    .error-toast {
        position: absolute;
        top: var(--spacing-sm);
        left: var(--spacing-sm);
        right: var(--spacing-sm);
        background: var(--color-error-bg, #fef2f2);
        border: 1px solid var(--color-error-border, #fecaca);
        border-radius: 6px;
        padding: var(--spacing-xs) var(--spacing-sm);
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        z-index: 200;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    }

    @media (prefers-color-scheme: dark) {
        .error-toast {
            background: var(--color-error-bg, #450a0a);
            border-color: var(--color-error-border, #7f1d1d);
        }
    }

    .error-toast-message {
        font-size: var(--font-size-sm);
        color: var(--color-error-text, #b91c1c);
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    @media (prefers-color-scheme: dark) {
        .error-toast-message {
            color: var(--color-error-text, #fca5a5);
        }
    }

    .error-toast-dismiss {
        background: none;
        border: none;
        font-size: 18px;
        cursor: pointer;
        color: var(--color-text-tertiary);
        padding: 0 4px;
        line-height: 1;
    }

    .error-toast-dismiss:hover {
        color: var(--color-text-primary);
    }
</style>
