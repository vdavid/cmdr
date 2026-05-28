<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/pane/DualPaneExplorer.svelte'
    import FunctionKeyBar from '$lib/file-explorer/pane/FunctionKeyBar.svelte'
    import OnboardingWizard from '$lib/onboarding/OnboardingWizard.svelte'
    import { openWizard as openOnboardingWizard } from '$lib/onboarding/onboarding-state.svelte'
    import { isForceOnboarding } from '$lib/tauri-commands'
    import ExpirationModal from '$lib/licensing/ExpirationModal.svelte'
    import CommercialReminderModal from '$lib/licensing/CommercialReminderModal.svelte'
    import AboutWindow from '$lib/licensing/AboutWindow.svelte'
    import LicenseKeyDialog from '$lib/licensing/LicenseKeyDialog.svelte'
    import CommandPalette from '$lib/command-palette/CommandPalette.svelte'
    import SearchDialog from '$lib/search/SearchDialog.svelte'
    import SelectionDialog from '$lib/selection-dialog/SelectionDialog.svelte'
    import type { FileEntry } from '$lib/file-explorer/types'
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
    import { notifyOnboardingComplete, setOnboardingShowing } from '$lib/updates/updater.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { getSetting, setSetting } from '$lib/settings'
    import { addToast } from '$lib/ui/toast'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import { startDownloadsEventBridge } from '$lib/downloads/event-bridge.svelte'
    import { startGlobalShortcutBridge } from '$lib/downloads/global-shortcut-bridge.svelte'
    import {
        handleCommandExecute as dispatchCommand,
        type CommandDispatchContext,
    } from './command-dispatch'
    import { setupMcpListeners } from './mcp-listeners'
    import { initQuickLookListeners } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
    import { initAppMode, getAppMode, type AppMode } from '$lib/app-mode'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
    import { updateLicenseCommandName } from '$lib/commands/command-registry'
    import type { FriendlyError } from '$lib/file-explorer/types'
    import type { ExplorerAPI } from './explorer-api'

    /**
     * Onboarding wizard visibility. The wizard owns the first-launch path: FDA, AI consent,
     * and the optional-settings step. Menu / palette re-entry opens the same wizard.
     * `CMDR_FORCE_ONBOARDING=1` overrides every gate and opens the wizard regardless of
     * persisted state.
     */
    let showOnboarding = $state(false)
    let showApp = $state(false)
    let showExpiredModal = $state(false)
    let expiredOrgName = $state<string | null>(null)
    let expiredAt = $state<string>('')
    let showCommercialReminder = $state(false)
    let showAboutWindow = $state(false)
    let showLicenseKeyDialog = $state(false)
    let showCommandPalette = $state(false)
    let showSearchDialog = $state(false)
    /**
     * Selection dialog state. `'add'` opens "Select files…", `'remove'` opens
     * "Deselect files…", `null` closes. The entries + cursor snapshot is captured
     * once when we flip from `null` to a non-null value.
     */
    let showSelectionDialog = $state<'add' | 'remove' | null>(null)
    let selectionDialogSnapshot = $state<{
        entries: FileEntry[]
        cursorIndex: number
        isSnapshotPane: boolean
    } | null>(null)
    let explorerRef: ExplorerAPI | undefined = $state()
    let windowTitle = $state('Cmdr')
    let appMode = $state<AppMode>(getAppMode())
    const showFunctionKeyBar = $state(true)
    /**
     * Volume id of the focused pane, mirrored from `DualPaneExplorer` via the
     * `onFocusedVolumeChange` callback. Drives the F-key bar's capability
     * flags so `search-results://`-pane actions render visibly disabled.
     */
    let focusedPaneVolumeId = $state<string>('root')
    const isFocusedPaneSearchResults = $derived(focusedPaneVolumeId === 'search-results')

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined
    let unlistenExecuteCommand: UnlistenFn | undefined
    let unlistenWindowFocus: UnlistenFn | undefined

    /** Opens the debug window (dev mode only) */
    async function openDebugWindow() {
        try {
            const { openDebugWindow: open } = await import('$lib/debug/debug-window')
            await open()
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

    // Unlisten functions for MCP and dialog listeners (cleaned up on destroy, important for HMR)
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
            await listenTauri('debug-trigger-transfer-error', (event) => {
                const { friendly } = event.payload as { friendly: FriendlyError }
                explorerRef?.triggerTransferError(friendly)
            })
        }
    }


    /** True if the user has selected text in the document (non-collapsed range). */
    function hasTextSelection(): boolean {
        const selection = window.getSelection()
        return !!selection && !selection.isCollapsed && selection.toString().length > 0
    }

    /**
     * True when focus is in a text-editing element. Used to let macOS's native
     * line-start/line-end behavior (⌘← / ⌘→) reach inputs even though those
     * combos are bound to "Copy path between panes" globally.
     */
    function isTextInputFocused(): boolean {
        const active = document.activeElement
        if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) return true
        return !!active?.closest('[contenteditable]')
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
            // Let the browser copy selected text natively (for example, from the error pane)
            // instead of triggering our file-copy command.
            if (shortcutString === '⌘C' && hasTextSelection()) {
                return
            }
            // Let macOS's native line-start / line-end (⌘← / ⌘→) reach text inputs
            // instead of triggering "Copy path between panes" from inside a rename
            // editor, the palette search, the search dialog, settings inputs, etc.
            if ((shortcutString === '⌘←' || shortcutString === '⌘→') && isTextInputFocused()) {
                return
            }
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

    /**
     * Reads `CMDR_FORCE_ONBOARDING`, settings, and the FDA probe, then flips the right
     * top-level state. Extracted from `onMount` to keep that function under the project's
     * complexity cap. See `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Mount + onboarding
     * flag" for the truth table this implements.
     */
    async function resolveOnboardingMount(): Promise<void> {
        const forceOnboarding = await isForceOnboarding().catch(() => false)
        const settings = await loadSettings()
        const hasFda = await checkFullDiskAccess()
        const ctx = {
            fullDiskAccessChoice: settings.fullDiskAccessChoice,
            isOnboarded: settings.isOnboarded,
            hasFda,
        }

        if (forceOnboarding) {
            openOnboardingWizard('force', ctx)
            showOnboarding = true
            setOnboardingShowing(true)
            showApp = true
            return
        }

        if (hasFda) {
            // Granted-now: mirror the setting if it diverged (covers OS-side toggles), then
            // either skip or mark onboarded based on the `isOnboarded` flag.
            if (settings.fullDiskAccessChoice !== 'allow') {
                await saveSettings({ fullDiskAccessChoice: 'allow' })
            }
            if (!settings.isOnboarded) {
                // Pre-wizard users who granted FDA before the wizard existed: unblock the
                // update toast by marking them onboarded.
                await notifyOnboardingComplete()
            }
            showApp = true
            maybeFireUpgradeNudge()
            return
        }

        if (settings.fullDiskAccessChoice === 'deny' && settings.isOnboarded) {
            // User explicitly denied and already finished onboarding. Don't re-prompt.
            showApp = true
            maybeFireUpgradeNudge()
            return
        }

        // Everything else routes through the wizard: first-launch (notAskedYet),
        // revoke-after-allow, first-time-stuck (Allow but didn't grant), or
        // Deny-but-not-onboarded.
        openOnboardingWizard('first-launch', ctx)
        showOnboarding = true
        setOnboardingShowing(true)
        showApp = true
    }

    /**
     * Fires the one-time `info` toast pointing existing users at the new
     * `Cmdr > Onboarding…` menu item (and the matching palette entry on Linux).
     * Persists `onboarding.upgradeNudgeShown: true` after firing so the toast
     * never appears again on this machine.
     *
     * Guarded against running when the wizard is up: this code only runs from
     * the `showApp = true` branches in `resolveOnboardingMount`, so the wizard
     * is closed by definition; no extra `onboardingShowing` check needed.
     *
     * Suppressed under E2E mode: the toast would leak into the first Playwright
     * test after every fresh-data-dir launch (each shard gets its own data dir),
     * tripping the fixture safety net. E2E mode isn't a real user and the
     * upgrade-discovery affordance doesn't matter there. The firing behaviour
     * is covered by Vitest unit tests instead.
     */
    function maybeFireUpgradeNudge(): void {
        if (getAppMode() === 'e2e') return
        if (getSetting('onboarding.upgradeNudgeShown')) return
        const message = isMacOS()
            ? "We've added new onboarding options. Open Cmdr > Onboarding… to review them."
            : "We've added new onboarding options. Open the command palette and run Onboarding… to review them."
        addToast(message, { level: 'info' })
        setSetting('onboarding.upgradeNudgeShown', true)
    }

    /**
     * Opens the onboarding wizard for re-entry from the menu item or the command
     * palette. Always opens at step 1 on macOS (step 2 on Linux) regardless of
     * `isOnboarded` — `openWizard()` itself enforces this when source is 'menu'.
     * No-op when the wizard is already open.
     */
    async function openOnboardingFromMenuOrPalette(source: 'menu' | 'palette'): Promise<void> {
        if (showOnboarding) return
        const settings = await loadSettings()
        const hasFda = await checkFullDiskAccess()
        const ctx = {
            fullDiskAccessChoice: settings.fullDiskAccessChoice,
            isOnboarded: settings.isOnboarded,
            hasFda,
        }
        openOnboardingWizard(source, ctx)
        showOnboarding = true
        setOnboardingShowing(true)
    }

    onMount(async () => {
        // Hide loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Resolve dev/E2E/prod mode before anything opens child windows so the
        // Settings and Viewer titles can be decorated at creation time.
        appMode = await initAppMode()

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

        // Hydrate localized macOS pane labels so the FDA onboarding modal
        // shows them in the same language the user sees in System Settings.
        await initSystemStrings()

        await resolveOnboardingMount()

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

        // Wait for Svelte to flush any pending DOM updates so DualPaneExplorer
        // (which renders when `showApp=true`) is in the DOM before we look for
        // it. Without this, the query below would miss the element on first
        // mount, and on remounts (e.g. navigating back from /settings) we'd
        // race the new render.
        await tick()

        // Mark the explorer as ready for E2E tests. This is a deterministic
        // signal that replaces the previous static 100 ms cushion in
        // `ensureAppReady`. By the time we set this, both the document-level
        // `keydown` listener and all MCP / dialog listeners are wired up, so
        // F-keys dispatched immediately after will reach their handlers.
        // The element is absent when showApp=false (FDA prompt path), but
        // E2E fixtures always grant FDA, so it will be there in tests.
        const explorer = document.querySelector('.dual-pane-explorer')
        if (explorer instanceof HTMLElement) {
            explorer.dataset.appReady = 'true'
        }
    })

    /**
     * Set up Tauri event listeners for menu actions, MCP events, etc.
     */
    async function setupTauriEventListeners() {
        await setupMenuListeners()
        await setupDialogListeners()
        await setupMcpListeners({
            getExplorer: () => explorerRef,
            listenTauri,
            openSearchDialog: () => {
                showSearchDialog = true
            },
            isAiEnabled: () => getSetting('ai.provider') !== 'off',
        })
        await initIndexState()
        await setupWindowFocusListener()
        // Native Quick Look (macOS) event wiring: `quick-look-closed` flips
        // `isOpen` on the state singleton; `quick-look-key` routes panel
        // keystrokes back into the focused pane (and intercepts Shift+Space
        // to close).
        const unlistenQuickLook = await initQuickLookListeners(() => explorerRef)
        tauriUnlistenFns.push(unlistenQuickLook)
        // Downloads notifications event bridge: one `download-detected`
        // listener that fans out to the in-app toast and/or the macOS
        // native notification per the current settings value.
        const unlistenDownloads = await startDownloadsEventBridge(explorerRef)
        tauriUnlistenFns.push(unlistenDownloads)
        // Global reveal-latest-download hotkey bridge (default ⌃⌥⌘J): one
        // `global-shortcut-fired` listener; routes through `revealLatestDownload`
        // and shows the first-trigger warn toast when `acknowledged === false`.
        const unlistenGlobalShortcut = await startGlobalShortcutBridge(explorerRef)
        tauriUnlistenFns.push(unlistenGlobalShortcut)
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

    /**
     * Wizard finished. Reached when the user clicks "Finish" on the last step (per-step
     * persistence is wired inside each `Step*.svelte`). Persists `isOnboarded: true` via
     * `notifyOnboardingComplete()` so the deferred update toast can fire and we stop
     * re-opening the wizard on next launch.
     */
    function handleWizardComplete() {
        showOnboarding = false
        setOnboardingShowing(false)
        void notifyOnboardingComplete()
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

    /**
     * Opens or closes the Selection dialog. On open, snapshot the focused pane's
     * entries + cursor so the dialog has a stable list to match against per the
     * plan's G15 contract.
     */
    async function setSelectionDialog(mode: 'add' | 'remove' | null): Promise<void> {
        if (mode === null) {
            showSelectionDialog = null
            selectionDialogSnapshot = null
            return
        }
        if (showSelectionDialog === mode) return // Already open.
        if (!explorerRef) return
        const snap = await explorerRef.getFocusedPaneEntries()
        selectionDialogSnapshot = snap
        showSelectionDialog = mode
    }

    function handleSelectionDialogClose() {
        showSelectionDialog = null
        selectionDialogSnapshot = null
        // Return focus to the pane so subsequent keystrokes land there.
        void Promise.resolve().then(() => {
            explorerRef?.refocus()
        })
    }

    function handleSelectionCommit(indices: number[], mode: 'add' | 'remove') {
        explorerRef?.applyIndicesToFocusedPane(indices, mode)
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

    /**
     * "Open in pane" handler from SearchDialog (M8b). The dialog has already stored
     * the snapshot and pinned the "last attempt" ref; we route the focused pane to
     * the search-results virtual volume. `openSearchSnapshotInPane` flows through
     * the standard `handleVolumeChange` so new-tab-on-pinned, focus, and history
     * push all apply uniformly — and `pushHistoryEntry` increments the snapshot
     * refcount via the M8a integration.
     */
    function handleOpenSearchInPane(snapshotId: string) {
        const pane = explorerRef?.getFocusedPane() ?? 'left'
        explorerRef?.openSearchSnapshotInPane(snapshotId, pane)
        // R3 U8: pull keyboard focus back to the pane so the user can
        // immediately navigate / select / press F4 etc. without an extra
        // click. The dialog closes itself, but the OS's focus-restore lands
        // on the previously-focused element (often the dialog overlay
        // wrapper), not the pane. Calling `refocus()` puts the cursor where
        // the user expects it: on the pane that just got the snapshot.
        void Promise.resolve().then(() => {
            explorerRef?.refocus()
        })
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

    /** Command dispatch context: wires reactive state to the extracted dispatch function */
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
            showSelectionDialog: (mode: 'add' | 'remove' | null) => {
                void setSelectionDialog(mode)
            },
            openOnboarding: () => {
                void openOnboardingFromMenuOrPalette('menu')
            },
        },
    }

    async function handleCommandExecute(commandId: string) {
        await dispatchCommand(commandId, commandDispatchCtx)
    }
</script>

<div class="page-container">
    {#if isMacOS()}
        <header
            class="title-bar"
            class:dev-mode={appMode === 'dev'}
            class:e2e-mode={appMode === 'e2e'}
            data-tauri-drag-region
        >
            <!-- Mark the text span as a drag region too. The header above
                 has `data-tauri-drag-region`, but Tauri's drag detection
                 looks for the attribute on the element under the cursor
                 (mousedown target) — without it on the span, mousedowns on
                 the title text don't initiate a window drag. -->
            <span class="title-text" data-tauri-drag-region>
                {#if appMode === 'dev'}DEV MODE - {windowTitle} - DEV MODE{:else if appMode === 'e2e'}E2E MODE - {windowTitle} - E2E MODE{:else}{windowTitle}{/if}
            </span>
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
                searchableFolder={explorerRef?.getFocusedPaneSearchableFolder() ?? {
                    path: explorerRef?.getFocusedPanePath() ?? '/',
                    disabled: false,
                    disabledReason: '',
                }}
                onShowAllInMainWindow={handleOpenSearchInPane}
            />
        {/if}

        {#if showSelectionDialog && selectionDialogSnapshot}
            <SelectionDialog
                mode={showSelectionDialog}
                entries={selectionDialogSnapshot.entries}
                cursorIndex={selectionDialogSnapshot.cursorIndex}
                isSnapshotPane={selectionDialogSnapshot.isSnapshotPane}
                onCommit={handleSelectionCommit}
                onClose={handleSelectionDialogClose}
            />
        {/if}

        {#if showExpiredModal}
            <ExpirationModal organizationName={expiredOrgName} {expiredAt} onClose={handleExpirationModalClose} />
        {/if}

        {#if showCommercialReminder}
            <CommercialReminderModal onClose={handleCommercialReminderClose} />
        {/if}

        {#if showOnboarding}
            <OnboardingWizard onComplete={handleWizardComplete} />
        {/if}

        {#if showApp}
            <DualPaneExplorer
                bind:this={explorerRef}
                onFocusedVolumeChange={(vid: string) => {
                    focusedPaneVolumeId = vid
                }}
                onCommand={(commandId: string) => {
                    void handleCommandExecute(commandId)
                }}
            />
            <ScanStatusOverlay />
            <ReplayStatusOverlay />
        {/if}

        {#if showApp}
            <FunctionKeyBar
                visible={showFunctionKeyBar}
                canMkdir={!isFocusedPaneSearchResults}
                canMkfile={!isFocusedPaneSearchResults}
                canRename={!isFocusedPaneSearchResults}
                canPasteInto={!isFocusedPaneSearchResults}
                canSourceOps={true}
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

    /* Dev/E2E mode title-bar tint at 25 % alpha — strong enough to read
       clearly as DEV / E2E, light enough to leave the underlying title
       bar visible. */
    /*noinspection CssUnusedSymbol*/
    .title-bar.dev-mode::after {
        content: '';
        position: absolute;
        inset: 0;
        background-color: color-mix(in srgb, hotpink, transparent 75%);
        pointer-events: none;
    }

    /*noinspection CssUnusedSymbol*/
    .title-bar.e2e-mode::after {
        content: '';
        position: absolute;
        inset: 0;
        background-color: color-mix(in srgb, dodgerblue, transparent 75%);
        pointer-events: none;
    }

    .title-text {
        /* Capped at 1.5× base (12 → 18px) so the title text never outgrows the
         * fixed-height title bar. The compounded scale (system × user) can
         * push `--font-size-sm` higher than this; `min()` keeps the rendered
         * size within bounds while still scaling down at small text sizes. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- absolute cap, not a token-eligible value */
        font-size: min(var(--font-size-sm), 18px);
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
