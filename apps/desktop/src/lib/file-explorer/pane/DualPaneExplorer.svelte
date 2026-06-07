<script lang="ts">
    import { onMount, onDestroy, untrack } from 'svelte'
    import FilePane from './FilePane.svelte'
    import type { FilePaneAPI } from './types'
    import PaneResizer from './PaneResizer.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import DialogManager from './DialogManager.svelte'
    import { openInEditor, quickLookSetPath } from '$lib/tauri-commands'
    import { closeFromPaneError, quickLookState } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
    import { type ViewMode } from '$lib/app-status-store'
    import type { CommandId, McpSelectMode, McpTabAction, ConfirmDialogType } from '$lib/commands'
    import type { SelectionAction } from '../../../routes/(main)/explorer-api'
    import { saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import {
        pathExists,
        listen,
        getDefaultVolumeId,
        resolvePathVolume,
        resortListing,
        type UnlistenFn,
        updateFocusedPane,
        updatePaneTabs,
        updateViewModeMenu,
        ejectVolume,
        onVolumeContextAction,
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
    import { defaultSortOrders } from '../types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'
    import { determineNavigationPath } from '../navigation/path-navigation'
    import { resolveValidPath } from '../navigation/path-resolution'

    import {
        getCurrentEntry,
        canGoBack,
        type NavigationHistory,
    } from '../navigation/navigation-history'
    import TabBar from '../tabs/TabBar.svelte'
    import {
        getActiveTab,
        getAllTabs,
        getTabCount,
        closeTabRecording,
        closeOtherTabsRecording,
        pushHistoryEntry,
        trimClosedStack,
        getClosedStackSize,
        MAX_TABS_PER_PANE,
        pinTab,
        unpinTab,
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
    import {
        initVolumeStore,
        getVolumes as getStoreVolumes,
        cleanupVolumeStore,
        requestVolumeRefresh,
    } from '$lib/stores/volume-store.svelte'
    import { initVolumeBusyStore, cleanupVolumeBusyStore } from '$lib/stores/volume-busy-store.svelte'
    import { initRestrictedPathsStore } from '$lib/stores/restricted-paths-store.svelte'
    import { initSystemStrings } from '$lib/system-strings.svelte'
    import { initialize as initMtpStore } from '$lib/mtp'
    import { smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { getNewSortOrder, applySortResult, collectSortState } from './sorting-handlers'
    import type { TransferOperationType } from '../types'
    import { createDialogState } from './dialog-state.svelte'
    import { explorerState } from './explorer-state.svelte'
    import type { PaneAccess } from './pane-access'
    import { createClipboardOperations } from './clipboard-operations'
    import { createFileOperationCommands } from './file-operation-commands'
    import { createPaneCommands } from './pane-commands'
    import {
        navigate as runNavigate,
        commitPathFromListing,
        type NavigateDeps,
        type NavigateIntent,
        type NavigateResult,
    } from './navigate'
    import { isTypeToJumpChar, isTypeToJumpResetKey } from './type-to-jump-keys'
    import { createDragDropController } from './drag-drop-controller.svelte'
    import { initPersistenceSubscriber } from './persistence-subscriber.svelte'
    import { recalculateWebviewOffset } from '../drag/drag-position'
    import { initIndexEvents } from '$lib/indexing/index'
    import { createIndexEventHandler } from './index-events'
    import { loadPersistedState } from './initialization'
    import { getDirectorySortMode } from '$lib/settings/reactive-settings.svelte'
    import { getSetting, onSettingChange } from '$lib/settings'
    import { setReopenClosedTabEnabled } from '$lib/tauri-commands'
    import DragOverlay from '../drag/DragOverlay.svelte'
    import { addToast } from '$lib/ui/toast'

    const log = getAppLogger('fileExplorer')

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

    // Debounced tab sync to MCP backend (~100ms trailing)
    let tabSyncTimer: ReturnType<typeof setTimeout> | null = null
    const TAB_SYNC_DEBOUNCE_MS = 100

    function syncTabsToBackend() {
        if (tabSyncTimer) clearTimeout(tabSyncTimer)
        tabSyncTimer = setTimeout(() => {
            const leftTabs = getAllTabs(leftTabMgr).map((t) => ({
                id: t.id,
                path: t.path,
                pinned: t.pinned,
                active: t.id === leftTabMgr.activeTabId,
            }))
            const rightTabs = getAllTabs(rightTabMgr).map((t) => ({
                id: t.id,
                path: t.path,
                pinned: t.pinned,
                active: t.id === rightTabMgr.activeTabId,
            }))
            void updatePaneTabs('left', leftTabs)
            void updatePaneTabs('right', rightTabs)
        }, TAB_SYNC_DEBOUNCE_MS)
    }

    // Reactive effect: sync tab structural changes to the MCP backend
    $effect(() => {
        // Read reactive values to establish Svelte reactivity dependencies.
        // Include path so MCP state updates when the active tab navigates.
        void getAllTabs(leftTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`)
        void getAllTabs(rightTabMgr).map((t) => `${t.id}:${t.pinned ? 'p' : ''}:${t.path}`)
        void leftTabMgr.activeTabId
        void rightTabMgr.activeTabId

        if (!initialized) return

        untrack(() => {
            syncTabsToBackend()
        })
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
        resolveVolume: (path) => resolvePathVolume(path),
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
        addToast: (message, opts) => addToast(message, opts),
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

    // Emit history state to debug window (dev mode only, skip in tests)
    $effect(() => {
        if (!import.meta.env.DEV || import.meta.env.MODE === 'test') return
        // Read the reactive values
        const left = leftHistory
        const right = rightHistory
        const focused = focusedPane
        // Emit without tracking to avoid infinite loops
        untrack(() => {
            void import('@tauri-apps/api/event').then(({ emit }) => {
                void emit('debug-history', { left, right, focusedPane: focused })
            })
        })
    })

    // Emit closed-tab stacks to debug window (dev mode only, skip in tests)
    $effect(() => {
        if (!import.meta.env.DEV || import.meta.env.MODE === 'test') return
        // Snapshot reads every property, setting up reactivity on push/pop/mutate.
        // It also produces plain JSON so Tauri's event channel can serialize it;
        // raw `$state` proxies + nested NavigationHistory throw on structured-clone.
        const leftSnap = $state.snapshot(leftTabMgr.closedStack)
        const rightSnap = $state.snapshot(rightTabMgr.closedStack)
        const focused = focusedPane
        untrack(() => {
            void import('@tauri-apps/api/event').then(({ emit }) => {
                void emit('debug-closed-tabs', { left: leftSnap, right: rightSnap, focusedPane: focused })
            })
        })
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

    async function handleSortChange(pane: 'left' | 'right', newColumn: SortColumn) {
        // Cancel any active rename on the affected pane (sort invalidates indices)
        getPaneRef(pane)?.cancelRename()
        // Re-sort changes the listing's index space; any in-flight type-to-jump
        // match would land on the wrong row.
        getPaneRef(pane)?.clearJumpState()

        const paneRef = getPaneRef(pane)
        const listingId = paneRef?.getListingId()
        if (!listingId) return

        const { sortBy, sortOrder } = getPaneSort(pane)
        const newOrder =
            newColumn === sortBy ? getNewSortOrder(newColumn, sortBy, sortOrder) : defaultSortOrders[newColumn]

        const sortState = collectSortState(paneRef)
        const result = await resortListing(
            listingId,
            newColumn,
            newOrder,
            sortState.cursorFilename,
            showHiddenFiles,
            sortState.backendSelectedIndices,
            sortState.allSelected,
            getDirectorySortMode(),
        )

        setPaneSort(pane, newColumn, newOrder)
        // Persistence (saveAppStatus sortBy + the pane's tab set) fires from the
        // single subscriber's per-pane effect, which reacts to this store change.
        applySortResult(paneRef, result, sortState.hasParent)
    }

    /** Re-sorts a single pane in-place using its current column/order but a new directorySortMode. */
    async function resortPaneWithCurrentSort(pane: 'left' | 'right') {
        const paneRef = getPaneRef(pane)
        const listingId = paneRef?.getListingId()
        if (!listingId) return

        const { sortBy, sortOrder } = getPaneSort(pane)
        const sortState = collectSortState(paneRef)
        const result = await resortListing(
            listingId,
            sortBy,
            sortOrder,
            sortState.cursorFilename,
            showHiddenFiles,
            sortState.backendSelectedIndices,
            sortState.allSelected,
            getDirectorySortMode(),
        )
        applySortResult(paneRef, result, sortState.hasParent)
    }

    // Re-sort both panes when directorySortMode setting changes
    $effect(() => {
        // Read the reactive value to establish the dependency
        void getDirectorySortMode()
        // Skip during initialization
        if (!initialized) return
        // Re-sort both panes with the new mode (untrack to avoid re-triggering)
        untrack(() => {
            void resortPaneWithCurrentSort('left')
            void resortPaneWithCurrentSort('right')
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

    function handleCancelLoading(pane: 'left' | 'right', cancelledPath: string, selectName?: string) {
        const history = getPaneHistory(pane)
        const entry = getCurrentEntry(history)
        const paneRef = getPaneRef(pane)

        if (entry.volumeId === 'network') {
            // Network restore: re-commit the network entry without leaving the
            // volume and without a load. A `'fallback'` volume "switch" to the same
            // network volume is a terminal commit (no old-path pre-save, no
            // correction); `pushHistory: false` keeps history put (the entry's
            // already current). The subscriber persists the store mutation.
            navigateIntent({
                pane,
                to: { volumeId: 'network', path: entry.path },
                source: 'fallback',
                pushHistory: false,
            })
            paneRef?.setNetworkHost(entry.networkHost ?? null)
            containerElement?.focus()
            return
        }

        if (entry.path === cancelledPath) {
            // Listing completed before cancel; history has the cancelled path pushed. Go back.
            // The history-back walk + commit lives in `navigate()`'s history arm.
            // `navigate()` re-checks `canGoBack`, so the gate here is the
            // cancel-specific guard, not a duplicate.
            if (canGoBack(history)) {
                navigateIntent({ pane, to: { history: 'back' }, source: 'cancel' })
                return
            }

            // Edge case: tab opened directly at this path, no history. Walk up to nearest valid parent.
            const parentPath = entry.path.substring(0, Math.max(1, entry.path.lastIndexOf('/')))
            const volumeRoot = volumes.find((v) => v.id === entry.volumeId)?.path
            void resolveValidPath(parentPath, { volumeRoot }).then((validPath) => {
                const target = validPath ?? '~'
                const isOutsideVolume = entry.volumeId !== 'root' && (target === '~' || target === '/')
                // Volume root unreachable ⇒ switch to root volume; otherwise stay on
                // the current volume at the resolved parent. Either way a terminal
                // `'fallback'` commit (no history push — the walk-up is a correction
                // to the cancelled destination, not a new Back target). The
                // subscriber persists the store mutation.
                navigateIntent({
                    pane,
                    to: { volumeId: isOutsideVolume ? 'root' : getPaneVolumeId(pane), path: target },
                    source: 'fallback',
                    pushHistory: false,
                })
                containerElement?.focus()
            })
            return
        }

        // Listing didn't complete; history still points at the previous folder (correct destination).
        // setPanePath won't trigger FilePane's $effect (path unchanged), so call navigateToPath directly.
        void paneRef?.navigateToPath(entry.path, selectName)
        containerElement?.focus()
    }

    async function handleMtpFatalError(pane: 'left' | 'right', errorMessage: string) {
        log.warn('{pane} pane MTP fatal error, falling back to default volume: {error}', { pane, error: errorMessage })
        const defaultVolumeId = await getDefaultVolumeId()
        const defaultVolume = volumes.find((v) => v.id === defaultVolumeId)
        const defaultPath = defaultVolume?.path ?? '~'

        // Fallback to the default volume, pushing a history entry. The subscriber
        // persists the store mutation `navigate()`'s commit makes.
        navigateIntent({ pane, to: { volumeId: defaultVolumeId, path: defaultPath }, source: 'fallback' })
    }

    async function handleRetryUnreachable(pane: 'left' | 'right') {
        const tab = getActiveTab(getTabMgr(pane))
        if (!tab.unreachable) return

        const originalPath = tab.unreachable.originalPath
        tab.unreachable = { originalPath, retrying: true }

        // Try to resolve the volume via statfs (backend has its own 2s timeout).
        // The resolve-timeout fallback to `getDefaultVolumeId` survives.
        const result = await resolvePathVolume(originalPath)

        const volumeId = result.volume ? result.volume.id : await getDefaultVolumeId()

        // Clear unreachable BEFORE navigating, then commit + refresh (ordering
        // preserved). Let FilePane try to load the directory directly: even if
        // volume resolution timed out, the directory itself may be reachable.
        tab.unreachable = null
        navigateIntent({ pane, to: { volumeId, path: originalPath }, source: 'fallback' })

        // Sync the volume selector; retry may have fixed a mount that was stale.
        requestVolumeRefresh()

        log.info('Volume retry navigating to {path} on volume {vol}', {
            path: originalPath,
            vol: volumeId,
        })
    }

    async function handleOpenHome(pane: 'left' | 'right') {
        const tab = getActiveTab(getTabMgr(pane))
        tab.unreachable = null

        const defaultId = await getDefaultVolumeId()
        const homePath = '~'
        navigateIntent({ pane, to: { volumeId: defaultId, path: homePath }, source: 'fallback' })
        log.info('Unreachable tab opened home folder for {pane} pane', { pane })
    }

    /** Routes to whichever pane has its volume chooser open. Returns true if handled. */
    function routeToVolumeChooser(e: KeyboardEvent): boolean {
        // Check if EITHER pane has a volume chooser open - if so, route events there
        // This is important because F1/F2 can open a volume chooser on the non-focused pane
        for (const side of ['left', 'right'] as const) {
            const ref = getPaneRef(side)
            if (ref?.isVolumeChooserOpen()) {
                if (ref.handleVolumeChooserKeyDown(e)) {
                    return true
                }
            }
        }
        return false
    }

    function handleEscapeDuringLoading(): boolean {
        const paneRef = getPaneRef(focusedPane)
        if (paneRef?.isLoading()) {
            paneRef.handleCancelLoading()
            return true
        }
        return false
    }

    /**
     * Prevents focus from escaping to buttons/links inside the explorer. Inputs (rename,
     * network login) and dialog content are exempt. The dialog exemption is load-bearing:
     * the rename dialogs mount INSIDE FilePane, and without it this guard yanks focus off
     * the dialog overlay while `use:trapFocus` pulls it back — an endless focus ping-pong
     * of microtasks that starves the event loop and freezes the whole webview (pinned by
     * the "rename to existing name is rejected on MTP" E2E). Focus containment inside a
     * dialog is the trap's job, not this guard's; the exemption also makes the dialogs'
     * buttons keyboard-reachable.
     */
    function handleFocusGuard(e: FocusEvent) {
        const target = e.target as HTMLElement
        if (
            target === containerElement ||
            target instanceof HTMLInputElement ||
            target instanceof HTMLTextAreaElement ||
            target instanceof HTMLSelectElement ||
            target.isContentEditable ||
            target.closest('[role="dialog"], [role="alertdialog"]') !== null
        )
            return
        containerElement?.focus()
    }

    function handleKeyDown(e: KeyboardEvent) {
        // ESC during loading = cancel and go back
        if (e.key === 'Escape' && handleEscapeDuringLoading()) {
            e.preventDefault()
            return
        }

        // Route to volume chooser if one is open
        if (routeToVolumeChooser(e)) {
            return
        }

        // Type-to-jump intercept: route printable letters/digits into the
        // active pane's buffer before any other shortcut sees them. Reset keys
        // (arrows, page nav, enter, tab, backspace, esc) clear an active buffer
        // and then fall through to their existing handlers.
        const activePaneRef = getPaneRef(focusedPane)
        if (activePaneRef && !isTypingInInput(e) && !activePaneRef.isRenaming()) {
            if (isTypeToJumpChar(e)) {
                activePaneRef.handleJumpKeystroke(e.key)
                e.preventDefault()
                e.stopPropagation()
                return
            }
            if (isTypeToJumpResetKey(e)) {
                activePaneRef.clearJumpState()
                // Fall through; Enter/arrows/Backspace/ESC keep their existing meaning.
            }
        }

        // Forward arrow keys and Enter to the focused pane
        activePaneRef?.handleKeyDown(e)
    }

    /** True if focus is in any text-entry control (rename, search dialog, login form, etc.). */
    function isTypingInInput(e: KeyboardEvent): boolean {
        const target = e.target as HTMLElement | null
        if (!target) return false
        return (
            target instanceof HTMLInputElement ||
            target instanceof HTMLTextAreaElement ||
            target instanceof HTMLSelectElement ||
            target.isContentEditable
        )
    }

    function handleKeyUp(e: KeyboardEvent) {
        // Forward to the focused pane for range selection finalization
        const activePaneRef = getPaneRef(focusedPane)
        activePaneRef?.handleKeyUp(e)
    }

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
        syncTabsToBackend()

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
        unlistenVolumeUnmount = await listen<{ volumePath: string }>('volume-unmounted', (event) => {
            const volume = volumes.find((v) => v.path === event.payload.volumePath)
            if (volume) {
                void handleVolumeUnmount(volume.id)
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
                    addToast(`Couldn't eject ${payload.volumeName}: ${getIpcErrorMessage(e)}`, { level: 'error' })
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

    async function handleVolumeUnmount(unmountedId: string) {
        const defaultVolumeId = await getDefaultVolumeId()
        // Navigate to home directory, falling back to / if home doesn't exist
        const homePath = (await pathExists('~')) ? '~' : '/'

        // Redirect each affected pane (independently — left and right) to the
        // default volume at home. `pushHistory: false` is the history-push
        // asymmetry: an unmount must NOT grow a Back target (unlike the MTP-fatal /
        // retry / open-home fallbacks, which DO push). The subscriber persists each
        // store mutation `navigate()`'s commit makes.
        for (const pane of ['left', 'right'] as const) {
            if (getPaneVolumeId(pane) === unmountedId) {
                navigateIntent({
                    pane,
                    to: { volumeId: defaultVolumeId, path: homePath },
                    source: 'fallback',
                    pushHistory: false,
                })
            }
        }

        // Volume list is now maintained reactively by the volume store
    }

    onDestroy(() => {
        unlistenSettings?.()
        unlistenVolumeUnmount?.()
        unlistenVolumeContextAction?.()
        unlistenIndexEvents?.()
        unlistenIndexAggregationComplete?.()
        if (tabSyncTimer) clearTimeout(tabSyncTimer)
        if (quickLookFollowTimer !== null) clearTimeout(quickLookFollowTimer)
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

    // Quick Look cursor-follow: while the panel is open, push the path under the
    // focused pane's cursor to the backend on every cursor move, pane switch, or
    // directory navigation. The backend silently no-ops for volumes without local-fs
    // access (MTP, virtual git portal), so no skip logic is needed here.
    //
    // Trailing-edge debounce ~100 ms: holding ArrowDown shouldn't fire `reloadData`
    // 60×/s. The generation counter (same pattern as `type-to-jump-state.svelte.ts`)
    // drops out-of-order responses if the user nav-bursts faster than IPC round-trip;
    // each scheduled call captures its generation and bails on a stale fire.
    const QUICK_LOOK_FOLLOW_DEBOUNCE_MS = 100
    let quickLookFollowGeneration = 0
    let quickLookFollowTimer: ReturnType<typeof setTimeout> | null = null
    let quickLookLastSentPath: string | undefined
    $effect(() => {
        if (!quickLookState.isOpen) {
            // Panel closed → cancel any pending dispatch and forget the last-sent path
            // so re-opening on the same entry doesn't get suppressed by the dedupe.
            if (quickLookFollowTimer !== null) {
                clearTimeout(quickLookFollowTimer)
                quickLookFollowTimer = null
            }
            quickLookLastSentPath = undefined
            return
        }
        const pane = focusedPane
        const paneRef = paneRefs[pane]
        const path = paneRef?.getPathUnderCursor()
        const volId = getPaneVolumeId(pane)
        // Bail when the pane isn't ready or the cursor isn't on a resolvable entry.
        // No path → don't reloadData with stale state; wait for the next $effect fire
        // once the entry resolves (FilePane fetches it on every cursorIndex change).
        if (!path || !volId) return
        const generation = ++quickLookFollowGeneration
        if (quickLookFollowTimer !== null) clearTimeout(quickLookFollowTimer)
        quickLookFollowTimer = setTimeout(() => {
            quickLookFollowTimer = null
            // Stale-generation check: a newer cursor move bumped the generation
            // while this timer was waiting. Drop this fire — the newer one will
            // schedule its own.
            if (generation !== quickLookFollowGeneration) return
            // Panel could have closed during the debounce window. Skip the IPC.
            if (!quickLookState.isOpen) return
            // Skip if the path hasn't actually changed since the last dispatch:
            // a focused-pane setCursorIndex during nav can fire the $effect with
            // the same entry resolved (debounced re-fetch lands later). Cheap to
            // dedupe; backend's `reloadData` is fine but the round-trip isn't free.
            if (path === quickLookLastSentPath) return
            quickLookLastSentPath = path
            void quickLookSetPath(path, volId).catch((e: unknown) => {
                log.warn('quickLookSetPath failed: {error}', { error: String(e) })
            })
        }, QUICK_LOOK_FOLLOW_DEBOUNCE_MS)
    })

    // Quick Look error-state close: when the focused pane transitions into
    // an error state (volume unmounted, listing failed) while the panel is open,
    // dismiss the panel. Sitting on a stale path while the underlying volume is
    // gone is worse UX than just closing — and the cursor-follow effect above
    // would otherwise be stuck on the last-known path until the user moves
    // focus or navigates somewhere reachable.
    $effect(() => {
        if (!quickLookState.isOpen) return
        const paneRef = paneRefs[focusedPane]
        // `isInErrorState` reads two `$state` fields under the hood
        // (`friendlyError`, `unreachable`), so Svelte tracks them through this
        // call and we re-run when either flips. Don't bother destructuring —
        // the call site is the dependency.
        if (paneRef?.isInErrorState()) {
            closeFromPaneError()
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

    /** Returns true if pane swap is safe (both panes ready, no dialogs open). */
    function canSwapPanes(): boolean {
        const leftRef = getPaneRef('left')
        const rightRef = getPaneRef('right')
        if (!leftRef || !rightRef) return false
        if (leftRef.isLoading() || rightRef.isLoading()) return false
        return !dialogs.isAnyTransferDialogOpen()
    }

    /** Swaps all active tab state between left and right panes. */
    function swapDualPaneState(): void {
        const leftTab = getActiveTab(leftTabMgr)
        const rightTab = getActiveTab(rightTabMgr)

        ;[leftTab.path, rightTab.path] = [rightTab.path, leftTab.path]
        ;[leftTab.volumeId, rightTab.volumeId] = [rightTab.volumeId, leftTab.volumeId]
        ;[leftTab.history, rightTab.history] = [rightTab.history, leftTab.history]
        ;[leftTab.viewMode, rightTab.viewMode] = [rightTab.viewMode, leftTab.viewMode]
        ;[leftTab.sortBy, rightTab.sortBy] = [rightTab.sortBy, leftTab.sortBy]
        ;[leftTab.sortOrder, rightTab.sortOrder] = [rightTab.sortOrder, leftTab.sortOrder]
    }

    /**
     * Swap left and right panes entirely (paths, volumes, history, sort, view mode, listing state).
     * Zero backend calls: we just swap listing ownership on the frontend.
     */
    export function swapPanes(): void {
        if (!canSwapPanes()) return

        const leftRef = getPaneRef('left')
        const rightRef = getPaneRef('right')
        if (!leftRef || !rightRef) return

        // 1. Snapshot both panes' listing state
        const leftSwap = leftRef.getSwapState()
        const rightSwap = rightRef.getSwapState()

        // 2. Swap DualPaneExplorer state variables
        swapDualPaneState()

        // 3. Each pane adopts the other's listing (no backend calls)
        leftRef.adoptListing(rightSwap)
        rightRef.adoptListing(leftSwap)

        // 4. Persistence (both panes' app-status fields + tab sets) fires from the
        // single subscriber: the swap mutates each pane's active-tab nav-state, so
        // both per-pane effects re-run.
        containerElement?.focus()
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
        void handleSortChange(pane ?? focusedPane, column)
    }

    /**
     * Set sort order for a specific pane (or focused pane if not specified).
     * Used by command palette.
     */
    export function setSortOrder(order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') {
        const targetPane = pane ?? focusedPane
        const { sortOrder: currentOrder, sortBy: currentColumn } = getPaneSort(targetPane)

        let newOrder: SortOrder
        if (order === 'toggle') {
            newOrder = currentOrder === 'ascending' ? 'descending' : 'ascending'
        } else {
            newOrder = order === 'asc' ? 'ascending' : 'descending'
        }

        // Re-apply sort with new order by pretending to click same column
        // This triggers the toggle logic in the handler
        if (newOrder !== currentOrder) {
            void handleSortChange(targetPane, currentColumn)
        }
    }

    /**
     * Set both sort column and order atomically for a specific pane.
     * Used by MCP sort command to avoid race conditions.
     */
    export async function setSort(column: SortColumn, order: 'asc' | 'desc', pane: 'left' | 'right') {
        const paneRef = getPaneRef(pane)
        const listingId = paneRef?.getListingId()
        if (!listingId) return

        const newOrder: SortOrder = order === 'asc' ? 'ascending' : 'descending'

        const sortState = collectSortState(paneRef)
        const result = await resortListing(
            listingId,
            column,
            newOrder,
            sortState.cursorFilename,
            showHiddenFiles,
            sortState.backendSelectedIndices,
            sortState.allSelected,
            getDirectorySortMode(),
        )

        setPaneSort(pane, column, newOrder)
        // Sort persistence (app-status sortBy + tab set) fires from the subscriber.
        applySortResult(paneRef, result, sortState.hasParent)
    }

    export function getFocusedPane(): 'left' | 'right' {
        return paneCommands.getFocusedPane()
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

    /**
     * Select a volume by index for a specific pane.
     * Matches the behavior of VolumeBreadcrumb's handleVolumeSelect.
     * @param pane - 'left' or 'right'
     * @param index - Zero-based index into the volumes array
     */
    async function selectVolumeByIndex(pane: 'left' | 'right', index: number): Promise<boolean> {
        if (index < 0 || index >= volumes.length) {
            log.warn('Invalid volume index: {index} (valid range: 0-{max})', { index, max: volumes.length - 1 })
            return false
        }

        const volume = volumes[index]

        // Handle favorites differently from actual volumes (same as VolumeBreadcrumb).
        // `navigate({ source: 'user' })`'s volume-switch arm shifts STORE focus to the
        // pane (matching the old `handleVolumeChange`), but does NOT re-anchor DOM focus
        // on the container — doing so drops a Space press during the multi-select-then-
        // delete sequence (regression guard: mtp.spec.ts:414).
        if (volume.category === 'favorite') {
            // For favorites, navigate to the favorite's path on its containing volume.
            const { volume: containingVolume } = await resolvePathVolume(volume.path)
            const volumeId = containingVolume?.id ?? 'root'
            navigateIntent({ pane, to: { volumeId, path: volume.path }, source: 'user' })
        } else {
            // For actual volumes, navigate to the volume's root.
            navigateIntent({ pane, to: { volumeId: volume.id, path: volume.path }, source: 'user' })
        }

        return true
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
     */
    export async function moveCursor(pane: 'left' | 'right', to: number | string) {
        explorerState.setFocusedPane(pane)
        const paneRef = getPaneRef(pane)
        if (!paneRef) return

        // Wait for the pane's current load (if any) to settle before touching
        // the listing. Without this, an MCP-driven `move_cursor` that lands
        // mid-navigation reads the FE's freshly-assigned `listingId` while the
        // backend's `LISTING_CACHE` insert is still in flight, surfacing as
        // "Listing not found" from `find_file_index`.
        await paneRef.whenLoadSettles()

        if (typeof to === 'number') {
            await paneRef.setCursorIndex(to)
        } else {
            await paneCommands.moveCursorByName(paneRef, to)
        }
        // MCP-driven cursor placement: re-anchor DOM focus on the explorer container
        // so the next keystroke (the agent often follows move_cursor with a shortcut)
        // lands in the right dispatcher chain. Also makes the awaited completion
        // genuine; `void` swallowed the cursor-set promise and let MCP report `OK`
        // before the cursor was observably positioned.
        containerElement?.focus()
    }

    export function scrollTo(pane: 'left' | 'right', index: number) {
        paneCommands.scrollTo(pane, index)
    }

    /**
     * Select a volume by name for a specific pane.
     * Used by MCP select_volume tool.
     */
    export async function selectVolumeByName(pane: 'left' | 'right', name: string): Promise<boolean> {
        // "Network" is a virtual volume not in the volumes list
        if (name === 'Network') {
            navigateIntent({ pane, to: { volumeId: 'network', path: 'smb://' }, source: 'user' })
            return true
        }

        const index = volumes.findIndex((v) => v.name === name)
        if (index !== -1) {
            return selectVolumeByIndex(pane, index)
        }

        log.warn('Volume not found: {name}', { name })
        return false
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
        if (source === target) return
        const sourcePaneRef = getPaneRef(source)
        if (!sourcePaneRef) return

        const sourceVolumeId = getPaneVolumeId(source)
        const sourcePath = getPanePath(source)
        const sourceHistoryEntry = getCurrentEntry(getPaneHistory(source))
        const sourceHost = sourceHistoryEntry.networkHost ?? null
        const sourceFocused = focusedPane === source

        // Normal listing on the source: cursor-on-folder refines the path.
        if (sourceVolumeId !== 'network') {
            let destPath = sourcePath
            if (sourceFocused) {
                const entry = sourcePaneRef.getCursorEntry()
                if (entry && entry.isDirectory && entry.name !== '..') {
                    destPath = entry.path
                }
            }
            mirrorLocalStateToPane(target, sourceVolumeId, destPath)
            return
        }

        // Source is on the network volume (host list or share list).
        let destHost: NetworkHost | null = sourceHost
        let destAutoMountShare: string | undefined
        if (sourceFocused) {
            const cursor = sourcePaneRef.getNetworkCursorEntry()
            if (cursor?.kind === 'host') {
                destHost = cursor.host
            } else if (cursor?.kind === 'share' && sourceHost) {
                destAutoMountShare = cursor.share.name
            }
        }
        mirrorNetworkStateToPane(target, destHost, destAutoMountShare)
    }

    /** Helper: mirror a {volumeId, path} state to a target pane without shifting focus. */
    function mirrorLocalStateToPane(target: 'left' | 'right', volumeId: string, path: string): void {
        const originalFocused = focusedPane
        const targetVolumeId = getPaneVolumeId(target)
        if (targetVolumeId !== volumeId) {
            // `source: 'mirror'` keeps focus on the source pane (no focus shift, L1).
            navigateIntent({ pane: target, to: { volumeId, path }, source: 'mirror' })
        } else if (getPanePath(target) === path) {
            // Already on the same volume and path; nothing to do.
        } else {
            // Same volume, different path: in-place nav (commit lands at listing-complete).
            navigateIntent({ pane: target, to: { path }, source: 'mirror' })
        }
        restoreFocus(originalFocused)
    }

    /** Helper: mirror a network state ({host, autoMountShare}) to a target pane without shifting focus. */
    function mirrorNetworkStateToPane(
        target: 'left' | 'right',
        host: NetworkHost | null,
        autoMountShare: string | undefined,
    ): void {
        const originalFocused = focusedPane
        const targetPaneRef = getPaneRef(target)
        if (getPaneVolumeId(target) !== 'network') {
            navigateIntent({ pane: target, to: { volumeId: 'network', path: 'smb://' }, source: 'mirror' })
        }
        targetPaneRef?.setNetworkHost(host)
        setPaneHistory(
            target,
            pushHistoryEntry(getPaneHistory(target), {
                volumeId: 'network',
                path: 'smb://',
                networkHost: host ?? undefined,
            }),
        )
        targetPaneRef?.setNetworkAutoMount(autoMountShare)
        restoreFocus(originalFocused)
    }

    /** Restore focus to a pane after a target-pane state change so the user keeps working where they were. */
    function restoreFocus(pane: 'left' | 'right'): void {
        if (focusedPane !== pane) {
            explorerState.setFocusedPane(pane)
            // focusedPane persistence fires from the subscriber's focus effect.
        }
    }

    export function refreshPane() {
        paneCommands.refreshPane()
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
        const identity = recordedIdentity
            ? { ...recordedIdentity, startedAt: Date.now() }
            : undefined
        void dragDrop.handleFileDrop(paths, targetPane, targetFolderPath, operation, identity)
    }

    /** Refresh network hosts in the focused pane (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        paneCommands.refreshNetworkHosts()
    }

    export function handleMcpSelect(pane: 'left' | 'right', start: number, count: number | 'all', mode: McpSelectMode) {
        paneCommands.handleMcpSelect(pane, start, count, mode)
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
        tabOpsHandleNewTab(pane, focusedPane, (p) => { explorerState.setFocusedPane(p); }, newTab)
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
    export function handleMcpTabAction(
        pane: 'left' | 'right',
        action: McpTabAction,
        tabId?: string,
        pinned?: boolean,
    ) {
        const mgr = getTabMgr(pane)
        const mcpTabHandlers: Record<McpTabAction, () => void> = {
            new: () => {
                if (!tabOpsNewTab(pane, getTabMgr, (h) => $state.snapshot(h))) {
                    log.warn(`MCP tab new: tab limit reached in ${pane} pane`)
                }
            },
            close: () => {
                if (getTabCount(mgr) <= 1) {
                    log.warn(`MCP tab close: can't close last tab in ${pane} pane`)
                    return
                }
                closeTabRecording(mgr, tabId ?? mgr.activeTabId, getClosedTabsCap())
                saveTabsForPaneSide(pane)
                if (pane === focusedPane) syncPinTabMenu()
                if (pane === focusedPane) syncReopenMenuState()
            },
            close_others: () => {
                closeOtherTabsRecording(mgr, tabId ?? mgr.activeTabId, getClosedTabsCap())
                saveTabsForPaneSide(pane)
                if (pane === focusedPane) syncReopenMenuState()
            },
            reopen: () => {
                if (pane === focusedPane) {
                    // Cheap path: existing helper handles the focused pane.
                    reopenLastClosedTab()
                    return
                }
                // For non-focused panes, call the tab-operations helper with the target pane.
                tabOpsReopenLastClosedTab(pane, getTabMgr)
            },
            activate: () => {
                if (tabId) switchToTab(pane, tabId)
            },
            set_pinned: () => {
                const pinId = tabId ?? mgr.activeTabId
                const tab = getAllTabs(mgr).find((t) => t.id === pinId)
                if (!tab) return
                if (pinned && !tab.pinned) pinTab(mgr, pinId)
                else if (!pinned && tab.pinned) unpinTab(mgr, pinId)
                saveTabsForPaneSide(pane)
                if (pane === focusedPane && pinId === mgr.activeTabId) syncPinTabMenu()
            },
        }
        mcpTabHandlers[action]()
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
                    navigateIntent({ pane: paneId, to: { volumeId, path: targetPath }, source: 'user' })
                }}
                onRequestFocus={() => {
                    handleFocus(paneId)
                }}
                onSortChange={(column: SortColumn) => handleSortChange(paneId, column)}
                onNetworkHostChange={(host: NetworkHost | null) => {
                    handleNetworkHostChange(paneId, host)
                }}
                onCancelLoading={(cancelledPath: string, selectName?: string) => {
                    handleCancelLoading(paneId, cancelledPath, selectName)
                }}
                onMtpFatalError={(msg: string) => handleMtpFatalError(paneId, msg)}
                unreachable={getActiveTab(tabMgr).unreachable}
                onRetryUnreachable={() => handleRetryUnreachable(paneId)}
                onOpenHome={() => handleOpenHome(paneId)}
                {onCommand}
            />
        {/key}
    </div>
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_tabindex,a11y_no_noninteractive_element_interactions -->
<div
    class="dual-pane-explorer"
    bind:this={containerElement}
    onfocusin={handleFocusGuard}
    onkeydown={handleKeyDown}
    onkeyup={handleKeyUp}
    tabindex="0"
    role="application"
    aria-label="File explorer"
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
    onTransferError={(error: WriteOperationError, friendly?: FriendlyError) => {
        dialogs.handleTransferError(error, friendly)
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
