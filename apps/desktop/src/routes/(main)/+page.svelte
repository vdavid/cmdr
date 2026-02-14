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
        getWindowTitle,
        registerKnownDialogs,
    } from '$lib/tauri-commands'
    import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
    import { loadSettings, saveSettings } from '$lib/settings-store'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
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
        getVolumes: () => { id: string; name: string; path: string }[]
        selectVolumeByIndex: (pane: 'left' | 'right', index: number) => Promise<boolean>
        selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
        handleSelectionAction: (action: string, startIndex?: number, endIndex?: number) => void
        handleMcpSelect: (pane: 'left' | 'right', start: number, count: number | 'all', mode: string) => void
        startRename: () => void
        openCopyDialog: () => Promise<void>
        openMoveDialog: () => Promise<void>
        openNewFolderDialog: () => Promise<void>
        closeConfirmationDialog: () => void
        isConfirmationDialogOpen: () => boolean
        openViewerForCursor: () => Promise<void>
        navigateToPath: (pane: 'left' | 'right', path: string) => void
        moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
        scrollTo: (pane: 'left' | 'right', index: number) => void
        refreshPane: () => void
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
    let explorerRef: ExplorerAPI | undefined = $state()
    let windowTitle = $state('Cmdr')
    const showFunctionKeyBar = $state(true)

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined
    let unlistenShowAbout: UnlistenFn | undefined
    let unlistenLicenseKeyDialog: UnlistenFn | undefined
    let unlistenCommandPalette: UnlistenFn | undefined
    let unlistenSwitchPane: UnlistenFn | undefined
    let unlistenSwapPanes: UnlistenFn | undefined
    let unlistenStartRename: UnlistenFn | undefined

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

    /** Check if key event matches ⌘⇧P (command palette) */
    function isCommandPaletteShortcut(e: KeyboardEvent): boolean {
        return e.metaKey && e.shiftKey && e.key.toLowerCase() === 'p'
    }

    /** Check if key event matches ⌘, (settings) */
    function isSettingsShortcut(e: KeyboardEvent): boolean {
        return e.metaKey && !e.shiftKey && !e.altKey && e.key === ','
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
        unlistenShowAbout = await safeListenTauri('show-about', () => {
            showAboutWindow = true
        })
        unlistenLicenseKeyDialog = await safeListenTauri('show-license-key-dialog', () => {
            showLicenseKeyDialog = true
        })
        unlistenCommandPalette = await safeListenTauri('show-command-palette', () => {
            showCommandPalette = true
        })
        await safeListenTauri('open-settings', () => {
            void openSettingsWindow()
        })
        unlistenSwitchPane = await safeListenTauri('switch-pane', () => {
            explorerRef?.switchPane()
        })
        unlistenSwapPanes = await safeListenTauri('swap-panes', () => {
            explorerRef?.swapPanes()
        })
        unlistenStartRename = await safeListenTauri('start-rename', () => {
            explorerRef?.startRename()
        })
    }

    /** Set up MCP dialog event listeners (close/focus) */
    async function setupDialogListeners() {
        // About dialog
        await safeListenTauri('close-about', () => {
            showAboutWindow = false
        })
        await safeListenTauri('focus-about', () => {
            // Already shown, just ensure it's visible
            showAboutWindow = true
        })

        // Volume picker
        await safeListenTauri('open-volume-picker', () => {
            explorerRef?.openVolumeChooser()
        })
        await safeListenTauri('close-volume-picker', () => {
            explorerRef?.closeVolumeChooser()
        })
        await safeListenTauri('focus-volume-picker', () => {
            // Volume picker is handled by DualPaneExplorer
        })

        // File viewer
        await safeListenTauri('open-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            if (payload?.path) {
                // Open viewer for specific path
                void openFileViewer(payload.path)
            } else {
                // Open viewer for cursor file
                void explorerRef?.openViewerForCursor()
            }
        })
        await safeListenTauri('close-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            void closeFileViewer(payload?.path)
        })
        await safeListenTauri('close-all-file-viewers', () => {
            void closeAllFileViewers()
        })
        await safeListenTauri('focus-file-viewer', (event) => {
            const payload = event.payload as { path?: string } | undefined
            void focusFileViewer(payload?.path)
        })

        // Confirmation dialog - handled by DualPaneExplorer
        await safeListenTauri('close-confirmation', () => {
            explorerRef?.closeConfirmationDialog()
        })
        await safeListenTauri('focus-confirmation', () => {
            // The confirmation dialog is a modal overlay in the main window.
            // If it's open, ensure the main window is focused so the dialog is visible.
            if (explorerRef?.isConfirmationDialogOpen()) {
                void focusMainWindow()
            }
        })
    }

    /** Set up MCP-related event listeners */
    async function setupMcpListeners() {
        await safeListenTauri('mcp-key', (event) => {
            const { key } = event.payload as { key: string }
            if (key === 'GoBack') {
                explorerRef?.navigate('back')
            } else if (key === 'GoForward') {
                explorerRef?.navigate('forward')
            } else {
                explorerRef?.sendKeyToFocusedPane(key)
            }
        })

        await safeListenTauri('menu-sort', (event) => {
            const { action, value } = event.payload as { action: string; value: string }
            if (action === 'sortBy') {
                const column = value as 'name' | 'extension' | 'size' | 'modified' | 'created'
                explorerRef?.setSortColumn(column)
            } else if (action === 'sortOrder') {
                const order = value as 'asc' | 'desc' | 'toggle'
                explorerRef?.setSortOrder(order)
            }
        })

        await safeListenTauri('mcp-sort', (event) => {
            const { pane, by, order } = event.payload as { pane: 'left' | 'right'; by: string; order: string }
            const column = by === 'ext' ? 'extension' : (by as 'name' | 'extension' | 'size' | 'modified' | 'created')
            void explorerRef?.setSort(column, order as 'asc' | 'desc', pane)
        })

        await safeListenTauri('mcp-volume-select', (event) => {
            const { pane, name } = event.payload as { pane: 'left' | 'right'; name: string }
            void explorerRef?.selectVolumeByName(pane, name)
        })

        await safeListenTauri('mcp-select', (event) => {
            const { pane, start, count, mode } = event.payload as {
                pane: 'left' | 'right'
                start: number
                count: number | 'all'
                mode: string
            }
            explorerRef?.handleMcpSelect(pane, start, count, mode)
        })

        await safeListenTauri('mcp-nav-to-path', (event) => {
            const { pane, path } = event.payload as { pane: 'left' | 'right'; path: string }
            explorerRef?.navigateToPath(pane, path)
        })

        await safeListenTauri('mcp-move-cursor', (event) => {
            const { pane, to } = event.payload as { pane: 'left' | 'right'; to: number | string }
            void explorerRef?.moveCursor(pane, to)
        })

        await safeListenTauri('mcp-scroll-to', (event) => {
            const { pane, index } = event.payload as { pane: 'left' | 'right'; index: number }
            explorerRef?.scrollTo(pane, index)
        })

        await safeListenTauri('mcp-set-view-mode', (event) => {
            const { pane, mode } = event.payload as { pane: 'left' | 'right'; mode: string }
            explorerRef?.setViewMode(mode as ViewMode, pane)
        })

        await safeListenTauri('mcp-refresh', () => {
            explorerRef?.refreshPane()
        })

        await safeListenTauri('mcp-copy', () => {
            void explorerRef?.openCopyDialog()
        })

        await safeListenTauri('mcp-mkdir', () => {
            void explorerRef?.openNewFolderDialog()
        })
    }

    /** Global keyboard handler for app-level shortcuts */
    function handleGlobalKeyDown(e: KeyboardEvent): void {
        if (isCommandPaletteShortcut(e)) {
            e.preventDefault()
            showCommandPalette = true
        } else if (isSettingsShortcut(e)) {
            e.preventDefault()
            void openSettingsWindow()
        } else if (isDebugWindowShortcut(e)) {
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

        // Register known dialog types with backend (for MCP "available dialogs" resource)
        void registerKnownDialogs(SOFT_DIALOG_REGISTRY)

        // Load license status first (non-blocking - don't prevent app load on failure)
        try {
            let licenseStatus = await loadLicenseStatus()

            // Trigger background validation if needed
            const validatedStatus = await triggerValidationIfNeeded()
            if (validatedStatus) {
                licenseStatus = validatedStatus
            }

            // Check if we need to show expiration modal
            if (licenseStatus.type === 'expired' && licenseStatus.showModal) {
                showExpiredModal = true
                expiredOrgName = licenseStatus.organizationName
                expiredAt = licenseStatus.expiredAt
            }

            // Check if we need to show commercial reminder for personal/supporter users
            if (
                (licenseStatus.type === 'personal' || licenseStatus.type === 'supporter') &&
                licenseStatus.showCommercialReminder
            ) {
                showCommercialReminder = true
            }

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

        // Set up Tauri event listeners (extracted to reduce complexity)
        await setupTauriEventListeners()

        // Global keyboard shortcuts
        handleKeyDown = handleGlobalKeyDown

        // Suppress right-click context menu
        handleContextMenu = (e: MouseEvent) => {
            e.preventDefault()
        }

        document.addEventListener('keydown', handleKeyDown)
        document.addEventListener('contextmenu', handleContextMenu)
    })

    /**
     * Set up Tauri event listeners for menu actions, MCP events, etc.
     */
    async function setupTauriEventListeners() {
        await setupMenuListeners()
        await setupDialogListeners()
        await setupMcpListeners()
    }

    onDestroy(() => {
        if (handleKeyDown) {
            document.removeEventListener('keydown', handleKeyDown)
        }
        if (handleContextMenu) {
            document.removeEventListener('contextmenu', handleContextMenu)
        }
        if (unlistenShowAbout) {
            unlistenShowAbout()
        }
        if (unlistenLicenseKeyDialog) {
            unlistenLicenseKeyDialog()
        }
        if (unlistenCommandPalette) {
            unlistenCommandPalette()
        }
        if (unlistenSwitchPane) {
            unlistenSwitchPane()
        }
        if (unlistenSwapPanes) {
            unlistenSwapPanes()
        }
        if (unlistenStartRename) {
            unlistenStartRename()
        }
    })

    function handleFdaComplete() {
        showFdaPrompt = false
        showApp = true
    }

    function handleExpirationModalClose() {
        showExpiredModal = false
        hideExpirationModal()
        explorerRef?.refocus()
    }

    function handleCommercialReminderClose() {
        showCommercialReminder = false
        explorerRef?.refocus()
    }

    function handleAboutClose() {
        showAboutWindow = false
        explorerRef?.refocus()
    }

    function handleLicenseKeyDialogClose() {
        showLicenseKeyDialog = false
        explorerRef?.refocus()
    }

    async function handleLicenseKeySuccess() {
        showLicenseKeyDialog = false
        // Refresh the window title to reflect new license status
        windowTitle = await getWindowTitle()
        // Show the About window so user can see their license status
        showAboutWindow = true
    }

    function handleCommandPaletteClose() {
        showCommandPalette = false
        explorerRef?.refocus()
    }

    function handleFnView() {
        void explorerRef?.openViewerForCursor()
        explorerRef?.refocus()
    }

    async function handleFnEdit() {
        const entry = explorerRef?.getFileAndPathUnderCursor()
        if (entry) {
            await openInEditor(entry.path)
        }
        explorerRef?.refocus()
    }

    function handleFnCopy() {
        void explorerRef?.openCopyDialog()
        explorerRef?.refocus()
    }

    function handleFnMove() {
        void explorerRef?.openMoveDialog()
        explorerRef?.refocus()
    }

    function handleFnRename() {
        explorerRef?.startRename()
        explorerRef?.refocus()
    }

    function handleFnNewFolder() {
        void explorerRef?.openNewFolderDialog()
        explorerRef?.refocus()
    }

    // eslint-disable-next-line complexity -- Command dispatcher handles many cases; switch is the clearest pattern
    async function handleCommandExecute(commandId: string) {
        showCommandPalette = false

        // Handle known commands by category
        switch (commandId) {
            // === App commands ===
            case 'app.quit':
                // Quit is handled by the OS/Tauri, we just need to trigger the window close
                try {
                    const { getCurrentWindow } = await import('@tauri-apps/api/window')
                    await getCurrentWindow().close()
                } catch {
                    // Not in Tauri environment
                }
                return

            case 'app.hide':
                try {
                    const { getCurrentWindow } = await import('@tauri-apps/api/window')
                    await getCurrentWindow().hide()
                } catch {
                    // Not in Tauri environment
                }
                return

            case 'app.about':
                showAboutWindow = true
                return

            // === View commands ===
            case 'view.showHidden':
                // Use Tauri command to toggle and sync menu checkbox state
                await toggleHiddenFiles()
                explorerRef?.refocus()
                return

            case 'view.briefMode':
                // Use Tauri command to set mode and sync menu radio state
                await setViewMode('brief')
                explorerRef?.refocus()
                return

            case 'view.fullMode':
                // Use Tauri command to set mode and sync menu radio state
                await setViewMode('full')
                explorerRef?.refocus()
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
                explorerRef?.refocus()
                return

            case 'pane.rightVolumeChooser':
                explorerRef?.toggleVolumeChooser('right')
                explorerRef?.refocus()
                return

            // === Navigation commands ===
            case 'nav.open':
                explorerRef?.sendKeyToFocusedPane('Enter')
                explorerRef?.refocus()
                return

            case 'nav.parent':
                explorerRef?.navigate('parent')
                explorerRef?.refocus()
                return

            case 'nav.back':
                explorerRef?.navigate('back')
                explorerRef?.refocus()
                return

            case 'nav.forward':
                explorerRef?.navigate('forward')
                explorerRef?.refocus()
                return

            case 'nav.home':
                explorerRef?.sendKeyToFocusedPane('Home')
                explorerRef?.refocus()
                return

            case 'nav.end':
                explorerRef?.sendKeyToFocusedPane('End')
                explorerRef?.refocus()
                return

            case 'nav.pageUp':
                explorerRef?.sendKeyToFocusedPane('PageUp')
                explorerRef?.refocus()
                return

            case 'nav.pageDown':
                explorerRef?.sendKeyToFocusedPane('PageDown')
                explorerRef?.refocus()
                return

            // === Sort commands ===
            case 'sort.byName':
                explorerRef?.setSortColumn('name')
                explorerRef?.refocus()
                return

            case 'sort.byExtension':
                explorerRef?.setSortColumn('extension')
                explorerRef?.refocus()
                return

            case 'sort.bySize':
                explorerRef?.setSortColumn('size')
                explorerRef?.refocus()
                return

            case 'sort.byModified':
                explorerRef?.setSortColumn('modified')
                explorerRef?.refocus()
                return

            case 'sort.byCreated':
                explorerRef?.setSortColumn('created')
                explorerRef?.refocus()
                return

            case 'sort.ascending':
                explorerRef?.setSortOrder('asc')
                explorerRef?.refocus()
                return

            case 'sort.descending':
                explorerRef?.setSortOrder('desc')
                explorerRef?.refocus()
                return

            case 'sort.toggleOrder':
                explorerRef?.setSortOrder('toggle')
                explorerRef?.refocus()
                return

            // === File action commands ===
            case 'file.rename':
                explorerRef?.startRename()
                explorerRef?.refocus()
                return

            case 'file.edit': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await openInEditor(entryUnderCursor.path)
                }
                explorerRef?.refocus()
                return
            }

            case 'file.showInFinder': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await showInFinder(entryUnderCursor.path)
                }
                explorerRef?.refocus()
                return
            }

            case 'file.copyPath': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await copyToClipboard(entryUnderCursor.path)
                }
                explorerRef?.refocus()
                return
            }

            case 'file.copyFilename': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await copyToClipboard(entryUnderCursor.filename)
                }
                explorerRef?.refocus()
                return
            }

            case 'file.quickLook': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await quickLook(entryUnderCursor.path)
                }
                explorerRef?.refocus()
                return
            }

            case 'file.getInfo': {
                const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
                if (entryUnderCursor) {
                    await getInfo(entryUnderCursor.path)
                }
                explorerRef?.refocus()
                return
            }

            // === About window commands ===
            case 'about.openWebsite':
                await openExternalUrl('https://getcmdr.com')
                explorerRef?.refocus()
                return

            case 'about.openUpgrade':
                await openExternalUrl('https://getcmdr.com/upgrade')
                explorerRef?.refocus()
                return

            case 'about.close':
                showAboutWindow = false
                explorerRef?.refocus()
                return

            default:
                // Unknown command - just refocus
                explorerRef?.refocus()
        }
    }
</script>

<div class="page-container">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="title-bar" onmousedown={handleTitleBarMouseDown}>
        <span class="title-text">{windowTitle}</span>
    </div>

    <div class="main-content">
        {#if showAboutWindow}
            <AboutWindow onClose={handleAboutClose} />
        {/if}

        {#if showLicenseKeyDialog}
            <LicenseKeyDialog onClose={handleLicenseKeyDialogClose} onSuccess={handleLicenseKeySuccess} />
        {/if}

        {#if showCommandPalette}
            <CommandPalette onExecute={handleCommandExecute} onClose={handleCommandPaletteClose} />
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
        {/if}
    </div>

    {#if showApp}
        <FunctionKeyBar
            visible={showFunctionKeyBar}
            onRename={handleFnRename}
            onView={handleFnView}
            onEdit={() => void handleFnEdit()}
            onCopy={handleFnCopy}
            onMove={handleFnMove}
            onNewFolder={handleFnNewFolder}
        />
    {/if}
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
        padding-top: 2px;
        background-color: var(--color-bg-secondary);
        flex-shrink: 0;
    }

    .title-text {
        font-size: var(--font-size-xs);
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
