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
        openInEditor,
        setMenuContext,
        getWindowTitle,
        registerKnownDialogs,
    } from '$lib/tauri-commands'
    import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
    import { loadSettings, saveSettings } from '$lib/settings-store'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import {
        handleCommandExecute as dispatchCommand,
        type CommandDispatchContext,
    } from './command-dispatch'
    import { setupMcpListeners } from './mcp-listeners'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
    import { updateLicenseCommandName } from '$lib/commands/command-registry'
    import type { FriendlyError } from '$lib/file-explorer/types'
    import type { ExplorerAPI } from './explorer-api'

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

        // Debug error injection (dev mode only)
        if (import.meta.env.DEV) {
            await listenTauri('debug-inject-error', (event) => {
                const { pane, friendly } = event.payload as { pane: 'left' | 'right'; friendly: FriendlyError }
                explorerRef?.injectError(pane, friendly)
            })
            await listenTauri('debug-reset-error', (event) => {
                const { pane } = event.payload as { pane: 'left' | 'right' | 'both' }
                explorerRef?.resetError(pane)
            })
        }
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
        await setupMcpListeners({ getExplorer: () => explorerRef, listenTauri })
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

    /** Command dispatch context — wires reactive state to the extracted dispatch function */
    const commandDispatchCtx: CommandDispatchContext = {
        getExplorer: () => explorerRef,
        dialogs: {
            showCommandPalette: (show: boolean) => {
                showCommandPalette = show
            },
            showSearchDialog: (show: boolean) => {
                if (show && showSearchDialog) return // Already open
                showSearchDialog = show
            },
            showAboutWindow: (show: boolean) => {
                showAboutWindow = show
            },
            showLicenseKeyDialog: (show: boolean) => {
                showLicenseKeyDialog = show
            },
        },
    }

    async function handleCommandExecute(commandId: string) {
        await dispatchCommand(commandId, commandDispatchCtx)
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
