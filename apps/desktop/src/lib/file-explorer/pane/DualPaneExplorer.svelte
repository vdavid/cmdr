<script lang="ts">
    import { onMount, onDestroy, untrack } from 'svelte'
    import FilePane from './FilePane.svelte'
    import type { FilePaneAPI } from './types'
    import PaneResizer from './PaneResizer.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import DialogManager from './DialogManager.svelte'
    import { openInEditor } from '$lib/tauri-commands'
    import { pluralize } from '$lib/utils/pluralize'
    import { type ViewMode } from '$lib/app-status-store'
    import type { CommandId, McpSelectMode, McpTabAction, ConfirmDialogType } from '$lib/commands'
    import type { SelectionAction } from '../../../routes/(main)/explorer-api'
    import { saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import {
        listen,
        type Location,
        type UnlistenFn,
        updateFocusedPane,
        updateViewModeMenu,
        ejectVolume,
        onVolumeContextAction,
        onVolumeUnmounted,
        getIpcErrorMessage,
    } from '$lib/tauri-commands'
    import type {
        SortColumn,
        SortOrder,
        NetworkHost,
        ConflictResolution,
        WriteOperationError,
        FriendlyError,
        FileEntry,
    } from '../types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'
    import { determineNavigationPath } from '../navigation/path-navigation'

    import { type NavigationHistory } from '../navigation/navigation-history'
    import TabBar from '../tabs/TabBar.svelte'
    import {
        getActiveTab,
        getAllTabs,
        pushHistoryEntry,
        trimClosedStack,
        getClosedStackSize,
        MAX_TABS_PER_PANE,
        type TabManager,
    } from '../tabs/tab-state-manager.svelte'
    import type { TabId } from '../tabs/tab-types'
    import {
        saveTabsForPane,
        handleTabClose as tabOpsHandleTabClose,
        handleTabMiddleClick as tabOpsHandleTabMiddleClick,
        handleTabContextMenu as tabOpsHandleTabContextMenu,
        handleNewTab as tabOpsHandleNewTab,
        newTab as tabOpsNewTab,
        closeActiveTabWithConfirmation as tabOpsCloseActiveTabWithConfirmation,
        togglePinActiveTab as tabOpsTogglePinActiveTab,
        closeOtherTabsInFocusedPane as tabOpsCloseOtherTabs,
        reopenLastClosedTabInPane as tabOpsReopenLastClosedTab,
        syncPinTabMenuForPane,
        cycleTab as tabOpsCycleTab,
        switchToTab as tabOpsSwitchToTab,
    } from './tab-operations'
    import { initNetworkDiscovery, cleanupNetworkDiscovery } from '../network/network-store.svelte'
    import { initVolumeStore, getVolumes as getStoreVolumes, cleanupVolumeStore } from '$lib/stores/volume-store.svelte'
    import { initVolumeBusyStore, cleanupVolumeBusyStore } from '$lib/stores/volume-busy-store.svelte'
    import { initRestrictedPathsStore } from '$lib/stores/restricted-paths-store.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import { initialize as initMtpStore } from '$lib/mtp'
    import { smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
    import type { TransferOperationType } from '../types'
    import { createDialogState } from './dialog-state.svelte'
    import { explorerState } from './explorer-state.svelte'
    import type { PaneAccess } from './pane-access'
    import { createClipboardOperations } from './clipboard-operations'
    import { createFileOperationCommands } from './file-operation-commands'
    import { createPaneCommands } from './pane-commands'
    import { createSortOperations } from './sort-operations'
    import { createSwapPanes } from './swap-panes'
    import { createVolumeSelection } from './volume-selection'
    import { createEdgeFlowHandlers } from './edge-flow-handlers'
    import { createPaneMirror } from './pane-mirror'
    import { createKeyDispatch } from './key-dispatch'
    import { createMcpTabAction } from './mcp-tab-action'
    import {
        navigate as runNavigate,
        commitPathFromListing,
        type NavigateDeps,
        type NavigateIntent,
        type NavigateResult,
    } from './navigate'
    import { createDragDropController } from './drag-drop-controller.svelte'
    import { initPersistenceSubscriber } from './persistence-subscriber.svelte'
    import { initDebugEmitters } from './debug-emitters.svelte'
    import { initTabMcpSync } from './tab-mcp-sync.svelte'
    import { initQuickLookFollow } from './quick-look-follow.svelte'
    import { recalculateWebviewOffset } from '../drag/drag-position'
    import { initIndexEvents } from '$lib/indexing/index'
    import { createIndexEventHandler } from './index-events'
    import { loadPersistedState } from './initialization'
    import { getDirectorySortMode } from '$lib/settings/reactive-settings.svelte'
    import { getSetting, onSettingChange } from '$lib/settings'
    import { setReopenClosedTabEnabled } from '$lib/tauri-commands'
    import DragOverlay from '../drag/DragOverlay.svelte'
    import { addToast, addToastForPane } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'

    function saveTabsForPaneSide(pane: 'left' | 'right') {
        saveTabsForPane(pane, getTabMgr)
    }

    /** Per-pane closed-tab history cap, lives in `fileExplorer.tabs.closedTabHistorySize` setting. */
    function getClosedTabsCap(): number {
        return getSetting('fileExplorer.tabs.closedTabHistorySize')
    }

    /** Pushes the focused pane's closed-stack-empty state to the backend so the
     *  File menu's "Reopen closed tab" item enables/disables in sync. */
    function syncReopenMenuState() {
        const enabled = getClosedStackSize(getTabMgr(focusedPane)) > 0
        void setReopenClosedTabEnabled(enabled)
    }

    // Live tab-manager holders live in the explorer store now. These `$derived`
    // aliases read the live `$state<TabManager>` reference through the store getter,
    // so every reader below keeps tracking both holder swaps (`setTabMgr`) and
    // in-place manager mutations.
    const leftTabMgr = $derived(explorerState.getTabMgr('left'))
    const rightTabMgr = $derived(explorerState.getTabMgr('right'))

    // Derived active tab state: these replace the old scalar variables
    const leftPath = $derived(getActiveTab(leftTabMgr).path)
    const rightPath = $derived(getActiveTab(rightTabMgr).path)
    const leftVolumeId = $derived(getActiveTab(leftTabMgr).volumeId)
    const rightVolumeId = $derived(getActiveTab(rightTabMgr).volumeId)
    const leftViewMode = $derived(getActiveTab(leftTabMgr).viewMode)
    const rightViewMode = $derived(getActiveTab(rightTabMgr).viewMode)
    const leftSortBy = $derived(getActiveTab(leftTabMgr).sortBy)
    const rightSortBy = $derived(getActiveTab(rightTabMgr).sortBy)
    const leftSortOrder = $derived(getActiveTab(leftTabMgr).sortOrder)
    const rightSortOrder = $derived(getActiveTab(rightTabMgr).sortOrder)
    const leftHistory = $derived(getActiveTab(leftTabMgr).history)
    const rightHistory = $derived(getActiveTab(rightTabMgr).history)

    interface Props {
        /**
         * Bubbles a high-level command id from a pane up to the route, which
         * routes it through `handleCommandExecute` (the unified dispatcher).
         * Used by the Selection dialog's bare `+` / `-` shortcuts.
         */
        onCommand?: (commandId: CommandId) => void
    }

    const { onCommand }: Props = $props()

    // Focus, hidden-files, and the layout split live in the explorer store now.
    // These `$derived` aliases read them reactively; writes go through the store's
    // named mutators (`setFocusedPane` / `setShowHiddenFiles` / `setLeftPaneWidthPercent`).
    const focusedPane = $derived(explorerState.getFocusedPane())
    const showHiddenFiles = $derived(explorerState.getShowHiddenFiles())
    const leftPaneWidthPercent = $derived(explorerState.getLeftPaneWidthPercent())
    // Volumes come from the shared store (pushed by backend via `volumes-changed` event)
    const volumes = $derived(getStoreVolumes())
    let initialized = $state(false)

    let containerElement: HTMLDivElement | undefined = $state()
    const paneRefs = $state<Record<'left' | 'right', FilePaneAPI | undefined>>({ left: undefined, right: undefined })
    let unlistenSettings: UnlistenFn | undefined
    let unlistenVolumeUnmount: UnlistenFn | undefined
    let unlistenVolumeContextAction: UnlistenFn | undefined
    let unlistenIndexEvents: UnlistenFn | undefined
    let unlistenIndexAggregationComplete: UnlistenFn | undefined

    // Debounced mirror of both panes' tab structure into the MCP backend store.
    // Owns its own reactive $effect + debounce timer; `syncTabsToBackend` is
    // exposed for the one-shot initial push onMount fires after init.
    const tabMcpSync = initTabMcpSync({
        getLeftTabMgr: () => leftTabMgr,
        getRightTabMgr: () => rightTabMgr,
        getInitialized: () => initialized,
    })

    // Refs for pane wrapper elements (used for hit-testing drop targets)
    const paneWrapperEls = $state<Record<'left' | 'right', HTMLDivElement | undefined>>({
        left: undefined,
        right: undefined,
    })

    // Dialog state (transfer, new folder, alert, error)
    const dialogs = createDialogState({
        getLeftPaneRef: () => paneRefs.left,
        getRightPaneRef: () => paneRefs.right,
        getFocusedPaneRef: () => getPaneRef(focusedPane),
        getFocusedPaneSide: () => focusedPane,
        getShowHiddenFiles: () => showHiddenFiles,
        onRefocus: () => containerElement?.focus(),
        onOpenInEditor: (path: string) => void openInEditor(path),
    })

    // --- Pane accessor helpers ---

    function getPaneRef(pane: 'left' | 'right'): FilePaneAPI | undefined {
        return paneRefs[pane]
    }

    function getPanePath(pane: 'left' | 'right'): string {
        return pane === 'left' ? leftPath : rightPath
    }

    function getPaneVolumeId(pane: 'left' | 'right'): string {
        return pane === 'left' ? leftVolumeId : rightVolumeId
    }

    function getPaneHistory(pane: 'left' | 'right'): NavigationHistory {
        return pane === 'left' ? leftHistory : rightHistory
    }

    function getPaneSort(pane: 'left' | 'right'): { sortBy: SortColumn; sortOrder: SortOrder } {
        return pane === 'left'
            ? { sortBy: leftSortBy, sortOrder: leftSortOrder }
            : { sortBy: rightSortBy, sortOrder: rightSortOrder }
    }

    function getTabMgr(pane: 'left' | 'right'): TabManager {
        return explorerState.getTabMgr(pane)
    }

    function setPanePath(pane: 'left' | 'right', path: string) {
        getActiveTab(getTabMgr(pane)).path = path
    }

    function setPaneVolumeId(pane: 'left' | 'right', volumeId: string) {
        getActiveTab(getTabMgr(pane)).volumeId = volumeId
    }

    function setPaneHistory(pane: 'left' | 'right', history: NavigationHistory) {
        getActiveTab(getTabMgr(pane)).history = history
    }

    function setPaneSort(pane: 'left' | 'right', sortBy: SortColumn, sortOrder: SortOrder) {
        const tab = getActiveTab(getTabMgr(pane))
        tab.sortBy = sortBy
        tab.sortOrder = sortOrder
    }

    function setPaneViewMode(pane: 'left' | 'right', viewMode: ViewMode) {
        getActiveTab(getTabMgr(pane)).viewMode = viewMode
    }

    function getPaneViewMode(pane: 'left' | 'right'): ViewMode {
        return pane === 'left' ? leftViewMode : rightViewMode
    }

    /** Pushes the full View menu state (active pane + per-pane modes) to the backend so
     * the per-pane menu items show correct check marks and the keyboard accelerator
     * (⌘1/⌘2 by default) attaches to the active pane's pair. */
    function pushViewMenuState() {
        void updateViewModeMenu(focusedPane, getPaneViewMode('left'), getPaneViewMode('right'))
    }

    function getPaneVolumePath(pane: 'left' | 'right'): string {
        return pane === 'left' ? leftVolumePath : rightVolumePath
    }

    function getPaneVolumeName(pane: 'left' | 'right'): string | undefined {
        return pane === 'left' ? leftVolumeName : rightVolumeName
    }

    function getPaneWidth(pane: 'left' | 'right'): number {
        return pane === 'left' ? leftPaneWidthPercent : 100 - leftPaneWidthPercent
    }

    function otherPane(pane: 'left' | 'right'): 'left' | 'right' {
        return pane === 'left' ? 'right' : 'left'
    }

    // Read API over this explorer's navigation + UI-chrome state, handed to command
    // factories so they don't reach into component closures. Getters return live
    // references (never copies / snapshots) so callers inside $derived/$effect keep
    // tracking once this state moves into a module store.
    const paneAccess: PaneAccess = {
        getPaneRef,
        getPanePath,
        getPaneVolumeId,
        getPaneSort,
        getPaneHistory,
        getFocusedPane: () => focusedPane,
        otherPane,
        getShowHiddenFiles: () => showHiddenFiles,
        getVolumes: () => volumes,
        focusContainer: () => containerElement?.focus(),
    }

    const clipboardOps = createClipboardOperations(paneAccess, dialogs)
    const fileOps = createFileOperationCommands(paneAccess, dialogs)
    const paneCommands = createPaneCommands(paneAccess, dialogs)
    const sortOps = createSortOperations({
        getPaneRef,
        getPaneSort,
        setPaneSort,
        getShowHiddenFiles: () => showHiddenFiles,
        getFocusedPane: () => focusedPane,
    })
    const swapper = createSwapPanes({
        getPaneRef,
        getLeftTabMgr: () => leftTabMgr,
        getRightTabMgr: () => rightTabMgr,
        isAnyTransferDialogOpen: () => dialogs.isAnyTransferDialogOpen(),
        focusContainer: () => containerElement?.focus(),
    })
    const volumeSelection = createVolumeSelection({
        getVolumes: () => volumes,
        navigate: navigateIntent,
    })
    // Recovery / fallback nav flows (cancel-loading, MTP-fatal, retry-unreachable,
    // open-home, volume-unmount). Each folds onto navigate({ source: 'fallback' | 'cancel' }).
    const edgeFlow = createEdgeFlowHandlers({
        navigate: navigateIntent,
        getPaneRef,
        getPaneHistory,
        getPaneVolumeId,
        getTabMgr,
        getVolumes: () => volumes,
        focusContainer: () => containerElement?.focus(),
    })
    // MCP `tab` tool per-pane action dispatch (new/close/close_others/reopen/
    // activate/set_pinned). Menu-sync + persistence driven the same way as before.
    const mcpTab = createMcpTabAction({
        getFocusedPane: () => focusedPane,
        getTabMgr,
        getClosedTabsCap,
        saveTabsForPaneSide,
        syncPinTabMenu,
        syncReopenMenuState,
        reopenLastClosedTab,
        switchToTab,
        snapshotHistory: (h) => $state.snapshot(h),
    })

    // Top-level keyboard + focus routing (onkeydown / onkeyup / onfocusin on the
    // container): escape-during-loading, volume-chooser routing, type-to-jump
    // intercept, and the focus guard. Owns no state; routes to the focused pane.
    const keyDispatch = createKeyDispatch({
        getPaneRef,
        getFocusedPane: () => focusedPane,
        getContainerElement: () => containerElement,
    })

    // "Copy path from <source> to <target> pane": mirror location + network state
    // into the other pane without shifting keyboard focus.
    const paneMirror = createPaneMirror({
        navigate: navigateIntent,
        getPaneRef,
        getPaneVolumeId,
        getPanePath,
        getPaneHistory,
        setPaneHistory,
        getFocusedPane: () => focusedPane,
        setFocusedPane: (pane) => {
            explorerState.setFocusedPane(pane)
        },
    })

    // Per-pane transaction-token map for `navigate()`. Caller-owned (it must
    // survive across `navigate()` calls), keyed by side. Gates the per-pane
    // cross-volume resolve bail. The background `determineNavigationPath`
    // correction is gated separately by `navCorrectionGen`, a SINGLE GLOBAL
    // counter (the old `volumeChangeGeneration`): a volume change on either pane
    // supersedes a pending correction on the other. The `onPathChange` re-entry
    // drops stale listings by the foreign-path policy (L6), not the token.
    const navTokens = new Map<'left' | 'right', number>()
    const navCorrectionGen = { value: 0 }

    // The store-backed `NavigateDeps`, built the same way `paneAccess` is — the
    // component owns the construction; `navigate()` owns the transaction logic.
    // The ONLY callers of `setPaneVolumeId` / `setPanePath` / `setPaneHistory` are
    // `navigate()`'s internal `commit` and the two orthogonal network-host pushes
    // (`handleNetworkHostChange`, `mirrorNetworkStateToPane`).
    const navigateDeps: NavigateDeps = {
        getTabMgr,
        getPaneVolumeId,
        getPanePath,
        getPaneHistory,
        getPaneVolumePath,
        getPaneVolumeName,
        otherPane,
        setPaneVolumeId,
        setPanePath,
        setPaneHistory,
        setFocusedPane: (pane) => {
            explorerState.setFocusedPane(pane)
        },
        getPaneRef,
        getVolumePathById: (volumeId) => volumes.find((v) => v.id === volumeId)?.path,
        determineNavigationPath: (volumeId, volumePath, targetPath, other) =>
            determineNavigationPath(volumeId, volumePath, targetPath, other),
        persist: (event) => {
            // The single nav-state persistence subscriber (A5) owns disk writes.
            // `pane-state` is covered REACTIVELY there (the subscriber's per-pane
            // effects watch the store), so `navigate()`'s pane-state commit doesn't
            // trigger a save from here — the store mutation it just made does. The
            // `last-used-path` DELTA (the old path of the old volume on a switch)
            // can't be derived from a snapshot, so it's forwarded explicitly.
            if (event.kind === 'last-used-path') {
                persistence.persistLastUsedPath(event.record)
            }
        },
        addToast: (pane, message, opts) => addToastForPane(pane, message, opts),
        tokens: navTokens,
        correctionGen: navCorrectionGen,
    }

    // The single nav-state persistence subscriber (A5). Created synchronously
    // during component init (L3): its per-pane reactive effects watch the store
    // and write `app-status.json`. The two deltas a snapshot can't express —
    // last-used-path (the old path on a volume switch) and the layout split
    // (drag-end-only) — come back as explicit hooks the component calls.
    const persistence = initPersistenceSubscriber({
        getInitialized: () => initialized,
        getFocusedPane: () => focusedPane,
        getPanePath,
        getPaneVolumeId,
        getPaneViewMode,
        getPaneSortBy: (pane) => getPaneSort(pane).sortBy,
        getPaneSortOrder: (pane) => getPaneSort(pane).sortOrder,
        saveTabsForPaneSide,
    })

    /** The single coordinator-level navigation entry. Delegates to the `navigate()` transaction. */
    function navigateIntent(intent: NavigateIntent): NavigateResult {
        return runNavigate(intent, navigateDeps)
    }

    // Native drag-and-drop band: drop-target highlight state, the drag handlers,
    // the three Tauri drag listeners, and the folder-highlight effect. The effect
    // is created synchronously inside the factory (L3); `init()`/`cleanup()` run
    // from onMount/onDestroy.
    const dragDrop = createDragDropController({
        access: paneAccess,
        dialogs,
        getPaneWrapperEls: () => paneWrapperEls,
    })

    // Quick Look cursor-follow + error-close effects. Owns its own reactive
    // $effects and debounce timer; cleanup() clears the pending timer onDestroy.
    const quickLookFollow = initQuickLookFollow({
        getFocusedPane: () => focusedPane,
        getPaneRef,
        getPaneVolumeId,
    })

    // Dev-only: mirror per-pane history + closed-tab stacks to the debug window.
    initDebugEmitters({
        getLeftHistory: () => leftHistory,
        getRightHistory: () => rightHistory,
        getLeftTabMgr: () => leftTabMgr,
        getRightTabMgr: () => rightTabMgr,
        getFocusedPane: () => focusedPane,
    })

    // Derived volume paths - handle 'network' virtual volume specially
    const leftVolumePath = $derived(
        leftVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === leftVolumeId)?.path ?? '/'),
    )
    const rightVolumePath = $derived(
        rightVolumeId === 'network' ? 'smb://' : (volumes.find((v) => v.id === rightVolumeId)?.path ?? '/'),
    )
    // Derived volume names for MCP state sync
    const leftVolumeName = $derived(
        leftVolumeId === 'network' ? 'Network' : volumes.find((v) => v.id === leftVolumeId)?.name,
    )
    const rightVolumeName = $derived(
        rightVolumeId === 'network' ? 'Network' : volumes.find((v) => v.id === rightVolumeId)?.name,
    )

    // --- Unified handler functions ---

    /**
     * The pane's `onPathChange` lands here when a listing completes (the in-place
     * path-nav arm + the parent-nav / walk-up self-re-entries). `commitPathFromListing`
     * (in `navigate()`) owns the commit + the stale-listing drop policy (L6, token +
     * foreign-path); on a successful commit we restore any persisted cursor from the
     * tab state (cold-load after a tab switch). The drop case returns `false`, so a
     * stale listing never disturbs the cursor.
     */
    function handlePathCommitted(pane: 'left' | 'right', path: string) {
        if (!commitPathFromListing(navigateDeps, pane, path)) return

        // Restore cursor from tab state if available (happens after cold-load on tab switch)
        const activeTab = getActiveTab(getTabMgr(pane))
        if (activeTab.cursorFilename) {
            const filename = activeTab.cursorFilename
            activeTab.cursorFilename = null
            void restoreCursorByFilename(pane, filename)
        }
    }

    async function restoreCursorByFilename(pane: 'left' | 'right', filename: string) {
        const paneRef = getPaneRef(pane)
        if (!paneRef) return
        await paneCommands.moveCursorByNameInFileListing(paneRef, filename)
    }

    function handleNetworkHostChange(pane: 'left' | 'right', host: NetworkHost | null) {
        setPaneHistory(
            pane,
            pushHistoryEntry(getPaneHistory(pane), {
                volumeId: 'network',
                path: 'smb://',
                networkHost: host ?? undefined,
            }),
        )
    }

    // Re-sort both panes when directorySortMode setting changes
    $effect(() => {
        // Read the reactive value to establish the dependency
        void getDirectorySortMode()
        // Skip during initialization
        if (!initialized) return
        // Re-sort both panes with the new mode (untrack to avoid re-triggering)
        untrack(() => {
            void sortOps.resortPaneWithCurrentSort('left')
            void sortOps.resortPaneWithCurrentSort('right')
        })
    })

    // Trim both panes' closed-tab stacks when the cap setting decreases.
    let unlistenClosedTabsCap: (() => void) | undefined
    $effect(() => {
        if (!initialized) return
        unlistenClosedTabsCap = onSettingChange((id, value) => {
            if (id !== 'fileExplorer.tabs.closedTabHistorySize') return
            const cap = typeof value === 'number' ? value : getClosedTabsCap()
            trimClosedStack(leftTabMgr, cap)
            trimClosedStack(rightTabMgr, cap)
            syncReopenMenuState()
        })
        return () => unlistenClosedTabsCap?.()
    })

    /**
     * Open a search-results snapshot in the target pane (defaults to focused).
     * Called by the SearchDialog's "Open in pane" action. The caller has already
     * stored the snapshot in `snapshotStore` and set the `lastAttemptId` slot;
     * here we route through `navigate({ to: { snapshot } })` (the volume-change
     * machinery) so all the standard pane mechanics (new-tab-on-pinned, focus
     * shift, history push via `pushHistoryEntry`) apply uniformly.
     * `pushHistoryEntry` increments the snapshot's refcount via the snapshot-store integration.
     */
    export function openSearchSnapshotInPane(snapshotId: string, pane?: 'left' | 'right'): void {
        const target = pane ?? focusedPane
        navigateIntent({ pane: target, to: { snapshot: snapshotId }, source: 'user' })
    }

    function handleFocus(pane: 'left' | 'right') {
        if (focusedPane !== pane) {
            // Clear the type-to-jump buffer on whichever pane is losing focus.
            // A buffer that the user can no longer see (because they switched panes)
            // shouldn't keep matching.
            getPaneRef(focusedPane)?.clearJumpState()
            explorerState.setFocusedPane(pane)
            void updateFocusedPane(pane)
            syncPinTabMenu()
            syncReopenMenuState()
            pushViewMenuState()
        }
        // Always restore DOM focus (needed after inline rename or dialog close within a pane)
        containerElement?.focus()
    }

    /**
     * Routes keys to whichever pane has its volume switcher dropdown open, and
     * SWALLOWS them from the pane behind it. Returns true whenever a chooser is
     * open (F1/F2 can open one on the non-focused pane, so we scan both):
     *
     * - If the dropdown's own handler consumes the key (arrow nav, Enter, Escape),
     *   we're done.
     * - If it doesn't (the inline favorite-rename `<input>` is active, so the
     *   dropdown deliberately ignores arrows/Home/End and lets the textbox edit),
     *   we STILL return true so the key never reaches `activePaneRef.handleKeyDown`
     *   and moves the pane cursor. While the switcher is open it owns keyboard
     *   focus; the panes behind it must stay inert (Fix E).
     */
    const handleIndexDirUpdated = createIndexEventHandler({
        getLeftPath: () => leftPath,
        getRightPath: () => rightPath,
        getPaneRef,
    })

    function handleResizeForDevTools() {
        void recalculateWebviewOffset()
    }

    onMount(async () => {
        // Start font metrics measurement in background (non-blocking)
        void ensureFontMetricsLoaded()

        // Start network discovery in background (non-blocking)
        void initNetworkDiscovery()

        // Initialize volume store (subscribes to backend-pushed volume list)
        // and MTP store (subscribes to device connection events). Also wire up
        // the SMB reconnect manager; it listens for `smb-connection-changed`
        // and runs the per-volume backoff cycle that drives `SmbReconnectingView`.
        await Promise.all([
            initVolumeStore(),
            initVolumeBusyStore(),
            initMtpStore(),
            smbReconnectManager.init(),
            initRestrictedPathsStore(),
            initSystemStrings(),
        ])

        // Load persisted state, resolve volumes, and create tab managers
        const persistedState = await loadPersistedState()
        explorerState.setTabMgr('left', persistedState.leftTabMgr)
        explorerState.setTabMgr('right', persistedState.rightTabMgr)
        explorerState.setFocusedPane(persistedState.focusedPane)
        explorerState.setShowHiddenFiles(persistedState.showHiddenFiles)
        explorerState.setLeftPaneWidthPercent(persistedState.leftPaneWidthPercent)

        initialized = true
        syncPinTabMenu()
        syncReopenMenuState()
        pushViewMenuState()

        // Sync initial tab state to MCP backend
        tabMcpSync.syncTabsToBackend()

        // Dev-only: correct drag coordinates when Web Inspector is docked.
        if (import.meta.env.DEV) {
            void recalculateWebviewOffset()
            window.addEventListener('resize', handleResizeForDevTools)
        }

        // Subscribe to settings changes from the backend menu
        unlistenSettings = await subscribeToSettingsChanges((newSettings) => {
            if (newSettings.showHiddenFiles !== undefined) {
                explorerState.setShowHiddenFiles(newSettings.showHiddenFiles)
                // Persist to settings store
                void saveSettings({ showHiddenFiles: newSettings.showHiddenFiles })
            }
        })

        // Subscribe to volume unmount events (redirect panes off ejected volumes)
        unlistenVolumeUnmount = await onVolumeUnmounted((payload) => {
            const volume = volumes.find((v) => v.path === payload.volumePath)
            if (volume) {
                void edgeFlow.handleVolumeUnmount(volume.id)
            }
        })

        // Native breadcrumb context menu's "Eject (name)" item routes back via this
        // event (see `on_menu_event` in `lib.rs`). The Svelte popup paths in
        // VolumeBreadcrumb call `ejectVolume` directly; this listener only handles
        // the native-menu case.
        unlistenVolumeContextAction = await onVolumeContextAction((payload) => {
            if (payload.action !== 'eject') return
            void (async () => {
                try {
                    await ejectVolume(payload.volumeId)
                } catch (e) {
                    addToast(
                        tString('fileExplorer.pane.ejectFailedToast', {
                            volumeName: payload.volumeName,
                            message: getIpcErrorMessage(e),
                        }),
                        { level: 'error' },
                    )
                }
            })()
        })

        // Listen for index directory updates to refresh panes when sizes change
        unlistenIndexEvents = await initIndexEvents(handleIndexDirUpdated)

        // Refresh both panes when aggregation completes (all dir_stats are now in the DB)
        unlistenIndexAggregationComplete = await listen('index-aggregation-complete', () => {
            getPaneRef('left')?.refreshIndexSizes()
            getPaneRef('right')?.refreshIndexSizes()
        })

        // Register the native drag-and-drop listeners (drag-image-size, drag-modifiers,
        // onDragDropEvent). The controller owns the band; the folder-highlight effect
        // was already created synchronously in the factory body.
        await dragDrop.init()
    })

    onDestroy(() => {
        unlistenSettings?.()
        unlistenVolumeUnmount?.()
        unlistenVolumeContextAction?.()
        unlistenIndexEvents?.()
        unlistenIndexAggregationComplete?.()
        tabMcpSync.cleanup()
        quickLookFollow.cleanup()
        // No cleanup needed for throttle (no pending timers)
        cleanupVolumeStore()
        cleanupVolumeBusyStore()
        cleanupNetworkDiscovery()
        dragDrop.cleanup()
        window.removeEventListener('resize', handleResizeForDevTools) // No-op in non-dev, safe to always call
    })

    function handlePaneResize(widthPercent: number) {
        explorerState.setLeftPaneWidthPercent(widthPercent)
    }

    function handlePaneResizeEnd() {
        // Layout persists drag-END only (never per frame) — the subscriber's
        // explicit hook, not a reactive effect. See persistence-subscriber.svelte.ts.
        persistence.persistLayout(leftPaneWidthPercent)
    }

    function handlePaneResizeReset() {
        explorerState.setLeftPaneWidthPercent(50)
        persistence.persistLayout(50)
    }

    /** Activates inline rename on the focused pane's cursor item. */
    export function startRename() {
        fileOps.startRename()
    }

    /** Cancels any active inline rename on either pane. */
    export function cancelRename() {
        fileOps.cancelRename()
    }

    /** Returns whether inline rename is active on either pane. */
    export function isRenaming(): boolean {
        return fileOps.isRenaming()
    }

    /** Opens the new folder dialog. Pre-fills with the entry name under cursor. */
    export async function openNewFolderDialog() {
        await fileOps.openNewFolderDialog()
    }

    /** Opens the new file dialog. Pre-fills with the filename under cursor. */
    export async function openNewFileDialog() {
        await fileOps.openNewFileDialog()
    }

    /** Closes any confirmation dialog (new folder, new file, or transfer) if open (for MCP). */
    export function closeConfirmationDialog() {
        fileOps.closeConfirmationDialog()
    }

    /** Returns whether any confirmation dialog is currently open. */
    export function isConfirmationDialogOpen(): boolean {
        return fileOps.isConfirmationDialogOpen()
    }

    /**
     * Returns whether the volume switcher dropdown is open on EITHER pane. The
     * dropdown hosts the inline favorite-rename `<input>` plus a focusable list,
     * so while it's open the app must stop firing pane/global shortcuts (⌘A,
     * ⌥←/→, ⌘[/], Backspace, etc.) that would otherwise steal keystrokes from
     * the textbox. `+page.svelte`'s `isModalDialogOpen()` reads this through the
     * ExplorerAPI so suppression rides the existing scope-suppression seam.
     */
    export function isVolumeChooserOpen(): boolean {
        return (paneRefs.left?.isVolumeChooserOpen() ?? false) || (paneRefs.right?.isVolumeChooserOpen() ?? false)
    }

    /** Opens the file viewer for the file under the cursor. */
    export async function openViewerForCursor() {
        await fileOps.openViewerForCursor()
    }

    /** Opens the transfer dialog with the current selection info. */
    export async function openTransferDialog(
        operationType: TransferOperationType,
        autoConfirm?: boolean,
        onConflict?: string,
    ) {
        await fileOps.openTransferDialog(operationType, autoConfirm, onConflict)
    }

    /** Opens the copy dialog (convenience wrapper for MCP/key binding). */
    export async function openCopyDialog(autoConfirm?: boolean, onConflict?: string) {
        await fileOps.openCopyDialog(autoConfirm, onConflict)
    }

    /** Opens the move dialog (convenience wrapper for MCP/key binding). */
    export async function openMoveDialog(autoConfirm?: boolean, onConflict?: string) {
        await fileOps.openMoveDialog(autoConfirm, onConflict)
    }

    /** Copies selected files (or cursor file) to the system clipboard. */
    export async function copyToClipboard() {
        await clipboardOps.copyToClipboard()
    }

    /** Cuts selected files (or cursor file) to the system clipboard. */
    export async function cutToClipboard() {
        await clipboardOps.cutToClipboard()
    }

    /** Pastes files from the system clipboard into the current directory. */
    export async function pasteFromClipboard(forceMove: boolean) {
        await clipboardOps.pasteFromClipboard(forceMove)
    }

    /** Opens the delete confirmation dialog for the current selection or cursor item. */
    export async function openDeleteDialog(permanent: boolean, autoConfirm?: boolean) {
        await fileOps.openDeleteDialog(permanent, autoConfirm)
    }

    // Focus the container after initialization so keyboard events work
    $effect(() => {
        if (initialized) {
            containerElement?.focus()
        }
    })

    /** Programmatically confirms an already-open dialog (for MCP). */
    export function confirmDialog(dialogType: ConfirmDialogType, onConflict?: string) {
        paneCommands.confirmDialog(dialogType, onConflict)
    }

    /**
     * Refocus the file explorer container.
     * Call this after closing modals to restore keyboard navigation.
     */
    export function refocus() {
        containerElement?.focus()
    }

    /**
     * Switch focus to the other pane.
     */
    export function switchPane() {
        getPaneRef('left')?.closeVolumeChooser()
        getPaneRef('right')?.closeVolumeChooser()
        const newFocus = otherPane(focusedPane)
        explorerState.setFocusedPane(newFocus)
        void updateFocusedPane(newFocus)
        pushViewMenuState()
        containerElement?.focus()
    }

    /**
     * Swap left and right panes entirely (paths, volumes, history, sort, view mode, listing state).
     * Zero backend calls: we just swap listing ownership on the frontend.
     */
    export function swapPanes(): void {
        swapper.swapPanes()
    }

    export function toggleVolumeChooser(pane: 'left' | 'right') {
        paneCommands.toggleVolumeChooser(pane)
    }

    export function openVolumeChooser() {
        paneCommands.openVolumeChooser()
    }

    export function closeVolumeChooser() {
        paneCommands.closeVolumeChooser()
    }

    /**
     * Toggle show hidden files. Synchronous FE state flip so the listing
     * re-fetch effects (FilePane includeHidden $effect → FullList cache reset)
     * land in the next Svelte tick, not after an IPC + event round-trip via
     * Rust. The caller is responsible for syncing the native menu's
     * `CheckMenuItem` checked state separately (`syncMenuShowHidden`).
     *
     * @returns The new `showHiddenFiles` state.
     */
    export function toggleHiddenFiles(): boolean {
        explorerState.toggleHiddenFiles()
        const next = showHiddenFiles
        void saveSettings({ showHiddenFiles: next })
        return next
    }

    /**
     * Set view mode for a specific pane (or focused pane if not specified).
     * Used by command palette and MCP.
     */
    export function setViewMode(mode: ViewMode, pane?: 'left' | 'right') {
        const targetPane = pane ?? focusedPane
        setPaneViewMode(targetPane, mode)
        // viewMode persistence (app-status + tab set) fires from the subscriber's
        // per-pane effect, which reacts to this store change.
        pushViewMenuState()
    }

    /**
     * Set a specific pane's view mode in response to a native-menu click
     * (the `view.setMode` command, dispatched from `view-mode-changed`). Same
     * set + persist as `setViewMode`, but deliberately OMITS `pushViewMenuState`:
     * the menu click already toggled its own CheckMenuItem and Rust ran
     * `sync_view_mode_check_states`, so pushing the state back would double-sync.
     * Focus-preserving — the target pane changes even when the other pane is
     * focused (an inactive-pane menu click).
     */
    export function setViewModeFromMenu(pane: 'left' | 'right', mode: ViewMode) {
        setPaneViewMode(pane, mode)
        // viewMode persistence fires from the subscriber's per-pane effect.
    }

    /**
     * The single coordinator-level navigation entry. Replaces the old
     * `navigate(action)` + `navigateToPath(pane, path)` pair: callers now pass a
     * typed `NavigateIntent` (volume/path, history walk, or snapshot) and get a
     * `NavigateResult` (started + `settled`, or a typed refusal). The bus nav cases,
     * the MCP adapter, and the four external write-callers all use this.
     */
    export function navigate(intent: NavigateIntent): NavigateResult {
        return navigateIntent(intent)
    }

    export function getFileAndPathUnderCursor(): { path: string; filename: string } | null {
        return paneCommands.getFileAndPathUnderCursor()
    }

    export function toggleTagOnFocusedSelection(color: number): Promise<void> {
        return paneCommands.toggleTagOnFocusedSelection(color)
    }

    export function sendKeyToFocusedPane(key: string) {
        paneCommands.sendKeyToFocusedPane(key)
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically by MCP listeners
    export async function openItemUnderCursor(): Promise<void> {
        await paneCommands.openItemUnderCursor()
    }

    /**
     * Set sort column for a specific pane (or focused pane if not specified).
     * Used by command palette.
     */
    export function setSortColumn(column: SortColumn, pane?: 'left' | 'right') {
        sortOps.setSortColumn(column, pane)
    }

    /**
     * Set sort order for a specific pane (or focused pane if not specified).
     * Used by command palette.
     */
    export function setSortOrder(order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') {
        sortOps.setSortOrder(order, pane)
    }

    /**
     * Set both sort column and order atomically for a specific pane.
     * Used by MCP sort command to avoid race conditions.
     */
    export async function setSort(column: SortColumn, order: 'asc' | 'desc', pane: 'left' | 'right') {
        await sortOps.setSort(column, order, pane)
    }

    export function getFocusedPane(): 'left' | 'right' {
        return paneCommands.getFocusedPane()
    }

    /** Shift keyboard focus to a pane (store-level; no DOM re-anchor, matching `restoreFocus`). */
    export function setFocusedPane(pane: 'left' | 'right'): void {
        explorerState.setFocusedPane(pane)
        // focusedPane persistence fires from the subscriber's focus effect.
    }

    /** The pane's active-tab location (volume id, volume mount path, current dir). */
    export function getPaneLocation(pane: 'left' | 'right'): { volumeId: string; volumePath: string; path: string } {
        return { volumeId: getPaneVolumeId(pane), volumePath: getPaneVolumePath(pane), path: getPanePath(pane) }
    }

    // noinspection JSUnusedGlobalSymbols -- consumed by quick-look-state
    export function routePanelKey(payload: {
        key: string
        code: string
        shiftKey: boolean
        metaKey: boolean
        altKey: boolean
        ctrlKey: boolean
    }) {
        paneCommands.routePanelKey(payload)
    }

    export function handleSelectionAction(action: SelectionAction, startIndex?: number, endIndex?: number) {
        paneCommands.handleSelectionAction(action, startIndex, endIndex)
    }

    export function applyIndicesToFocusedPane(idxs: number[], mode: 'add' | 'remove') {
        paneCommands.applyIndicesToFocusedPane(idxs, mode)
    }

    // noinspection JSUnusedGlobalSymbols -- consumed by +page.svelte for Selection dialog
    export async function getFocusedPaneEntries(): Promise<{
        entries: FileEntry[]
        cursorIndex: number
        isSnapshotPane: boolean
    }> {
        return paneCommands.getFocusedPaneEntries()
    }

    /**
     * Move cursor to a specific index or filename.
     * Used by MCP move_cursor tool.
     *
     * Throws when the target doesn't exist (filename not in the listing, index out
     * of range, pane unavailable). The `cursor.moveTo` dispatch awaits this, so the
     * exception reaches the MCP adapter's try/catch and the tool reports the real
     * failure instead of a false-positive "OK: Moved cursor".
     */
    export async function moveCursor(pane: 'left' | 'right', to: number | string) {
        explorerState.setFocusedPane(pane)
        const paneRef = getPaneRef(pane)
        if (!paneRef) throw new Error(`The ${pane} pane is unavailable`)

        // Wait for the pane's current load (if any) to settle before touching
        // the listing. Without this, an MCP-driven `move_cursor` that lands
        // mid-navigation reads the FE's freshly-assigned `listingId` while the
        // backend's `LISTING_CACHE` insert is still in flight, surfacing as
        // "Listing not found" from `find_file_index`.
        await paneRef.whenLoadSettles()

        if (typeof to === 'number') {
            // `setCursorIndex` stores the value unclamped, so range-check first.
            // Network views own their cursor (hosts/shares), skip the listing count.
            if (!paneRef.isInNetworkView()) {
                const total = paneRef.getEffectiveTotalCount()
                if (to < 0 || to >= total) {
                    throw new Error(
                        `Index ${String(to)} is out of range in the ${pane} pane (${String(total)} ${pluralize(total, 'item')})`,
                    )
                }
            }
            await paneRef.setCursorIndex(to)
        } else {
            const found = await paneCommands.moveCursorByName(paneRef, to)
            if (!found) {
                throw new Error(`"${to}" not found in the ${pane} pane listing`)
            }
        }
        // MCP-driven cursor placement: re-anchor DOM focus on the explorer container
        // so the next keystroke (the agent often follows move_cursor with a shortcut)
        // lands in the right dispatcher chain. Also makes the awaited completion
        // genuine; `void` swallowed the cursor-set promise and let MCP report `OK`
        // before the cursor was observably positioned.
        containerElement?.focus()

        // Flush the new cursor position to the backend's PaneStateStore BEFORE the
        // round-trip replies ok, so a follow-up tool call (move_cursor → copy/move/
        // delete) reads fresh state. Without this, the cursor lives only in FE state
        // until the debounced pane→MCP sync fires; the immediately-following file-op
        // runs `check_operation_has_target` against a stale store (cursor still on
        // `..`) and rejects with "Nothing to copy". This mirrors `select`, which
        // flushes for the same reason (see pane-commands.ts handleMcpSelect*). Not a
        // per-keystroke path — keyboard cursor moves use `setCursorIndex` directly via
        // handleKeyDown, never this exported MCP/search entry.
        await paneRef.syncStateToMcpNow()
    }

    export function scrollTo(pane: 'left' | 'right', index: number) {
        paneCommands.scrollTo(pane, index)
    }

    /**
     * Select a volume by name for a specific pane.
     * Used by MCP select_volume tool.
     */
    export async function selectVolumeByName(pane: 'left' | 'right', name: string): Promise<boolean> {
        return volumeSelection.selectVolumeByName(pane, name)
    }

    /**
     * "Copy path from <source> to <target> pane" command. Mirrors the source
     * pane's location (volume + path + network state) into the target pane,
     * without shifting keyboard focus. When the source pane is focused, the
     * cursor refines the destination: cursor-on-folder uses the folder's path;
     * cursor-on-server (network browser) sets the target's selected host;
     * cursor-on-share (share browser) queues auto-mount on the target.
     */
    export function copyPathBetweenPanes(source: 'left' | 'right', target: 'left' | 'right'): void {
        paneMirror.copyPathBetweenPanes(source, target)
    }

    export async function refreshPane(): Promise<void> {
        await paneCommands.refreshPane()
    }

    /** Debug only: inject a FriendlyError into the specified pane. */
    export function injectError(pane: 'left' | 'right', friendly: FriendlyError) {
        paneCommands.injectError(pane, friendly)
    }

    /** Debug only: reset a pane's error state by re-navigating to its current path. */
    export function resetError(pane: 'left' | 'right' | 'both') {
        paneCommands.resetError(pane)
    }

    /** Debug only: open the TransferErrorDialog with a synthetic error carrying the given FriendlyError. */
    export function triggerTransferError(friendly: FriendlyError) {
        paneCommands.triggerTransferError(friendly)
    }

    /** E2E only: drive the native drag-and-drop entry path programmatically.
     *  Real OS drag can't be synthesized in Playwright, so this calls the SAME
     *  `dragDrop.handleFileDrop` the live `onDragDropEvent` 'drop' branch calls —
     *  the shared destination guard, source-volume resolution, and transfer
     *  dialog all run identically. Wired only behind the E2E gate in
     *  `+page.svelte`; never invoked in production. */
    export function triggerFileDrop(
        paths: string[],
        targetPane: 'left' | 'right',
        targetFolderPath?: string,
        operation: TransferOperationType = 'copy',
        recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] },
    ): void {
        // `recordedIdentity` set → model an in-app self-drag: build the transfer
        // from the recorded source volume + the paths the volume knows, exactly
        // as the live drop's recorded-identity branch does. Absent → model a
        // genuine external drop (resolver path). The recorded identity carries
        // `startedAt` for shape parity with the real record.
        const identity = recordedIdentity ? { ...recordedIdentity, startedAt: Date.now() } : undefined
        void dragDrop.handleFileDrop(paths, targetPane, targetFolderPath, operation, identity)
    }

    /** Refresh network hosts in the focused pane (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        paneCommands.refreshNetworkHosts()
    }

    export async function handleMcpSelect(
        pane: 'left' | 'right',
        start: number,
        count: number | 'all',
        mode: McpSelectMode,
    ): Promise<void> {
        // Focus follows the selection (same as `moveCursor`). The backend already
        // set ITS focused-pane store; without the FE following, a subsequent
        // focused-pane operation (copy/delete) acts on the previously-focused
        // pane while the backend pre-check validates against the selected one.
        explorerState.setFocusedPane(pane)
        await paneCommands.handleMcpSelect(pane, start, count, mode)
    }

    export async function handleMcpSelectNames(
        pane: 'left' | 'right',
        names: string[],
        mode: McpSelectMode,
    ): Promise<void> {
        // Focus follows the selection — see `handleMcpSelect`.
        explorerState.setFocusedPane(pane)
        await paneCommands.handleMcpSelectNames(pane, names, mode)
    }

    // --- Tab bar handler functions (logic in tab-operations.ts) ---

    function handleTabClose(pane: 'left' | 'right', tabId: TabId) {
        void tabOpsHandleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu, getClosedTabsCap).then(() => {
            syncReopenMenuState()
        })
    }

    function handleTabMiddleClick(pane: 'left' | 'right', tabId: TabId) {
        tabOpsHandleTabMiddleClick(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu, getClosedTabsCap)
        syncReopenMenuState()
    }

    function handleNewTab(pane: 'left' | 'right') {
        tabOpsHandleNewTab(
            pane,
            focusedPane,
            (p) => {
                explorerState.setFocusedPane(p)
            },
            newTab,
        )
    }

    function handleTabContextMenu(pane: 'left' | 'right', tabId: TabId, event: MouseEvent) {
        void tabOpsHandleTabContextMenu(
            pane,
            tabId,
            event,
            getTabMgr,
            focusedPane,
            syncPinTabMenu,
            getClosedTabsCap,
        ).then(() => {
            syncReopenMenuState()
        })
    }

    export function newTab(): boolean {
        return tabOpsNewTab(focusedPane, getTabMgr, (h) => $state.snapshot(h))
    }

    export async function closeActiveTabWithConfirmation(): Promise<'closed' | 'last-tab' | 'cancelled'> {
        const result = await tabOpsCloseActiveTabWithConfirmation(focusedPane, getTabMgr, getClosedTabsCap)
        syncReopenMenuState()
        return result
    }

    export function reopenLastClosedTab(): 'reopened' | 'empty' | 'cap' {
        const result = tabOpsReopenLastClosedTab(focusedPane, getTabMgr)
        syncReopenMenuState()
        return result
    }

    export function togglePinActiveTab(): void {
        tabOpsTogglePinActiveTab(focusedPane, getTabMgr)
    }

    export function closeOtherTabs(): void {
        tabOpsCloseOtherTabs(focusedPane, getTabMgr, getClosedTabsCap)
        syncReopenMenuState()
    }

    /**
     * Per-pane tab action from the MCP `tab` tool, routed here through the command
     * bus (`tab.mcpAction`). Owns the tab-mutation primitives; the dispatch layer
     * just forwards the typed args.
     */
    export function handleMcpTabAction(pane: 'left' | 'right', action: McpTabAction, tabId?: string, pinned?: boolean) {
        mcpTab.handleMcpTabAction(pane, action, tabId, pinned)
    }

    function syncPinTabMenu() {
        syncPinTabMenuForPane(focusedPane, getTabMgr)
    }

    export function cycleTab(direction: 'next' | 'prev'): void {
        tabOpsCycleTab(direction, focusedPane, getTabMgr, getPaneRef)
    }

    function switchToTab(pane: 'left' | 'right', tabId: TabId): boolean {
        return tabOpsSwitchToTab(pane, tabId, getTabMgr, getPaneRef, focusedPane)
    }
</script>

{#snippet paneBlock(paneId: 'left' | 'right')}
    {@const tabMgr = getTabMgr(paneId)}
    <div
        class="pane-wrapper"
        class:drop-target-active={dragDrop.getDropTargetPane() === paneId}
        style="width: {getPaneWidth(paneId)}%"
        bind:this={paneWrapperEls[paneId]}
    >
        <TabBar
            tabs={getAllTabs(tabMgr)}
            activeTabId={tabMgr.activeTabId}
            {paneId}
            maxTabs={MAX_TABS_PER_PANE}
            onTabSwitch={(tabId: TabId) => {
                switchToTab(paneId, tabId)
            }}
            onTabClose={(tabId: TabId) => {
                handleTabClose(paneId, tabId)
            }}
            onTabMiddleClick={(tabId: TabId) => {
                handleTabMiddleClick(paneId, tabId)
            }}
            onNewTab={() => {
                handleNewTab(paneId)
            }}
            onContextMenu={(tabId: TabId, event: MouseEvent) => {
                handleTabContextMenu(paneId, tabId, event)
            }}
            onPaneFocus={() => {
                handleFocus(paneId)
            }}
        />
        <!--suppress JSUnresolvedReference -->
        {#key getActiveTab(tabMgr).id}
            <FilePane
                bind:this={paneRefs[paneId]}
                {paneId}
                initialPath={getPanePath(paneId)}
                volumeId={getPaneVolumeId(paneId)}
                volumePath={getPaneVolumePath(paneId)}
                volumeName={getPaneVolumeName(paneId)}
                isFocused={focusedPane === paneId}
                {showHiddenFiles}
                viewMode={getPaneViewMode(paneId)}
                sortBy={getPaneSort(paneId).sortBy}
                sortOrder={getPaneSort(paneId).sortOrder}
                directorySortMode={getDirectorySortMode()}
                onPathChange={(path: string) => {
                    handlePathCommitted(paneId, path)
                }}
                onVolumeChange={(volumeId: string, volumePath: string, targetPath: string) => {
                    navigateIntent({
                        pane: paneId,
                        to: { selectVolume: { volumeId, path: targetPath } },
                        source: 'user',
                    })
                }}
                onGoToLocation={(location: Location) => {
                    // A search-results row opening a real entry: `{ location }` routes
                    // itself (cross-volume → switch arm), landing on the entry's real
                    // volume. `onVolumeChange` is the other intent (deliberate volume
                    // (re)select); they map to the two destination shapes.
                    navigateIntent({ pane: paneId, to: { goTo: location }, source: 'user' })
                }}
                onRequestFocus={() => {
                    handleFocus(paneId)
                }}
                onSortChange={(column: SortColumn) => sortOps.handleSortChange(paneId, column)}
                onNetworkHostChange={(host: NetworkHost | null) => {
                    handleNetworkHostChange(paneId, host)
                }}
                onCancelLoading={(cancelledPath: string, selectName?: string) => {
                    edgeFlow.handleCancelLoading(paneId, cancelledPath, selectName)
                }}
                onMtpFatalError={(msg: string) => edgeFlow.handleMtpFatalError(paneId, msg)}
                unreachable={getActiveTab(tabMgr).unreachable}
                onRetryUnreachable={() => edgeFlow.handleRetryUnreachable(paneId)}
                onOpenHome={() => edgeFlow.handleOpenHome(paneId)}
                {onCommand}
            />
        {/key}
    </div>
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_tabindex,a11y_no_noninteractive_element_interactions -->
<div
    class="dual-pane-explorer"
    bind:this={containerElement}
    onfocusin={keyDispatch.handleFocusGuard}
    onkeydown={keyDispatch.handleKeyDown}
    onkeyup={keyDispatch.handleKeyUp}
    tabindex="0"
    role="application"
    aria-label={tString('fileExplorer.pane.fileExplorerAriaLabel')}
    data-app-ready="false"
>
    {#if initialized}
        <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
        {@render paneBlock('left')}
        <PaneResizer onResize={handlePaneResize} onResizeEnd={handlePaneResizeEnd} onReset={handlePaneResizeReset} />
        <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
        {@render paneBlock('right')}
    {:else}
        <LoadingIcon />
    {/if}
</div>

<DragOverlay />

<DialogManager
    showTransferDialog={dialogs.showTransferDialog}
    transferDialogProps={dialogs.transferDialogProps}
    showTransferProgressDialog={dialogs.showTransferProgressDialog}
    transferProgressProps={dialogs.transferProgressProps}
    showNewFolderDialog={dialogs.showNewFolderDialog}
    newFolderDialogProps={dialogs.newFolderDialogProps}
    showNewFileDialog={dialogs.showNewFileDialog}
    newFileDialogProps={dialogs.newFileDialogProps}
    showAlertDialog={dialogs.showAlertDialog}
    alertDialogProps={dialogs.alertDialogProps}
    showTransferErrorDialog={dialogs.showTransferErrorDialog}
    transferErrorProps={dialogs.transferErrorProps}
    onTransferConfirm={(
        dest: string,
        volId: string,
        prevId: string | null,
        resolution: ConflictResolution,
        opType: TransferOperationType,
        scanning: boolean,
        preKnownConflicts: string[],
    ) => {
        dialogs.handleTransferConfirm(dest, volId, prevId, resolution, opType, scanning, preKnownConflicts)
    }}
    onTransferCancel={() => {
        dialogs.handleTransferCancel()
    }}
    onTransferComplete={(files: number, skipped: number, bytes: number) => {
        dialogs.handleTransferComplete(files, skipped, bytes)
    }}
    onTransferCancelled={(files: number) => {
        dialogs.handleTransferCancelled(files)
    }}
    onTransferError={(error: WriteOperationError) => {
        dialogs.handleTransferError(error)
    }}
    onTransferQueue={() => {
        dialogs.handleTransferQueue()
    }}
    onTransferErrorClose={() => {
        dialogs.handleTransferErrorClose()
    }}
    onNewFolderCreated={(name: string) => {
        dialogs.handleNewFolderCreated(name)
    }}
    onNewFolderCancel={() => {
        dialogs.handleNewFolderCancel()
    }}
    onNewFileCreated={(name: string) => {
        dialogs.handleNewFileCreated(name)
    }}
    onNewFileCancel={() => {
        dialogs.handleNewFileCancel()
    }}
    onAlertClose={() => {
        dialogs.handleAlertClose()
    }}
    showDeleteDialog={dialogs.showDeleteDialog}
    deleteDialogProps={dialogs.deleteDialogProps}
    onDeleteConfirm={(previewId: string | null, isPermanent: boolean) => {
        dialogs.handleDeleteConfirm(previewId, isPermanent)
    }}
    onDeleteCancel={() => {
        dialogs.handleDeleteCancel()
    }}
/>

<style>
    .dual-pane-explorer {
        display: flex;
        width: 100%;
        flex: 1;
        min-height: 0;
        gap: 0;
        outline: none;
    }

    .pane-wrapper {
        display: flex;
        flex-direction: column;
        height: 100%;
        min-width: 0;
        position: relative;
    }

    .pane-wrapper.drop-target-active::after {
        content: '';
        position: absolute;
        inset: 0;
        border: 2px solid var(--color-accent);
        pointer-events: none;
        z-index: 1;
    }

    /* Folder-level drop target highlight (class managed imperatively, elements in child components) */
    /*noinspection CssUnusedSymbol*/
    :global(.file-entry.folder-drop-target) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        background-color: var(--color-accent-subtle);
    }
</style>
