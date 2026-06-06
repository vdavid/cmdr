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
    import GoToPathDialog from '$lib/go-to-path/GoToPathDialog.svelte'
    import { goToPath } from '$lib/go-to-path/go-to-path'
    import { getFocusedPanePath, getFocusedPaneSearchableFolder } from '$lib/file-explorer/pane/focused-pane-reads'
    import type { FileEntry } from '$lib/file-explorer/types'
    import IndexingStatusIndicator from '$lib/indexing/IndexingStatusIndicator.svelte'
    import { initPathLimits } from '$lib/utils/filename-validation'
    import { initIndexState, destroyIndexState } from '$lib/indexing/index'
    import { initShortcutDispatch, destroyShortcutDispatch, lookupCommand } from '$lib/shortcuts/shortcut-dispatch'
    import { formatKeyCombo, isMacOS } from '$lib/shortcuts/key-capture'
    import {
        showMainWindow,
        checkFullDiskAccess,
        listen,
        type UnlistenFn,
        setMenuContext,
        getWindowTitle,
        registerKnownDialogs,
    } from '$lib/tauri-commands'
    import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
    import { loadSettings, saveSettings } from '$lib/settings-store'
    import { getAppLogger } from '$lib/logging/logger'
    import { notifyOnboardingComplete, setOnboardingShowing } from '$lib/updates/updater.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { getSetting, setSetting } from '$lib/settings'
    import { addToast } from '$lib/ui/toast'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import { startDownloadsEventBridge } from '$lib/downloads/event-bridge.svelte'
    import { startGlobalShortcutBridge } from '$lib/downloads/global-shortcut-bridge.svelte'
    import { startDragOutEventBridge } from '$lib/file-explorer/drag/drag-out-event-bridge'
    import {
        handleCommandExecute as dispatchCommand,
        type CommandDispatchContext,
    } from './command-dispatch'
    import { isCommandId, type CommandId, type CommandDispatchArgs } from '$lib/commands'
    import type { ViewMode } from '$lib/app-status-store'
    import { setupMcpListeners } from './mcp-listeners'
    import { initQuickLookListeners } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
    import { initAppMode, getAppMode, type AppMode } from '$lib/app-mode'
    import {
        hideExpirationModal,
        loadLicenseStatus,
        triggerValidationIfNeeded,
    } from '$lib/licensing/licensing-store.svelte'
    import { updateLicenseCommandName } from '$lib/commands/command-registry'
    import type { FriendlyError, TransferOperationType } from '$lib/file-explorer/types'
    import type { ExplorerAPI } from './explorer-api'

    const log = getAppLogger('main-page')

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
    const showFunctionKeyBar = $state(true)

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
            // The Rust menu emit (`menu_id_to_command`) and cross-window emits send a bare
            // string across IPC; the `CommandId` union can't reach over that boundary. Narrow
            // at the edge so a stale Rust id is dropped here rather than no-oping in the switch
            // `default`. The Rust↔registry drift test pins the two id sets together.
            if (isCommandId(commandId)) {
                void handleCommandExecute(commandId)
            }
        })

        // Per-pane view change from a native-menu click. Rust emits this directly
        // (not via `execute-command`) because the CheckMenuItem already toggled
        // its own state; the dispatch maps it onto the `view.setMode` command. The
        // payload is validated rather than `as`-cast: an unknown `mode` is dropped,
        // and an absent/unknown `pane` falls back to the focused pane (matching the
        // old in-component listener's `event.payload.pane ?? focusedPane`).
        await listenTauri('view-mode-changed', (event) => {
            const raw = (event.payload ?? {}) as { mode?: unknown; pane?: unknown }
            const mode: ViewMode | undefined =
                raw.mode === 'full' || raw.mode === 'brief' ? raw.mode : undefined
            if (!mode) return
            const pane: 'left' | 'right' =
                raw.pane === 'left' || raw.pane === 'right' ? raw.pane : (explorerRef?.getFocusedPane() ?? 'left')
            // `viewSetModeCommand` is a typed const (not an inline literal) so a
            // registry rename breaks compilation and `cmdr/no-raw-command-dispatch`
            // stays satisfied (A3). `fromMenu: true` → the handler skips
            // `pushViewMenuState` (the menu already toggled its CheckMenuItem).
            void handleCommandExecute(viewSetModeCommand, { pane, mode, fromMenu: true })
        })

        // Native sort-menu clicks. Rust emits this directly (not via
        // `execute-command`) with `{ action, value }`; the dispatch maps each
        // value onto the focused-pane `sort.*` command. Validated, not `as`-cast:
        // an unknown `action`/`value` pair is dropped.
        await listenTauri('menu-sort', (event) => {
            const raw = (event.payload ?? {}) as { action?: unknown; value?: unknown }
            const command = menuSortToCommand(raw.action, raw.value)
            if (command) void handleCommandExecute(command)
        })
    }

    /** Typed id for the per-pane view command (keeps dispatch off raw literals; A3). */
    const viewSetModeCommand: CommandId = 'view.setMode'

    /**
     * Maps a native `menu-sort` payload onto a focused-pane `sort.*` command id,
     * or `undefined` for an unrecognized payload. `sortBy` selects the column;
     * `sortOrder` selects ascending/descending (the menu never emits `toggle`).
     */
    function menuSortToCommand(action: unknown, value: unknown): CommandId | undefined {
        if (action === 'sortBy') {
            const byColumn: Record<string, CommandId> = {
                name: 'sort.byName',
                extension: 'sort.byExtension',
                size: 'sort.bySize',
                modified: 'sort.byModified',
                created: 'sort.byCreated',
            }
            return typeof value === 'string' ? byColumn[value] : undefined
        }
        if (action === 'sortOrder') {
            const byOrder: Record<string, CommandId> = {
                asc: 'sort.ascending',
                desc: 'sort.descending',
                toggle: 'sort.toggleOrder',
            }
            return typeof value === 'string' ? byOrder[value] : undefined
        }
        return undefined
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
        // Settings with section (MCP-specific: "dialog open settings --section shortcuts").
        // The Rust MCP executor (`mcp/executor/dialogs.rs`) emits `{ section: <string> }`
        // — a BARE string, no anchor (the `dialog` tool has no anchor param, so MCP can't
        // deep-link to a row today; that's future work). Parse defensively (no `as` cast,
        // same discipline as `mcp-listeners.ts`) and wrap the bare string in an array.
        await listenTauri('open-settings', (event) => {
            const payload = event.payload
            const section =
                payload && typeof payload === 'object' && 'section' in payload && typeof payload.section === 'string'
                    ? payload.section
                    : undefined
            void openSettingsWindow(section ? [section] : undefined)
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

        // E2E only: drive the native drag-and-drop drop entry programmatically.
        // Real OS drag can't be synthesized in Playwright, so the harness emits
        // this event to exercise OUR drop handling (the shared destination guard,
        // source-volume resolution, and transfer dialog) through the SAME
        // `dragDrop.handleFileDrop` the live drop branch runs. Gated on
        // `getAppMode() === 'e2e'` (set by CMDR_E2E_MODE=1, never true in prod),
        // so production never reacts even if the event were somehow emitted.
        await listenTauri('e2e-trigger-file-drop', (event) => {
            if (getAppMode() !== 'e2e') return
            const { paths, targetPane, targetFolderPath, operation, recordedIdentity } = event.payload as {
                paths: string[]
                targetPane: 'left' | 'right'
                targetFolderPath?: string
                operation?: TransferOperationType
                recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] }
            }
            explorerRef?.triggerFileDrop(paths, targetPane, targetFolderPath, operation, recordedIdentity)
        })
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
            showGoToPathDialog ||
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
                if (!(await saveSettings({ fullDiskAccessChoice: 'allow' }))) {
                    log.warn('Could not mirror fullDiskAccessChoice=allow; FDA may re-prompt on next launch')
                }
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
            // The MCP adapter dispatches through the same typed command bus as the
            // keyboard / palette / menu paths. `handleCommandExecute` already binds
            // the shared dispatch context, so MCP events get the uniform preamble
            // (log + breadcrumb + search-results guard).
            dispatch: handleCommandExecute,
            listenTauri,
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
        // Global go-to-latest-download hotkey bridge (default ⌃⌥⌘J): one
        // `global-shortcut-fired` listener; routes through `goToLatestDownload`
        // and shows the first-trigger warn toast when `acknowledged === false`.
        const unlistenGlobalShortcut = await startGlobalShortcutBridge(explorerRef)
        tauriUnlistenFns.push(unlistenGlobalShortcut)
        // Drag-out completion bridge: one `drag-out-session-started` +
        // `drag-out-session-complete` pair per drag session, turned into a single
        // signs-of-life → completion toast (downloading a phone/NAS file to
        // Finder shows nothing on Finder's side; this is our feedback surface).
        const unlistenDragOut = await startDragOutEventBridge()
        tauriUnlistenFns.push(unlistenDragOut)
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
        // Navigate the focused pane to the file's parent directory, then move cursor to the file
        const lastSlash = path.lastIndexOf('/')
        const parentDir = lastSlash > 0 ? path.slice(0, lastSlash) : '/'
        const fileName = path.slice(lastSlash + 1)
        const pane = explorerRef?.getFocusedPane() ?? 'left'
        const result = explorerRef?.navigate({ pane, to: { path: parentDir }, source: 'user' })
        // On a started navigation, await `settled` then move the cursor onto the
        // file. `moveCursor`'s own `whenLoadSettles` bridges the cross-volume arm,
        // where `settled` resolves before the listing loads (L2-adjacent).
        if (result?.status === 'started') {
            void result.settled.then(() => explorerRef?.moveCursor(pane, fileName))
        }
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
            openOnboarding: () => {
                void openOnboardingFromMenuOrPalette('menu')
            },
        },
    }

    async function handleCommandExecute<K extends CommandId>(
        commandId: K,
        ...args: CommandDispatchArgs<K>
    ): Promise<void> {
        await dispatchCommand(commandId, commandDispatchCtx, ...args)
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
                searchableFolder={getFocusedPaneSearchableFolder()}
                onShowAllInMainWindow={handleOpenSearchInPane}
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

        {#if showOnboarding}
            <OnboardingWizard onComplete={handleWizardComplete} />
        {/if}

        {#if showApp}
            <DualPaneExplorer bind:this={explorerRef} onCommand={handleCommandExecute} />
            <IndexingStatusIndicator />
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
