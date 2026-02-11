<script lang="ts">
    import { onMount, onDestroy, untrack, tick } from 'svelte'
    import FilePane from './FilePane.svelte'
    import PaneResizer from './PaneResizer.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import DialogManager from './DialogManager.svelte'
    import { toBackendCursorIndex } from '$lib/file-operations/transfer/transfer-dialog-utils'
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
        type TransferDialogPropsData,
        type TransferContext,
        buildTransferPropsFromSelection,
        buildTransferPropsFromCursor,
        buildTransferPropsFromDroppedPaths,
        getDestinationVolumeInfo,
    } from './transfer-operations'
    import type { TransferOperationType } from '../types'
    import { getInitialFolderName, moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
    import { getCurrentWebview } from '@tauri-apps/api/webview'
    import {
        getIsDraggingFromSelf,
        resetDraggingFromSelf,
        matchesSelfDragFingerprint,
        markAsSelfDrag,
        storeSelfDragFingerprint,
        clearSelfDragFingerprint,
        getSelfDragFileInfos,
    } from '../drag-drop'
    import { resolveDropTarget } from '../drop-target-hit-testing'
    import DragOverlay from '../DragOverlay.svelte'
    import { showOverlay, updateOverlay, hideOverlay, type OverlayFileInfo } from '../drag-overlay.svelte'
    import { getCachedIcon } from '$lib/icon-cache'
    import {
        startModifierTracking,
        stopModifierTracking,
        getIsAltHeld,
        setAltHeld,
    } from '../modifier-key-tracker.svelte'

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
    let unlistenDragDrop: UnlistenFn | undefined
    let unlistenDragImageSize: UnlistenFn | undefined
    let unlistenDragModifiers: UnlistenFn | undefined

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
    let leftPaneWrapperEl: HTMLDivElement | undefined = $state()
    let rightPaneWrapperEl: HTMLDivElement | undefined = $state()

    // Transfer dialog state (copy/move)
    let showTransferDialog = $state(false)
    let transferDialogProps = $state<TransferDialogPropsData | null>(null)

    // Transfer progress dialog state
    let showTransferProgressDialog = $state(false)
    let transferProgressProps = $state<{
        operationType: TransferOperationType
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

    // Transfer error dialog state
    let showTransferErrorDialog = $state(false)
    let transferErrorProps = $state<{
        operationType: TransferOperationType
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
            sortState.backendSelectedIndices,
            sortState.allSelected,
        )

        setPaneSort(pane, newColumn, newOrder)
        void saveAppStatus({ [paneKey(pane, 'sortBy')]: newColumn })
        void saveColumnSortOrder(newColumn, newOrder)
        applySortResult(paneRef, result, sortState.hasParent)
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
                void openTransferDialog('copy')
                return true
            case 'F6':
                void openTransferDialog('move')
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

        transferDialogProps = {
            ...buildTransferPropsFromDroppedPaths(operation, paths, destPath, targetPane, destVolId, sortBy, sortOrder),
            allowOperationToggle: true,
        }
        showTransferDialog = true
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
        // Skip the overlay when:
        // - Self-drag: Cmdr already renders a native drag image via canvas
        // - External drag with large source image: the OS drag preview is informative (like Finder)
        const suppressOverlay = getIsDraggingFromSelf() || externalDragHasLargeImage
        if (!suppressOverlay) {
            const overlayInfos = buildOverlayFileInfos(paths)
            showOverlay(overlayInfos, paths.length)
        }
        startModifierTracking()
        handleDragOver(position)
    }

    /** Updates drop-target highlights and overlay as the cursor moves during a drag. */
    function handleDragOver(position: { x: number; y: number }) {
        const resolved = resolveDropTarget(position.x, position.y, leftPaneWrapperEl, rightPaneWrapperEl)

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
        const resolved = resolveDropTarget(position.x, position.y, leftPaneWrapperEl, rightPaneWrapperEl)
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
                handleDragEnter(paths, event.payload.position)
            } else if (type === 'over') {
                handleDragOver(event.payload.position)
            } else if (type === 'drop') {
                handleDrop(event.payload.paths, event.payload.position)
                resetDraggingFromSelf()
                clearSelfDragFingerprint()
                externalDragHasLargeImage = false
            } else {
                // 'leave' — cursor left the window or drag was cancelled
                clearDropTargets()
                hideOverlay()
                stopModifierTracking()
                resetDraggingFromSelf()
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
        if (leftVolumeId === unmountedId) {
            leftVolumeId = defaultVolumeId
            leftPath = homePath
            void saveAppStatus({ leftVolumeId: defaultVolumeId, leftPath: homePath })
        }
        if (rightVolumeId === unmountedId) {
            rightVolumeId = defaultVolumeId
            rightPath = homePath
            void saveAppStatus({ rightVolumeId: defaultVolumeId, rightPath: homePath })
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
        unlistenDragImageSize?.()
        unlistenDragModifiers?.()
        unlistenDragDrop?.()
        cleanupNetworkDiscovery()
        stopModifierTracking()
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

    /** Closes any confirmation dialog (new folder or transfer) if open (for MCP). */
    export function closeConfirmationDialog() {
        if (showNewFolderDialog) {
            showNewFolderDialog = false
            newFolderDialogProps = null
            containerElement?.focus()
        }
        if (showTransferDialog) {
            showTransferDialog = false
            transferDialogProps = null
            containerElement?.focus()
        }
    }

    /** Returns whether any confirmation dialog is currently open. */
    export function isConfirmationDialogOpen(): boolean {
        return showNewFolderDialog || showTransferDialog
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
            transferDialogProps = props
            showTransferDialog = true
        }
    }

    /** Opens the transfer dialog with the current selection info. */
    export async function openTransferDialog(operationType: TransferOperationType) {
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

        // MTP move guard: move to/from MTP not yet supported
        if (operationType === 'move') {
            const sourceVolId = getPaneVolumeId(focusedPane)
            if (sourceVolId.startsWith('mtp-') || destVolId.startsWith('mtp-')) {
                alertDialogProps = {
                    title: 'Not supported yet',
                    message: "Move between MTP devices isn't supported yet. You can use copy instead.",
                }
                showAlertDialog = true
                return
            }
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

    function handleTransferConfirm(
        destination: string,
        _volumeId: string,
        previewId: string | null,
        conflictResolution: ConflictResolution,
        operationType: TransferOperationType,
    ) {
        if (!transferDialogProps) return

        // Store the props needed for the progress dialog (operationType may have been toggled by the user)
        transferProgressProps = {
            operationType,
            sourcePaths: transferDialogProps.sourcePaths,
            sourceFolderPath: transferDialogProps.sourceFolderPath,
            destinationPath: destination,
            direction: transferDialogProps.direction,
            sortColumn: transferDialogProps.sortColumn,
            sortOrder: transferDialogProps.sortOrder,
            previewId,
            sourceVolumeId: transferDialogProps.sourceVolumeId,
            destVolumeId: transferDialogProps.destVolumeId,
            conflictResolution,
        }

        // Close transfer dialog and open progress dialog
        showTransferDialog = false
        transferDialogProps = null
        showTransferProgressDialog = true
    }

    function handleTransferCancel() {
        showTransferDialog = false
        transferDialogProps = null
        containerElement?.focus()
    }

    /** Refreshes panes after a transfer completes — for move, refresh both panes. */
    function refreshPanesAfterTransfer() {
        const destPaneRef = transferProgressProps?.direction === 'right' ? rightPaneRef : leftPaneRef
        const sourcePaneRef = transferProgressProps?.direction === 'right' ? leftPaneRef : rightPaneRef
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call
        destPaneRef?.refreshView?.()
        // For move, source files disappeared — refresh source pane too
        if (transferProgressProps?.operationType === 'move') {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            sourcePaneRef?.refreshView?.()
        }
    }

    function handleTransferComplete(filesProcessed: number, bytesProcessed: number) {
        const op = transferProgressProps?.operationType ?? 'copy'
        log.info(
            `${op === 'copy' ? 'Copy' : 'Move'} complete: ${String(filesProcessed)} files (${formatBytes(bytesProcessed)})`,
        )

        refreshPanesAfterTransfer()

        showTransferProgressDialog = false
        transferProgressProps = null
        containerElement?.focus()
    }

    function handleTransferCancelled(filesProcessed: number) {
        const op = transferProgressProps?.operationType ?? 'copy'
        log.info(`${op === 'copy' ? 'Copy' : 'Move'} cancelled after ${String(filesProcessed)} files`)

        refreshPanesAfterTransfer()

        showTransferProgressDialog = false
        transferProgressProps = null
        containerElement?.focus()
    }

    function handleTransferError(error: WriteOperationError) {
        const op = transferProgressProps?.operationType ?? 'copy'
        log.error('{op} failed: {errorType}', { op: op === 'copy' ? 'Copy' : 'Move', errorType: error.type, error })

        refreshPanesAfterTransfer()

        showTransferProgressDialog = false
        transferProgressProps = null

        // Show the error dialog
        transferErrorProps = { operationType: op, error }
        showTransferErrorDialog = true
    }

    function handleTransferErrorClose() {
        showTransferErrorDialog = false
        transferErrorProps = null
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
            sortState.backendSelectedIndices,
            sortState.allSelected,
        )

        setPaneSort(pane, column, newOrder)
        void saveAppStatus({ [paneKey(pane, 'sortBy')]: column })
        void saveColumnSortOrder(column, newOrder)
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
        <div
            class="pane-wrapper"
            class:drop-target-active={dropTargetPane === 'left'}
            style="width: {leftPaneWidthPercent}%"
            bind:this={leftPaneWrapperEl}
        >
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
        <div
            class="pane-wrapper"
            class:drop-target-active={dropTargetPane === 'right'}
            style="width: {100 - leftPaneWidthPercent}%"
            bind:this={rightPaneWrapperEl}
        >
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

<DragOverlay />

<DialogManager
    {showTransferDialog}
    {transferDialogProps}
    {volumes}
    {showTransferProgressDialog}
    {transferProgressProps}
    {showNewFolderDialog}
    {newFolderDialogProps}
    {showAlertDialog}
    {alertDialogProps}
    {showTransferErrorDialog}
    {transferErrorProps}
    onTransferConfirm={handleTransferConfirm}
    onTransferCancel={handleTransferCancel}
    onTransferComplete={handleTransferComplete}
    onTransferCancelled={handleTransferCancelled}
    onTransferError={handleTransferError}
    onTransferErrorClose={handleTransferErrorClose}
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
        background-color: var(--color-bg-hover);
    }
</style>
