<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import DualPaneExplorer from '$lib/file-explorer/pane/DualPaneExplorer.svelte'
    import FunctionKeyBar from '$lib/file-explorer/pane/FunctionKeyBar.svelte'
    import OnboardingWizard from '$lib/onboarding/OnboardingWizard.svelte'
    import ExpirationModal from '$lib/licensing/ExpirationModal.svelte'
    import CommercialReminderModal from '$lib/licensing/CommercialReminderModal.svelte'
    import AboutWindow from '$lib/licensing/AboutWindow.svelte'
    import LicenseKeyDialog from '$lib/licensing/LicenseKeyDialog.svelte'
    import CommandPalette from '$lib/command-palette/CommandPalette.svelte'
    import SearchDialog from '$lib/search/SearchDialog.svelte'
    import SelectionDialog from '$lib/selection-dialog/SelectionDialog.svelte'
    import GoToPathDialog from '$lib/go-to-path/GoToPathDialog.svelte'
    import WhatsNewDialog from '$lib/whats-new/WhatsNewDialog.svelte'
    import AcknowledgementsDialog from '$lib/licensing/AcknowledgementsDialog.svelte'
    import {
        acknowledgementsState,
        closeAcknowledgements,
    } from '$lib/licensing/acknowledgements-trigger.svelte'
    import { whatsNewState } from '$lib/whats-new/whats-new-trigger.svelte'
    import OperationLogDialog from '$lib/operation-log/OperationLogDialog.svelte'
    import { operationLogState } from '$lib/operation-log/operation-log-trigger.svelte'
    import AskCmdrRail from '$lib/ask-cmdr/AskCmdrRail.svelte'
    import BulkRenameReviewDialog from '$lib/ask-cmdr/BulkRenameReviewDialog.svelte'
    import { askCmdrState } from '$lib/ask-cmdr/ask-cmdr-trigger.svelte'
    import { goToPath } from '$lib/go-to-path/go-to-path'
    import {
        getFocusedPanePath,
        getFocusedPaneSearchScope,
        getFocusedPaneSearchTargetVolume,
    } from '$lib/file-explorer/pane/focused-pane-reads'
    import type { FileEntry } from '$lib/file-explorer/types'
    import IndexingStatusIndicator from '$lib/indexing/IndexingStatusIndicator.svelte'
    import StaleDriveDialog from '$lib/indexing/StaleDriveDialog.svelte'
    import { initPathLimits } from '$lib/utils/filename-validation'
    import {
        initIndexState,
        destroyIndexState,
        initMediaEnrichState,
        destroyMediaEnrichState,
    } from '$lib/indexing/index'
    import { initShortcutDispatch, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'
    import { markDispatchSource } from './dispatch-dedup'
    import { navCommandForMouseButton } from './mouse-nav'
    import { resolveGlobalKeyAction } from './global-keydown'
    import { resolveGlobalContextMenuAction } from './global-contextmenu'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { type UnlistenFn, getWindowTitle, registerKnownDialogs } from '$lib/tauri-commands'
    import { showMainOnMount } from './show-main-on-mount'
    import {
        type StartupGatesContext,
        maybeRunWhatsNew,
        openOnboardingFromMenuOrPalette,
        resolveOnboardingMount,
    } from './startup-gates'
    import {
        type ListenerSetupContext,
        makeListenTauri,
        setupMenuListeners,
        setupDialogListeners,
        setupWindowFocusListener,
    } from './listener-setup'
    import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
    import { isGalleryDialogOpen } from '$lib/dialog-gallery/gallery-state.svelte'
    import { notifyOnboardingComplete, setOnboardingShowing } from '$lib/updates/updater.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import { getSetting } from '$lib/settings'
    import { getShowFunctionKeyBar } from '$lib/settings/reactive-settings.svelte'
    import { startDownloadsEventBridge } from '$lib/downloads/event-bridge.svelte'
    import { startGlobalShortcutBridge } from '$lib/downloads/global-shortcut-bridge.svelte'
    import { startLowDiskSpaceEventBridge } from '$lib/low-disk-space/event-bridge.svelte'
    import { startDragOutEventBridge } from '$lib/file-explorer/drag/drag-out-event-bridge'
    import { revealSearchResultInPane } from '$lib/file-explorer/navigation/navigate-and-select'
    import {
        handleCommandExecute as dispatchCommand,
        type CommandDispatchContext,
    } from './command-dispatch'
    import { type CommandId, type CommandDispatchArgs } from '$lib/commands'
    import { setupMcpListeners } from './mcp-listeners'
    import { initQuickLookListeners } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
    import { initAppMode, getAppMode, decorateMainWindowTitle, type AppMode } from '$lib/app-mode'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
    import { updateLicenseCommandName } from '$lib/commands/command-registry'
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
    let showGoToPathDialog = $state(false)
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
    const showFunctionKeyBar = $derived(getShowFunctionKeyBar())

    // Event handlers stored for cleanup
    let handleKeyDown: ((e: KeyboardEvent) => void) | undefined
    let handleContextMenu: ((e: MouseEvent) => void) | undefined
    let handleMouseDown: ((e: MouseEvent) => void) | undefined
    let handleMouseUp: ((e: MouseEvent) => void) | undefined

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

    // Unlisten functions for menu, MCP, and dialog listeners (cleaned up on
    // destroy, important for HMR). Shared with the extracted `listener-setup.ts`
    // helpers and with `setupMcpListeners` so every registered listener tears
    // down through one array.
    const tauriUnlistenFns: UnlistenFn[] = []

    /** `listenTauri` bound to the shared cleanup array; passed to `setupMcpListeners`. */
    const listenTauri = makeListenTauri(tauriUnlistenFns)

    /**
     * Explorer-owned overlays that should suppress centralized dispatch: a
     * confirmation dialog, an active inline rename, OR the volume switcher
     * dropdown (it hosts the inline favorite-rename input + a focusable list, so
     * while it's open pane/global shortcuts must not fire — text-editing keys
     * reach the textbox instead — Fix E).
     */
    function isExplorerOverlayOpen(): boolean {
        if (!explorerRef) return false
        return explorerRef.isConfirmationDialogOpen() || explorerRef.isRenaming() || explorerRef.isVolumeChooserOpen()
    }

    /** Check if any modal dialog is open that should suppress centralized dispatch. */
    function isModalDialogOpen(): boolean {
        return (
            showCommandPalette ||
            showSearchDialog ||
            showGoToPathDialog ||
            showAboutWindow ||
            showLicenseKeyDialog ||
            showExpiredModal ||
            showCommercialReminder ||
            whatsNewState.open ||
            acknowledgementsState.open ||
            operationLogState.open ||
            isGalleryDialogOpen() || // Dev-only in practice; ❌ don't add a build-time DEV guard (breaks knip, see dialog-gallery/DETAILS.md)
            isExplorerOverlayOpen()
        )
    }

    /**
     * Global keyboard handler for app-level shortcuts. The decision is
     * `global-keydown.ts`'s (pure, unit-tested); this only runs the side effects.
     */
    function handleGlobalKeyDown(e: KeyboardEvent): void {
        const action = resolveGlobalKeyAction(e, isModalDialogOpen())
        switch (action.kind) {
            case 'dispatch':
                e.preventDefault()
                e.stopPropagation()
                // Tag the source so the dispatch core can swallow the spurious
                // second half of a macOS keyboard+menu double-fire (dispatch-dedup.ts).
                markDispatchSource('keyboard')
                void handleCommandExecute(action.commandId)
                break
            case 'openDebugWindow':
                e.preventDefault()
                void openDebugWindow()
                break
            case 'suppress':
                e.preventDefault()
                break
            case 'ignore':
                break
        }
    }

    /**
     * Global right-click handler. The decision is `global-contextmenu.ts`'s (pure,
     * unit-tested); this only runs the side effects. Registered in the CAPTURE
     * phase so a text field can claim the click before the row / tab / chip
     * handler underneath it opens a Cmdr menu.
     */
    function handleGlobalContextMenu(e: MouseEvent): void {
        if (resolveGlobalContextMenuAction(e) === 'native-text-menu') {
            e.stopPropagation()
            return
        }
        e.preventDefault()
    }

    /**
     * Suppresses WKWebView's built-in page back / forward on a mouse's X1/X2 side
     * buttons. We drive pane history from `mouseup` instead (`handleGlobalMouseUp`);
     * without this, the webview would also walk the SPA's own history. Suppressing on
     * `mousedown` is what cancels the default nav, so it can't move alongside the
     * `mouseup` dispatch.
     */
    function handleGlobalMouseDown(e: MouseEvent): void {
        if (navCommandForMouseButton(e.button)) {
            e.preventDefault()
        }
    }

    /**
     * Global handler for a mouse's dedicated back / forward side buttons (issue #31):
     * dispatch `nav.back` / `nav.forward` through the same command bus as the `⌘[` /
     * `⌘]` shortcuts. Left untagged for the cross-source dedup (a mouse button has no
     * native-menu twin to double-fire), and gated by the same modal-open guard as the
     * keyboard path so the buttons stay inert while a dialog or overlay is up.
     */
    function handleGlobalMouseUp(e: MouseEvent): void {
        if (isModalDialogOpen()) return
        const commandId = navCommandForMouseButton(e.button)
        if (!commandId) return
        e.preventDefault()
        e.stopPropagation()
        void handleCommandExecute(commandId)
    }

    /**
     * Flips the wizard's visibility. The updater reads its own mirror of the flag
     * (to hold the update toast back while onboarding is up), so the two always
     * move together; every wizard open and close goes through here.
     */
    function setOnboardingVisible(visible: boolean): void {
        showOnboarding = visible
        setOnboardingShowing(visible)
    }

    /**
     * Context for the extracted `startup-gates.ts` decisions (what the user sees on
     * launch): setters for the `$state` they flip, live getters for the flags they
     * read. See that module for the onboarding truth table.
     */
    const startupGatesCtx: StartupGatesContext = {
        setOnboardingVisible,
        isOnboardingVisible: () => showOnboarding,
        showApp: () => {
            showApp = true
        },
        isOtherStartupModalOpen: () => showExpiredModal || showCommercialReminder,
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

        await resolveOnboardingMount(startupGatesCtx)

        // Automatic "What's new" post-update check. Runs after onboarding resolves so it can
        // see whether the wizard is up; if it is (or another startup modal), the check waits
        // and re-attempts on `handleWizardComplete`.
        void maybeRunWhatsNew(startupGatesCtx)

        // Show the window once the webview has actually painted (avoids a blank-window race); fire-and-forget.
        void showMainOnMount()

        // Initialize centralized shortcut dispatch and global keyboard/context menu
        // handlers. These must be registered BEFORE setupTauriEventListeners() because
        // that call may throw in non-Tauri environments (e.g. Playwright smoke tests).
        initShortcutDispatch()

        handleKeyDown = handleGlobalKeyDown
        handleContextMenu = handleGlobalContextMenu
        handleMouseDown = handleGlobalMouseDown
        handleMouseUp = handleGlobalMouseUp
        document.addEventListener('keydown', handleKeyDown)
        document.addEventListener('contextmenu', handleContextMenu, true)
        document.addEventListener('mousedown', handleMouseDown)
        document.addEventListener('mouseup', handleMouseUp)

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
        await setupMenuListeners(listenerSetupCtx)
        await setupDialogListeners(listenerSetupCtx)
        await setupMcpListeners({
            getExplorer: () => explorerRef,
            // The MCP adapter dispatches through the same typed command bus as the
            // keyboard / palette / menu paths. `handleCommandExecute` already binds
            // the shared dispatch context, so MCP events get the uniform preamble
            // (log + breadcrumb + search-results guard).
            dispatch: handleCommandExecute,
            listenTauri,
            isAiEnabled: () => getSetting('ai.provider') !== 'off',
        })
        await initIndexState()
        // Image-enrichment progress joins the same top-right indicator, a
        // second publisher; listen-first-then-query, like initIndexState.
        await initMediaEnrichState()
        await setupWindowFocusListener(listenerSetupCtx)
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
        // Global go-to-latest-download hotkey bridge (default ⌃⌥⌘J): one
        // `global-shortcut-fired` listener; routes through `goToLatestDownload`
        // and shows the first-trigger warn toast when `acknowledged === false`.
        const unlistenGlobalShortcut = await startGlobalShortcutBridge(explorerRef)
        tauriUnlistenFns.push(unlistenGlobalShortcut)
        // Low-disk-space warning bridge: one `low-disk-space` listener (the
        // backend poller's boot-volume hysteresis detector) dispatched to a
        // persistent warn toast or a macOS notification per the settings value.
        const unlistenLowDiskSpace = await startLowDiskSpaceEventBridge()
        tauriUnlistenFns.push(unlistenLowDiskSpace)
        // Drag-out completion bridge: one `drag-out-session-started` +
        // `drag-out-session-complete` pair per drag session, turned into a single
        // signs-of-life → completion toast (downloading a phone/NAS file to
        // Finder shows nothing on Finder's side; this is our feedback surface).
        const unlistenDragOut = await startDragOutEventBridge()
        tauriUnlistenFns.push(unlistenDragOut)
    }

    onDestroy(() => {
        destroyShortcutDispatch()
        destroyIndexState()
        destroyMediaEnrichState()
        if (handleKeyDown) {
            document.removeEventListener('keydown', handleKeyDown)
        }
        if (handleContextMenu) {
            document.removeEventListener('contextmenu', handleContextMenu, true)
        }
        if (handleMouseDown) {
            document.removeEventListener('mousedown', handleMouseDown)
        }
        if (handleMouseUp) {
            document.removeEventListener('mouseup', handleMouseUp)
        }
        // Clean up every menu / MCP / dialog / window-focus listener (prevents
        // duplicate listeners after HMR). All of them register into this one array.
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
        setOnboardingVisible(false)
        void notifyOnboardingComplete()
        // Re-attempt the "What's new" check now that onboarding is closed: a popup that
        // `wait`ed on the wizard can show on this pass (matches the update-toast re-attempt).
        void maybeRunWhatsNew(startupGatesCtx)
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

    function handleGoToPathDialogClose() {
        showGoToPathDialog = false
        explorerRef?.refocus()
    }

    /** Resolve + jump for the Go-to-path dialog, in the focused pane. */
    function handleGoToPath(input: string) {
        return goToPath(explorerRef, input)
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
        // Reveal the result's file in the focused pane: the shared edge helper
        // resolves the parent dir's volume (the index isn't scoped to the pane's
        // volume), navigates there, then moves the cursor onto the file.
        const explorer = explorerRef
        if (!explorer) return
        void revealSearchResultInPane(explorer, explorer.getFocusedPane(), path)
    }

    /**
     * "Open in pane" handler from SearchDialog (M8b). The dialog has already stored
     * the snapshot and pinned the "last attempt" ref; we route the focused pane to
     * the search-results virtual volume. `openSearchSnapshotInPane` flows through
     * `navigate({ to: { snapshot } })` so new-tab-on-pinned, focus, and history
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
            showGoToPathDialog: (show: boolean) => {
                // Idempotency guard: ⌘G's menu accelerator + JS keydown both
                // fire on macOS (see the plan's "Menu double-dispatch"). With
                // this guard, a double-fire opens the dialog exactly once.
                if (show && showGoToPathDialog) return // Already open
                showGoToPathDialog = show
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
            openOnboarding: () => openOnboardingFromMenuOrPalette(startupGatesCtx, 'menu'),
        },
    }

    async function handleCommandExecute<K extends CommandId>(
        commandId: K,
        ...args: CommandDispatchArgs<K>
    ): Promise<void> {
        await dispatchCommand(commandId, commandDispatchCtx, ...args)
    }

    /**
     * Context for the extracted `listener-setup.ts` helpers: live getters for
     * reads, setter callbacks for writes, the shared cleanup array, and the
     * dispatch + whats-new callbacks that stay component-owned (they touch
     * reactive `$state`).
     */
    const listenerSetupCtx: ListenerSetupContext = {
        getExplorer: () => explorerRef,
        dispatch: handleCommandExecute,
        unlistenFns: tauriUnlistenFns,
        dialogs: {
            setAboutWindow: (show: boolean) => {
                showAboutWindow = show
            },
        },
        maybeRunWhatsNew: (force: boolean) => maybeRunWhatsNew(startupGatesCtx, force),
    }
</script>

<div class="page-container">
    {#if isMacOS()}
        <header
            class="title-bar"
            class:dev-mode={appMode === 'dev'}
            class:e2e-mode={appMode === 'e2e'}
            class:capture-mode={appMode === 'capture'}
            data-tauri-drag-region
        >
            <!-- Mark the text span as a drag region too. The header above
                 has `data-tauri-drag-region`, but Tauri's drag detection
                 looks for the attribute on the element under the cursor
                 (mousedown target) — without it on the span, mousedowns on
                 the title text don't initiate a window drag. -->
            <span class="title-text" data-tauri-drag-region>
                <!-- eslint-disable-next-line cmdr/no-raw-user-facing-string -- dev/E2E-only title-bar markers (incl. the worktree label), not shipped user copy; they only render under non-default app modes. -->
                {decorateMainWindowTitle(windowTitle, appMode)}
            </span>
        </header>
    {/if}

    <main class="main-content">
        <!-- eslint-disable-next-line cmdr/no-raw-user-facing-string -- "Cmdr" is the brand name, never translated (style guide); a screen-reader-only app heading. -->
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
                scopePresets={getFocusedPaneSearchScope()}
                searchVolume={getFocusedPaneSearchTargetVolume()}
                onShowAllInMainWindow={handleOpenSearchInPane}
                onReopen={() => {
                    showSearchDialog = true
                }}
                onGrantFullDiskAccess={() => {
                    void openOnboardingFromMenuOrPalette(startupGatesCtx, 'palette')
                }}
            />
        {/if}

        {#if showGoToPathDialog}
            <GoToPathDialog
                baseDir={getFocusedPanePath()}
                onGo={handleGoToPath}
                onCancel={handleGoToPathDialogClose}
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

        {#if whatsNewState.open}
            <WhatsNewDialog />
        {/if}

        {#if acknowledgementsState.open}
            <AcknowledgementsDialog onClose={closeAcknowledgements} />
        {/if}

        {#if operationLogState.open}
            <OperationLogDialog />
        {/if}

        {#if showOnboarding}
            <OnboardingWizard onComplete={handleWizardComplete} />
        {/if}

        {#if showApp}
            <div class="explorer-rail-row">
                <DualPaneExplorer bind:this={explorerRef} onCommand={handleCommandExecute} />
                {#if askCmdrState.open}
                    <AskCmdrRail />
                {/if}
            </div>
            {#if askCmdrState.renameReview}
                <BulkRenameReviewDialog />
            {/if}
            <IndexingStatusIndicator />
            <StaleDriveDialog />
        {/if}

        {#if showApp}
            <FunctionKeyBar
                visible={showFunctionKeyBar}
                onCommand={handleCommandExecute}
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
        height: var(--titlebar-height);
        display: flex;
        align-items: center;
        justify-content: center;
        padding-top: var(--spacing-xxs);
        background-color: var(--color-bg-secondary);
        flex-shrink: 0;
        position: relative;
    }

    /* Dev/E2E/capture mode title-bar tint at 25 % alpha — strong enough to read
       clearly as DEV / E2E / SCREENSHOT, light enough to leave the underlying
       title bar visible. */
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

    /* Screenshot-capture runs: yellow, so a glance says "a capture is in flight,
       leave the machine alone" rather than "just another E2E run". `goldenrod`
       over plain `yellow`: at 25 % alpha a pure yellow washes out against the
       title bar and fights the white title text, while goldenrod stays clearly
       yellow AND keeps the text readable. */
    /*noinspection CssUnusedSymbol*/
    .title-bar.capture-mode::after {
        content: '';
        position: absolute;
        inset: 0;
        background-color: color-mix(in srgb, goldenrod, transparent 75%);
        pointer-events: none;
    }

    .title-text {
        /* Capped at 1.5× base (12 → 18px) so the title text never outgrows the
         * fixed-height title bar. The compounded scale (system × user) can
         * push `--font-size-sm` higher than this; `min()` keeps the rendered
         * size within bounds while still scaling down at small text sizes. */
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

    /* Panes + the Ask Cmdr rail sit in a row; the panes take the remainder, the rail its
       fixed width. Opening the rail grows the whole window so the panes keep their size
       (see ask-cmdr/rail-window.ts); the panes only give up space when the window is capped
       at the screen width. Below ~900px the rail overlays the right pane (see AskCmdrRail). */
    .explorer-rail-row {
        display: flex;
        flex: 1;
        min-height: 0;
        min-width: 0;
        position: relative;
    }

    .explorer-rail-row > :global(.dual-pane-explorer) {
        flex: 1;
        width: auto;
        min-width: 0;
    }
</style>
