<script lang="ts">
    import { onMount, onDestroy, untrack, tick } from 'svelte'
    import FilePane from './FilePane.svelte'
    import PaneResizer from './PaneResizer.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import DialogManager from './DialogManager.svelte'
    import { toBackendCursorIndex } from '../../file-operations/copy/copy-dialog-utils'
    import { formatBytes, getFileAt } from '$lib/tauri-commands'
    import {
        loadAppStatus,
        saveAppStatus,
        saveLastUsedPathForVolume,
        getColumnSortOrder,
        saveColumnSortOrder,
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
        findFileIndex,
    } from '$lib/tauri-commands'
    import type {
        VolumeInfo,
        SortColumn,
        SortOrder,
        NetworkHost,
        ConflictResolution,
        WriteOperationError,
    } from '../types'
    import { DEFAULT_SORT_BY, defaultSortOrders } from '../types'
    import { ensureFontMetricsLoaded } from '$lib/font-metrics'
    import { determineNavigationPath, resolveValidPath } from '../navigation/path-navigation'
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
    import { initNetworkDiscovery, cleanupNetworkDiscovery } from '../network/network-store.svelte'
    import { openFileViewer } from '$lib/file-viewer/open-viewer'
    import { getAppLogger } from '$lib/logger'
    import { getMtpVolumes } from '$lib/mtp'
    import { getNewSortOrder, applySortResult, collectSortState } from './sorting-handlers'
    import {
        type CopyDialogPropsData,
        type CopyContext,
        buildCopyPropsFromSelection,
        buildCopyPropsFromCursor,
        getDestinationVolumeInfo,
    } from './copy-operations'
    import { getInitialFolderName, moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'

    const log = getAppLogger('fileExplorer')

    let leftPath = $state('~')
    let rightPath = $state('~')
    let focusedPane = $state<'left' | 'right'>('left')
    let showHiddenFiles = $state(true)
    let leftViewMode = $state<ViewMode>('brief')
    let rightViewMode = $state<ViewMode>('brief')
    let leftVolumeId = $state(DEFAULT_VOLUME_ID)
    let rightVolumeId = $state(DEFAULT_VOLUME_ID)
    let volumes = $state<VolumeInfo[]>([])
    let initialized = $state(false)
    let leftPaneWidthPercent = $state(50)

    // Sorting state - per-pane
    let leftSortBy = $state<SortColumn>(DEFAULT_SORT_BY)
    let rightSortBy = $state<SortColumn>(DEFAULT_SORT_BY)
    let leftSortOrder = $state<SortOrder>(defaultSortOrders[DEFAULT_SORT_BY])
    let rightSortOrder = $state<SortOrder>(defaultSortOrders[DEFAULT_SORT_BY])

    let containerElement: HTMLDivElement | undefined = $state()
    let leftPaneRef: FilePane | undefined = $state()
    let rightPaneRef: FilePane | undefined = $state()
    let unlistenSettings: UnlistenFn | undefined
    let unlistenViewMode: UnlistenFn | undefined
    let unlistenVolumeMount: UnlistenFn | undefined
    let unlistenVolumeUnmount: UnlistenFn | undefined
    let unlistenNavigation: UnlistenFn | undefined

    // Copy dialog state
    let showCopyDialog = $state(false)
    let copyDialogProps = $state<CopyDialogPropsData | null>(null)

    // Copy progress dialog state
    let showCopyProgressDialog = $state(false)
    let copyProgressProps = $state<{
        sourcePaths: string[]
        sourceFolderPath: string
        destinationPath: string
        direction: 'left' | 'right'
        sortColumn: SortColumn
        sortOrder: SortOrder
        previewId: string | null
        sourceVolumeId: string
        destVolumeId: string
        conflictResolution: ConflictResolution
    } | null>(null)

    // New folder dialog state
    let showNewFolderDialog = $state(false)
    let newFolderDialogProps = $state<{
        currentPath: string
        listingId: string
        showHiddenFiles: boolean
        initialName: string
        volumeId: string
    } | null>(null)

    // Alert dialog state
    let showAlertDialog = $state(false)
    let alertDialogProps = $state<{
        title: string
        message: string
    } | null>(null)

    // Copy error dialog state
    let showCopyErrorDialog = $state(false)
    let copyErrorProps = $state<{
        error: WriteOperationError
    } | null>(null)

    // Navigation history for each pane (per-pane, session-only)
    // Initialize with default volume - will be updated on mount with actual state
    let leftHistory = $state<NavigationHistory>(createHistory(DEFAULT_VOLUME_ID, '~'))
    let rightHistory = $state<NavigationHistory>(createHistory(DEFAULT_VOLUME_ID, '~'))

    // --- Pane accessor helpers ---

    function getPaneRef(pane: 'left' | 'right'): FilePane | undefined {
        return pane === 'left' ? leftPaneRef : rightPaneRef
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

    function setPanePath(pane: 'left' | 'right', path: string) {
        if (pane === 'left') leftPath = path
        else rightPath = path
    }

    function setPaneVolumeId(pane: 'left' | 'right', volumeId: string) {
        if (pane === 'left') leftVolumeId = volumeId
        else rightVolumeId = volumeId
    }

    function setPaneHistory(pane: 'left' | 'right', history: NavigationHistory) {
        if (pane === 'left') leftHistory = history
        else rightHistory = history
    }

    function setPaneSort(pane: 'left' | 'right', sortBy: SortColumn, sortOrder: SortOrder) {
        if (pane === 'left') {
            leftSortBy = sortBy
            leftSortOrder = sortOrder
        } else {
            rightSortBy = sortBy
            rightSortOrder = sortOrder
        }
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
        setPanePath(pane, path)
        setPaneHistory(pane, pushPath(getPaneHistory(pane), path))
        void saveAppStatus({ [paneKey(pane, 'path')]: path })
        void saveLastUsedPathForVolume(getPaneVolumeId(pane), path)
        containerElement?.focus()
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
        containerElement?.focus()
    }

    async function handleSortChange(pane: 'left' | 'right', newColumn: SortColumn) {
        const paneRef = getPaneRef(pane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = paneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        const { sortBy, sortOrder } = getPaneSort(pane)
        const newOrder =
            newColumn === sortBy ? getNewSortOrder(newColumn, sortBy, sortOrder) : await getColumnSortOrder(newColumn)

        const sortState = collectSortState(paneRef)
        const result = await resortListing(
            listingId,
            newColumn,
            newOrder,
            sortState.cursorFilename,
            showHiddenFiles,
            sortState.selectedIndices,
            sortState.allSelected,
        )

        setPaneSort(pane, newColumn, newOrder)
        void saveAppStatus({ [paneKey(pane, 'sortBy')]: newColumn })
        void saveColumnSortOrder(newColumn, newOrder)
        applySortResult(paneRef, result)
    }

    async function handleVolumeChange(
        pane: 'left' | 'right',
        volumeId: string,
        volumePath: string,
        targetPath: string,
    ) {
        void saveLastUsedPathForVolume(getPaneVolumeId(pane), getPanePath(pane))

        if (!volumes.find((v) => v.id === volumeId)) {
            volumes = await listVolumes()
        }

        const other = otherPane(pane)
        const pathToNavigate = await determineNavigationPath(volumeId, volumePath, targetPath, {
            otherPaneVolumeId: getPaneVolumeId(other),
            otherPanePath: getPanePath(other),
        })

        setPaneVolumeId(pane, volumeId)
        setPanePath(pane, pathToNavigate)
        setPaneHistory(pane, push(getPaneHistory(pane), { volumeId, path: pathToNavigate }))

        focusedPane = pane
        void saveAppStatus({
            [paneKey(pane, 'volumeId')]: volumeId,
            [paneKey(pane, 'path')]: pathToNavigate,
            focusedPane: pane,
        })
    }

    function handleFocus(pane: 'left' | 'right') {
        if (focusedPane !== pane) {
            focusedPane = pane
            void saveAppStatus({ focusedPane: pane })
            void updateFocusedPane(pane)
        }
    }

    function handleCancelLoading(pane: 'left' | 'right', selectName?: string) {
        const entry = getCurrentEntry(getPaneHistory(pane))
        const paneRef = getPaneRef(pane)

        if (entry.volumeId === 'network') {
            setPanePath(pane, entry.path)
            setPaneVolumeId(pane, 'network')
            void saveAppStatus({ [paneKey(pane, 'volumeId')]: 'network', [paneKey(pane, 'path')]: entry.path })
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef?.setNetworkHost?.(entry.networkHost ?? null)
        } else {
            void resolveValidPath(entry.path).then((resolvedPath) => {
                if (resolvedPath !== null) {
                    setPanePath(pane, resolvedPath)
                    if (entry.volumeId !== getPaneVolumeId(pane)) {
                        setPaneVolumeId(pane, entry.volumeId)
                        void saveAppStatus({
                            [paneKey(pane, 'volumeId')]: entry.volumeId,
                            [paneKey(pane, 'path')]: resolvedPath,
                        })
                    } else {
                        void saveAppStatus({ [paneKey(pane, 'path')]: resolvedPath })
                    }
                    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                    paneRef?.navigateToPath?.(resolvedPath, selectName)
                } else {
                    setPanePath(pane, '~')
                    setPaneVolumeId(pane, DEFAULT_VOLUME_ID)
                    void saveAppStatus({ [paneKey(pane, 'path')]: '~', [paneKey(pane, 'volumeId')]: DEFAULT_VOLUME_ID })
                }
            })
        }
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
        void saveAppStatus({ [paneKey(pane, 'volumeId')]: defaultVolumeId, [paneKey(pane, 'path')]: defaultPath })
    }

    /** Routes to whichever pane has its volume chooser open. Returns true if handled. */
    function routeToVolumeChooser(e: KeyboardEvent): boolean {
        // Check if EITHER pane has a volume chooser open - if so, route events there
        // This is important because F1/F2 can open a volume chooser on the non-focused pane
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (leftPaneRef?.isVolumeChooserOpen?.()) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            if (leftPaneRef.handleVolumeChooserKeyDown?.(e)) {
                return true
            }
        }
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        if (rightPaneRef?.isVolumeChooserOpen?.()) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            if (rightPaneRef.handleVolumeChooserKeyDown?.(e)) {
                return true
            }
        }
        return false
    }

    function handleTabKey() {
        const newFocus = focusedPane === 'left' ? 'right' : 'left'
        focusedPane = newFocus
        void saveAppStatus({ focusedPane: newFocus })
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

    /** Handles function key shortcuts (F1-F7). Returns true if a function key was handled. */
    function handleFunctionKey(e: KeyboardEvent): boolean {
        switch (e.key) {
            case 'F1':
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                rightPaneRef?.closeVolumeChooser()
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                leftPaneRef?.toggleVolumeChooser()
                return true
            case 'F2':
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                leftPaneRef?.closeVolumeChooser()
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                rightPaneRef?.toggleVolumeChooser()
                return true
            case 'F3':
                void openViewerForCursor()
                return true
            case 'F5':
                void openCopyDialog()
                return true
            case 'F7':
                void openNewFolderDialog()
                return true
            default:
                return false
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Tab') {
            e.preventDefault()
            handleTabKey()
            return
        }

        // ESC during loading = cancel and go back
        if (e.key === 'Escape' && handleEscapeDuringLoading()) {
            e.preventDefault()
            return
        }

        if (handleFunctionKey(e)) {
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

    onMount(async () => {
        // Start font metrics measurement in background (non-blocking)
        void ensureFontMetricsLoaded()

        // Start network discovery in background (non-blocking)
        void initNetworkDiscovery()

        // Load volumes first
        volumes = await listVolumes()

        // Load persisted state and settings in parallel
        const [status, settings] = await Promise.all([loadAppStatus(pathExists), loadSettings()])

        leftPath = status.leftPath
        rightPath = status.rightPath
        focusedPane = status.focusedPane
        showHiddenFiles = settings.showHiddenFiles
        leftViewMode = status.leftViewMode
        rightViewMode = status.rightViewMode
        leftPaneWidthPercent = status.leftPaneWidthPercent

        // Load sort state
        leftSortBy = status.leftSortBy
        rightSortBy = status.rightSortBy
        // Load remembered sort orders for each column
        leftSortOrder = await getColumnSortOrder(leftSortBy)
        rightSortOrder = await getColumnSortOrder(rightSortBy)

        // Determine the correct volume IDs by finding which volume contains each path
        // This is more reliable than trusting the stored volumeId, which may be stale
        // Exception: 'network' is a virtual volume, trust the stored ID for that
        const defaultId = await getDefaultVolumeId()

        if (status.leftVolumeId === 'network') {
            leftVolumeId = 'network'
        } else {
            const leftContaining = await findContainingVolume(status.leftPath)
            leftVolumeId = leftContaining?.id ?? defaultId
        }

        if (status.rightVolumeId === 'network') {
            rightVolumeId = 'network'
        } else {
            const rightContaining = await findContainingVolume(status.rightPath)
            rightVolumeId = rightContaining?.id ?? defaultId
        }

        // Initialize history with loaded paths and their volumes
        leftHistory = createHistory(leftVolumeId, status.leftPath)
        rightHistory = createHistory(rightVolumeId, status.rightPath)

        initialized = true

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
            // Apply to the focused pane
            if (focusedPane === 'left') {
                leftViewMode = newMode
                void saveAppStatus({ leftViewMode: newMode })
            } else {
                rightViewMode = newMode
                void saveAppStatus({ rightViewMode: newMode })
            }
            // Refocus after Svelte re-renders the new list component to restore keyboard navigation
            void tick().then(() => {
                containerElement?.focus()
            })
        })

        // Subscribe to volume mount events (refresh volume list when new volumes appear)
        unlistenVolumeMount = await listen<{ volumePath: string }>('volume-mounted', () => {
            void (async () => {
                volumes = await listVolumes()
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
                    volumes = await listVolumes()
                }
            })()
        })

        // Subscribe to navigation actions from Go menu
        unlistenNavigation = await listen<{ action: string }>('navigation-action', (event) => {
            void handleNavigationAction(event.payload.action)
        })
    })

    async function handleVolumeUnmount(unmountedId: string) {
        const defaultVolumeId = await getDefaultVolumeId()
        const defaultVolume = volumes.find((v) => v.id === defaultVolumeId)
        const defaultPath = defaultVolume?.path ?? '/'

        // Switch affected panes to default volume
        if (leftVolumeId === unmountedId) {
            leftVolumeId = defaultVolumeId
            leftPath = defaultPath
            void saveAppStatus({ leftVolumeId: defaultVolumeId, leftPath: defaultPath })
        }
        if (rightVolumeId === unmountedId) {
            rightVolumeId = defaultVolumeId
            rightPath = defaultPath
            void saveAppStatus({ rightVolumeId: defaultVolumeId, rightPath: defaultPath })
        }

        // Refresh volume list
        volumes = await listVolumes()
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
            void saveAppStatus({ [paneKey(pane, 'volumeId')]: entry.volumeId, [paneKey(pane, 'path')]: targetPath })
        } else {
            void saveAppStatus({ [paneKey(pane, 'path')]: targetPath })
        }
        void saveLastUsedPathForVolume(entry.volumeId, targetPath)

        if (entry.volumeId === 'network') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            paneRef?.setNetworkHost?.(entry.networkHost ?? null)
        }

        containerElement?.focus()
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
        if (targetEntry.volumeId === 'network') {
            updatePaneAfterHistoryNavigation(pane, newHistory, targetEntry.path)
            return
        }

        const resolvedPath = await resolveValidPath(targetEntry.path)
        if (resolvedPath !== null) {
            updatePaneAfterHistoryNavigation(pane, newHistory, resolvedPath)
        }
    }

    onDestroy(() => {
        unlistenSettings?.()
        unlistenViewMode?.()
        unlistenVolumeMount?.()
        unlistenVolumeUnmount?.()
        unlistenNavigation?.()
        cleanupNetworkDiscovery()
    })

    function handlePaneResize(widthPercent: number) {
        leftPaneWidthPercent = widthPercent
    }

    function handlePaneResizeEnd() {
        void saveAppStatus({ leftPaneWidthPercent })
    }

    function handlePaneResizeReset() {
        leftPaneWidthPercent = 50
        void saveAppStatus({ leftPaneWidthPercent: 50 })
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

        newFolderDialogProps = {
            currentPath: path,
            listingId: paneListingId,
            showHiddenFiles,
            initialName,
            volumeId: volumeIdForPane,
        }
        showNewFolderDialog = true
    }

    function handleNewFolderCreated(folderName: string) {
        const paneRef = getPaneRef(focusedPane)
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const paneListingId = paneRef?.getListingId?.() as string | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = paneRef?.hasParentEntry?.() as boolean | undefined

        showNewFolderDialog = false
        newFolderDialogProps = null
        containerElement?.focus()

        if (!paneListingId) return
        void moveCursorToNewFolder(
            paneListingId,
            folderName,
            paneRef,
            hasParent ?? false,
            showHiddenFiles,
            listen,
            findFileIndex,
        )
    }

    function handleNewFolderCancel() {
        showNewFolderDialog = false
        newFolderDialogProps = null
        containerElement?.focus()
    }

    /** Closes any confirmation dialog (new folder or copy) if open (for MCP). */
    export function closeConfirmationDialog() {
        if (showNewFolderDialog) {
            showNewFolderDialog = false
            newFolderDialogProps = null
            containerElement?.focus()
        }
        if (showCopyDialog) {
            showCopyDialog = false
            copyDialogProps = null
            containerElement?.focus()
        }
    }

    /** Returns whether any confirmation dialog is currently open. */
    export function isConfirmationDialogOpen(): boolean {
        return showNewFolderDialog || showCopyDialog
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

    /** Builds a CopyContext from pane state. */
    function buildCopyContext(pane: 'left' | 'right'): CopyContext {
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

    /** Opens the unified copy dialog for all volume types (local, MTP, etc.). */
    async function openUnifiedCopyDialog(sourcePaneRef: FilePane | undefined, pane: 'left' | 'right') {
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const listingId = sourcePaneRef?.getListingId?.() as string | undefined
        if (!listingId) return

        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const hasParent = sourcePaneRef?.hasParentEntry?.() as boolean | undefined
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        const selectedIndices = sourcePaneRef?.getSelectedIndices?.() as number[] | undefined
        const hasSelection = selectedIndices && selectedIndices.length > 0

        const context = buildCopyContext(pane)
        const isLeft = pane === 'left'

        const props = hasSelection
            ? await buildCopyPropsFromSelection(listingId, selectedIndices, hasParent ?? false, isLeft, context)
            : await buildCopyPropsFromCursor(listingId, sourcePaneRef, hasParent ?? false, isLeft, context)

        if (props) {
            copyDialogProps = props
            showCopyDialog = true
        }
    }

    /** Opens the copy dialog with the current selection info. */
    export async function openCopyDialog() {
        const sourcePaneRef = getPaneRef(focusedPane)
        const destVolId = getPaneVolumeId(otherPane(focusedPane))

        const destVolume = getDestinationVolumeInfo(destVolId, volumes, getMtpVolumes())
        if (destVolume?.isReadOnly) {
            alertDialogProps = {
                title: 'Read-only device',
                message: `"${destVolume.name}" is read-only. You can copy files from it, but not to it.`,
            }
            showAlertDialog = true
            return
        }

        await openUnifiedCopyDialog(sourcePaneRef, focusedPane)
    }

    function handleCopyConfirm(
        destination: string,
        _volumeId: string,
        previewId: string | null,
        conflictResolution: ConflictResolution,
    ) {
        if (!copyDialogProps) return

        // Store the props needed for the progress dialog
        // Sort settings now come from copyDialogProps (captured at dialog open time)
        copyProgressProps = {
            sourcePaths: copyDialogProps.sourcePaths,
            sourceFolderPath: copyDialogProps.sourceFolderPath,
            destinationPath: destination,
            direction: copyDialogProps.direction,
            sortColumn: copyDialogProps.sortColumn,
            sortOrder: copyDialogProps.sortOrder,
            previewId,
            sourceVolumeId: copyDialogProps.sourceVolumeId,
            destVolumeId: copyDialogProps.destVolumeId,
            conflictResolution,
        }

        // Close copy dialog and open progress dialog
        showCopyDialog = false
        copyDialogProps = null
        showCopyProgressDialog = true
    }

    function handleCopyCancel() {
        showCopyDialog = false
        copyDialogProps = null
        containerElement?.focus()
    }

    function handleCopyComplete(filesProcessed: number, bytesProcessed: number) {
        log.info(`Copy complete: ${String(filesProcessed)} files (${formatBytes(bytesProcessed)})`)

        // Refresh the destination pane to show the new files
        const destPaneRef = copyProgressProps?.direction === 'right' ? rightPaneRef : leftPaneRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        destPaneRef?.refreshView?.()

        showCopyProgressDialog = false
        copyProgressProps = null
        containerElement?.focus()
    }

    function handleCopyCancelled(filesProcessed: number) {
        log.info(`Copy cancelled after ${String(filesProcessed)} files`)

        // Refresh the destination pane to show any files that were copied
        const destPaneRef = copyProgressProps?.direction === 'right' ? rightPaneRef : leftPaneRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        destPaneRef?.refreshView?.()

        showCopyProgressDialog = false
        copyProgressProps = null
        containerElement?.focus()
    }

    function handleCopyError(error: WriteOperationError) {
        log.error('Copy failed: {errorType}', { errorType: error.type, error })

        // Refresh the destination pane to show any files that were partially copied
        const destPaneRef = copyProgressProps?.direction === 'right' ? rightPaneRef : leftPaneRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        destPaneRef?.refreshView?.()

        showCopyProgressDialog = false
        copyProgressProps = null

        // Show the error dialog
        copyErrorProps = { error }
        showCopyErrorDialog = true
    }

    function handleCopyErrorClose() {
        showCopyErrorDialog = false
        copyErrorProps = null
        containerElement?.focus()
    }

    function handleAlertClose() {
        showAlertDialog = false
        alertDialogProps = null
        containerElement?.focus()
    }

    // Focus the container after initialization so keyboard events work
    $effect(() => {
        if (initialized) {
            containerElement?.focus()
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
        const newFocus = otherPane(focusedPane)
        focusedPane = newFocus
        void saveAppStatus({ focusedPane: newFocus })
        void updateFocusedPane(newFocus)
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
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        leftPaneRef?.closeVolumeChooser()
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        rightPaneRef?.closeVolumeChooser()
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
        if (targetPane === 'left') {
            leftViewMode = mode
            void saveAppStatus({ leftViewMode: mode })
        } else {
            rightViewMode = mode
            void saveAppStatus({ rightViewMode: mode })
        }
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
            sortState.selectedIndices,
            sortState.allSelected,
        )

        setPaneSort(pane, column, newOrder)
        void saveAppStatus({ [paneKey(pane, 'sortBy')]: column })
        void saveColumnSortOrder(column, newOrder)
        applySortResult(paneRef, result)
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
            const containingVolume = await findContainingVolume(volume.path)
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
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
            const inNetwork: boolean = paneRef.isInNetworkView?.() ?? false
            if (inNetwork) {
                // Network views handle name lookup locally
                // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
                const idx: number = paneRef.findNetworkItemIndex?.(to) ?? -1
                if (idx >= 0) {
                    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                    paneRef.setCursorIndex?.(idx)
                }
            } else {
                // File listing: find index via backend
                // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
                const listingId: string | undefined = paneRef.getListingId?.()
                if (listingId) {
                    const backendIndex = await findFileIndex(listingId, to, showHiddenFiles)
                    if (backendIndex !== null) {
                        // Backend index doesn't include ".." entry, but frontend does
                        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-call
                        const hasParent: boolean = paneRef.hasParentEntry?.() ?? false
                        const frontendIndex = hasParent ? backendIndex + 1 : backendIndex
                        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
                        paneRef.setCursorIndex?.(frontendIndex)
                    }
                }
            }
        }
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
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex,a11y_no_noninteractive_element_interactions -->
<div
    class="dual-pane-explorer"
    bind:this={containerElement}
    onkeydown={handleKeyDown}
    onkeyup={handleKeyUp}
    tabindex="0"
    role="application"
    aria-label="File explorer"
>
    {#if initialized}
        <div class="pane-wrapper" style="width: {leftPaneWidthPercent}%">
            <!--suppress JSUnresolvedReference -->
            <FilePane
                bind:this={leftPaneRef}
                paneId="left"
                initialPath={leftPath}
                volumeId={leftVolumeId}
                volumePath={leftVolumePath}
                volumeName={leftVolumeName}
                isFocused={focusedPane === 'left'}
                {showHiddenFiles}
                viewMode={leftViewMode}
                sortBy={leftSortBy}
                sortOrder={leftSortOrder}
                onPathChange={(path: string) => {
                    handlePathChange('left', path)
                }}
                onVolumeChange={(volumeId: string, volumePath: string, targetPath: string) =>
                    handleVolumeChange('left', volumeId, volumePath, targetPath)}
                onRequestFocus={() => {
                    handleFocus('left')
                }}
                onSortChange={(column: SortColumn) => handleSortChange('left', column)}
                onNetworkHostChange={(host: NetworkHost | null) => {
                    handleNetworkHostChange('left', host)
                }}
                onCancelLoading={(selectName: string | undefined) => {
                    handleCancelLoading('left', selectName)
                }}
                onMtpFatalError={(msg: string) => handleMtpFatalError('left', msg)}
            />
        </div>
        <PaneResizer onResize={handlePaneResize} onResizeEnd={handlePaneResizeEnd} onReset={handlePaneResizeReset} />
        <div class="pane-wrapper" style="width: {100 - leftPaneWidthPercent}%">
            <!--suppress JSUnresolvedReference -->
            <FilePane
                bind:this={rightPaneRef}
                paneId="right"
                initialPath={rightPath}
                volumeId={rightVolumeId}
                volumePath={rightVolumePath}
                volumeName={rightVolumeName}
                isFocused={focusedPane === 'right'}
                {showHiddenFiles}
                viewMode={rightViewMode}
                sortBy={rightSortBy}
                sortOrder={rightSortOrder}
                onPathChange={(path: string) => {
                    handlePathChange('right', path)
                }}
                onVolumeChange={(volumeId: string, volumePath: string, targetPath: string) =>
                    handleVolumeChange('right', volumeId, volumePath, targetPath)}
                onRequestFocus={() => {
                    handleFocus('right')
                }}
                onSortChange={(column: SortColumn) => handleSortChange('right', column)}
                onNetworkHostChange={(host: NetworkHost | null) => {
                    handleNetworkHostChange('right', host)
                }}
                onCancelLoading={(selectName: string | undefined) => {
                    handleCancelLoading('right', selectName)
                }}
                onMtpFatalError={(msg: string) => handleMtpFatalError('right', msg)}
            />
        </div>
    {:else}
        <LoadingIcon />
    {/if}
</div>

<DialogManager
    {showCopyDialog}
    {copyDialogProps}
    {volumes}
    {showCopyProgressDialog}
    {copyProgressProps}
    {showNewFolderDialog}
    {newFolderDialogProps}
    {showAlertDialog}
    {alertDialogProps}
    {showCopyErrorDialog}
    {copyErrorProps}
    onCopyConfirm={handleCopyConfirm}
    onCopyCancel={handleCopyCancel}
    onCopyComplete={handleCopyComplete}
    onCopyCancelled={handleCopyCancelled}
    onCopyError={handleCopyError}
    onCopyErrorClose={handleCopyErrorClose}
    onNewFolderCreated={handleNewFolderCreated}
    onNewFolderCancel={handleNewFolderCancel}
    onAlertClose={handleAlertClose}
/>

<style>
    .dual-pane-explorer {
        display: flex;
        width: 100%;
        height: 100%;
        gap: 0;
        outline: none;
    }

    .pane-wrapper {
        display: flex;
        flex-direction: column;
        height: 100%;
        min-width: 0;
    }
</style>
