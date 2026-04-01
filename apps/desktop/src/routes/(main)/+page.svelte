<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/pane/DualPaneExplorer.svelte'
    import FunctionKeyBar from '$lib/file-explorer/pane/FunctionKeyBar.svelte'
    import FullDiskAccessPrompt from '$lib/onboarding/FullDiskAccessPrompt.svelte'
    import ExpirationModal from '$lib/licensing/ExpirationModal.svelte'
    import CommercialReminderModal from '$lib/licensing/CommercialReminderModal.svelte'
    import AboutWindow from '$lib/licensing/AboutWindow.svelte'
    import LicenseKeyDialog from '$lib/licensing/LicenseKeyDialog.svelte'
    import CommandPalette from '$lib/command-palette/CommandPalette.svelte'
    import SearchDialog from '$lib/search/SearchDialog.svelte'
    import ScanStatusOverlay from '$lib/indexing/ScanStatusOverlay.svelte'
    import ReplayStatusOverlay from '$lib/indexing/ReplayStatusOverlay.svelte'
    import { initPathLimits } from '$lib/utils/filename-validation'
    import { initIndexState, destroyIndexState } from '$lib/indexing/index'
    import { initShortcutDispatch, destroyShortcutDispatch, lookupCommand } from '$lib/shortcuts/shortcut-dispatch'
    import { formatKeyCombo, isMacOS } from '$lib/shortcuts/key-capture'
    import {
        showMainWindow,
        checkFullDiskAccess,
        listen,
        type UnlistenFn,
        openExternalUrl,
        showInFinder,
        copyToClipboard,
        quickLook,
        getInfo,
        openInEditor,
        toggleHiddenFiles,
        setViewMode,
        setMenuContext,
        getWindowTitle,
        registerKnownDialogs,
        readClipboardText,
    } from '$lib/tauri-commands'
    import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
    import { addToast } from '$lib/ui/toast'
    import { loadSettings, saveSettings } from '$lib/settings-store'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
    import { updateLicenseCommandName } from '$lib/commands/command-registry'
    import type { ViewMode } from '$lib/app-status-store'

    // Interface for DualPaneExplorer's exported methods
    interface ExplorerAPI {
        refocus: () => void
        switchPane: () => void
        swapPanes: () => void
        toggleVolumeChooser: (pane: 'left' | 'right') => void
        openVolumeChooser: () => void
        closeVolumeChooser: () => void
        toggleHiddenFiles: () => void
        setViewMode: (mode: ViewMode, pane?: 'left' | 'right') => void
        navigate: (action: 'back' | 'forward' | 'parent') => void
        getFileAndPathUnderCursor: () => { path: string; filename: string } | null
        sendKeyToFocusedPane: (key: string) => void
        setSortColumn: (column: 'name' | 'extension' | 'size' | 'modified' | 'created', pane?: 'left' | 'right') => void
        setSortOrder: (order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') => void
        setSort: (
            column: 'name' | 'extension' | 'size' | 'modified' | 'created',
            order: 'asc' | 'desc',
            pane: 'left' | 'right',
        ) => Promise<void>
        getFocusedPane: () => 'left' | 'right'
        getFocusedPanePath: () => string
        getVolumes: () => { id: string; name: string; path: string }[]
        selectVolumeByIndex: (pane: 'left' | 'right', index: number) => Promise<boolean>
        selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
        handleSelectionAction: (action: string, startIndex?: number, endIndex?: number) => void
        handleMcpSelect: (pane: 'left' | 'right', start: number, count: number | 'all', mode: string) => void
        startRename: () => void
        openCopyDialog: () => Promise<void>
        openMoveDialog: () => Promise<void>
        copyToClipboard: () => Promise<void>
        cutToClipboard: () => Promise<void>
        pasteFromClipboard: (forceMove: boolean) => Promise<void>
        openNewFolderDialog: () => Promise<void>
        openNewFileDialog: () => Promise<void>
        openDeleteDialog: (permanent: boolean) => Promise<void>
        closeConfirmationDialog: () => void
        isConfirmationDialogOpen: () => boolean
        isRenaming: () => boolean
        openViewerForCursor: () => Promise<void>
        navigateToPath: (pane: 'left' | 'right', path: string) => string | Promise<void>
        moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
        scrollTo: (pane: 'left' | 'right', index: number) => void
        refreshPane: () => void
        refreshNetworkHosts: () => void
        newTab: () => boolean
        closeActiveTab: () => 'closed' | 'last-tab'
        closeActiveTabWithConfirmation: () => Promise<'closed' | 'last-tab' | 'cancelled'>
        cycleTab: (direction: 'next' | 'prev') => void
        togglePinActiveTab: () => void
        closeOtherTabs: () => void
    }

    let showFdaPrompt = $state(false)
    let fdaWasRevoked = $state(false)
    let showApp = $state(false)
    let showExpiredModal = $state(false)
    let expiredOrgName = $state<string | null>(null)
    let expiredAt = $state<string>('')
    let showCommercialReminder = $state(false)
    let showAboutWindow = $state(false)
    let showLicenseKeyDialog = $state(false)
    let showCommandPalette = $state(false)
    let showSearchDialog = $state(false)
    let explorerRef: ExplorerAPI | undefined = $state()
    let windowTitle = $state('Cmdr')
    const showFunctionKeyBar = $state(true)

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined
    let unlistenExecuteCommand: UnlistenFn | undefined
    let unlistenWindowFocus: UnlistenFn | undefined

    /** Opens the debug window (dev mode only) */
    async function openDebugWindow() {
        try {
            const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
            // Check if debug window already exists
            const existing = await WebviewWindow.getByLabel('debug')
            if (existing) {
                await existing.setFocus()
                return
            }
            // Create new debug window
            new WebviewWindow('debug', {
                url: '/debug',
                title: 'Debug',
                width: 480,
                height: 500,
                resizable: true,
                minimizable: false,
                maximizable: false,
                closable: true,
                focus: true,
            })
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('Failed to open debug window:', error)
        }
    }

    /** Check if key event matches ⌘D (debug window, dev only) */
    function isDebugWindowShortcut(e: KeyboardEvent): boolean {
        return import.meta.env.DEV && e.metaKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'd'
    }

    /** Check if key event should be suppressed (Cmd+A, Cmd+Opt+I in prod) */
    function shouldSuppressKey(e: KeyboardEvent): boolean {
        if (e.metaKey && e.key === 'a') return true
        return !import.meta.env.DEV && e.metaKey && e.altKey && e.key === 'i'
    }

    /** Safe wrapper for Tauri event listeners - handles non-Tauri environment */
    async function safeListenTauri(
        event: string,
        handler: (event: { payload: unknown }) => void,
    ): Promise<UnlistenFn | undefined> {
        try {
            return await listen(event, handler)
        } catch {
            return undefined
        }
    }

    /** Get all file viewer windows (labels starting with 'viewer-'), sorted by creation time (most recent first) */
    async function getFileViewerWindows() {
        try {
            const { getAllWindows } = await import('@tauri-apps/api/window')
            const windows = await getAllWindows()
            return windows
                .filter((w) => w.label.startsWith('viewer-'))
                .sort((a, b) => {
                    const aTime = parseInt(a.label.replace('viewer-', ''), 10)
                    const bTime = parseInt(b.label.replace('viewer-', ''), 10)
                    return bTime - aTime // Most recent first
                })
        } catch {
            return []
        }
    }

    /** Emit an event to file viewer windows. Returns true if the event was emitted to at least one viewer. */
    async function emitToFileViewers(event: string, payload?: { path?: string }): Promise<boolean> {
        try {
            const { emit } = await import('@tauri-apps/api/event')
            await emit(event, payload)
            return true
        } catch {
            return false
        }
    }

    /** Close a file viewer window. If path is provided, closes the viewer with that path. Otherwise closes the most recent. */
    async function closeFileViewer(path?: string) {
        const viewers = await getFileViewerWindows()
        if (viewers.length === 0) return

        if (path) {
            // Emit event with path - the viewer with that path will close itself
            await emitToFileViewers('mcp-viewer-close', { path })
        } else {
            // Close the most recent viewer directly
            try {
                await viewers[0].close()
            } catch {
                // Window may already be closed
            }
        }
    }

    /** Close all file viewer windows sequentially to avoid concurrent destruction races */
    async function closeAllFileViewers() {
        const viewers = await getFileViewerWindows()
        for (const viewer of viewers) {
            try {
                await viewer.close()
            } catch {
                // Window may already be closed
            }
        }
    }

    /** Focus a file viewer window. If path is provided, focuses the viewer with that path. Otherwise focuses the most recent. */
    async function focusFileViewer(path?: string) {
        const viewers = await getFileViewerWindows()
        if (viewers.length === 0) return

        if (path) {
            // Emit event with path - the viewer with that path will focus itself
            await emitToFileViewers('mcp-viewer-focus', { path })
        } else {
            try {
                await viewers[0].setFocus()
            } catch {
                // Window may already be closed
            }
        }
    }

    /** Focus the main window */
    async function focusMainWindow() {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            await getCurrentWindow().setFocus()
        } catch {
            // Not in Tauri environment
        }
    }

    /** Set up menu-related event listeners */
    async function setupMenuListeners() {
        // Single unified listener for all menu commands routed through "execute-command"
        unlistenExecuteCommand = await safeListenTauri('execute-command', (event) => {
            const { commandId } = event.payload as { commandId: string }
            void handleCommandExecute(commandId)
        })
    }

    // Unlisten functions for MCP and dialog listeners — cleaned up on destroy (important for HMR)
    const tauriUnlistenFns: UnlistenFn[] = []

    /** Like safeListenTauri but also stores the unlisten function for cleanup. */
    async function listenTauri(event: string, handler: (event: { payload: unknown }) => void): Promise<void> {
        const unlisten = await safeListenTauri(event, handler)
        if (unlisten) tauriUnlistenFns.push(unlisten)
    }

    /** Set up MCP dialog event listeners (close/focus) */
    async function setupDialogListeners() {
        // Settings with section (MCP-specific: "dialog open settings --section shortcuts")
        await listenTauri('open-settings', () => {
            void openSettingsWindow()
        })

        // About dialog
        await listenTauri('close-about', () => {
            showAboutWindow = false
        })
        await listenTauri('focus-about', () => {
            // Already shown, just ensure it's visible
            showAboutWindow = true
        })

        // Volume picker
        await listenTauri('open-volume-picker', () => {
            explorerRef?.openVolumeChooser()
        })
        await listenTauri('close-volume-picker', () => {
            explorerRef?.closeVolumeChooser()
        })
        await listenTauri('focus-volume-picker', () => {
            // Volume picker is handled by DualPaneExplorer
        })

        // File viewer
        await listenTauri('open-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            if (payload?.path) {
                // Open viewer for specific path
                void openFileViewer(payload.path)
            } else {
                // Open viewer for cursor file
                void explorerRef?.openViewerForCursor()
            }
        })
        await listenTauri('close-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            void closeFileViewer(payload?.path)
        })
        await listenTauri('close-all-file-viewers', () => {
            void closeAllFileViewers()
        })
        await listenTauri('focus-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            void focusFileViewer(payload?.path)
        })

        // Confirmation dialog - handled by DualPaneExplorer
        await listenTauri('close-confirmation', () => {
            explorerRef?.closeConfirmationDialog()
        })
        await listenTauri('focus-confirmation', () => {
            // The confirmation dialog is a modal overlay in the main window.
            // If it's open, ensure the main window is focused so the dialog is visible.
            if (explorerRef?.isConfirmationDialogOpen()) {
                void focusMainWindow()
            }
        })
    }

    /** Set up MCP-related event listeners */
    async function setupMcpListeners() {
        await listenTauri('mcp-key', (event) => {
            const { key } = event.payload as { key: string }
            if (key === 'GoBack') {
                explorerRef?.navigate('back')
            } else if (key === 'GoForward') {
                explorerRef?.navigate('forward')
            } else {
                explorerRef?.sendKeyToFocusedPane(key)
            }
        })

        await listenTauri('menu-sort', (event) => {
            const { action, value } = event.payload as { action: string; value: string }
            if (action === 'sortBy') {
                const column = value as 'name' | 'extension' | 'size' | 'modified' | 'created'
                explorerRef?.setSortColumn(column)
            } else if (action === 'sortOrder') {
                const order = value as 'asc' | 'desc' | 'toggle'
                explorerRef?.setSortOrder(order)
            }
        })

        await listenTauri('mcp-sort', (event) => {
            const { pane, by, order } = event.payload as { pane: 'left' | 'right'; by: string; order: string }
            const column = by === 'ext' ? 'extension' : (by as 'name' | 'extension' | 'size' | 'modified' | 'created')
            void explorerRef?.setSort(column, order as 'asc' | 'desc', pane)
        })

        await listenTauri('mcp-volume-select', (event) => {
            const { pane, name } = event.payload as { pane: 'left' | 'right'; name: string }
            void explorerRef?.selectVolumeByName(pane, name)
        })

        await listenTauri('mcp-select', (event) => {
            const { pane, start, count, mode } = event.payload as {
                pane: 'left' | 'right'
                start: number
                count: number | 'all'
                mode: string
            }
            explorerRef?.handleMcpSelect(pane, start, count, mode)
        })

        await listenTauri('mcp-nav-to-path', (event) => {
            const { pane, path, requestId } = event.payload as {
                pane: 'left' | 'right'
                path: string
                requestId?: string
            }
            // explorerRef may be null during HMR — skip silently, let the backend timeout handle it
            if (!explorerRef) return
            const result = explorerRef.navigateToPath(pane, path)
            if (requestId) {
                void (async () => {
                    const { emit } = await import('@tauri-apps/api/event')
                    if (typeof result === 'string') {
                        // Synchronous error (pane not available, wrong volume, etc.)
                        await emit('mcp-response', { requestId, ok: false, error: result })
                    } else {
                        // Promise — wait for directory listing to complete
                        try {
                            await result
                            await emit('mcp-response', { requestId, ok: true })
                        } catch (e) {
                            const error = e instanceof Error ? e.message : String(e)
                            await emit('mcp-response', { requestId, ok: false, error })
                        }
                    }
                })()
            }
        })

        await listenTauri('mcp-move-cursor', (event) => {
            const { pane, to } = event.payload as { pane: 'left' | 'right'; to: number | string }
            void explorerRef?.moveCursor(pane, to)
        })

        await listenTauri('mcp-scroll-to', (event) => {
            const { pane, index } = event.payload as { pane: 'left' | 'right'; index: number }
            explorerRef?.scrollTo(pane, index)
        })

        await listenTauri('mcp-set-view-mode', (event) => {
            const { pane, mode } = event.payload as { pane: 'left' | 'right'; mode: string }
            explorerRef?.setViewMode(mode as ViewMode, pane)
        })

        await listenTauri('mcp-refresh', () => {
            explorerRef?.refreshPane()
        })

        await listenTauri('mcp-copy', () => {
            void explorerRef?.openCopyDialog()
        })

        await listenTauri('mcp-mkdir', () => {
            void explorerRef?.openNewFolderDialog()
        })

        await listenTauri('mcp-mkfile', () => {
            void explorerRef?.openNewFileDialog()
        })

        await listenTauri('mcp-delete', () => {
            void explorerRef?.openDeleteDialog(false)
        })
    }

    /** Check if any modal dialog is open that should suppress centralized dispatch. */
    function isModalDialogOpen(): boolean {
        return (
            showCommandPalette ||
            showSearchDialog ||
            showAboutWindow ||
            showLicenseKeyDialog ||
            showExpiredModal ||
            showCommercialReminder ||
            (explorerRef?.isConfirmationDialogOpen() ?? false) ||
            (explorerRef?.isRenaming() ?? false)
        )
    }

    /** Global keyboard handler for app-level shortcuts */
    function handleGlobalKeyDown(e: KeyboardEvent): void {
        // Centralized dispatch: look up the command for this key combo
        if (!isModalDialogOpen()) {
            const shortcutString = formatKeyCombo(e)
            const commandId = lookupCommand(shortcutString)
            if (commandId) {
                e.preventDefault()
                e.stopPropagation()
                void handleCommandExecute(commandId)
                return
            }
        }

        // Special cases not handled by centralized dispatch:
        // - Debug window: dev-only, not worth registering as a command
        // - Key suppression: browser behavior overrides, not commands
        if (isDebugWindowShortcut(e)) {
            e.preventDefault()
            void openDebugWindow()
        } else if (shouldSuppressKey(e)) {
            e.preventDefault()
        }
    }

    /** Start window drag when title bar is clicked */
    async function handleTitleBarMouseDown(e: MouseEvent) {
        if (e.buttons === 1) {
            e.preventDefault() // Prevent focus shift away from explorer
            try {
                const { getCurrentWindow } = await import('@tauri-apps/api/window')
                await getCurrentWindow().startDragging()
            } catch {
                // Not in Tauri environment
            }
        }
    }

    onMount(async () => {
        // Hide loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Fetch platform-specific path limits (non-blocking, macOS defaults until resolved)
        void initPathLimits()

        // Register known dialog types with backend (for MCP "available dialogs" resource)
        void registerKnownDialogs(SOFT_DIALOG_REGISTRY)

        // Load license status from cache (fast, no network)
        try {
            const licenseStatus = await loadLicenseStatus()

            // Fire-and-forget: validate with server in background if needed.
            // Updates the cache silently; next launch picks up the result.
            void triggerValidationIfNeeded()

            // Check if we need to show expiration modal
            if (licenseStatus.type === 'expired' && licenseStatus.showModal) {
                showExpiredModal = true
                expiredOrgName = licenseStatus.organizationName
                expiredAt = licenseStatus.expiredAt
            }

            // Check if we need to show commercial reminder for personal users
            if (licenseStatus.type === 'personal' && licenseStatus.showCommercialReminder) {
                showCommercialReminder = true
            }

            // Update command palette label to match native menu
            updateLicenseCommandName(licenseStatus.type !== 'personal')

            // Load window title based on license status
            windowTitle = await getWindowTitle()
        } catch {
            // License check failed (expected in E2E tests without Tauri backend)
            // App continues without license features
        }

        // Check FDA status
        const settings = await loadSettings()
        const hasFda = await checkFullDiskAccess()

        if (hasFda) {
            // Already have FDA - ensure setting reflects this
            if (settings.fullDiskAccessChoice !== 'allow') {
                await saveSettings({ fullDiskAccessChoice: 'allow' })
            }
            showApp = true
        } else if (settings.fullDiskAccessChoice === 'notAskedYet') {
            // First time - show onboarding
            showFdaPrompt = true
        } else if (settings.fullDiskAccessChoice === 'allow') {
            // User previously allowed but FDA was revoked - show prompt with different text
            showFdaPrompt = true
            fdaWasRevoked = true
        } else {
            // User explicitly denied - proceed without prompting
            showApp = true
        }

        // Show window when ready
        void showMainWindow()

        // Initialize centralized shortcut dispatch and global keyboard/context menu
        // handlers. These must be registered BEFORE setupTauriEventListeners() because
        // that call may throw in non-Tauri environments (e.g. Playwright smoke tests).
        initShortcutDispatch()

        handleKeyDown = handleGlobalKeyDown
        handleContextMenu = (e: MouseEvent) => {
            e.preventDefault()
        }
        document.addEventListener('keydown', handleKeyDown)
        document.addEventListener('contextmenu', handleContextMenu)

        // Set up Tauri event listeners (extracted to reduce complexity)
        await setupTauriEventListeners()
    })

    /**
     * Set up Tauri event listeners for menu actions, MCP events, etc.
     */
    async function setupTauriEventListeners() {
        await setupMenuListeners()
        await setupDialogListeners()
        await setupMcpListeners()
        await initIndexState()
        await setupWindowFocusListener()
    }

    /** Sync file-scoped menu items with main window focus state. */
    async function setupWindowFocusListener() {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            unlistenWindowFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
                void setMenuContext(focused ? 'explorer' : 'other')
            })
        } catch {
            // Not in Tauri environment
        }
    }

    onDestroy(() => {
        destroyShortcutDispatch()
        destroyIndexState()
        if (handleKeyDown) {
            document.removeEventListener('keydown', handleKeyDown)
        }
        if (handleContextMenu) {
            document.removeEventListener('contextmenu', handleContextMenu)
        }
        if (unlistenExecuteCommand) {
            unlistenExecuteCommand()
        }
        if (unlistenWindowFocus) {
            unlistenWindowFocus()
        }
        // Clean up MCP and dialog listeners (prevents duplicate listeners after HMR)
        for (const unlisten of tauriUnlistenFns) {
            unlisten()
        }
        tauriUnlistenFns.length = 0
    })

    function handleFdaComplete() {
        showFdaPrompt = false
        showApp = true
    }

    function handleExpirationModalClose() {
        showExpiredModal = false
        hideExpirationModal()
    }

    function handleCommercialReminderClose() {
        showCommercialReminder = false
    }

    function handleAboutClose() {
        showAboutWindow = false
    }

    async function handleLicenseKeyDialogClose() {
        showLicenseKeyDialog = false
        windowTitle = await getWindowTitle()
    }

    async function handleLicenseKeySuccess() {
        showLicenseKeyDialog = false
        // Refresh the window title and command palette label to reflect new license status
        updateLicenseCommandName(true)
        windowTitle = await getWindowTitle()
        // Show the About window so user can see their license status
        showAboutWindow = true
    }

    function handleCommandPaletteClose() {
        showCommandPalette = false
    }

    function handleSearchDialogClose() {
        showSearchDialog = false
    }

    function handleSearchNavigate(path: string) {
        showSearchDialog = false
        // Navigate the focused pane to the file's parent directory, then move cursor to the file
        const lastSlash = path.lastIndexOf('/')
        const parentDir = lastSlash > 0 ? path.slice(0, lastSlash) : '/'
        const fileName = path.slice(lastSlash + 1)
        const pane = explorerRef?.getFocusedPane() ?? 'left'
        const result = explorerRef?.navigateToPath(pane, parentDir)
        if (result instanceof Promise) {
            void result.then(() => explorerRef?.moveCursor(pane, fileName))
        }
    }

    function handleFnView() {
        void explorerRef?.openViewerForCursor()
    }

    async function handleFnEdit() {
        const entry = explorerRef?.getFileAndPathUnderCursor()
        if (entry) {
            await openInEditor(entry.path)
        }
    }

    function handleFnCopy() {
        void explorerRef?.openCopyDialog()
    }

    function handleFnMove() {
        void explorerRef?.openMoveDialog()
    }

    function handleFnRename() {
        explorerRef?.startRename()
    }

    function handleFnNewFile() {
        void explorerRef?.openNewFileDialog()
    }

    function handleFnNewFolder() {
        void explorerRef?.openNewFolderDialog()
    }

    function handleFnDelete() {
        void explorerRef?.openDeleteDialog(false)
    }

    function handleFnDeletePermanently() {
        void explorerRef?.openDeleteDialog(true)
    }

    // eslint-disable-next-line complexity -- Command dispatcher handles many cases; switch is the clearest pattern
    async function handleCommandExecute(commandId: string) {
        showCommandPalette = false

        // Handle known commands by category
        switch (commandId) {
            // === App commands ===
            // app.quit, app.hide, app.hideOthers, app.showAll are native-only —
            // handled by PredefinedMenuItems (terminate:, hide:, etc.), not JS dispatch.

            case 'app.commandPalette':
                showCommandPalette = true
                return

            case 'search.open':
                if (!showSearchDialog) {
                    showSearchDialog = true
                }
                return

            case 'app.settings':
                void openSettingsWindow()
                return

            case 'app.about':
                showAboutWindow = true
                return

            case 'app.licenseKey':
                showLicenseKeyDialog = true
                return

            // === View commands ===
            case 'view.showHidden':
                // Use Tauri command to toggle and sync menu checkbox state
                await toggleHiddenFiles()
                return

            case 'view.briefMode':
                // Use Tauri command to set mode and sync menu radio state
                await setViewMode('brief')
                return

            case 'view.fullMode':
                // Use Tauri command to set mode and sync menu radio state
                await setViewMode('full')
                return

            // === Pane commands ===
            case 'pane.switch':
                explorerRef?.switchPane()
                return

            case 'pane.swap':
                explorerRef?.swapPanes()
                return

            case 'pane.leftVolumeChooser':
                explorerRef?.toggleVolumeChooser('left')
                return

            case 'pane.rightVolumeChooser':
                explorerRef?.toggleVolumeChooser('right')
                return

            // === Tab commands ===
            case 'tab.new': {
                const success = explorerRef?.newTab()
                if (success === false) {
                    addToast('Tab limit reached')
                }
                return
            }

            case 'tab.close': {
                const result = await explorerRef?.closeActiveTabWithConfirmation()
                if (result === 'last-tab') {
                    const { getCurrentWindow } = await import('@tauri-apps/api/window')
                    await getCurrentWindow().close()
                }
                return
            }

            case 'tab.next':
                explorerRef?.cycleTab('next')
                return

            case 'tab.prev':
                explorerRef?.cycleTab('prev')
                return

            case 'tab.togglePin':
                explorerRef?.togglePinActiveTab()
                return

            case 'tab.closeOthers':
                explorerRef?.closeOtherTabs()
                return

            // === Navigation commands ===
            case 'nav.open':
                explorerRef?.sendKeyToFocusedPane('Enter')
                return

            case 'nav.parent':
                explorerRef?.navigate('parent')
                return

            case 'nav.back':
                explorerRef?.navigate('back')
                return

            case 'nav.forward':
                explorerRef?.navigate('forward')
                return

            case 'nav.home':
                explorerRef?.sendKeyToFocusedPane('Home')
                return

            case 'nav.end':
                explorerRef?.sendKeyToFocusedPane('End')
                return

            case 'nav.pageUp':
                explorerRef?.sendKeyToFocusedPane('PageUp')
                return

            case 'nav.pageDown':
                explorerRef?.sendKeyToFocusedPane('PageDown')
                return

            // === Network commands ===
            case 'network.refresh':
                explorerRef?.refreshNetworkHosts()
                return

            // === Sort commands ===
            case 'sort.byName':
                explorerRef?.setSortColumn('name')
                return

            case 'sort.byExtension':
                explorerRef?.setSortColumn('extension')
                return

            case 'sort.bySize':
                explorerRef?.setSortColumn('size')
                return

            case 'sort.byModified':
                explorerRef?.setSortColumn('modified')
                return

            case 'sort.byCreated':
                explorerRef?.setSortColumn('created')
                return

            case 'sort.ascending':
                explorerRef?.setSortOrder('asc')
                return

            case 'sort.descending':
                explorerRef?.setSortOrder('desc')
                return

            case 'sort.toggleOrder':
                explorerRef?.setSortOrder('toggle')
                return

            // === File action commands ===
            case 'file.view':
                void explorerRef?.openViewerForCursor()
                return

            case 'file.rename':
                explorerRef?.startRename()
                return

            case 'file.edit': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await openInEditor(entryUnderCursor.path)
                }
                return
            }

            case 'file.copy':
                void explorerRef?.openCopyDialog()
                return

            case 'file.move':
                void explorerRef?.openMoveDialog()
                return

            case 'file.newFolder':
                void explorerRef?.openNewFolderDialog()
                return

            case 'file.newFile':
                void explorerRef?.openNewFileDialog()
                return

            case 'file.delete':
                void explorerRef?.openDeleteDialog(false)
                return

            case 'file.deletePermanently':
                void explorerRef?.openDeleteDialog(true)
                return

            case 'file.showInFinder': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await showInFinder(entryUnderCursor.path)
                }
                return
            }

            case 'file.copyPath': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await copyToClipboard(entryUnderCursor.path)
                }
                return
            }

            case 'file.copyFilename': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await copyToClipboard(entryUnderCursor.filename)
                }
                return
            }

            case 'file.quickLook': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await quickLook(entryUnderCursor.path)
                }
                return
            }

            case 'file.getInfo': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await getInfo(entryUnderCursor.path)
                }
                return
            }

            // === Selection commands ===
            case 'selection.selectAll': {
                // ⌘A is a native menu accelerator (so it shows in the Edit menu), which means
                // macOS intercepts it before the webview. When a text input is focused, route
                // to the input's select-all instead of file selection.
                const active = document.activeElement
                if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
                    active.select()
                    return
                }
                explorerRef?.handleSelectionAction('selectAll')
                return
            }

            case 'selection.deselectAll':
                explorerRef?.handleSelectionAction('deselectAll')
                return

            // === Edit commands (clipboard) ===
            case 'edit.copy': {
                const active = document.activeElement
                if (
                    active instanceof HTMLInputElement ||
                    active instanceof HTMLTextAreaElement ||
                    active?.closest('[contenteditable]')
                ) {
                    // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native copy in text inputs
                    document.execCommand('copy')
                    return
                }
                void explorerRef?.copyToClipboard()
                return
            }

            case 'edit.cut': {
                const active = document.activeElement
                if (
                    active instanceof HTMLInputElement ||
                    active instanceof HTMLTextAreaElement ||
                    active?.closest('[contenteditable]')
                ) {
                    // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native cut in text inputs
                    document.execCommand('cut')
                    return
                }
                void explorerRef?.cutToClipboard()
                return
            }

            case 'edit.paste': {
                const active = document.activeElement
                if (
                    active instanceof HTMLInputElement ||
                    active instanceof HTMLTextAreaElement ||
                    active?.closest('[contenteditable]')
                ) {
                    // Read clipboard text via Rust (bypasses WebKit's navigator.clipboard
                    // permission popup that shows a "Paste" button the user must click).
                    const text = await readClipboardText()
                    if (text) {
                        // eslint-disable-next-line @typescript-eslint/no-deprecated -- insertText is the only way to insert at cursor position in inputs
                        document.execCommand('insertText', false, text)
                    }
                    return
                }
                void explorerRef?.pasteFromClipboard(false)
                return
            }

            case 'edit.pasteAsMove':
                // Option+Cmd+V is not a text shortcut, so no activeElement check needed
                void explorerRef?.pasteFromClipboard(true)
                return

            // === About window commands ===
            case 'about.openWebsite':
                await openExternalUrl('https://getcmdr.com')
                return

            case 'about.openUpgrade':
                await openExternalUrl('https://getcmdr.com/upgrade')
                return

            case 'about.close':
                showAboutWindow = false
                return
        }
    }
</script>

<div class="page-container">
    {#if isMacOS()}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <header class="title-bar" class:dev-mode={import.meta.env.DEV} onmousedown={handleTitleBarMouseDown}>
            <span class="title-text">{import.meta.env.DEV ? `DEV MODE - ${windowTitle} - DEV MODE` : windowTitle}</span>
        </header>
    {/if}

    <main class="main-content">
        <h1 class="sr-only">Cmdr</h1>
        {#if showAboutWindow}
            <AboutWindow onClose={handleAboutClose} />
        {/if}

        {#if showLicenseKeyDialog}
            <LicenseKeyDialog onClose={handleLicenseKeyDialogClose} onSuccess={handleLicenseKeySuccess} />
        {/if}

        {#if showCommandPalette}
            <CommandPalette onExecute={handleCommandExecute} onClose={handleCommandPaletteClose} />
        {/if}

        {#if showSearchDialog}
            <SearchDialog
                onNavigate={handleSearchNavigate}
                onClose={handleSearchDialogClose}
                currentFolderPath={explorerRef?.getFocusedPanePath() ?? '/'}
            />
        {/if}

        {#if showExpiredModal}
            <ExpirationModal organizationName={expiredOrgName} {expiredAt} onClose={handleExpirationModalClose} />
        {/if}

        {#if showCommercialReminder}
            <CommercialReminderModal onClose={handleCommercialReminderClose} />
        {/if}

        {#if showFdaPrompt}
            <FullDiskAccessPrompt onComplete={handleFdaComplete} wasRevoked={fdaWasRevoked} />
        {:else if showApp}
            <DualPaneExplorer bind:this={explorerRef} />
            <ScanStatusOverlay />
            <ReplayStatusOverlay />
        {/if}

        {#if showApp}
            <FunctionKeyBar
                visible={showFunctionKeyBar}
                onRename={handleFnRename}
                onView={handleFnView}
                onEdit={() => void handleFnEdit()}
                onCopy={handleFnCopy}
                onMove={handleFnMove}
                onNewFile={handleFnNewFile}
                onNewFolder={handleFnNewFolder}
                onDelete={handleFnDelete}
                onDeletePermanently={handleFnDeletePermanently}
            />
        {/if}
    </main>
</div>

<style>
    .page-container {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }

    .title-bar {
        height: 27px;
        display: flex;
        align-items: center;
        justify-content: center;
        padding-top: var(--spacing-xxs);
        background-color: var(--color-bg-secondary);
        flex-shrink: 0;
        position: relative;
    }

    /*noinspection CssUnusedSymbol*/
    .title-bar.dev-mode::after {
        content: '';
        position: absolute;
        inset: 0;
        background-color: color-mix(in srgb, hotpink, transparent 40%);
        pointer-events: none;
    }

    .title-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        font-weight: 500;
    }

    .main-content {
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        min-height: 0;
    }
</style>
