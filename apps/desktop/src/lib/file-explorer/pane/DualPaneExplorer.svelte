<script lang="ts">
    import { onMount, onDestroy, untrack } from 'svelte'
    import FilePane from './FilePane.svelte'
    import PaneResizer from './PaneResizer.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import DialogManager from './DialogManager.svelte'
    import { toBackendCursorIndex, toBackendIndices } from '$lib/file-operations/transfer/transfer-dialog-utils'
    import { getFileAt } from '$lib/tauri-commands'
    import {
        loadAppStatus,
        saveAppStatus,
        loadPaneTabs,
        saveLastUsedPathForVolume,
        type ViewMode,
    } from '$lib/app-status-store'
    import { loadSettings, saveSettings, subscribeToSettingsChanges } from '$lib/settings-store'
    import {
        pathExists,
        listen,
        listVolumes,
        getDefaultVolumeId,
        findContainingVolume,
        resortListing,
        DEFAULT_VOLUME_ID,
        type UnlistenFn,
        updateFocusedPane,
        updatePaneTabs,
        findFileIndex,
        getE2eStartPath,
        copyFilesToClipboard,
        cutFilesToClipboard,
        readClipboardFiles,
        clearClipboardCutState,
    } from '$lib/tauri-commands'
    import type {
        VolumeInfo,
        SortColumn,
        SortOrder,
        NetworkHost,
        ConflictResolution,
        WriteOperationError,
    } from '../types'
    import { defaultSortOrders } from '../types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'
    import { determineNavigationPath, resolveValidPath } from '../navigation/path-navigation'
    import { withTimeout } from '$lib/utils/timing'
    import {
        createHistory,
        push,
        pushPath,
        back,
        forward,
        getCurrentEntry,
        canGoBack,
        canGoForward,
        type NavigationHistory,
    } from '../navigation/navigation-history'
    import TabBar from '../tabs/TabBar.svelte'
    import {
        createTabManager,
        getActiveTab,
        getAllTabs,
        MAX_TABS_PER_PANE,
        pinTab,
        unpinTab,
        type TabManager,
    } from '../tabs/tab-state-manager.svelte'
    import type { TabState, TabId, PersistedTab, PersistedPaneTabs } from '../tabs/tab-types'
    import {
        createInitialTabState,
        createTabManagerFromPersisted,
        saveTabsForPane,
        handleTabClose as tabOpsHandleTabClose,
        handleTabMiddleClick as tabOpsHandleTabMiddleClick,
        handleTabContextMenu as tabOpsHandleTabContextMenu,
        handleNewTab as tabOpsHandleNewTab,
        newTab as tabOpsNewTab,
        closeActiveTab as tabOpsCloseActiveTab,
        closeActiveTabWithConfirmation as tabOpsCloseActiveTabWithConfirmation,
        togglePinActiveTab as tabOpsTogglePinActiveTab,
        closeOtherTabsInFocusedPane as tabOpsCloseOtherTabs,
        syncPinTabMenuForPane,
        cycleTab as tabOpsCycleTab,
        switchToTab as tabOpsSwitchToTab,
        getTabsForPane as tabOpsGetTabsForPane,
    } from './tab-operations'
    import { initNetworkDiscovery, cleanupNetworkDiscovery } from '../network/network-store.svelte'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import { getAppLogger } from '$lib/logging/logger'
    import { getMtpVolumes } from '$lib/mtp'
    import { getNewSortOrder, applySortResult, collectSortState } from './sorting-handlers'
    import {
        type TransferContext,
        buildTransferPropsFromSelection,
        buildTransferPropsFromCursor,
        buildTransferPropsFromDroppedPaths,
        getDestinationVolumeInfo,
        getCommonParentPath,
    } from './transfer-operations'
    import type { TransferOperationType } from '../types'
    import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
    import { getInitialFolderName } from '$lib/file-operations/mkdir/new-folder-operations'
    import { createDialogState } from './dialog-state.svelte'
    import { getCurrentWebview } from '@tauri-apps/api/webview'
    import { recalculateWebviewOffset, toViewportPosition } from '../drag/drag-position'
    import {
        getIsDraggingFromSelf,
        resetDraggingFromSelf,
        matchesSelfDragFingerprint,
        markAsSelfDrag,
        storeSelfDragFingerprint,
        clearSelfDragFingerprint,
        getSelfDragFileInfos,
        endSelfDragSession,
    } from '../drag/drag-drop'
    import { initIndexEvents } from '$lib/indexing/index'
    import { getDirectorySortMode } from '$lib/settings/reactive-settings.svelte'
    import { resolveDropTarget } from '../drag/drop-target-hit-testing'
    import DragOverlay from '../drag/DragOverlay.svelte'
    import { showOverlay, updateOverlay, hideOverlay, type OverlayFileInfo } from '../drag/drag-overlay.svelte.js'
    import { getCachedIcon } from '$lib/icon-cache'
    import {
        startModifierTracking,
        stopModifierTracking,
        getIsAltHeld,
        setAltHeld,
    } from '../modifier-key-tracker.svelte'
    import { addToast } from '$lib/ui/toast'

    const log = getAppLogger('fileExplorer')

    function saveTabsForPaneSide(pane: 'left' | 'right') {
        saveTabsForPane(pane, getTabMgr)
    }

    let leftTabMgr = $state<TabManager>(createTabManager(createInitialTabState('~', DEFAULT_VOLUME_ID)))
    let rightTabMgr = $state<TabManager>(createTabManager(createInitialTabState('~', DEFAULT_VOLUME_ID)))

    // Derived active tab state — these replace the old scalar variables
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

    let focusedPane = $state<'left' | 'right'>('left')
    let showHiddenFiles = $state(true)
    let volumes = $state<VolumeInfo[]>([])
    let initialized = $state(false)
    let leftPaneWidthPercent = $state(50)

    let containerElement: HTMLDivElement | undefined = $state()
    const paneRefs = $state<Record<'left' | 'right', FilePane | undefined>>({ left: undefined, right: undefined })
    let unlistenSettings: UnlistenFn | undefined
    let unlistenViewMode: UnlistenFn | undefined
    let unlistenVolumeMount: UnlistenFn | undefined
    let unlistenVolumeUnmount: UnlistenFn | undefined
    let unlistenDragDrop: UnlistenFn | undefined
    let unlistenDragImageSize: UnlistenFn | undefined
    let unlistenDragModifiers: UnlistenFn | undefined
    let unlistenIndexEvents: UnlistenFn | undefined
    let unlistenIndexScanComplete: UnlistenFn | undefined
    let unlistenMcpActivateTab: UnlistenFn | undefined
    let unlistenMcpPinTab: UnlistenFn | undefined

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

    // Drag image size from the source app (macOS only, via swizzle).
    // If the source provides a large preview (like Finder), we suppress our overlay.
    const smallDragImageThreshold = 32
    let externalDragHasLargeImage = false

    // Drop target highlight state: which pane (if any) is the active drop target
    let dropTargetPane = $state<'left' | 'right' | null>(null)

    // Folder-level drop target state: when hovering over a directory row
    let dropTargetFolderPath = $state<string | null>(null)
    let dropTargetFolderEl = $state<HTMLElement | null>(null)

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
        getShowHiddenFiles: () => showHiddenFiles,
        onRefocus: () => containerElement?.focus(),
    })

    // Guards against stale background path corrections from determineNavigationPath.
    // Each handleVolumeChange increments this; the background callback checks its captured
    // generation still matches before applying a correction.
    let volumeChangeGeneration = 0

    // --- Pane accessor helpers ---

    function getPaneRef(pane: 'left' | 'right'): FilePane | undefined {
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
        return pane === 'left' ? leftTabMgr : rightTabMgr
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

    /** Builds a save-status key like 'leftPath' or 'rightVolumeId' from pane and field name. */
    function paneKey(pane: 'left' | 'right', field: string): string {
        return `${pane}${field.charAt(0).toUpperCase()}${field.slice(1)}`
    }

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

    function handlePathChange(pane: 'left' | 'right', path: string) {
        const mgr = getTabMgr(pane)
        const activeTab = getActiveTab(mgr)

        // Pinned tab: open a new tab with the target path instead of navigating in-place
        if (activeTab.pinned && path !== activeTab.path) {
            if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
                addToast('Tab limit reached')
                applyPathChange(pane, path)
                return
            }

            const newTab: TabState = {
                id: crypto.randomUUID(),
                path,
                volumeId: activeTab.volumeId,
                history: createHistory(activeTab.volumeId, path),
                sortBy: activeTab.sortBy,
                sortOrder: activeTab.sortOrder,
                viewMode: activeTab.viewMode,
                pinned: false,
                cursorFilename: null,
                unreachable: null,
            }

            const activeIndex = mgr.tabs.findIndex((t) => t.id === activeTab.id)
            mgr.tabs.splice(activeIndex + 1, 0, newTab)
            mgr.activeTabId = newTab.id

            saveTabsForPaneSide(pane)
            saveAppStatus({ [paneKey(pane, 'path')]: path })
            void saveLastUsedPathForVolume(activeTab.volumeId, path)
            return
        }

        applyPathChange(pane, path)
    }

    /** Applies a path change to the active tab in-place (the normal non-pinned flow). */
    function applyPathChange(pane: 'left' | 'right', path: string) {
        setPanePath(pane, path)
        setPaneHistory(pane, pushPath(getPaneHistory(pane), path))
        saveAppStatus({ [paneKey(pane, 'path')]: path })
        void saveLastUsedPathForVolume(getPaneVolumeId(pane), path)
        saveTabsForPaneSide(pane)

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
        await moveCursorByNameInFileListing(paneRef, filename)
    }

    function handleNetworkHostChange(pane: 'left' | 'right', host: NetworkHost | null) {
        setPaneHistory(
            pane,
            push(getPaneHistory(pane), {
                volumeId: 'network',
                path: 'smb://',
                networkHost: host ?? undefined,
            }),
        )
    }

    async function handleSortChange(pane: 'left' | 'right', newColumn: SortColumn) {
        // Cancel any active rename on the affected pane (sort invalidates indices)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef(pane)?.cancelRename?.()

        const paneRef = getPaneRef(pane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = paneRef?.getListingId?.() as string | undefined
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
        saveAppStatus({ [paneKey(pane, 'sortBy')]: newColumn })
        saveTabsForPaneSide(pane)
        applySortResult(paneRef, result, sortState.hasParent)
    }

    /** Re-sorts a single pane in-place using its current column/order but a new directorySortMode. */
    async function resortPaneWithCurrentSort(pane: 'left' | 'right') {
        const paneRef = getPaneRef(pane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = paneRef?.getListingId?.() as string | undefined
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

    async function handleVolumeChange(
        pane: 'left' | 'right',
        volumeId: string,
        volumePath: string,
        targetPath: string,
    ) {
        const mgr = getTabMgr(pane)
        const activeTab = getActiveTab(mgr)
        const oldPath = activeTab.path
        void saveLastUsedPathForVolume(activeTab.volumeId, oldPath)

        if (!volumes.find((v) => v.id === volumeId)) {
            volumes = (await listVolumes()).data
        }

        // Pinned tab: open a new tab with the target volume instead of navigating in-place
        if (activeTab.pinned && (volumeId !== activeTab.volumeId || targetPath !== activeTab.path)) {
            if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
                addToast('Tab limit reached')
            } else {
                const newTab: TabState = {
                    id: crypto.randomUUID(),
                    path: targetPath,
                    volumeId,
                    history: createHistory(volumeId, targetPath),
                    sortBy: activeTab.sortBy,
                    sortOrder: activeTab.sortOrder,
                    viewMode: activeTab.viewMode,
                    pinned: false,
                    cursorFilename: null,
                    unreachable: null,
                }

                const activeIndex = mgr.tabs.findIndex((t) => t.id === activeTab.id)
                mgr.tabs.splice(activeIndex + 1, 0, newTab)
                mgr.activeTabId = newTab.id

                saveTabsForPaneSide(pane)
                focusedPane = pane
                saveAppStatus({
                    [paneKey(pane, 'volumeId')]: volumeId,
                    [paneKey(pane, 'path')]: targetPath,
                    focusedPane: pane,
                })
                // Background path correction applies to the new active tab
                applyVolumePathCorrection(pane, volumeId, volumePath, targetPath)
                return
            }
        }

        // In-place navigation (normal flow or pinned tab at cap)
        setPaneVolumeId(pane, volumeId)
        setPanePath(pane, targetPath)
        setPaneHistory(pane, push(getPaneHistory(pane), { volumeId, path: targetPath }))
        focusedPane = pane

        saveAppStatus({
            [paneKey(pane, 'volumeId')]: volumeId,
            [paneKey(pane, 'path')]: targetPath,
            focusedPane: pane,
        })
        saveTabsForPaneSide(pane)

        applyVolumePathCorrection(pane, volumeId, volumePath, targetPath)
    }

    /** Resolves the "best" path for a volume change in the background; corrects the active tab if needed. */
    function applyVolumePathCorrection(
        pane: 'left' | 'right',
        volumeId: string,
        volumePath: string,
        targetPath: string,
    ) {
        const generation = ++volumeChangeGeneration
        const other = otherPane(pane)
        void determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: getPaneVolumeId(other),
            otherPanePath: getPanePath(other),
        }).then((betterPath) => {
            if (generation !== volumeChangeGeneration) return
            if (betterPath !== targetPath && betterPath !== getPanePath(pane)) {
                setPanePath(pane, betterPath)
                setPaneHistory(pane, push(getPaneHistory(pane), { volumeId, path: betterPath }))
                saveAppStatus({ [paneKey(pane, 'path')]: betterPath })
                saveTabsForPaneSide(pane)
            }
        })
    }

    function handleFocus(pane: 'left' | 'right') {
        if (focusedPane !== pane) {
            focusedPane = pane
            saveAppStatus({ focusedPane: pane })
            void updateFocusedPane(pane)
            syncPinTabMenu()
        }
        // Always restore DOM focus (needed after inline rename or dialog close within a pane)
        containerElement?.focus()
    }

    function handleCancelLoading(pane: 'left' | 'right', cancelledPath: string, selectName?: string) {
        const history = getPaneHistory(pane)
        const entry = getCurrentEntry(history)
        const paneRef = getPaneRef(pane)

        if (entry.volumeId === 'network') {
            setPanePath(pane, entry.path)
            setPaneVolumeId(pane, 'network')
            saveAppStatus({ [paneKey(pane, 'volumeId')]: 'network', [paneKey(pane, 'path')]: entry.path })
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef?.setNetworkHost?.(entry.networkHost ?? null)
            saveTabsForPaneSide(pane)
            containerElement?.focus()
            return
        }

        if (entry.path === cancelledPath) {
            // Listing completed before cancel — history has the cancelled path pushed. Go back.
            if (canGoBack(history)) {
                const newHistory = back(history)
                const target = getCurrentEntry(newHistory)
                updatePaneAfterHistoryNavigation(pane, newHistory, target.path)
                return
            }

            // Edge case: tab opened directly at this path, no history. Walk up to nearest valid parent.
            const parentPath = entry.path.substring(0, Math.max(1, entry.path.lastIndexOf('/')))
            void resolveValidPath(parentPath).then((validPath) => {
                const target = validPath ?? '~'
                setPanePath(pane, target)
                saveAppStatus({ [paneKey(pane, 'path')]: target })
                saveTabsForPaneSide(pane)
                containerElement?.focus()
            })
            return
        }

        // Listing didn't complete — history still points at the previous folder (correct destination).
        // setPanePath won't trigger FilePane's $effect (path unchanged), so call navigateToPath directly.
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.navigateToPath?.(entry.path, selectName)
        containerElement?.focus()
    }

    async function handleMtpFatalError(pane: 'left' | 'right', errorMessage: string) {
        log.warn('{pane} pane MTP fatal error, falling back to default volume: {error}', { pane, error: errorMessage })
        const defaultVolumeId = await getDefaultVolumeId()
        const defaultVolume = volumes.find((v) => v.id === defaultVolumeId)
        const defaultPath = defaultVolume?.path ?? '~'

        setPaneVolumeId(pane, defaultVolumeId)
        setPanePath(pane, defaultPath)
        setPaneHistory(pane, push(getPaneHistory(pane), { volumeId: defaultVolumeId, path: defaultPath }))
        saveAppStatus({ [paneKey(pane, 'volumeId')]: defaultVolumeId, [paneKey(pane, 'path')]: defaultPath })
        saveTabsForPaneSide(pane)
    }

    async function handleRetryUnreachable(pane: 'left' | 'right') {
        const tab = getActiveTab(getTabMgr(pane))
        if (!tab.unreachable) return

        const originalPath = tab.unreachable.originalPath
        tab.unreachable = { originalPath, retrying: true }

        // Try to resolve the volume, but don't block on it — navigate directly if it times out
        const volumeResolveTimeoutMs = 3000
        const result = await withTimeout(findContainingVolume(originalPath), volumeResolveTimeoutMs, null)

        const resolved = result !== null && !result.timedOut && result.data !== null
        const volumeId = resolved && result.data ? result.data.id : await getDefaultVolumeId()

        // Clear unreachable and navigate — let FilePane try to load the directory directly.
        // Even if volume resolution timed out, the directory itself may be reachable.
        tab.unreachable = null
        setPaneVolumeId(pane, volumeId)
        setPanePath(pane, originalPath)
        setPaneHistory(pane, push(getPaneHistory(pane), { volumeId, path: originalPath }))
        saveTabsForPaneSide(pane)
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
        setPaneVolumeId(pane, defaultId)
        setPanePath(pane, homePath)
        setPaneHistory(pane, push(getPaneHistory(pane), { volumeId: defaultId, path: homePath }))
        saveTabsForPaneSide(pane)
        log.info('Unreachable tab opened home folder for {pane} pane', { pane })
    }

    /** Routes to whichever pane has its volume chooser open. Returns true if handled. */
    function routeToVolumeChooser(e: KeyboardEvent): boolean {
        // Check if EITHER pane has a volume chooser open - if so, route events there
        // This is important because F1/F2 can open a volume chooser on the non-focused pane
        for (const side of ['left', 'right'] as const) {
            const ref = getPaneRef(side)
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            if (ref?.isVolumeChooserOpen?.()) {
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                if (ref.handleVolumeChooserKeyDown?.(e)) {
                    return true
                }
            }
        }
        return false
    }

    function handleEscapeDuringLoading(): boolean {
        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (paneRef?.isLoading?.()) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef.handleCancelLoading?.()
            return true
        }
        return false
    }

    /** Prevents focus from escaping to buttons/links inside the explorer. Only inputs (rename, network login) are allowed. */
    function handleFocusGuard(e: FocusEvent) {
        const target = e.target as HTMLElement
        if (
            target === containerElement ||
            target instanceof HTMLInputElement ||
            target instanceof HTMLTextAreaElement ||
            target instanceof HTMLSelectElement ||
            target.isContentEditable
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

        // Forward arrow keys and Enter to the focused pane
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion -- TypeScript thinks FilePane methods are unused without this
        const activePaneRef = getPaneRef(focusedPane) as FilePane | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        activePaneRef?.handleKeyDown(e)
    }

    function handleKeyUp(e: KeyboardEvent) {
        // Forward to the focused pane for range selection finalization
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion -- TypeScript thinks FilePane methods are unused without this
        const activePaneRef = getPaneRef(focusedPane) as FilePane | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        activePaneRef?.handleKeyUp(e)
    }

    /** Handles a file drop onto a target pane by opening the transfer confirmation dialog. */
    function handleFileDrop(
        paths: string[],
        targetPane: 'left' | 'right',
        targetFolderPath?: string,
        operation: TransferOperationType = 'copy',
    ) {
        if (paths.length === 0) return

        const { sortBy, sortOrder } = getPaneSort(targetPane)
        const destPath = targetFolderPath ?? getPanePath(targetPane)
        const destVolId = getPaneVolumeId(targetPane)

        dialogs.showTransfer({
            ...buildTransferPropsFromDroppedPaths(operation, paths, destPath, targetPane, destVolId, sortBy, sortOrder),
            allowOperationToggle: true,
        })
    }

    /** Extracts the last path component as a display name. */
    function extractFolderName(path: string): string {
        const segments = path.split('/')
        return segments[segments.length - 1] || path
    }

    /** Builds overlay file infos from drag paths, using self-drag data when available for proper icons. */
    function buildOverlayFileInfos(paths: string[]): OverlayFileInfo[] {
        // For self-drags, use stored file infos with proper icon IDs
        const selfInfos = getIsDraggingFromSelf() ? getSelfDragFileInfos() : null
        if (selfInfos && selfInfos.length > 0) {
            return selfInfos.map((info) => ({
                name: info.name,
                iconUrl: getCachedIcon(info.iconId),
                isDirectory: info.isDirectory,
            }))
        }

        // For external drags, extract names and try extension-based icon lookup
        return paths.slice(0, 20).map((p) => {
            const name = p.split('/').pop() || p
            const ext = name.includes('.') ? name.split('.').pop() || '' : ''
            const iconUrl = ext ? getCachedIcon(`ext:${ext}`) : undefined
            return { name, iconUrl, isDirectory: false }
        })
    }

    /** Resolves the target display name for the overlay action line. */
    function resolveTargetDisplayName(
        resolved: ReturnType<typeof resolveDropTarget>,
        folderPath: string | null,
    ): string | null {
        if (!resolved) return null
        if (resolved.type === 'folder' && folderPath) {
            return extractFolderName(folderPath)
        }
        if (resolved.type === 'pane') {
            return extractFolderName(getPanePath(resolved.paneId))
        }
        return null
    }

    /** Called on drag enter to initialize the overlay with file infos. */
    function handleDragEnter(paths: string[], position: { x: number; y: number }) {
        // Skip the overlay when an external drag has a large source image (like Finder's preview).
        // Self-drags always show the overlay (the OS drag image is transparent inside the window).
        const suppressOverlay = externalDragHasLargeImage && !getIsDraggingFromSelf()
        if (!suppressOverlay) {
            const overlayInfos = buildOverlayFileInfos(paths)
            showOverlay(overlayInfos, paths.length)
        }
        startModifierTracking()
        handleDragOver(position)
    }

    /** Updates drop-target highlights and overlay as the cursor moves during a drag. */
    function handleDragOver(position: { x: number; y: number }) {
        const resolved = resolveDropTarget(position.x, position.y, paneWrapperEls.left, paneWrapperEls.right)

        if (resolved?.type === 'folder') {
            dropTargetPane = null
            dropTargetFolderPath = resolved.path
            dropTargetFolderEl = resolved.element
        } else if (resolved?.type === 'pane') {
            // Suppress highlight when self-drag targets the source pane (no-op)
            const suppress = getIsDraggingFromSelf() && resolved.paneId === focusedPane
            dropTargetPane = suppress ? null : resolved.paneId
            dropTargetFolderPath = null
            dropTargetFolderEl = null
        } else {
            clearDropTargets()
        }

        // Determine if dropping is allowed
        const isSelfNoOp = resolved?.type === 'pane' && getIsDraggingFromSelf() && resolved.paneId === focusedPane
        const canDrop = resolved !== null && !isSelfNoOp
        const targetName = resolveTargetDisplayName(resolved, dropTargetFolderPath)
        const operation = getIsAltHeld() ? 'move' : 'copy'

        updateOverlay(position.x, position.y, targetName, canDrop, operation)
    }

    /** Handles the drop event: resolves the target and opens the transfer dialog. */
    function handleDrop(paths: string[], position: { x: number; y: number }) {
        const resolved = resolveDropTarget(position.x, position.y, paneWrapperEls.left, paneWrapperEls.right)
        const folderPath = dropTargetFolderPath

        // Read the modifier BEFORE stopping the tracker (which resets altKeyHeld)
        const operation = getIsAltHeld() ? 'move' : 'copy'

        clearDropTargets()
        hideOverlay()
        stopModifierTracking()

        if (!resolved) return
        const targetPane = resolved.paneId
        // For same-pane pane-level drops (not folder), suppress (no-op)
        if (resolved.type === 'pane' && getIsDraggingFromSelf() && targetPane === focusedPane) return

        handleFileDrop(paths, targetPane, resolved.type === 'folder' ? (folderPath ?? undefined) : undefined, operation)
    }

    /** Clears all drop target highlight state and hides overlay. */
    function clearDropTargets() {
        dropTargetPane = null
        dropTargetFolderPath = null
        dropTargetFolderEl = null
    }

    /** Ensures a path ends with '/' for correct prefix matching. */
    function ensureTrailingSlash(path: string): string {
        return path.endsWith('/') ? path : path + '/'
    }

    /** Returns true if any updated path is a descendant of `dir`. */
    function hasDescendantUpdate(paths: string[], dir: string): boolean {
        return paths.some((p) => {
            const withSlash = ensureTrailingSlash(p)
            return withSlash.startsWith(dir) && withSlash !== dir
        })
    }

    // Throttle state for index size refreshes (one per pane).
    // Throttle fires on the first event, then ignores subsequent events for the cooldown period.
    // This ensures updates appear promptly even when events fire continuously.
    let leftThrottleUntil = 0
    let rightThrottleUntil = 0
    const indexRefreshCooldownMs = 2000

    /** Throttled refresh: fires immediately on first relevant event, then skips for the cooldown period. */
    function throttledRefresh(
        shouldRefresh: boolean,
        throttleUntil: number,
        setThrottle: (v: number) => void,
        paneRef: FilePane | undefined,
    ) {
        if (!shouldRefresh) return
        const now = Date.now()
        if (now < throttleUntil) return
        setThrottle(now + indexRefreshCooldownMs)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.refreshIndexSizes?.()
    }

    /** Called when the drive index updates directory stats. Refreshes only index sizes (no full list rebuild). */
    function handleIndexDirUpdated(paths: string[]) {
        const refreshLeft = hasDescendantUpdate(paths, ensureTrailingSlash(leftPath))
        const refreshRight = hasDescendantUpdate(paths, ensureTrailingSlash(rightPath))

        throttledRefresh(refreshLeft, leftThrottleUntil, (v) => (leftThrottleUntil = v), getPaneRef('left'))
        throttledRefresh(refreshRight, rightThrottleUntil, (v) => (rightThrottleUntil = v), getPaneRef('right'))
    }

    function handleResizeForDevTools() {
        void recalculateWebviewOffset()
    }

    onMount(async () => {
        // Start font metrics measurement in background (non-blocking)
        void ensureFontMetricsLoaded()

        // Start network discovery in background (non-blocking)
        void initNetworkDiscovery()

        // Load volumes first
        volumes = (await listVolumes()).data

        // Load persisted state (tabs + app status + settings) in parallel
        const [leftPaneTabs, rightPaneTabs, status, settings] = await Promise.all([
            loadPaneTabs('left', pathExists),
            loadPaneTabs('right', pathExists),
            loadAppStatus(pathExists),
            loadSettings(),
        ])

        focusedPane = status.focusedPane
        showHiddenFiles = settings.showHiddenFiles
        leftPaneWidthPercent = status.leftPaneWidthPercent

        // E2E test override: use CMDR_E2E_START_PATH subdirectories when set
        const e2eStartPath = await getE2eStartPath()

        // Determine the correct volume IDs by finding which volume contains each tab's path
        // This is more reliable than trusting the stored volumeId, which may be stale
        // Exception: 'network' is a virtual volume, trust the stored ID for that
        const defaultId = await getDefaultVolumeId()

        const volumeResolveTimeoutMs = 3000

        interface VolumeResolution {
            volumeId: string
            timedOut: boolean
        }

        async function resolveVolumeId(
            volumeId: string,
            path: string,
            hasE2eOverride: boolean,
        ): Promise<VolumeResolution> {
            if (volumeId === 'network' && !hasE2eOverride) return { volumeId: 'network', timedOut: false }
            const result = await withTimeout(findContainingVolume(path), volumeResolveTimeoutMs, null)
            // Frontend timeout: withTimeout returned null
            if (result === null) {
                log.warn('Volume resolution timed out (frontend) for path: {path}', { path })
                return { volumeId: defaultId, timedOut: true }
            }
            // Backend timeout: findContainingVolume returned { data: null, timedOut: true }
            if (result.timedOut && result.data === null) {
                log.warn('Volume resolution timed out (backend) for path: {path}', { path })
                return { volumeId: defaultId, timedOut: true }
            }
            return { volumeId: result.data?.id ?? defaultId, timedOut: false }
        }

        // Resolve volume IDs for all tabs in parallel, tracking timeouts
        const resolvedLeftTabs = await Promise.all(
            leftPaneTabs.tabs.map(async (tab) => {
                const resolution = await resolveVolumeId(tab.volumeId, tab.path, !!e2eStartPath)
                return {
                    ...tab,
                    volumeId: resolution.volumeId,
                    unreachablePath: resolution.timedOut ? tab.path : null,
                }
            }),
        )
        const resolvedRightTabs = await Promise.all(
            rightPaneTabs.tabs.map(async (tab) => {
                const resolution = await resolveVolumeId(tab.volumeId, tab.path, !!e2eStartPath)
                return {
                    ...tab,
                    volumeId: resolution.volumeId,
                    unreachablePath: resolution.timedOut ? tab.path : null,
                }
            }),
        )

        // Collect unreachable paths by tab ID before stripping extra fields
        const unreachableByTabId: Record<string, string> = {}
        for (const tab of [...resolvedLeftTabs, ...resolvedRightTabs]) {
            if (tab.unreachablePath) {
                unreachableByTabId[tab.id] = tab.unreachablePath
            }
        }

        const toPersistedTab = (tab: (typeof resolvedLeftTabs)[number]): PersistedTab => ({
            id: tab.id,
            path: tab.path,
            volumeId: tab.volumeId,
            sortBy: tab.sortBy,
            sortOrder: tab.sortOrder,
            viewMode: tab.viewMode,
            pinned: tab.pinned,
        })
        const resolvedLeftPaneTabs: PersistedPaneTabs = {
            tabs: resolvedLeftTabs.map(toPersistedTab),
            activeTabId: leftPaneTabs.activeTabId,
        }
        const resolvedRightPaneTabs: PersistedPaneTabs = {
            tabs: resolvedRightTabs.map(toPersistedTab),
            activeTabId: rightPaneTabs.activeTabId,
        }

        // E2E override: apply fixture paths to the active tab data BEFORE creating tab managers,
        // so the managers are initialized with the correct paths from the start.
        // Must override both path AND volumeId — persisted state may have a non-root volume
        // (e.g. VirtioFS mount) whose path resolver would mangle the absolute fixture path.
        if (e2eStartPath) {
            const leftActiveTab = resolvedLeftPaneTabs.tabs.find((t) => t.id === resolvedLeftPaneTabs.activeTabId)
            const rightActiveTab = resolvedRightPaneTabs.tabs.find((t) => t.id === resolvedRightPaneTabs.activeTabId)
            const leftTarget = leftActiveTab ?? resolvedLeftPaneTabs.tabs[0]
            const rightTarget = rightActiveTab ?? resolvedRightPaneTabs.tabs[0]
            if (!leftActiveTab) log.warn('E2E path override: left active tab ID mismatch, using first tab')
            if (!rightActiveTab) log.warn('E2E path override: right active tab ID mismatch, using first tab')
            leftTarget.path = `${e2eStartPath}/left`
            leftTarget.volumeId = defaultId
            rightTarget.path = `${e2eStartPath}/right`
            rightTarget.volumeId = defaultId
        }

        // Create tab managers from persisted tab data
        leftTabMgr = createTabManagerFromPersisted(resolvedLeftPaneTabs)
        rightTabMgr = createTabManagerFromPersisted(resolvedRightPaneTabs)

        // Apply unreachable state to tabs that timed out during volume resolution
        for (const tab of [...getAllTabs(leftTabMgr), ...getAllTabs(rightTabMgr)]) {
            const originalPath = unreachableByTabId[tab.id]
            if (originalPath) {
                tab.unreachable = { originalPath, retrying: false }
            }
        }

        initialized = true
        syncPinTabMenu()

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
                showHiddenFiles = newSettings.showHiddenFiles
                // Persist to settings store
                void saveSettings({ showHiddenFiles: newSettings.showHiddenFiles })
            }
        })

        // Subscribe to view mode changes from the backend menu
        unlistenViewMode = await listen<{ mode: ViewMode }>('view-mode-changed', (event) => {
            const newMode = event.payload.mode
            setPaneViewMode(focusedPane, newMode)
            saveAppStatus({ [paneKey(focusedPane, 'viewMode')]: newMode })
            saveTabsForPaneSide(focusedPane)
        })

        // Subscribe to volume mount events (refresh volume list when new volumes appear)
        unlistenVolumeMount = await listen<{ volumePath: string }>('volume-mounted', () => {
            void (async () => {
                volumes = (await listVolumes()).data
            })()
        })

        // Subscribe to volume unmount events
        unlistenVolumeUnmount = await listen<{ volumePath: string }>('volume-unmounted', (event) => {
            void (async () => {
                // Find the volume ID from the path
                const volume = volumes.find((v) => v.path === event.payload.volumePath)
                if (volume) {
                    void handleVolumeUnmount(volume.id)
                } else {
                    // Volume already gone, just refresh the list
                    volumes = (await listVolumes()).data
                }
            })()
        })

        // Listen for drag image size from native swizzle (macOS).
        // Fires before the Tauri drag enter event, so the flag is ready when handleDragEnter runs.
        unlistenDragImageSize = await listen<{ width: number; height: number }>('drag-image-size', (event) => {
            const { width, height } = event.payload
            externalDragHasLargeImage = width > smallDragImageThreshold || height > smallDragImageThreshold
        })

        // Listen for native modifier key state during drags (macOS).
        // [NSEvent modifierFlags] works even when the webview doesn't have keyboard focus.
        unlistenDragModifiers = await listen<{ altHeld: boolean }>('drag-modifiers', (event) => {
            setAltHeld(event.payload.altHeld)
        })

        // Listen for index directory updates to refresh panes when sizes change
        unlistenIndexEvents = await initIndexEvents(handleIndexDirUpdated)

        // Refresh both panes when scan completes (aggregation just finished, all sizes available)
        unlistenIndexScanComplete = await listen('index-scan-complete', () => {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            getPaneRef('left')?.refreshIndexSizes?.()
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            getPaneRef('right')?.refreshIndexSizes?.()
        })

        // Listen for MCP activate_tab events
        unlistenMcpActivateTab = await listen<{ pane: string; tabId: string }>('mcp-activate-tab', (event) => {
            const { pane, tabId } = event.payload
            if (pane === 'left' || pane === 'right') {
                switchToTab(pane, tabId)
            }
        })

        // Listen for MCP pin_tab events
        unlistenMcpPinTab = await listen<{ pane: string; tabId: string; pinned: boolean }>('mcp-pin-tab', (event) => {
            const { pane, tabId, pinned } = event.payload
            if (pane !== 'left' && pane !== 'right') return
            const mgr = getTabMgr(pane)
            const tab = getAllTabs(mgr).find((t) => t.id === tabId)
            if (!tab) return
            if (pinned) {
                pinTab(mgr, tabId)
            } else {
                unpinTab(mgr, tabId)
            }
            saveTabsForPaneSide(pane)
            if (pane === focusedPane && tabId === mgr.activeTabId) syncPinTabMenu()
        })

        // Register drag-and-drop target handler for external and pane-to-pane drops
        unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
            const { type } = event.payload
            if (type === 'enter') {
                const paths = event.payload.paths
                // Re-entry detection: if not currently flagged as self-drag but
                // fingerprint matches, restore the flag before any highlight logic
                if (!getIsDraggingFromSelf() && matchesSelfDragFingerprint(paths)) {
                    markAsSelfDrag()
                }
                // On first entry of a self-drag, store fingerprint for re-entry detection
                if (getIsDraggingFromSelf() && !matchesSelfDragFingerprint(paths)) {
                    storeSelfDragFingerprint(paths)
                }
                handleDragEnter(paths, toViewportPosition(event.payload.position))
            } else if (type === 'over') {
                handleDragOver(toViewportPosition(event.payload.position))
            } else if (type === 'drop') {
                handleDrop(event.payload.paths, toViewportPosition(event.payload.position))
                resetDraggingFromSelf()
                clearSelfDragFingerprint()
                void endSelfDragSession()
                externalDragHasLargeImage = false
            } else {
                // 'leave' — cursor left the window or drag was cancelled
                clearDropTargets()
                hideOverlay()
                stopModifierTracking()
                resetDraggingFromSelf()
                // Do NOT call endSelfDragSession() here — the native swizzle needs
                // SELF_DRAG_ACTIVE + rich image path to swap images on window exit.
                // State is cleaned up when startDrag resolves (finally block) or on drop.
                externalDragHasLargeImage = false
                // Do NOT clear the fingerprint here — that's the key to re-entry detection
            }
        })
    })

    async function handleVolumeUnmount(unmountedId: string) {
        const defaultVolumeId = await getDefaultVolumeId()
        // Navigate to home directory, falling back to / if home doesn't exist
        const homePath = (await pathExists('~')) ? '~' : '/'

        // Switch affected panes to default volume
        if (getPaneVolumeId('left') === unmountedId) {
            setPaneVolumeId('left', defaultVolumeId)
            setPanePath('left', homePath)
            saveAppStatus({ leftVolumeId: defaultVolumeId, leftPath: homePath })
            saveTabsForPaneSide('left')
        }
        if (getPaneVolumeId('right') === unmountedId) {
            setPaneVolumeId('right', defaultVolumeId)
            setPanePath('right', homePath)
            saveAppStatus({ rightVolumeId: defaultVolumeId, rightPath: homePath })
            saveTabsForPaneSide('right')
        }

        // Refresh volume list
        volumes = (await listVolumes()).data
    }

    function updatePaneAfterHistoryNavigation(
        pane: 'left' | 'right',
        newHistory: NavigationHistory,
        targetPath: string,
    ) {
        const entry = getCurrentEntry(newHistory)
        const paneRef = getPaneRef(pane)

        setPaneHistory(pane, newHistory)
        setPanePath(pane, targetPath)
        if (entry.volumeId !== getPaneVolumeId(pane)) {
            setPaneVolumeId(pane, entry.volumeId)
            saveAppStatus({ [paneKey(pane, 'volumeId')]: entry.volumeId, [paneKey(pane, 'path')]: targetPath })
        } else {
            saveAppStatus({ [paneKey(pane, 'path')]: targetPath })
        }
        saveTabsForPaneSide(pane)
        void saveLastUsedPathForVolume(entry.volumeId, targetPath)

        if (entry.volumeId === 'network') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef?.setNetworkHost?.(entry.networkHost ?? null)
        }
    }

    async function handleNavigationAction(action: string) {
        const pane = focusedPane
        const paneRef = getPaneRef(pane)

        if (action === 'parent') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            await paneRef?.navigateToParent()
            return
        }

        const history = getPaneHistory(pane)
        let newHistory: NavigationHistory

        if (action === 'back' && canGoBack(history)) {
            newHistory = back(history)
        } else if (action === 'forward' && canGoForward(history)) {
            newHistory = forward(history)
        } else {
            return
        }

        const targetEntry = getCurrentEntry(newHistory)
        // Navigate immediately — if path is gone, FilePane's error handler resolves upward
        updatePaneAfterHistoryNavigation(pane, newHistory, targetEntry.path)
    }

    onDestroy(() => {
        unlistenSettings?.()
        unlistenViewMode?.()
        unlistenVolumeMount?.()
        unlistenVolumeUnmount?.()
        unlistenDragImageSize?.()
        unlistenDragModifiers?.()
        unlistenDragDrop?.()
        unlistenIndexEvents?.()
        unlistenIndexScanComplete?.()
        unlistenMcpActivateTab?.()
        unlistenMcpPinTab?.()
        if (tabSyncTimer) clearTimeout(tabSyncTimer)
        // No cleanup needed for throttle (no pending timers)
        cleanupNetworkDiscovery()
        stopModifierTracking()
        window.removeEventListener('resize', handleResizeForDevTools) // No-op in non-dev, safe to always call
    })

    function handlePaneResize(widthPercent: number) {
        leftPaneWidthPercent = widthPercent
    }

    function handlePaneResizeEnd() {
        saveAppStatus({ leftPaneWidthPercent })
    }

    function handlePaneResizeReset() {
        leftPaneWidthPercent = 50
        saveAppStatus({ leftPaneWidthPercent: 50 })
    }

    /** Activates inline rename on the focused pane's cursor item. */
    export function startRename() {
        // Check if the volume is read-only before starting rename
        const volId = getPaneVolumeId(focusedPane)
        const volumeInfo = getDestinationVolumeInfo(volId, volumes, getMtpVolumes())
        if (volumeInfo?.isReadOnly) {
            dialogs.showAlert('Read-only volume', "This is a read-only volume. Renaming isn't possible here.")
            return
        }

        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.startRename()
    }

    /** Cancels any active inline rename on either pane. */
    export function cancelRename() {
        for (const side of ['left', 'right'] as const) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            getPaneRef(side)?.cancelRename?.()
        }
    }

    /** Returns whether inline rename is active on either pane. */
    export function isRenaming(): boolean {
        return (['left', 'right'] as const).some((side) => {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            return getPaneRef(side)?.isRenaming?.() as boolean
        })
    }

    /** Opens the new folder dialog. Pre-fills with the entry name under cursor. */
    export async function openNewFolderDialog() {
        const paneRef = getPaneRef(focusedPane)
        const path = getPanePath(focusedPane)
        const volumeIdForPane = getPaneVolumeId(focusedPane)

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const paneListingId = paneRef?.getListingId?.() as string | undefined
        if (!paneListingId) return

        const initialName = await getInitialFolderName(paneRef, paneListingId, showHiddenFiles, getFileAt)

        dialogs.showNewFolder({
            currentPath: path,
            listingId: paneListingId,
            showHiddenFiles,
            initialName,
            volumeId: volumeIdForPane,
        })
    }

    /** Closes any confirmation dialog (new folder or transfer) if open (for MCP). */
    export function closeConfirmationDialog() {
        dialogs.closeConfirmationDialog()
    }

    /** Returns whether any confirmation dialog is currently open. */
    export function isConfirmationDialogOpen(): boolean {
        return dialogs.isConfirmationDialogOpen()
    }

    /** Opens the file viewer for the file under the cursor. */
    export async function openViewerForCursor() {
        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = paneRef?.getListingId?.() as string | undefined
        if (!listingId) return
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const cursorIndex = paneRef?.getCursorIndex?.() as number | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = paneRef?.hasParentEntry?.() as boolean | undefined
        const backendIndex = toBackendCursorIndex(cursorIndex ?? -1, hasParent ?? false)
        if (backendIndex === null) return

        const file = await getFileAt(listingId, backendIndex, showHiddenFiles)
        if (!file || file.isDirectory || file.name === '..') return

        void openFileViewer(file.path)
    }

    /** Builds a TransferContext from pane state. */
    function buildTransferContext(pane: 'left' | 'right'): TransferContext {
        const other = otherPane(pane)
        const { sortBy, sortOrder } = getPaneSort(pane)
        return {
            showHiddenFiles,
            sourcePath: getPanePath(pane),
            destPath: getPanePath(other),
            sourceVolumeId: getPaneVolumeId(pane),
            destVolumeId: getPaneVolumeId(other),
            sortColumn: sortBy,
            sortOrder,
        }
    }

    /** Opens the unified transfer dialog for all volume types (local, MTP, etc.). */
    async function openUnifiedTransferDialog(
        operationType: TransferOperationType,
        sourcePaneRef: FilePane | undefined,
        pane: 'left' | 'right',
    ) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = sourcePaneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = sourcePaneRef?.hasParentEntry?.() as boolean | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const selectedIndices = sourcePaneRef?.getSelectedIndices?.() as number[] | undefined
        const hasSelection = selectedIndices && selectedIndices.length > 0

        const context = buildTransferContext(pane)
        const isLeft = pane === 'left'

        const props = hasSelection
            ? await buildTransferPropsFromSelection(
                  operationType,
                  listingId,
                  selectedIndices,
                  hasParent ?? false,
                  isLeft,
                  context,
              )
            : await buildTransferPropsFromCursor(
                  operationType,
                  listingId,
                  sourcePaneRef,
                  hasParent ?? false,
                  isLeft,
                  context,
              )

        if (props) {
            dialogs.showTransfer(props)
        }
    }

    /** Opens the transfer dialog with the current selection info. */
    export async function openTransferDialog(operationType: TransferOperationType) {
        const sourcePaneRef = getPaneRef(focusedPane)
        const destVolId = getPaneVolumeId(otherPane(focusedPane))

        const destVolume = getDestinationVolumeInfo(destVolId, volumes, getMtpVolumes())
        if (destVolume?.isReadOnly) {
            dialogs.showAlert(
                'Read-only device',
                `"${destVolume.name}" is read-only. You can copy files from it, but not to it.`,
            )
            return
        }

        await openUnifiedTransferDialog(operationType, sourcePaneRef, focusedPane)
    }

    /** Opens the copy dialog (convenience wrapper for MCP/key binding). */
    export async function openCopyDialog() {
        await openTransferDialog('copy')
    }

    /** Opens the move dialog (convenience wrapper for MCP/key binding). */
    export async function openMoveDialog() {
        await openTransferDialog('move')
    }

    /** Gathers pane state needed for clipboard copy/cut. Returns null if unavailable. */
    function getClipboardPaneState() {
        const sourcePaneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = sourcePaneRef?.getListingId?.() as string | undefined
        if (!listingId) return null

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = (sourcePaneRef?.hasParentEntry?.() as boolean | undefined) ?? false
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const selectedIndices = (sourcePaneRef?.getSelectedIndices?.() as number[] | undefined) ?? []
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const cursorIndex = (sourcePaneRef?.getCursorIndex?.() as number | undefined) ?? 0
        const volumeId = getPaneVolumeId(focusedPane)

        return { listingId, hasParent, selectedIndices, cursorIndex, volumeId }
    }

    /** Copies selected files (or cursor file) to the system clipboard. */
    export async function copyToClipboard() {
        const state = getClipboardPaneState()
        if (!state) return

        if (state.volumeId.startsWith('mtp-')) {
            addToast('Use F5 to copy files from MTP devices')
            return
        }

        try {
            const count = await copyFilesToClipboard(
                state.listingId,
                state.selectedIndices,
                state.cursorIndex,
                state.hasParent,
                showHiddenFiles,
            )
            addToast(`Copied ${String(count)} ${count === 1 ? 'item' : 'items'}`)
        } catch (error) {
            log.error('Clipboard copy failed: {error}', { error })
        }
    }

    /** Cuts selected files (or cursor file) to the system clipboard. */
    export async function cutToClipboard() {
        const state = getClipboardPaneState()
        if (!state) return

        if (state.volumeId.startsWith('mtp-')) {
            addToast('Use F6 to move files from MTP devices')
            return
        }

        try {
            const count = await cutFilesToClipboard(
                state.listingId,
                state.selectedIndices,
                state.cursorIndex,
                state.hasParent,
                showHiddenFiles,
            )
            addToast(`${String(count)} ${count === 1 ? 'item' : 'items'} ready to move. Paste to complete.`)
        } catch (error) {
            log.error('Clipboard cut failed: {error}', { error })
        }
    }

    /** Pastes files from the system clipboard into the current directory. */
    export async function pasteFromClipboard(forceMove: boolean) {
        try {
            const result = await readClipboardFiles()

            if (result.paths.length === 0) {
                addToast('No files on the clipboard. Copy files first with \u2318C.')
                return
            }

            const volumeId = getPaneVolumeId(focusedPane)
            if (volumeId.startsWith('mtp-')) {
                addToast('Use F5 to copy files to MTP devices')
                return
            }

            const operationType: TransferOperationType = result.isCut || forceMove ? 'move' : 'copy'
            const destPath = getPanePath(focusedPane)
            const { sortBy, sortOrder } = getPaneSort(focusedPane)
            const destVolId = getPaneVolumeId(focusedPane)
            const sourceFolderPath = getCommonParentPath(result.paths)

            dialogs.startTransferProgress({
                operationType,
                sourcePaths: result.paths,
                sourceFolderPath,
                destinationPath: destPath,
                direction: focusedPane === 'left' ? 'left' : 'right',
                sortColumn: sortBy,
                sortOrder,
                previewId: null,
                sourceVolumeId: DEFAULT_VOLUME_ID,
                destVolumeId: destVolId,
            })

            if (result.isCut) {
                void clearClipboardCutState()
            }
        } catch (error) {
            log.error('Clipboard paste failed: {error}', { error })
        }
    }

    /** Opens the delete confirmation dialog for the current selection or cursor item. */
    export async function openDeleteDialog(permanent: boolean) {
        const sourcePaneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = sourcePaneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = sourcePaneRef?.hasParentEntry?.() as boolean | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const selectedIndices = sourcePaneRef?.getSelectedIndices?.() as number[] | undefined
        const hasSelection = selectedIndices && selectedIndices.length > 0

        const backendIndices = hasSelection
            ? toBackendIndices(selectedIndices, hasParent ?? false)
            : (() => {
                  // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                  const cursorIndex = sourcePaneRef?.getCursorIndex?.() as number | undefined
                  const idx = toBackendCursorIndex(cursorIndex ?? -1, hasParent ?? false)
                  return idx !== null ? [idx] : []
              })()
        if (backendIndices.length === 0) return

        // Fetch full FileEntry data for each item
        const entries = await Promise.all(backendIndices.map((idx) => getFileAt(listingId, idx, showHiddenFiles)))
        const validEntries = entries.filter((e): e is NonNullable<typeof e> => e !== null && e.name !== '..')
        if (validEntries.length === 0) return

        const sourceItems: DeleteSourceItem[] = validEntries.map((e) => ({
            name: e.name,
            size: e.size,
            isDirectory: e.isDirectory,
            isSymlink: e.isSymlink,
            recursiveSize: e.recursiveSize,
            recursiveFileCount: e.recursiveFileCount,
        }))
        const sourcePaths = validEntries.map((e) => e.path)

        // Look up supportsTrash from the source volume
        const sourceVolId = getPaneVolumeId(focusedPane)
        const sourceVolume = volumes.find((v) => v.id === sourceVolId)
        const supportsTrash = sourceVolume?.supportsTrash !== false

        const { sortBy, sortOrder } = getPaneSort(focusedPane)

        dialogs.showDeleteConfirmation({
            sourceItems,
            sourcePaths,
            sourceFolderPath: getPanePath(focusedPane),
            isPermanent: permanent,
            supportsTrash,
            isFromCursor: !hasSelection,
            sortColumn: sortBy,
            sortOrder,
            sourceVolumeId: sourceVolId,
        })
    }

    // Focus the container after initialization so keyboard events work
    $effect(() => {
        if (initialized) {
            containerElement?.focus()
        }
    })

    // Manage folder drop-target highlight class imperatively (elements live in child components)
    $effect(() => {
        const el = dropTargetFolderEl
        if (el) {
            el.classList.add('folder-drop-target')
            return () => {
                el.classList.remove('folder-drop-target')
            }
        }
    })

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
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef('left')?.closeVolumeChooser()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef('right')?.closeVolumeChooser()
        const newFocus = otherPane(focusedPane)
        focusedPane = newFocus
        saveAppStatus({ focusedPane: newFocus })
        void updateFocusedPane(newFocus)
        containerElement?.focus()
    }

    /** Returns true if pane swap is safe (both panes ready, no dialogs open). */
    function canSwapPanes(): boolean {
        const leftRef = getPaneRef('left')
        const rightRef = getPaneRef('right')
        if (!leftRef || !rightRef) return false
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (leftRef.isLoading?.() || rightRef.isLoading?.()) return false
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
     * Zero backend calls — we just swap listing ownership on the frontend.
     */
    export function swapPanes(): void {
        if (!canSwapPanes()) return

        const leftRef = getPaneRef('left')
        const rightRef = getPaneRef('right')
        if (!leftRef || !rightRef) return

        // 1. Snapshot both panes' listing state
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-assignment
        const leftSwap = leftRef.getSwapState?.()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-assignment
        const rightSwap = rightRef.getSwapState?.()
        if (!leftSwap || !rightSwap) return

        // 2. Swap DualPaneExplorer state variables
        swapDualPaneState()

        // 3. Each pane adopts the other's listing (no backend calls)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        leftRef.adoptListing?.(rightSwap)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        rightRef.adoptListing?.(leftSwap)

        // 4. Persist
        saveAppStatus({
            leftPath,
            rightPath,
            leftVolumeId,
            rightVolumeId,
            leftViewMode,
            rightViewMode,
            leftSortBy,
            rightSortBy,
        })
        saveTabsForPaneSide('left')
        saveTabsForPaneSide('right')

        containerElement?.focus()
    }

    /**
     * Open/toggle volume chooser for the specified pane.
     * Closes the other pane's volume chooser to ensure only one is open at a time.
     */
    export function toggleVolumeChooser(pane: 'left' | 'right') {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef(otherPane(pane))?.closeVolumeChooser()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef(pane)?.toggleVolumeChooser()
    }

    /**
     * Open volume chooser for the focused pane.
     * Closes the other pane's volume chooser first.
     */
    export function openVolumeChooser() {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef(otherPane(focusedPane))?.closeVolumeChooser()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        getPaneRef(focusedPane)?.openVolumeChooser()
    }

    /**
     * Close volume chooser on all panes.
     */
    export function closeVolumeChooser() {
        for (const side of ['left', 'right'] as const) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            getPaneRef(side)?.closeVolumeChooser()
        }
    }

    /**
     * Toggle show hidden files.
     */
    export function toggleHiddenFiles() {
        showHiddenFiles = !showHiddenFiles
        void saveSettings({ showHiddenFiles })
    }

    /**
     * Set view mode for a specific pane (or focused pane if not specified).
     * Used by command palette and MCP.
     */
    export function setViewMode(mode: ViewMode, pane?: 'left' | 'right') {
        const targetPane = pane ?? focusedPane
        setPaneViewMode(targetPane, mode)
        saveAppStatus({ [paneKey(targetPane, 'viewMode')]: mode })
        saveTabsForPaneSide(targetPane)
    }

    /**
     * Navigate the focused pane (back/forward/parent).
     */
    export function navigate(action: 'back' | 'forward' | 'parent') {
        void handleNavigationAction(action)
    }

    /**
     * Get the path and filename of the file under the cursor in the focused pane.
     */
    export function getFileAndPathUnderCursor(): { path: string; filename: string } | null {
        const paneRef = getPaneRef(focusedPane)
        const currentPath = getPanePath(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const filename = paneRef?.getFilenameUnderCursor?.() as string | undefined
        if (!filename || filename === '..') return null
        const path = `${currentPath}/${filename}`
        return { path, filename }
    }

    /**
     * Simulate a key press on the focused pane (for commands like Enter to open).
     */
    export function sendKeyToFocusedPane(key: string) {
        const paneRef = getPaneRef(focusedPane)
        const event = new KeyboardEvent('keydown', { key, bubbles: false })
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.handleKeyDown(event)
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
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = paneRef?.getListingId?.() as string | undefined
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
        saveAppStatus({ [paneKey(pane, 'sortBy')]: column })
        saveTabsForPaneSide(pane)
        applySortResult(paneRef, result, sortState.hasParent)
    }

    /**
     * Get the focused pane identifier.
     * Used by MCP context tools.
     */
    export function getFocusedPane(): 'left' | 'right' {
        return focusedPane
    }

    /**
     * Get the list of available volumes.
     * Used by MCP volume.list tool.
     */
    export function getVolumes(): VolumeInfo[] {
        return volumes
    }

    /**
     * Select a volume by index for a specific pane.
     * Used by MCP volume.selectLeft/volume.selectRight tools.
     * Matches the behavior of VolumeBreadcrumb's handleVolumeSelect.
     * @param pane - 'left' or 'right'
     * @param index - Zero-based index into the volumes array
     */
    export async function selectVolumeByIndex(pane: 'left' | 'right', index: number): Promise<boolean> {
        if (index < 0 || index >= volumes.length) {
            log.warn('Invalid volume index: {index} (valid range: 0-{max})', { index, max: volumes.length - 1 })
            return false
        }

        const volume = volumes[index]

        // Handle favorites differently from actual volumes (same as VolumeBreadcrumb)
        if (volume.category === 'favorite') {
            // For favorites, find the actual containing volume
            const { data: containingVolume } = await findContainingVolume(volume.path)
            if (containingVolume) {
                // Navigate to the favorite's path, but set the volume to the containing volume
                await handleVolumeChange(pane, containingVolume.id, containingVolume.path, volume.path)
            } else {
                // Fallback: use root volume
                await handleVolumeChange(pane, 'root', '/', volume.path)
            }
        } else {
            // For actual volumes, navigate to the volume's root
            await handleVolumeChange(pane, volume.id, volume.path, volume.path)
        }

        return true
    }

    /**
     * Handle selection action from MCP.
     * @param action - The selection action (clear, selectAll, deselectAll, toggleAtCursor, selectRange)
     * @param startIndex - Start index for range selection
     * @param endIndex - End index for range selection
     */
    export function handleSelectionAction(action: string, startIndex?: number, endIndex?: number) {
        const paneRef = getPaneRef(focusedPane)
        if (!paneRef) return

        switch (action) {
            case 'clear':
            case 'deselectAll':
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                paneRef.clearSelection?.()
                break
            case 'selectAll':
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                paneRef.selectAll?.()
                break
            case 'toggleAtCursor':
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                paneRef.toggleSelectionAtCursor?.()
                break
            case 'selectRange':
                if (startIndex !== undefined && endIndex !== undefined) {
                    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                    paneRef.selectRange?.(startIndex, endIndex)
                }
                break
        }
    }

    /**
     * Navigate a pane to a specific path.
     * Used by MCP nav_to_path tool.
     */
    export function navigateToPath(pane: 'left' | 'right', path: string) {
        const paneRef = getPaneRef(pane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.navigateToPath?.(path)
    }

    /**
     * Move cursor to a specific index or filename.
     * Used by MCP move_cursor tool.
     */
    export async function moveCursor(pane: 'left' | 'right', to: number | string) {
        const paneRef = getPaneRef(pane)
        if (!paneRef) return

        if (typeof to === 'number') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef.setCursorIndex?.(to)
        } else {
            await moveCursorByName(paneRef, to)
        }
    }

    async function moveCursorByName(paneRef: NonNullable<ReturnType<typeof getPaneRef>>, name: string) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const inNetwork: boolean = paneRef.isInNetworkView?.() ?? false
        if (inNetwork) {
            // Network views handle name lookup locally
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
            const idx: number = paneRef.findNetworkItemIndex?.(name) ?? -1
            if (idx >= 0) {
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                paneRef.setCursorIndex?.(idx)
            }
        } else {
            await moveCursorByNameInFileListing(paneRef, name)
        }
    }

    async function moveCursorByNameInFileListing(paneRef: NonNullable<ReturnType<typeof getPaneRef>>, name: string) {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const listingId: string | undefined = paneRef.getListingId?.()
        if (!listingId) return

        const backendIndex = await findFileIndex(listingId, name, showHiddenFiles)
        if (backendIndex === null) return

        // Backend index doesn't include ".." entry, but frontend does
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
        const hasParent: boolean = paneRef.hasParentEntry?.() ?? false
        const frontendIndex = hasParent ? backendIndex + 1 : backendIndex
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef.setCursorIndex?.(frontendIndex)
    }

    /**
     * Scroll to load a region around a specific index in a large directory.
     * Used by MCP scroll_to tool.
     */
    export function scrollTo(pane: 'left' | 'right', index: number) {
        const paneRef = getPaneRef(pane)
        // For now, just set cursor to that index - virtualization handles the rest
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.setCursorIndex?.(index)
    }

    /**
     * Select a volume by name for a specific pane.
     * Used by MCP select_volume tool.
     */
    export async function selectVolumeByName(pane: 'left' | 'right', name: string): Promise<boolean> {
        // "Network" is a virtual volume not in the volumes list
        if (name === 'Network') {
            await handleVolumeChange(pane, 'network', 'smb://', 'smb://')
            return true
        }

        const index = volumes.findIndex((v) => v.name === name)
        if (index === -1) {
            log.warn('Volume not found: {name}', { name })
            return false
        }
        return selectVolumeByIndex(pane, index)
    }

    /**
     * Refresh the focused pane.
     * Used by MCP refresh tool.
     */
    export function refreshPane() {
        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.refreshView?.()
    }

    /** Refresh network hosts in the focused pane (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef?.refreshNetworkHosts?.()
    }

    /**
     * Handle unified select command from MCP.
     * @param pane - Which pane to select in
     * @param start - Start index (0-based)
     * @param count - Number of items to select, or 'all' for select all
     * @param mode - 'replace', 'add', or 'subtract'
     */
    export function handleMcpSelect(pane: 'left' | 'right', start: number, count: number | 'all', mode: string) {
        const paneRef = getPaneRef(pane)
        if (!paneRef) return

        // Get current selection for add/subtract modes (local Set, not reactive state)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-argument, svelte/prefer-svelte-reactivity
        const currentSelection = new Set<number>(paneRef.getSelectedIndices?.() ?? [])

        if (count === 0) {
            // Clear selection
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef.setSelectedIndices?.([])
            return
        }

        if (count === 'all') {
            // Select all
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef.selectAll?.()
            return
        }

        // Calculate the indices to select
        const endIndex = start + count - 1
        const targetIndices: number[] = []
        for (let i = start; i <= endIndex; i++) {
            targetIndices.push(i)
        }

        let newSelection: number[]
        if (mode === 'add') {
            // Add to current selection
            targetIndices.forEach((i) => currentSelection.add(i))
            newSelection = Array.from(currentSelection)
        } else if (mode === 'subtract') {
            // Remove from current selection
            targetIndices.forEach((i) => currentSelection.delete(i))
            newSelection = Array.from(currentSelection)
        } else {
            // Replace mode (default)
            newSelection = targetIndices
        }

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        paneRef.setSelectedIndices?.(newSelection)
    }

    // --- Tab bar handler functions (logic in tab-operations.ts) ---

    function handleTabClose(pane: 'left' | 'right', tabId: TabId) {
        void tabOpsHandleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu)
    }

    function handleTabMiddleClick(pane: 'left' | 'right', tabId: TabId) {
        tabOpsHandleTabMiddleClick(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu)
    }

    function handleNewTab(pane: 'left' | 'right') {
        tabOpsHandleNewTab(pane, focusedPane, (p) => (focusedPane = p), newTab)
    }

    function handleTabContextMenu(pane: 'left' | 'right', tabId: TabId, event: MouseEvent) {
        void tabOpsHandleTabContextMenu(pane, tabId, event, getTabMgr, focusedPane, syncPinTabMenu)
    }

    export function newTab(): boolean {
        return tabOpsNewTab(focusedPane, getTabMgr, (h) => $state.snapshot(h))
    }

    export function closeActiveTab(): 'closed' | 'last-tab' {
        return tabOpsCloseActiveTab(focusedPane, getTabMgr)
    }

    export async function closeActiveTabWithConfirmation(): Promise<'closed' | 'last-tab' | 'cancelled'> {
        return tabOpsCloseActiveTabWithConfirmation(focusedPane, getTabMgr)
    }

    export function togglePinActiveTab(): void {
        tabOpsTogglePinActiveTab(focusedPane, getTabMgr)
    }

    export function closeOtherTabs(): void {
        tabOpsCloseOtherTabs(focusedPane, getTabMgr)
    }

    function syncPinTabMenu() {
        syncPinTabMenuForPane(focusedPane, getTabMgr)
    }

    export function cycleTab(direction: 'next' | 'prev'): void {
        tabOpsCycleTab(direction, focusedPane, getTabMgr, getPaneRef)
    }

    export function switchToTab(pane: 'left' | 'right', tabId: TabId): boolean {
        return tabOpsSwitchToTab(pane, tabId, getTabMgr, getPaneRef, focusedPane)
    }

    export function getTabsForPane(pane: 'left' | 'right'): { tabs: TabState[]; activeTabId: TabId } {
        return tabOpsGetTabsForPane(pane, getTabMgr)
    }
</script>

{#snippet paneBlock(paneId: 'left' | 'right')}
    {@const tabMgr = getTabMgr(paneId)}
    <div
        class="pane-wrapper"
        class:drop-target-active={dropTargetPane === paneId}
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
                    handlePathChange(paneId, path)
                }}
                onVolumeChange={(volumeId: string, volumePath: string, targetPath: string) =>
                    handleVolumeChange(paneId, volumeId, volumePath, targetPath)}
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
    {volumes}
    showTransferProgressDialog={dialogs.showTransferProgressDialog}
    transferProgressProps={dialogs.transferProgressProps}
    showNewFolderDialog={dialogs.showNewFolderDialog}
    newFolderDialogProps={dialogs.newFolderDialogProps}
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
    ) => {
        dialogs.handleTransferConfirm(dest, volId, prevId, resolution, opType)
    }}
    onTransferCancel={() => {
        dialogs.handleTransferCancel()
    }}
    onTransferComplete={(files: number, bytes: number) => {
        dialogs.handleTransferComplete(files, bytes)
    }}
    onTransferCancelled={(files: number) => {
        dialogs.handleTransferCancelled(files)
    }}
    onTransferError={(error: WriteOperationError) => {
        dialogs.handleTransferError(error)
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
    onAlertClose={() => {
        dialogs.handleAlertClose()
    }}
    showDeleteDialog={dialogs.showDeleteDialog}
    deleteDialogProps={dialogs.deleteDialogProps}
    onDeleteConfirm={(previewId: string | null) => {
        dialogs.handleDeleteConfirm(previewId)
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
