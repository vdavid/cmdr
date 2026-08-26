<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import type {
        FileEntry,
        FriendlyError,
        NetworkHost,
        SelectPayload,
        SortColumn,
        SortOrder,
        VisibleRangePayload,
    } from '../types'
    import {
        refreshListingIndexSizes,
        type Location,
        updateMenuContext,
    } from '$lib/tauri-commands'
    import { createTypeToJumpController } from './type-to-jump-controller.svelte'
    import TypeToJumpIndicator from './TypeToJumpIndicator.svelte'
    import type { ViewMode } from '$lib/app-status-store'
    import type { CommandId } from '$lib/commands'
    import { tooltip } from '$lib/tooltip/tooltip'

    /** State snapshot for swapping panes without backend calls. */
    export interface SwapState {
        currentPath: string
        listingId: string
        totalCount: number
        cursorIndex: number
        selectedIndices: number[]
        lastSequence: number
    }
    import FullList from '../views/FullList.svelte'
    import BriefList from '../views/BriefList.svelte'
    import SelectionInfo from '../selection/SelectionInfo.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
    import VolumeBreadcrumb from '../navigation/VolumeBreadcrumb.svelte'
    import { splitPathSegments } from '../navigation/path-segments'
    import { enrichBreadcrumbSegments } from '../navigation/breadcrumb-navigation'
    import RepoChip from '../git/RepoChip.svelte'
    import { createGitBrowserSync } from './git-browser-sync.svelte'
    import { createListingLoader } from './listing-loader'
    import { createSmbViewState } from './smb-view-state.svelte'
    import { createVolumeSpace } from './volume-space.svelte'
    import ErrorPane from './ErrorPane.svelte'
    import VolumeUnreachableBanner from './VolumeUnreachableBanner.svelte'
    import SmbReauthView from './SmbReauthView.svelte'
    import NetworkMountView from './NetworkMountView.svelte'
    import SearchResultsView from './SearchResultsView.svelte'
    import type { SearchResultsViewAPI, VolumeChangePayload } from './types'
    import { getSnapshot } from '$lib/search/snapshot-store.svelte'
    import MtpConnectionView from './MtpConnectionView.svelte'
    import SmbReconnectingView from './SmbReconnectingView.svelte'
    import { smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
    import NetworkLoginForm from '../network/NetworkLoginForm.svelte'
    import { createSelectionState } from './selection-state.svelte'
    import { createPaneMcpSync } from './pane-mcp-sync.svelte'
    import { initListingDiffSync } from './listing-diff-sync.svelte'
    import { createRenameState } from '../rename/rename-state.svelte'
    import { type DirectorySortMode } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'
    import { createRenameFlow } from './rename-flow.svelte'
    import ExtensionChangeDialog from '../rename/ExtensionChangeDialog.svelte'
    import RenameConflictDialog from '../rename/RenameConflictDialog.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { createDebounce } from '$lib/utils/timing'

    const log = getAppLogger('fileExplorer')
    import { isMtpVolumeId } from '$lib/mtp'
    import { getPaneTintBg, getPaneTintName } from './volume-tint.svelte'
    import { createCursorNavKeys } from './cursor-nav-keys'
    import { createSearchPaneKeys } from './search-pane-keys'
    import { computeHasParent } from './has-parent'
    import { firstSelectedIndex } from './first-selected-index'
    import { capabilitiesForPane } from './volume-capabilities'
    import { createEnterMenu } from './enter-menu.svelte'
    import Menu from '$lib/ui/Menu.svelte'
    import { homeDir } from '@tauri-apps/api/path'
    import { type CanonicalPath, parentOf, toCanonical } from '$lib/path/canonical'
    import { getVolumes as getStoreVolumes } from '$lib/stores/volume-store.svelte'
    import type { UnreachableState } from '../tabs/tab-types'
    import { getDiskUsageLevel, getUsedPercent, formatBarTooltip } from '../disk-space-utils'
    import { getTypeToJumpResetDelay } from '$lib/settings/reactive-settings.svelte'
    import { createRowOverlays } from './row-overlays.svelte'
    import { createSelectionInfoFeed } from './selection-info-feed.svelte'
    import { createPaneKeyRouter } from './pane-key-router'
    import { createEntryActivation } from './entry-activation'
    import { createPanePointer } from './pane-pointer'
    import { breadcrumbDisplayPath, createBreadcrumbHandlers } from './breadcrumb-bar'
    import { createDeletedDirPoll } from './deleted-dir-poll'
    import { fetchEntriesSnapshot, fetchSelectedNames } from './entries-snapshot'
    import { resolveInitialPathAction, shouldReloadAfterReachable } from './path-sync'
    import { resyncAfterHiddenFilesToggle } from './hidden-files-resync'
    import { createNetworkHostState } from './network-host-state.svelte'
    import { createMtpDisconnectWatch } from './mtp-disconnect-watch.svelte'
    import { formatByteSize } from '$lib/units'

    interface Props {
        initialPath: string
        paneId?: 'left' | 'right'
        volumeId?: string
        volumePath?: string
        volumeName?: string
        isFocused?: boolean
        showHiddenFiles?: boolean
        viewMode?: ViewMode
        sortBy?: SortColumn
        sortOrder?: SortOrder
        directorySortMode?: DirectorySortMode
        onPathChange?: (path: string) => void
        onVolumeChange?: (change: VolumeChangePayload) => void
        /**
         * Go to an already-resolved `Location` (volume id + path). Used when a row
         * on the search-results pane opens a real entry: the pane resolves the
         * entry's volume, then bubbles it here so `navigate()` switches volumes and
         * lands on the right one. Distinct from `onVolumeChange` (deliberate volume
         * (re)select): `onGoToLocation` is the go-to-a-location intent and carries
         * no `volumePath` (the switch arm resolves it via `getVolumePathById`).
         */
        onGoToLocation?: (location: Location) => void
        onSortChange?: (column: SortColumn) => void
        onRequestFocus?: () => void
        /** Called when active network host changes (for history tracking) */
        onNetworkHostChange?: (host: NetworkHost | null) => void
        /** Called when user cancels loading (ESC key) - parent navigates back to previous folder */
        onCancelLoading?: (cancelledPath: string, selectName?: string) => void
        /** Called when MTP connection fails fatally (device disconnected, timeout) - parent should fall back to previous volume */
        onMtpFatalError?: (error: string) => void
        /**
         * A header-encrypted archive needs a password even to browse it: bubble the
         * browse-time prompt request to the parent (which owns the dialog state).
         * `retry` re-lists the same directory after the password is stored.
         */
        onArchiveNeedsPassword?: (info: {
            volumeId: string
            archivePath: string
            wrongAttempt: boolean
            retry: () => void
        }) => void
        /** Volume resolution timed out for this tab: show banner instead of file list */
        unreachable?: UnreachableState | null
        /** Called when user clicks "Retry" on the unreachable banner */
        onRetryUnreachable?: () => void
        /** Called when user clicks "Open home folder" on the unreachable banner or the error screen */
        onOpenHome?: () => void
        /**
         * Whether this pane's active tab can walk back in history. Drives the error
         * screen's "Go back" button, which stays hidden when `nav.back` would no-op
         * (a first-paint error: history isn't persisted across sessions).
         */
        canGoBack?: boolean
        /** Called when user clicks "Go back" on the error screen */
        onGoBack?: () => void
        /**
         * Bubbles a high-level command id out of the pane. Used by the Selection
         * dialog's `+` / `-` shortcuts so the parent route can dispatch via the
         * unified command-dispatch path without FilePane importing it. Receives
         * a `CommandId` (`'selection.selectFiles'` / `'selection.deselectFiles'`).
         */
        onCommand?: (commandId: CommandId) => void
    }

    const {
        initialPath,
        paneId = 'left',
        volumeId = 'root',
        volumePath = '/',
        volumeName,
        isFocused = false,
        showHiddenFiles = false,
        viewMode = 'full',
        sortBy = 'name',
        sortOrder = 'ascending',
        directorySortMode = 'likeFiles',
        onPathChange,
        onVolumeChange,
        onGoToLocation,
        onSortChange,
        onRequestFocus,
        onNetworkHostChange,
        onCancelLoading,
        onMtpFatalError,
        onArchiveNeedsPassword,
        unreachable = null,
        onRetryUnreachable,
        onOpenHome,
        canGoBack = false,
        onGoBack,
        onCommand,
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // New architecture: store listingId and totalCount, not files. These lifecycle
    // slots are written primarily by the listing loader (`listing-loader.ts`,
    // via injected setters) but read by many non-loader concerns, so they stay here.
    let listingId = $state('')
    let totalCount = $state(0)
    let loading = $state(true)
    let error = $state<string | null>(null)
    let friendlyError = $state<FriendlyError | null>(null)

    let cursorIndex = $state(0)

    // Selection state (extracted to selection-state.svelte.ts)
    const selection = createSelectionState({
        onChanged: () => {
            debouncedSyncMcp.call()
        },
    })

    // Operation snapshot: tracks which files were selected when an operation started,
    // so the diff handler can adjust selection as files disappear.
    let operationSelectedNames = $state<string[] | 'all' | null>(null)
    let diffGeneration = 0 // NOT $state: only used in async callbacks, never for rendering

    // Type-to-jump: per-pane buffer + indicator + the IPC fuzzy-match runner and
    // the MCP mirror of the last matched name, all in a `*.svelte.ts` controller.
    // The reset delay is read live from Settings on each keystroke (reactive
    // getter), so moving the slider takes effect on the next keystroke. FilePane
    // reads `jump.buffer` / `.indicatorVisible` / `.indicatorStale` /
    // `.lastMatchedName` and keeps one-line handleJumpKeystroke / isJumpActive /
    // clearJumpState delegates.
    const jump = createTypeToJumpController({
        getResetMs: () => getTypeToJumpResetDelay(),
        getListingId: () => listingId,
        getLoading: () => loading,
        getHasBackendListing: () => caps.hasBackendListing,
        getIsMtpDeviceOnly: () => isMtpDeviceOnly,
        getIncludeHidden: () => includeHidden,
        getHasParent: () => hasParent,
        setCursorIndex: (index: number) => void setCursorIndex(index),
        onSyncMcp: () => { debouncedSyncMcp.call(); },
    })

    // Rename state (inline rename editor)
    const rename = createRenameState()

    // Listing loader: the streaming directory-load pipeline + the generation /
    // listingId drop-foreign-listings token model, in a `*.svelte.ts` factory.
    // The pane's lifecycle `$state` (listingId / loading / totalCount / error /
    // …) STAYS here (many non-loader readers); the loader reads/writes it through
    // the accessors below. Deps are deferred closures, so the state they touch may
    // be declared later in this file (the pattern `jump` already uses for
    // `debouncedSyncMcp`).
    const loader = createListingLoader({
        paneId,
        getVolumeId: () => volumeId,
        getVolumePath: () => volumePath,
        getCurrentPath: () => currentPath,
        setCurrentPath: (path) => {
            currentPath = path
        },
        getCanonicalPath: () => canonicalPath,
        getIncludeHidden: () => includeHidden,
        getSortBy: () => sortBy,
        getSortOrder: () => sortOrder,
        getDirectorySortMode: () => directorySortMode,
        getCaps: () => caps,
        getHasParent: () => hasParent,
        getIsMtpView: () => isMtpView,
        getViewMode: () => viewMode,
        getBriefListRef: () => briefListRef,
        getFullListRef: () => fullListRef,
        getListingId: () => listingId,
        setListingId: (id) => {
            listingId = id
        },
        getLoading: () => loading,
        setLoading: (value) => {
            loading = value
        },
        getTotalCount: () => totalCount,
        setTotalCount: (count) => {
            totalCount = count
        },
        getLastSequence: () => lastSequence,
        setLastSequence: (sequence) => {
            lastSequence = sequence
        },
        setError: (value) => {
            error = value
        },
        setFriendlyError: (value) => {
            friendlyError = value
        },
        setOpeningFolder: (value) => {
            openingFolder = value
        },
        setLoadingCount: (count) => {
            loadingCount = count
        },
        setFinalizingCount: (count) => {
            finalizingCount = count
        },
        setVolumeRootFromEvent: (root) => {
            volumeRootFromEvent = root
        },
        getCursorIndex: () => cursorIndex,
        setCursorIndexRaw: (index) => {
            cursorIndex = index
        },
        clearEntryUnderCursor: () => {
            selectionInfo.clearEntry()
        },
        clearSyncStatusMap: () => {
            overlays.clearSyncStatusMap()
        },
        clearIndexStatusMap: () => {
            overlays.clearIndexStatusMap()
        },
        clearFolderCoverageMap: () => {
            overlays.clearFolderCoverageMap()
        },
        clearSyncRetryTimer: () => {
            overlays.clearSyncRetryTimer()
        },
        bumpCacheGeneration: () => {
            cacheGeneration++
        },
        selection,
        renameCancel: () => { rename.cancel(); },
        renameForgetChainReports: () => { renameFlow.forgetChainReports(); },
        jumpClear: () => { jump.clear(); },
        syncMcp: () => {
            debouncedSyncMcp.call()
        },
        fetchEntryUnderCursor: () => void selectionInfo.fetchEntry(),
        fetchListingStats: () => void selectionInfo.fetchStats(),
        onPathChange: (path) => onPathChange?.(path),
        onVolumeChange: (change) => onVolumeChange?.(change),
        onMtpFatalError: (message) => onMtpFatalError?.(message),
        onCancelLoading: (cancelledPath, selectName) => onCancelLoading?.(cancelledPath, selectName),
        onArchiveNeedsPassword: (info) => onArchiveNeedsPassword?.(info),
    })

    // Volume root path from listing-complete event (accurate for MTP and all volume types)
    let volumeRootFromEvent = $state<string | undefined>(undefined)


    import type { ListViewAPI, VolumeBreadcrumbAPI, NetworkMountViewAPI, NetworkCursorEntry } from './types'
    import type { StartRenameOptions } from './types'
    import type { DragAutoScrollFrameResult, DragAutoScrollPointer } from '../drag/drag-auto-scroll'

    // Component refs for keyboard navigation
    let fullListRef: ListViewAPI | undefined = $state()
    let briefListRef: ListViewAPI | undefined = $state()
    let volumeBreadcrumbRef: VolumeBreadcrumbAPI | undefined = $state()
    let networkMountViewRef: NetworkMountViewAPI | undefined = $state()
    let searchResultsViewRef: SearchResultsViewAPI | undefined = $state()
    // The pane's root element, so the Enter-behavior popup can anchor at the cursor row.
    let paneEl: HTMLElement | undefined = $state()

    // The Browse | Open | Configure popup shown when an archive/bundle set to Ask is
    // opened. Rendered near the pane root; opened from `handleNavigate`.
    const enterMenu = createEnterMenu({
        getPaneElement: () => paneEl ?? null,
        browse: (entry) => void activation.browseIntoEntry(entry),
        open: (entry) => void activation.openEntryExternally(entry),
        restoreFocus: () => onRequestFocus?.(),
    })
    onDestroy(() => {
        enterMenu.dispose()
    })

    /**
     * This pane's volume capabilities: what this pane is allowed to do. Resolved from `volumeId` AND `currentPath`
     * (kind-from-path): a path inside a supported archive resolves the read-only
     * `archive` kind even though `volumeId` is the writable parent drive; the two
     * virtual ids short-circuit in `volumeKindOf` before the store lookup, and
     * other real ids read `fsType`/`category` from the volume store. The
     * view-selection discriminant, the named view deriveds below, and the
     * per-feature gates all read off this.
     */
    const caps = $derived(capabilitiesForPane(volumeId, currentPath))

    // Check if we're viewing the network (special virtual volume). Sourced from
    // the kind, not a `volumeId === 'network'` string compare.
    const isNetworkView = $derived(caps.kind === 'network')

    /**
     * Check if we're viewing a search-results snapshot (the other virtual volume,
     * `search-results://<id>`). Behaves like the network view: no backend listing,
     * no file watcher, no git lookups, no pane-state-to-MCP sync. The pane renders
     * `SearchResultsView` which pulls the snapshot from the in-memory store.
     * Most code paths that gate on `isNetworkView` also gate on this; the few
     * exceptions are noted at each call site. Sourced from the kind, not a
     * `volumeId === 'search-results'` string compare.
     */
    const isSearchResultsView = $derived(caps.kind === 'search-results')

    /**
     * The phone-storage caveat for the disk-space readout, only on MTP volumes
     * (keyed on `caps.kind`, not a volume-id string). Over USB a phone
     * exposes only its shared storage, so the browsable folders add up to far
     * less than the space reported as used; this explains the gap on hover.
     */
    const mtpSpaceHint = $derived(caps.kind === 'mtp' ? tString('fileExplorer.navigation.spaceMtpHint') : undefined)

    /**
     * Snapshot id encoded in `currentPath` for the search-results pane (`search-results://<id>`),
     * or `null` for any other pane / unparseable path. Drives the breadcrumb label, the
     * row-count for keyboard cursor clamping, and the view's snapshot lookup.
     */
    const searchSnapshotId = $derived(
        isSearchResultsView && currentPath.startsWith('search-results://')
            ? currentPath.slice('search-results://'.length)
            : null,
    )

    /** Live snapshot lookup. Re-derives on path/id change. */
    const searchSnapshot = $derived(searchSnapshotId ? getSnapshot(searchSnapshotId) : undefined)

    /** Number of result rows in the active snapshot, or 0 when not on a search-results pane. */
    const searchResultsCount = $derived(searchSnapshot?.entries.length ?? 0)

    // User's home directory path (e.g. "/Users/veszelovszki"), fetched once on mount
    let userHomePath = $state('')

    // Canonical form of `currentPath` (`~` expanded). Null until `userHomePath`
    // resolves on mount, or when `currentPath` is not absolute / ~-rooted
    // (e.g. transient values during volume switches).
    const canonicalPath = $derived.by((): CanonicalPath | null => {
        if (!userHomePath) return null
        try {
            return toCanonical(currentPath, userHomePath)
        } catch {
            return null
        }
    })

    // The three per-row badge feeds (cloud sync, image-index file badge, folder
    // coverage): their maps, fetchers, live setting gates, idle poll, and the
    // enrichment-driven refresh live in a `*.svelte.ts` factory. The List
    // components call the fetchers and render the maps; the listing loader clears
    // them on a listing swap.
    const overlays = createRowOverlays({
        getVolumeId: () => volumeId,
        getListingId: () => listingId,
        getIsLocalPane: () => caps.kind === 'local',
    })

    // The deleted-directory fallback poll: two confirmed misses before walking up,
    // with the SMB/timeout and virtual-git-path skips (`deleted-dir-poll.ts`).
    const deletedDirPoll = createDeletedDirPoll({
        getListingId: () => listingId,
        getLoading: () => loading,
        getHasBackendListing: () => caps.hasBackendListing,
        getIsMtpView: () => isMtpView,
        getCurrentPath: () => currentPath,
        getVolumePath: () => volumePath,
        navigateToFallback: loader.navigateToFallback,
    })

    // ── Git browser ─────────────────────────────────────────────────────
    // The breadcrumb repo chip + file-list git-status column: their toggles,
    // the reactive RepoInfo lookup, and the subscribe/unsubscribe lifecycle live
    // in a `*.svelte.ts` factory. The factory owns the path-change `$effect`; the
    // component reads `gitBrowser.gitRepoInfo` / `.showRepoChip` /
    // `.showGitStatusColumn` and calls `cleanup()` on destroy.
    const gitBrowser = createGitBrowserSync({
        getCurrentPath: () => currentPath,
        getVolumeId: () => volumeId,
        getHasBackendListing: () => caps.hasBackendListing,
    })

    // The path shown after the volume name (`~`-folded, volume-relative, MTP
    // form, or the snapshot label). Pure derivation in `breadcrumb-bar.ts`.
    const breadcrumbDisplayPathValue = $derived(
        breadcrumbDisplayPath({
            currentPath,
            volumeId,
            volumePath,
            userHomePath,
            isSearchResultsView,
            searchLabel: searchSnapshot?.label,
        }),
    )

    // Segmented form of the breadcrumb path so we can color anything inside
    // a `.git/...` portal with the git-portal token. Pure derivation; the
    // helper is unit-tested in `path-segments.test.ts`.
    //
    // R3 B6: for search-results panes the displayPath is the snapshot label
    // (e.g. `*.pdf` or `/some/regex/`), not a real filesystem path. We render
    // it as a single segment so a regex label containing `/` doesn't get
    // broken up into path-style segments with separator glyphs.
    const breadcrumbSegments = $derived(
        isSearchResultsView
            ? [{ text: breadcrumbDisplayPathValue, gitPortal: false }]
            : splitPathSegments(breadcrumbDisplayPathValue),
    )

    // Each segment enriched with a navigation `target` (null when not clickable)
    // and a friendly `displayPath` for the tooltip. Clicking a clickable segment
    // navigates to that ancestor via the normal pane nav (history + errors).
    // Pure logic lives in `breadcrumb-navigation.ts` (unit-tested).
    const clickableBreadcrumbSegments = $derived(
        enrichBreadcrumbSegments(breadcrumbSegments, {
            volumeId,
            volumePath,
            currentPath,
            userHomePath,
            isSearchResults: isSearchResultsView,
        }),
    )

    // Check if we're viewing an MTP device
    const isMtpView = $derived(isMtpVolumeId(volumeId))

    // Check if this is a device-only MTP ID (needs connection)
    // Device-only IDs start with "mtp-" but don't contain ":" (no storage ID)
    const isMtpDeviceOnly = $derived(isMtpView && volumeId.startsWith('mtp-') && !volumeId.includes(':'))

    /**
     * The KIND-structural alt-view selector for the `{#if}` chain below. It picks
     * which non-list view a pane renders purely as a function of `caps.kind` (plus
     * the MTP device-only connection sub-state, which the kind table doesn't carry
     * — it's a runtime connection state, not a kind). This is NOT a new component:
     * it's a derived discriminant the existing chain branches on.
     *
     * Only the KIND-driven branches live here. The runtime-state branches
     * (`unreachable`, SMB reconnecting / gave-up, the inline SMB upgrade login,
     * `loading` / `friendlyError` / `error`) stay per-feature and gate IN FRONT of
     * this in the chain, with byte-identical precedence: a runtime
     * state always wins over the kind view, exactly as the string-compare chain did.
     */
    const paneViewKind = $derived<'network' | 'search-results' | 'mtp-connect' | 'normal'>(
        isNetworkView ? 'network' : isSearchResultsView ? 'search-results' : isMtpDeviceOnly ? 'mtp-connect' : 'normal',
    )

    // Look up the live volume info (used for the share name in the reconnecting
    // view and to decide whether subscribing to the SMB reconnect manager is
    // even relevant for this pane).
    const currentVolumeInfo = $derived(getStoreVolumes().find((v) => v.id === volumeId) ?? null)
    /**
     * True on a mounted disk image (.dmg): a transient, effectively-full mount. Its free space
     * is meaningless, so we skip the space query and hide the bottom disk-usage bar.
     */
    const isDiskImageVolume = $derived(currentVolumeInfo?.isDiskImage === true)
    /**
     * Background tint for this pane based on the user's volume-type tint settings.
     * `null` when the user picked "no tint" for this volume's kind (the common case).
     */
    const paneTintBg = $derived(getPaneTintBg(volumeId, currentVolumeInfo?.fsType, currentVolumeInfo?.category))
    /**
     * Active tint name (or null) for `data-pane-tint` on `.file-pane`. The
     * selection-fg fallback rule in `app.css` keys off this attribute to
     * switch text color when the tinted bg + cursor-active would otherwise
     * push selection-fg below AA. Always tracks `paneTintBg`.
     */
    const paneTintName = $derived(getPaneTintName(volumeId, currentVolumeInfo?.fsType, currentVolumeInfo?.category))

    // SMB reconnect + direct-upgrade view state: the alt-view decision deriveds
    // (reconnecting / gave-up / needs-auth), the reconnect-manager subscription
    // effect, and the cancel / disconnect / connect-directly handlers live in a
    // `*.svelte.ts` factory. The pane keeps the shared `currentVolumeInfo` derived
    // (tint + disk-image + eject read it too) and passes it in.
    const smbView = createSmbViewState({
        getVolumeId: () => volumeId,
        getCurrentPath: () => currentPath,
        getVolumePath: () => volumePath,
        getCurrentVolumeInfo: () => currentVolumeInfo,
        loadDirectory: (path: string) => void loader.loadDirectory(path),
        navigateToFallback: loader.navigateToFallback,
    })

    // Live per-pane disk space: the readout, the fetch, the backend live-update
    // listener, and the watch/unwatch registration live in a `*.svelte.ts` factory.
    // The pane keeps a one-line `refreshVolumeSpace` delegate (a FilePaneAPI export)
    // and drives watch/unwatch across mount, volume-switch, and destroy.
    const diskSpace = createVolumeSpace({
        paneId,
        getVolumeId: () => volumeId,
        getCurrentPath: () => currentPath,
        getVolumePath: () => volumePath,
        getIsDiskImage: () => isDiskImageVolume,
    })

    // The Network host the pane has open (and any share queued to auto-mount on
    // it), cleared whenever the pane leaves the network volume by ANY route
    // (`network-host-state.svelte.ts`).
    const networkHost = createNetworkHostState({
        getIsNetworkView: () => isNetworkView,
        onHostChange: (host) => onNetworkHostChange?.(host),
    })

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function toggleVolumeChooser() {
        volumeBreadcrumbRef?.toggle()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isVolumeChooserOpen(): boolean {
        return volumeBreadcrumbRef?.getIsOpen() ?? false
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function closeVolumeChooser() {
        volumeBreadcrumbRef?.close()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function openVolumeChooser() {
        volumeBreadcrumbRef?.open()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleVolumeChooserKeyDown(e: KeyboardEvent): boolean {
        return volumeBreadcrumbRef?.handleKeyDown(e) ?? false
    }

    export function getListingId(): string {
        return listingId
    }

    export function isLoading(): boolean {
        return loading
    }

    /**
     * Returns a promise that resolves when the current load (if any) settles.
     * Used by `moveCursor` to avoid acting on a not-yet-cached `listingId`.
     * Delegates to the listing loader (which owns the `pendingLoad` machinery).
     */
    export function whenLoadSettles(): Promise<void> {
        return loader.whenLoadSettles()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getFilenameUnderCursor(): string | undefined {
        return selectionInfo.entry?.name
    }

    /**
     * Absolute path of the entry under the cursor (or `undefined` when the listing is empty
     * or hasn't resolved the entry yet). Reads the feed's reactive entry, so
     * Quick Look's cursor-follow $effect in `DualPaneExplorer.svelte` stays subscribed
     * across cursor moves, listing swaps, and pane switches.
     */
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getPathUnderCursor(): string | undefined {
        return selectionInfo.entry?.path
    }

    /**
     * The full `FileEntry` under the cursor (or `null`). Used by the
     * "Copy path between panes" command to detect whether the cursor sits on
     * a directory (incl. symlinks-to-directories) vs. a file or `..`.
     * `..` is reported as-is (as a synthetic parent entry); callers should
     * filter on `name === '..'` if needed.
     */
    // noinspection JSUnusedGlobalSymbols -- used by DualPaneExplorer.copyPathBetweenPanes
    export function getCursorEntry(): FileEntry | null {
        return selectionInfo.entry
    }

    /**
     * The network browser's cursor target (host or share), or `null` when
     * this pane is not in the network view or nothing valid is under the cursor.
     */
    // noinspection JSUnusedGlobalSymbols -- used by DualPaneExplorer.copyPathBetweenPanes
    export function getNetworkCursorEntry(): NetworkCursorEntry | null {
        if (!isNetworkView) return null
        return networkMountViewRef?.getNetworkCursorEntry() ?? null
    }

    /** Also scrolls to make the cursor visible and syncs state to MCP. */
    export async function setCursorIndex(index: number): Promise<void> {
        if (isNetworkView) {
            networkMountViewRef?.setCursorIndex(index)
            return
        }
        if (isSearchResultsView) {
            cursorIndex = index
            searchResultsViewRef?.setCursorIndex(index)
            await tick()
            return
        }
        cursorIndex = index
        // The cursor-entry refetch is handled by the feed's $effect tracking cursorIndex
        // Scroll to make cursor visible
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        listRef?.scrollToIndex(index)
        // Wait for scroll effects to complete before syncing to MCP
        await tick()
        debouncedSyncMcp.call()
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getCursorIndex(): number {
        return cursorIndex
    }

    /**
     * Total cursor-addressable rows (includes the `..` row; snapshot panes use the
     * snapshot's count). Used by MCP `move_cursor` to range-check an index before
     * setting it, since `setCursorIndex` stores the value unclamped.
     */
    export function getEffectiveTotalCount(): number {
        return effectiveTotalCount
    }

    export function autoScrollDuringDrag(
        position: DragAutoScrollPointer,
        elapsedMs: number,
    ): DragAutoScrollFrameResult {
        if (paneViewKind !== 'normal') return { active: false, scrolled: false }
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        return listRef?.autoScrollDuringDrag?.(position, elapsedMs) ?? { active: false, scrolled: false }
    }

    /**
     * Awaitable, immediate MCP state push (skips the 300 ms debounce). MCP
     * round-trips that mutate pane state (by-name selection) call this before
     * replying, so the backend's `PaneStateStore` is fresh when the tool returns
     * OK — otherwise a follow-up tool call (select → copy) reads stale state.
     */
    export async function syncStateToMcpNow(): Promise<void> {
        await syncPaneStateToMcp()
    }

    /**
     * Sets the "land the cursor on this name when the next diff applies" marker.
     * The diff handler already reads `renameFlow.pendingCursorName` for the rename
     * flow; mkdir/mkfile reuse the same channel so a freshly-created entry can
     * dodge the structural cursor shift `adjustSelectionIndices` would otherwise
     * apply when an `add` lands at or above the cursor's index.
     */
    export function setPendingCursorName(name: string | null): void {
        renameFlow.pendingCursorName = name
    }

    /**
     * Handles one keystroke for the type-to-jump feature. Appends to the buffer,
     * fires the IPC match, and (on the response) moves the cursor.
     *
     * Streaming listings: per the plan, we do NOT auto-jump on
     * `listing-progress`: each keystroke = exactly one match against the
     * cache as it stands at that moment.
     */
    export function handleJumpKeystroke(char: string): void {
        jump.handleJumpKeystroke(char)
    }

    /**
     * True while a type-to-jump is active: the buffer holds at least one character
     * (i.e. before the reset timeout empties it). DualPaneExplorer reads this to
     * decide whether a printable keystroke extends the buffer or runs its command.
     */
    export function isJumpActive(): boolean {
        return jump.isJumpActive()
    }

    /** Clears the type-to-jump buffer + indicator + timers. Safe to call repeatedly. */
    export function clearJumpState(): void {
        jump.clearJumpState()
    }

    /** Find an item by name in network views. Returns index or -1. */
    export function findNetworkItemIndex(name: string): number {
        return networkMountViewRef?.findItemIndex(name) ?? -1
    }

    /** Cursor-addressable rows in the network view (hosts or shares), `0` outside it. */
    // noinspection JSUnusedGlobalSymbols -- used by DualPaneExplorer.moveCursor's range check
    export function getNetworkItemCount(): number {
        if (!isNetworkView) return 0
        return networkMountViewRef?.getItemCount() ?? 0
    }

    export function isInNetworkView(): boolean {
        return isNetworkView
    }

    /** Refresh network hosts (used by ⌘R shortcut). */
    export function refreshNetworkHosts(): void {
        networkMountViewRef?.refreshNetworkHosts()
    }

    export function getSelectedIndices(): number[] {
        return selection.getSelectedIndices()
    }

    /** Whether ".." is shown (needed for index adjustment in copy/move). */
    export function hasParentEntry(): boolean {
        return hasParent
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isAllSelected(): boolean {
        return selection.isAllSelected(hasParent, effectiveTotalCount)
    }

    export function setSelectedIndices(indices: number[]): void {
        selection.setSelectedIndices(indices)
    }

    export function clearSelection(): void {
        selection.clearSelection()
    }

    export function selectAll(): void {
        selection.selectAll(hasParent, effectiveTotalCount)
    }

    export function toggleSelectionAtCursor(): void {
        selection.toggleAt(cursorIndex, hasParent)
    }

    /**
     * Toggle selection at cursor, then move cursor down by one row. Mirrors
     * the Total Commander Insert-key behavior. `toggleAt` no-ops on `..` (the
     * parent entry isn't selectable); the cursor still advances. At the last
     * row the selection toggles but the cursor stays put (no wrap-around).
     */
    export function toggleSelectionAndMoveDownAtCursor(): void {
        selection.toggleAt(cursorIndex, hasParent)
        if (cursorIndex < effectiveTotalCount - 1) {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            cursorNav.applyNavigation({ newIndex: cursorIndex + 1, listRef, shiftKey: false })
        }
    }

    export function selectRange(startIndex: number, endIndex: number): void {
        selection.selectRange(startIndex, endIndex, hasParent)
    }

    /**
     * Bulk-apply indices to the selection (add or remove). Used by the Selection
     * dialog at commit time. Skips `..` per `hasParent`. Range anchor/end state
     * is untouched so the user's prior keyboard/mouse anchor survives.
     *
     * On a SELECT (`mode === 'add'`), the cursor jumps to the first newly-selected
     * file and scrolls into view, so the user lands looking at their selection
     * instead of wherever the cursor happened to sit. We derive the target through
     * the SAME `hasParent` skip `selection.applyIndices` uses (`firstSelectedIndex`),
     * so the cursor can never land on the synthetic `..` row. On a DESELECT
     * (`mode === 'remove'`) we leave the cursor put: there's nothing freshly
     * selected to reveal, and yanking the cursor onto a just-deselected row is odd.
     */
    export function applyIndices(idxs: number[], mode: 'add' | 'remove'): void {
        selection.applyIndices(idxs, mode, hasParent)
        if (mode === 'add') {
            const target = firstSelectedIndex(idxs, hasParent)
            if (target !== null) void setCursorIndex(target)
        }
    }

    /**
     * Returns a snapshot of the pane's entries for the Selection dialog. The
     * dialog needs the full list at open-time to run its matcher; this method
     * fetches all entries via `getFileRange` for normal panes, or reads them
     * directly from the search-results snapshot.
     *
     * Indices in the returned array match the pane's selection-state indices,
     * so the `..` parent row (when present) is INCLUDED at index 0 as a synthetic
     * entry. Selection's matcher will skip index 0 via the existing `hasParent`
     * rule in `selection-state::applyIndices`.
     */
    // noinspection JSUnusedGlobalSymbols -- consumed by DualPaneExplorer.getFocusedPaneEntries
    export function getEntriesSnapshot(): Promise<FileEntry[]> {
        return fetchEntriesSnapshot({
            listingId,
            totalCount,
            hasParent,
            showHiddenFiles,
            canonicalPath,
            isSearchResultsView,
            searchSnapshot,
        })
    }

    /** Cursor index inside the entries-snapshot returned by `getEntriesSnapshot()`. */
    // noinspection JSUnusedGlobalSymbols -- consumed by DualPaneExplorer.getFocusedPaneEntries
    export function getEntriesCursorIndex(): number {
        return cursorIndex
    }

    /** Snapshots the current selection as file names for diff-driven adjustment during operations. */
    export async function snapshotSelectionForOperation(): Promise<void> {
        operationSelectedNames = await fetchSelectedNames({
            listingId,
            includeHidden,
            hasParent,
            isAllSelected: selection.isAllSelected(hasParent, effectiveTotalCount),
            selectedIndices: selection.getSelectedIndices(),
        })
    }

    /** Clears the operation snapshot and invalidates in-flight findFileIndices callbacks. Returns the previous value. */
    export function clearOperationSnapshot(): string[] | 'all' | null {
        const prev = operationSelectedNames
        operationSelectedNames = null
        diffGeneration++
        return prev
    }

    // ==== Rename flow (logic in rename-flow.svelte.ts) ====

    const renameFlow = createRenameFlow({
        rename,
        paneId,
        getListingId: () => listingId,
        getTotalCount: () => totalCount,
        getIncludeHidden: () => includeHidden,
        getCurrentPath: () => currentPath,
        getShowHiddenFiles: () => showHiddenFiles,
        getVolumeId: () => volumeId,
        getEntryUnderCursor,
        onRequestFocus: () => onRequestFocus?.(),
        getCursorIndex: () => cursorIndex,
        getEffectiveTotalCount: () => effectiveTotalCount,
        getHasParent: () => hasParent,
        getEntryAt: (index: number) => activeListRef()?.getEntryAt(index),
        indexOfEntry: (path: string) => activeListRef()?.indexOfEntry(path),
        moveCursorTo,
    })

    // Destructure handlers: factory methods don't use `this`, safe to destructure
    /* eslint-disable @typescript-eslint/unbound-method -- factory return, no `this` */
    const {
        handleRenameInput,
        handleRenameSubmit,
        handleRenameCancel,
        handleRenameClickAway,
        handleRenameStep,
        handleRenameShakeEnd,
        handleExtensionKeepOld,
        handleExtensionUseNew,
        handleConflictResolve,
    } = renameFlow
    /* eslint-enable @typescript-eslint/unbound-method */

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isRenaming(): boolean {
        return rename.active
    }

    export function startRename(options?: StartRenameOptions): void {
        // Type-to-jump must not linger over the inline rename editor.
        jump.clear()
        renameFlow.startRename(options)
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function cancelRename(): void {
        renameFlow.cancelRename()
    }

    // Cache generation counter — bumped on **cold context changes** (sort,
    // hidden-files toggle, explicit refresh, listing swap). The List components
    // treat this as a hard reset: wipe rendered entries and column widths,
    // refetch from scratch.
    let cacheGeneration = $state(0)

    // Soft-refresh tick — bumped on every `directory-diff` event (bulk delete,
    // copy, rename). The List components refetch the visible range in the
    // background and atomically replace, keeping existing entries on screen
    // until the new ones land. This is what prevents the empty-pane flicker
    // that destructive `cacheGeneration` bumps caused mid-bulk-op.
    let softRefreshTick = $state(0)

    // Throttle the brief-mode column-width refetch during diff bursts. Without
    // this, a 10 k-file delete fires one `get_brief_column_text_widths` IPC per
    // coalesced event (~20/sec), each forcing a layout reflow. ~200 ms trailing
    // means at most ~5 width recomputes/sec, with the final widths always
    // landing after the burst ends.
    let columnWidthRefetchTimer: ReturnType<typeof setTimeout> | null = null
    function scheduleColumnWidthRefetch(): void {
        if (viewMode !== 'brief') return
        if (columnWidthRefetchTimer !== null) return
        columnWidthRefetchTimer = setTimeout(() => {
            columnWidthRefetchTimer = null
            briefListRef?.refetchColumnWidths?.()
        }, 200)
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function refreshView(): void {
        cacheGeneration++
    }

    export async function refreshVolumeSpace(): Promise<void> {
        await diskSpace.refresh()
    }

    /** Re-fetches index sizes (recursive_size, etc.) without a full list rebuild. */
    export function refreshIndexSizes(): void {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        listRef?.refreshIndexSizes()
        // Re-enrich backend cache entries so the stats fetch sees fresh recursive_size values
        if (listingId) {
            void refreshListingIndexSizes(listingId).then(() => selectionInfo.fetchStats())
        }
        // Refresh the cursor entry too so SelectionInfo's Brief size readout (and
        // its "size updating" hourglass) tracks the storm live, not just on cursor moves.
        void selectionInfo.fetchEntry()
        // Mirror the refreshed sizes (and the `recursiveSizePending` hourglass flag)
        // into the MCP pane state so agents see `[size-pending]` update live during
        // an index storm, not just on cursor/nav changes. Debounced (300ms), so a
        // burst of index-dir-updated refreshes coalesces into one sync.
        debouncedSyncMcp.call()
    }

    export function getSwapState(): SwapState {
        return loader.getSwapState()
    }

    export function adoptListing(state: SwapState): void {
        loader.adoptListing(state)
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function isMtp(): boolean {
        return isMtpView
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getVolumeId(): string {
        return volumeId
    }

    export function getCurrentPath(): string {
        return currentPath
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function setNetworkHost(host: NetworkHost | null): void {
        networkHost.setHost(host)
        networkMountViewRef?.setNetworkHost(host)
    }

    /**
     * Queues a share to auto-mount once `NetworkMountView`'s `ShareBrowser` is ready.
     * Survives a not-yet-mounted view because the value is held here and re-passed
     * via the `initialAutoMountShare` prop. Cleared automatically when the pane
     * leaves the network volume.
     */
    // noinspection JSUnusedGlobalSymbols -- used by DualPaneExplorer.copyPathBetweenPanes
    export function setNetworkAutoMount(shareName: string | undefined): void {
        networkHost.setAutoMountShare(shareName)
    }

    /**
     * Navigates up and selects the folder we came from. Returns false if already at root.
     *
     * On the Network volume "up" walks the browser stack, not a directory tree: a
     * host's share list (and the mount-error pane it turns into on a mount that
     * didn't go through) steps back to the host list, exactly like the Escape /
     * Backspace / ⌘↑ the share list binds. Without this the file-list primitive had
     * nothing to walk, so ⌘↑ and the MCP `nav_to_parent` tool were silent no-ops
     * there and an agent had no way out of a failed mount.
     */
    export function navigateToParent(): Promise<boolean> {
        if (isNetworkView) {
            if (networkHost.host === null) return Promise.resolve(false)
            // Clear the view first (it also drops the mount error), then the pane
            // state, which bubbles the change out for history tracking.
            networkMountViewRef?.setNetworkHost(null)
            networkHost.handleHostChange(null)
            return Promise.resolve(true)
        }
        return loader.navigateToParent()
    }

    // Track last sequence for file watcher diffs (read/written by the loader's
    // swap-state accessors and by `listing-diff-sync`).
    let lastSequence = 0
    // Opening folder state (before read_dir starts - slow for network folders)
    let openingFolder = $state(false)
    // Loading progress state for streaming
    let loadingCount = $state<number | undefined>(undefined)
    // Finalizing state (read_dir done, now sorting/caching)
    let finalizingCount = $state<number | undefined>(undefined)

    // Derive includeHidden from showHiddenFiles prop
    const includeHidden = $derived(showHiddenFiles)

    // MCP state-sync factory: mirrors this pane into the `PaneState` store. Deps
    // pass reactive reads via getters so the factory lives in a plain `.svelte.ts`.
    const mcpSync = createPaneMcpSync({
        paneId,
        // The network + search-results skip folds into the kind's `syncsToMcp`
        // capability (false for both), read off the pane's derived `caps` rather
        // than the two `volumeId ===` deriveds.
        getSyncsToMcp: () => caps.syncsToMcp,
        getListingId: () => listingId,
        getTotalCount: () => totalCount,
        getHasParent: () => hasParent,
        getVisibleRangeStart: () => visibleRangeStart,
        getVisibleRangeEnd: () => visibleRangeEnd,
        getCanonicalPath: () => canonicalPath,
        getIncludeHidden: () => includeHidden,
        getCurrentPath: () => currentPath,
        getVolumeId: () => volumeId,
        getVolumeName: () => volumeName,
        getCursorIndex: () => cursorIndex,
        getViewMode: () => viewMode,
        getSelectedIndices: () => selection.getSelectedIndices(),
        getSortBy: () => sortBy,
        getSortOrder: () => sortOrder,
        getShowHiddenFiles: () => showHiddenFiles,
        getTypeToJump: () => ({
            buffer: jump.buffer,
            indicatorVisible: jump.indicatorVisible,
            indicatorStale: jump.indicatorStale,
        }),
        getLastJumpMatchedName: () => jump.lastMatchedName,
    })
    const syncPaneStateToMcp = mcpSync.syncPaneStateToMcp

    // Debounced/throttled IPC wrappers to avoid flooding the backend during rapid navigation.
    // The virtual scroll (cursorIndex → scrollToIndex → DOM) is fully synchronous and unaffected.
    const debouncedMenuContext = createDebounce(() => {
        const entry = selectionInfo.entry
        if (entry && entry.name !== '..') {
            void updateMenuContext(entry.path, entry.name)
        }
    }, 100)
    const debouncedSyncMcp = createDebounce(() => void syncPaneStateToMcp(), 300)

    /** Handle visible range change from list components */
    function handleVisibleRangeChange({ start, end }: VisibleRangePayload) {
        visibleRangeStart = start
        visibleRangeEnd = end
        debouncedSyncMcp.call()
    }

    // Check if current directory has a parent (not at filesystem root AND not at volume root)
    // Prefer volumeRoot from the listing event (accurate for MTP), fall back to prop (for initial state).
    // Inside an archive the backend emits the `.zip` path as the listing's volume root, but the FE's
    // volume is the PARENT drive (the tab keeps its id) — so at the archive root `/foo.zip` there IS a
    // parent (the zip's containing dir). Use the parent mount (`volumePath`), so `..` shows and
    // navigateToParent bubbles out of the archive instead of stopping at a false "volume root".
    const effectiveVolumeRoot = $derived(caps.kind === 'archive' ? volumePath : (volumeRootFromEvent ?? volumePath))
    // Search-results panes have NO `..` row: the snapshot is a flat result set, not a directory.
    // Without this gate, the path comparison was true (search-results://sr-N never matches a real
    // volume root), causing `hasParent` to be `true`, which made `selectAll` skip index 0 (P6).
    // R3 T1: the derivation lives in `has-parent.ts` so the regression test
    // (`has-parent.test.ts`) can pin the integration with `selection.selectAll`
    // without spinning up the whole `FilePane` component.
    const hasParent = $derived(
        computeHasParent({
            // The snapshot no-`..` rule comes from the kind capability, not a
            // `volumeId === 'search-results'` string compare, read off the
            // pane's derived `caps`.
            hasParentRow: caps.hasParentRow,
            currentPath,
            effectiveVolumeRoot,
        }),
    )

    // Effective total count includes ".." entry if not at root.
    // For search-results panes, the snapshot owns the count (the backend
    // `totalCount` state stays at 0 because no listing IPC ran). M8d depends on
    // this so Cmd+A / range-select span the snapshot's entries.
    const effectiveTotalCount = $derived.by(() => {
        if (isSearchResultsView) return searchResultsCount
        return hasParent ? totalCount + 1 : totalCount
    })

    // Track the visible range for MCP state sync
    // This is updated by the list components when they scroll
    let visibleRangeStart = $state(0)
    let visibleRangeEnd = $state(100)

    // The pending-load promise machinery, the streaming load pipeline, and the
    // drop-foreign token model live in the listing loader (`listing-loader.ts`).
    // FilePane keeps thin exported delegates for the FilePaneAPI surface.

    // Handle cancellation during loading (called from DualPaneExplorer on ESC)
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleCancelLoading() {
        loader.handleCancelLoading()
    }

    // Navigate to a specific path with optional item selection (used when cancelling navigation).
    // Returns a Promise that resolves when the directory listing completes, or rejects on error.
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function navigateToPath(path: string, selectName?: string): Promise<void> {
        return loader.navigateToPath(path, selectName)
    }

    // Mouse handling: row select (plain / Shift / Cmd), the context menu's
    // selection-vs-entry rule, the focus click, and the background double-click
    // that goes up a folder. All in `pane-pointer.ts`.
    const pointer = createPanePointer({
        getCursorIndex: () => cursorIndex,
        setCursorIndex: (index) => {
            cursorIndex = index
        },
        getHasParent: () => hasParent,
        getListingId: () => listingId,
        getIncludeHidden: () => includeHidden,
        getVolumeId: () => volumeId,
        getSelectedIndices: () => Array.from(selection.selectedIndices),
        onRequestFocus: () => onRequestFocus?.(),
        fetchCursorEntry: () => void selectionInfo.fetchEntry(),
        extendSelectionFromMouse: ({ index, cursorIndex: cursor, hasParent: parent }) =>
            { selection.handleShiftMouseNavigation(index, cursor, parent); },
        toggleSelectionAt: (index, parent) => { selection.toggleAt(index, parent); },
        clearRangeState: () => { selection.clearRangeState(); },
        clearJump: () => { jump.clear(); },
        navigateToParent: () => void navigateToParent(),
    })
    const handleSelect = pointer.handleSelect
    const handleContextMenu = pointer.handleContextMenu

    // Opening an entry (Enter, ⌘↓, double-click, or a popup choice): the redirect
    // arm, the archive/bundle Enter policy, the browse-in-place arm, the viewer
    // interim for files inside an archive, and the search-results "leave the
    // snapshot volume first" rule all live in `entry-activation.ts`.
    const activation = createEntryActivation({
        getCurrentPath: () => currentPath,
        setCurrentPath: (path) => {
            currentPath = path
        },
        getCanonicalPath: () => canonicalPath,
        getVolumeId: () => volumeId,
        getIsSearchResultsView: () => isSearchResultsView,
        loadDirectory: (path, selectName) => loader.loadDirectory(path, selectName),
        // The popup exists only for the `ask` policy, so the highlight starts there.
        openEnterMenu: (entry) => {
            enterMenu.openFor(entry, 'ask')
        },
        onGoToLocation: (location) => onGoToLocation?.(location),
    })
    const handleNavigate = activation.handleNavigate

    // The breadcrumb's three interactions (ancestor click, right-click menu with
    // its eject item, volume switch with the disk-space watch that follows it).
    const breadcrumb = createBreadcrumbHandlers({
        getCurrentVolumeInfo: () => currentVolumeInfo,
        navigateToPath: (path) => navigateToPath(path),
        setCurrentPath: (path) => {
            currentPath = path
        },
        onVolumeChange: (change) => onVolumeChange?.(change),
        onRequestFocus: () => onRequestFocus?.(),
        loadDirectory: (path) => void loader.loadDirectory(path),
        refreshSpace: () => void diskSpace.refresh(),
        watchSpace: (id, path) => {
            diskSpace.watch(id, path)
        },
        unwatchSpace: () => {
            diskSpace.unwatch()
        },
        clearSpace: () => {
            diskSpace.clear()
        },
    })

    // Cursor movement for the Brief/Full list views (arrows, Page/Home/End,
    // Shift-extend). The per-view step math lives in `../navigation/keyboard-shortcuts`
    // and the list components; this factory is the glue turning a keystroke into a
    // cursor move + scroll + selection fill. `applyNavigation` stays reachable for
    // `toggleSelectionAndMoveDownAtCursor`.
    const cursorNav = createCursorNavKeys({
        getCursorIndex: () => cursorIndex,
        applyCursor: (index: number) => {
            cursorIndex = index
        },
        extendSelection: ({ fromIndex, toIndex, overflow, hasParent: parent }) =>
            { selection.handleShiftKeyboardNavigation(fromIndex, toIndex, overflow, parent); },
        getHasParent: () => hasParent,
        getEffectiveTotalCount: () => effectiveTotalCount,
        getBriefListRef: () => briefListRef,
        getFullListRef: () => fullListRef,
    })

    /** The list view on screen, for row reads and scrolling. */
    function activeListRef(): ListViewAPI | undefined {
        return viewMode === 'brief' ? briefListRef : fullListRef
    }

    /** Gets the file entry under the cursor from the current list view */
    function getEntryUnderCursor(): FileEntry | undefined {
        return activeListRef()?.getEntryAt(cursorIndex)
    }

    /** Lands the cursor on a row and scrolls it into view. */
    function moveCursorTo(index: number): void {
        cursorNav.applyNavigation({ newIndex: index, listRef: activeListRef() })
    }

    /**
     *  Opens the entry under the cursor exactly like pressing Enter: navigates into a
     *  directory or hands a file to the OS default app. Returns a promise that resolves
     *  once the action completes (or rejects on failure), so callers (the MCP
     *  `open_under_cursor` round-trip) can ack on real completion rather than guessing.
     */
    // noinspection JSUnusedGlobalSymbols -- Used dynamically by DualPaneExplorer/MCP
    export async function openCursorItem(): Promise<void> {
        if (isNetworkView) {
            // Network view: cursor lives in NetworkBrowser/ShareBrowser, not the file list.
            // Delegate to NetworkMountView, which forwards to whichever child is active.
            networkMountViewRef?.openCursorItem()
            return
        }
        if (isSearchResultsView) {
            searchResultsViewRef?.openCursorItem()
            return
        }
        const entry = getEntryUnderCursor()
        if (!entry) {
            throw new Error('No entry under cursor')
        }
        await handleNavigate(entry)
    }

    // Search-results pane keyboard: the pure `computeSearchPaneKeyAction` dispatch
    // stays in `search-results-keys.ts`; the side-effect wiring (view/edit-file,
    // toggle, move + shift-extend) lives in `search-pane-keys.ts`. The snapshot
    // pane has no `..` row, so selection runs with `hasParent = false`.
    const searchPaneKeys = createSearchPaneKeys({
        getCursorIndex: () => cursorIndex,
        setCursorIndex: (index: number) => void setCursorIndex(index),
        getSearchResultsCount: () => searchResultsCount,
        getVisibleItemsCount: () => fullListRef?.getVisibleItemsCount?.() ?? 20,
        getSnapshotEntryAt: (index: number) => searchSnapshot?.entries[index],
        extendSelection: ({ fromIndex, toIndex, overflow }) =>
            { selection.handleShiftKeyboardNavigation(fromIndex, toIndex, overflow, false); },
        toggleSelectionAt: (index: number) => selection.toggleAt(index, false),
        openCursorItem: () => void openCursorItem(),
    })

    // Keydown routing for a focused pane: the rename / network / search-results
    // bails, the open + parent keys, the Selection dialog's `+` / `-`, the four
    // selection commands, and the Brief/Full split. All of it in
    // `pane-key-router.ts`; the pane keeps the refs and state it reads.
    const keyRouter = createPaneKeyRouter({
        getRenameActive: () => rename.active,
        getIsNetworkView: () => isNetworkView,
        getIsSearchResultsView: () => isSearchResultsView,
        getViewMode: () => viewMode,
        getHasParent: () => hasParent,
        getEntryUnderCursor,
        handleNetworkKeyDown: (e) => networkMountViewRef?.handleKeyDown(e),
        handleSearchResultsKeyDown: (e) => { searchPaneKeys.handleSearchResultsKeyDown(e); },
        handleBriefModeKeys: (e) => { cursorNav.handleBriefModeKeys(e); },
        handleFullModeKeys: (e) => { cursorNav.handleFullModeKeys(e); },
        openEntry: (entry) => void handleNavigate(entry),
        navigateToParent: () => void navigateToParent(),
        onCommand: (commandId) => onCommand?.(commandId),
        toggleSelectionAtCursor: () => { selection.toggleAt(cursorIndex, hasParent); },
        toggleSelectionAndMoveDown: toggleSelectionAndMoveDownAtCursor,
        selectAll: () => { selection.selectAll(hasParent, effectiveTotalCount); },
        deselectAll: () => { selection.deselectAll(); },
        clearRangeState: () => { selection.clearRangeState(); },
    })

    // Exported so DualPaneExplorer can forward keyboard events
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyDown(e: KeyboardEvent) {
        keyRouter.handleKeyDown(e)
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyUp(e: KeyboardEvent) {
        keyRouter.handleKeyUp(e)
    }

    /** Debug only: inject a FriendlyError into this pane to preview the error state. */
    export function injectError(friendly: FriendlyError) {
        error = null
        friendlyError = friendly
        loading = false
    }

    /**
     * Reactive: true when this pane is showing a full-pane error state — either
     * a `FriendlyError` (listing failed for an existing path) or the
     * `unreachable` banner (volume couldn't be resolved at startup, or SMB
     * reconnect gave up). Used by Quick Look's error-state hook in
     * DualPaneExplorer to close the panel when the focused pane goes into a
     * state where its `getPathUnderCursor()` would no longer return a
     * meaningful path.
     */
    // noinspection JSUnusedGlobalSymbols -- consumed by DualPaneExplorer's Quick Look effect
    export function isInErrorState(): boolean {
        return friendlyError !== null || unreachable !== null
    }

    // When includeHidden changes, cancel rename and re-sync the count + cursor
    // (`hidden-files-resync.ts`).
    $effect(() => {
        if (listingId && !loading) {
            // Cancel rename on hidden files toggle (spec: sort change / toggle hidden = cancel)
            untrack(() => {
                rename.cancel()
            })
            void resyncAfterHiddenFilesToggle({
                listingId,
                includeHidden,
                // Read cursor state without tracking to avoid infinite re-triggers
                nameToFollow: untrack(() => selectionInfo.entry?.name),
                cursorIndex: untrack(() => cursorIndex),
                getHasParent: () => hasParent,
                setTotalCount: (count) => {
                    totalCount = count
                },
                setCursorIndex,
            })
        }
    })

    // A tab that timed out at startup shows the unreachable banner; a successful
    // Retry clears it, and nothing else would load the listing. The decision
    // (including the "path changed, so the effect below owns it" case) is pure,
    // in `path-sync.ts`.
    let prevUnreachable = $state(unreachable)

    $effect(() => {
        if (shouldReloadAfterReachable({
            prevUnreachable,
            unreachable,
            initialPath,
            currentPath: untrack(() => currentPath),
        })) {
            log.info('Tab became reachable (retry succeeded), loading directory: {path}', { path: initialPath })
            void loader.loadDirectory(initialPath)
            void refreshVolumeSpace()
        }
        prevUnreachable = unreachable
    })

    // Track the previous volumeId to detect MTP connection completion
    let prevVolumeId = $state(volumeId)

    // Reactive path loading: handles persistence restore AND MTP connection
    // completion in one effect, so overlapping triggers can't both fire a
    // `loadDirectory`. The truth table is pure, in `path-sync.ts`.
    $effect(() => {
        const action = resolveInitialPathAction({
            initialPath, // Track this
            currentPath: untrack(() => currentPath), // Don't track: user navigation changes this
            prevVolumeId,
            volumeId,
            isSearchResultsView,
            isNetworkView,
            isMtpDeviceOnly,
        })
        prevVolumeId = volumeId

        switch (action.kind) {
            case 'mtp-connected':
                log.info('MTP volume connected, loading directory: {path}', { path: action.path })
                currentPath = action.path
                void loader.loadDirectory(action.path)
                break
            case 'load':
                log.debug('[FilePane] initialPath effect: triggering loadDirectory, paneId={paneId}, newPath={newPath}', {
                    paneId,
                    newPath: action.path,
                })
                currentPath = action.path
                void loader.loadDirectory(action.path)
                break
            case 'sync-path':
                currentPath = action.path
                break
            case 'none':
                break
        }
    })

    // Update global menu context when cursor position or focus changes (debounced: only matters for right-click)
    $effect(() => {
        if (!isFocused) return
        const entry = selectionInfo.entry
        if (entry && entry.name !== '..') {
            debouncedMenuContext.call()
        }
    })

    // The pane's cursor-entry + listing-stats feed: the two fetchers, their
    // debounce/throttle wrappers, the cursor-move and selection-change effects,
    // and the search-results snapshot mirror live in a `*.svelte.ts` factory.
    // Created here so its effects keep their place in the pane's effect order.
    const selectionInfo = createSelectionInfoFeed({
        getListingId: () => listingId,
        getLoading: () => loading,
        getTotalCount: () => totalCount,
        getCursorIndex: () => cursorIndex,
        getHasParent: () => hasParent,
        getCanonicalPath: () => canonicalPath,
        getIncludeHidden: () => includeHidden,
        getIsSearchResultsView: () => isSearchResultsView,
        getSearchSnapshot: () => searchSnapshot,
        getSelectedIndices: () => Array.from(selection.selectedIndices),
        getSelectionSize: () => selection.selectedIndices.size,
        syncMcp: () => {
            debouncedSyncMcp.call()
        },
    })

    // Scroll the entry under the cursor into view when view mode changes
    $effect(() => {
        void viewMode
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            listRef?.scrollToIndex(cursorIndex)
        })
    })

    // File-watcher sync: directory-diff reconciliation (cursor + selection),
    // write-source-item-done gradual deselection, and directory-deleted fallback.
    // Registered once during init; deps pass reactive reads via getters and the
    // few mutations back via setters/callbacks (see `listing-diff-sync.svelte.ts`).
    initListingDiffSync({
        selection,
        rename,
        renameFlow,
        getListingId: () => listingId,
        getIncludeHidden: () => includeHidden,
        getHasParent: () => hasParent,
        getCursorIndex: () => cursorIndex,
        setCursorIndex,
        applyCursorIndex: (index: number) => {
            cursorIndex = index
        },
        getCurrentPath: () => currentPath,
        getVolumePath: () => volumePath,
        getOperationSelectedNames: () => operationSelectedNames,
        getLastSequence: () => lastSequence,
        setLastSequence: (sequence: number) => {
            lastSequence = sequence
        },
        getDiffGeneration: () => diffGeneration,
        bumpDiffGeneration: () => ++diffGeneration,
        setTotalCount: (count: number) => {
            totalCount = count
        },
        bumpSoftRefreshTick: () => {
            softRefreshTick++
        },
        scheduleColumnWidthRefetch,
        fetchEntryUnderCursor: () => void selectionInfo.fetchEntry(),
        fetchListingStats: () => void selectionInfo.fetchStats(),
        onRequestFocus,
        navigateToFallback: loader.navigateToFallback,
    })

    // The pane's MTP device being unplugged: the listener re-registers itself on
    // every volume switch, so it can't fire on a stale device id.
    // (`mtp-disconnect-watch.svelte.ts`.)
    createMtpDisconnectWatch({
        getVolumeId: () => volumeId,
        onFatal: (message) => onMtpFatalError?.(message),
    })

    // NOTE: MTP file watching now uses the unified directory-diff event system
    // (same as local volumes). The existing directory-diff listener above handles
    // both local and MTP changes, providing smooth incremental updates.

    onMount(() => {
        // Fetch user home dir for breadcrumb display (~ substitution)
        void homeDir().then((h) => {
            userHomePath = h.endsWith('/') ? h.slice(0, -1) : h
        })

        // Live disk-space updates from the backend poller (typed event).
        diskSpace.startListening()

        // Skip directory loading for:
        // - Network views (they handle their own data via NetworkBrowser/ShareBrowser)
        // - Device-only MTP views (they need connection first, handled by auto-connect effect)
        // But DO load for connected MTP views (storage-specific volume ID)
        log.debug(
            '[FilePane] onMount: paneId={paneId}, volumeId={volumeId}, currentPath={currentPath}, isNetworkView={isNetworkView}, isMtpDeviceOnly={isMtpDeviceOnly}',
            { paneId, volumeId, currentPath, isNetworkView, isMtpDeviceOnly },
        )
        if (unreachable) {
            log.debug('[FilePane] onMount: SKIPPING loadDirectory for unreachable tab, paneId={paneId}', { paneId })
            loading = false
        } else if (!isNetworkView && !isMtpDeviceOnly && !isSearchResultsView) {
            log.debug('[FilePane] onMount: triggering loadDirectory for paneId={paneId}', { paneId })
            void loader.loadDirectory(currentPath)
            // Disk images have no meaningful free space: no poll, no bar, no SelectionInfo text.
            if (!isDiskImageVolume) {
                void diskSpace.refresh()
                // Register for live disk-space polling
                diskSpace.watch(volumeId, currentPath)
            }
        } else {
            log.debug('[FilePane] onMount: SKIPPING loadDirectory for paneId={paneId}', { paneId })
            // Clear the initial `loading = true` for virtual-volume panes (network /
            // search-results) — they don't go through the loadDirectory pipeline that
            // would otherwise flip it false. Without this clear, the LoadingIcon stays
            // up forever and the virtual view never renders.
            loading = false
        }

        // Start the idle sync-status poll and the image-index enrichment listeners.
        overlays.start()

        // Detect a directory deleted behind our back and walk up to the nearest
        // surviving parent (`deleted-dir-poll.ts`).
        deletedDirPoll.start()
    })

    onDestroy(() => {
        // Stop the background Finder-tag sweep, cancel the active listing, drop its
        // per-path icons, and unlisten the six streaming listeners. All loader-owned.
        loader.cleanup()
        deletedDirPoll.stop()
        // Drop the badge feeds' poll, retry timer, enrichment listeners, and pending refresh.
        overlays.cleanup()
        selectionInfo.cleanup()
        debouncedMenuContext.cancel()
        debouncedSyncMcp.cancel()
        // Stop type-to-jump timers so they can't fire after the FilePane is gone
        // (otherwise orphan setTimeouts mutate $state slots on the dead instance).
        jump.dispose()
        // Drop the disk-space live listener + this pane's space watch.
        diskSpace.cleanup()
        // Drop the git subscriptions (setting listeners + repo watcher) on unmount.
        gitBrowser.cleanup()
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    bind:this={paneEl}
    class="file-pane"
    class:is-focused={isFocused}
    onclick={pointer.handlePaneClick}
    ondblclick={pointer.handlePaneBackgroundDblClick}
    onkeydown={() => {}}
    role="region"
    aria-label={tString('fileExplorer.pane.filePaneAriaLabel', { side: paneId })}
    style={paneTintBg ? `--color-pane-bg: ${paneTintBg}` : undefined}
    data-pane-tint={paneTintName ?? undefined}
>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="header" oncontextmenu={breadcrumb.handleContextMenu}>
        <VolumeBreadcrumb
            bind:this={volumeBreadcrumbRef}
            {volumeId}
            {currentPath}
            onVolumeChange={breadcrumb.handleVolumeChange}
            onSmbUpgradeLogin={smbView.handleSmbUpgradeLogin}
        />
        <span class="path">{#each clickableBreadcrumbSegments as seg, i (i)}{#if i > 0 && seg.text !== ''}<span class="path-sep">/</span>{/if}{#if seg.target !== null}<button
                    type="button"
                    class="path-segment"
                    class:git-portal={seg.gitPortal}
                    use:tooltip={tString('fileExplorer.breadcrumb.navigateTooltip', { path: seg.displayPath })}
                    onclick={() => { breadcrumb.handleSegmentClick(seg.target as string); }}
                >{seg.text}</button>{:else}<span class:git-portal={seg.gitPortal}>{seg.text}</span>{/if}{/each}</span>
        {#if gitBrowser.showRepoChip && gitBrowser.gitRepoInfo}
            <RepoChip info={gitBrowser.gitRepoInfo} />
        {/if}
    </div>
    <div class="content">
        <TypeToJumpIndicator
            buffer={jump.buffer}
            visible={jump.indicatorVisible}
            stale={jump.indicatorStale}
        />
        {#if unreachable}
            <VolumeUnreachableBanner
                originalPath={unreachable.originalPath}
                retrying={unreachable.retrying}
                onRetry={() => onRetryUnreachable?.()}
                onOpenHome={() => onOpenHome?.()}
            />
        {:else if smbView.showSmbReconnecting && smbView.reconnectState}
            <SmbReconnectingView
                {volumeId}
                shareName={currentVolumeInfo?.name ?? volumeId}
                cycleState={smbView.reconnectState}
                onCancel={smbView.handleSmbReconnectCancel}
                onDisconnect={smbView.handleSmbReconnectDisconnect}
            />
        {:else if smbView.showSmbNeedsAuth}
            <SmbReauthView
                {volumeId}
                serverLabel={currentVolumeInfo?.name ?? volumePath}
                onCancel={smbView.handleSmbReconnectDisconnect}
            />
        {:else if smbView.showSmbGaveUp}
            <VolumeUnreachableBanner
                originalPath={currentVolumeInfo?.name ?? volumePath}
                retrying={false}
                onRetry={() => { smbReconnectManager.retryNow(volumeId); }}
                smbGaveUp={true}
                onDisconnect={smbView.handleSmbReconnectDisconnect}
            />
        {:else if paneViewKind === 'network'}
            <NetworkMountView
                bind:this={networkMountViewRef}
                {paneId}
                {isFocused}
                initialNetworkHost={networkHost.host}
                initialAutoMountShare={networkHost.autoMountShare}
                {onVolumeChange}
                onNetworkHostChange={networkHost.handleHostChange}
            />
        {:else if paneViewKind === 'search-results'}
            <SearchResultsView
                bind:this={searchResultsViewRef}
                path={currentPath}
                {cursorIndex}
                {isFocused}
                {sortBy}
                {sortOrder}
                selectedIndices={selection.selectedIndices}
                onNavigate={(entry: FileEntry) => { void handleNavigate(entry) }}
                onSelect={({ index, shiftKey, metaKey }: SelectPayload) => {
                    // Reuse the regular pane's click semantics so shift-range
                    // and cmd-toggle behave identically. The snapshot pane has
                    // no `..` row, so `hasParent` is always false; `handleSelect`
                    // honours it via the bound `hasParent` state. M8d.
                    handleSelect({ index, shiftKey: shiftKey ?? false, metaKey: metaKey ?? false })
                }}
                onVisibleRangeChange={handleVisibleRangeChange}
            />
        {:else if paneViewKind === 'mtp-connect'}
            <MtpConnectionView {volumeId} {onVolumeChange} />
        {:else if smbView.smbUpgradeLogin}
            <NetworkLoginForm
                host={{
                    id: smbView.smbUpgradeLogin.volumeId,
                    name: smbView.smbUpgradeLogin.displayName,
                    port: smbView.smbUpgradeLogin.port,
                }}
                shareName={smbView.smbUpgradeLogin.share}
                authMode="guest_allowed"
                defaultConnectionMode="credentials"
                errorMessage={smbView.smbUpgradeLogin.errorMessage}
                isConnecting={smbView.smbUpgradeLogin.isConnecting}
                onConnect={smbView.handleSmbUpgradeConnect}
                onCancel={smbView.handleSmbUpgradeCancel}
            />
        {:else if loading}
            <LoadingIcon {openingFolder} loadedCount={loadingCount} {finalizingCount} showCancelHint={true} />
        {:else if friendlyError}
            <ErrorPane
                friendly={friendlyError}
                folderPath={currentPath}
                onRetry={() => navigateToPath(currentPath)}
                {canGoBack}
                onGoBack={() => onGoBack?.()}
                onGoHome={() => onOpenHome?.()}
                {isFocused}
            />
        {:else if error}
            <div class="error-message">{error}</div>
        {:else if viewMode === 'brief'}
            <BriefList
                bind:this={briefListRef}
                {listingId}
                {volumeId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {cacheGeneration}
                {softRefreshTick}
                {cursorIndex}
                {isFocused}
                syncStatusMap={overlays.syncStatusMap}
                indexStatusMap={overlays.indexStatusMap}
                folderCoverageMap={overlays.folderCoverageMap}
                selectedIndices={selection.selectedIndices}
                {hasParent}
                {sortBy}
                {sortOrder}
                renameState={rename.active ? rename : null}
                parentPath={hasParent && canonicalPath ? parentOf(canonicalPath) : ''}
                {currentPath}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={overlays.fetchSyncStatusForPaths}
                onIndexStatusRequest={overlays.fetchIndexStatusForPaths}
                onFolderCoverageRequest={overlays.fetchFolderCoverageForPaths}
                onSortChange={onSortChange
                    ? (column: SortColumn) => {
                          onSortChange(column)
                      }
                    : undefined}
                onVisibleRangeChange={handleVisibleRangeChange}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
                onRenameClickAway={handleRenameClickAway}
                onRenameStep={handleRenameStep}
                onRenameShakeEnd={handleRenameShakeEnd}
                onStartRename={startRename}
                onDragInitiate={clearJumpState}
            />
        {:else}
            <FullList
                bind:this={fullListRef}
                {listingId}
                {volumeId}
                totalCount={effectiveTotalCount}
                {includeHidden}
                {cacheGeneration}
                {softRefreshTick}
                {cursorIndex}
                {isFocused}
                syncStatusMap={overlays.syncStatusMap}
                indexStatusMap={overlays.indexStatusMap}
                folderCoverageMap={overlays.folderCoverageMap}
                selectedIndices={selection.selectedIndices}
                {hasParent}
                {sortBy}
                {sortOrder}
                gitRepoRoot={gitBrowser.gitRepoInfo?.repoRoot ?? null}
                showGitColumn={gitBrowser.showGitStatusColumn}
                renameState={rename.active ? rename : null}
                parentPath={hasParent && canonicalPath ? parentOf(canonicalPath) : ''}
                {currentPath}
                onSelect={handleSelect}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                onSyncStatusRequest={overlays.fetchSyncStatusForPaths}
                onIndexStatusRequest={overlays.fetchIndexStatusForPaths}
                onFolderCoverageRequest={overlays.fetchFolderCoverageForPaths}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
                onRenameClickAway={handleRenameClickAway}
                onRenameStep={handleRenameStep}
                onRenameShakeEnd={handleRenameShakeEnd}
                onStartRename={startRename}
                onSortChange={onSortChange
                    ? (column: SortColumn) => {
                          onSortChange(column)
                      }
                    : undefined}
                onVisibleRangeChange={handleVisibleRangeChange}
                onDragInitiate={clearJumpState}
            />
        {/if}
    </div>
    <!-- SelectionInfo shown in both modes (not in network view, MTP connecting state, or error states) -->
    {#if paneViewKind === 'normal' && !friendlyError && !error && !unreachable}
        {#if !isDiskImageVolume}
        <div
            class="disk-usage-bar-wrapper"
            use:tooltip={diskSpace.volumeSpace
                ? { text: formatBarTooltip(diskSpace.volumeSpace, formatByteSize, mtpSpaceHint) }
                : ''}
        >
            <div
                class="disk-usage-bar"
                role="meter"
                aria-label={tString('fileExplorer.pane.diskUsageAriaLabel')}
                aria-valuenow={diskSpace.volumeSpace ? getUsedPercent(diskSpace.volumeSpace) : 0}
                aria-valuemin={0}
                aria-valuemax={100}
            >
                {#if diskSpace.volumeSpace}
                    <div
                        class="disk-usage-fill"
                        style:width="{getUsedPercent(diskSpace.volumeSpace)}%"
                        style:background-color="var({getDiskUsageLevel(getUsedPercent(diskSpace.volumeSpace)).cssVar})"
                    ></div>
                {/if}
            </div>
        </div>
        {/if}
        <SelectionInfo
            {viewMode}
            {volumeId}
            entry={selectionInfo.entry}
            currentDirModifiedAt={undefined}
            stats={selectionInfo.stats}
            selectedCount={selection.selectedIndices.size}
            volumeSpace={diskSpace.volumeSpace}
            {mtpSpaceHint}
        />
    {/if}
</div>

<!-- Enter-behavior popup (archive/bundle set to Ask). Portaled to body. Keyboard
     nav is driven by the controller's document listener (`enterMenu.handleKey`); Ark
     owns rendering, positioning, and pointer selection. Mounted only while open (an
     `{#if}`) so closing UNMOUNTS it — Ark's controlled-open machine doesn't reliably
     close on `open=false` alone. -->
{#if enterMenu.open}
    <Menu
        items={enterMenu.items}
        onSelect={enterMenu.onSelect}
        onClose={() => {
            enterMenu.onOpenChange(false)
        }}
        anchorPoint={enterMenu.anchorPoint}
        highlightedValue={enterMenu.highlighted}
        onHighlightChange={enterMenu.setHighlighted}
        ariaLabel={tString('fileExplorer.archiveEnterMenu.ariaLabel')}
    />
{/if}

{#if renameFlow.extensionDialogState}
    <ExtensionChangeDialog
        oldExtension={renameFlow.extensionDialogState.oldExtension}
        newExtension={renameFlow.extensionDialogState.newExtension}
        onKeepOld={handleExtensionKeepOld}
        onUseNew={handleExtensionUseNew}
    />
{/if}

{#if renameFlow.conflictDialogState?.validity.conflict}
    <RenameConflictDialog
        renamedFile={{
            name: rename.target?.originalName ?? '',
            size: selectionInfo.entry?.size ?? 0,
            modifiedAt: selectionInfo.entry?.modifiedAt,
        }}
        existingFile={{
            name: renameFlow.conflictDialogState.validity.conflict.name,
            size: renameFlow.conflictDialogState.validity.conflict.size,
            modifiedAt: renameFlow.conflictDialogState.validity.conflict.modified ?? undefined,
        }}
        onResolve={handleConflictResolve}
    />
{/if}

<style>
    .file-pane {
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        /* Pane bg propagation hook. The inline style on `.file-pane`
           overrides this with the tinted color when the user picks a
           tint for the volume's kind; otherwise it falls back to the
           untinted base. `.content` reads it so the bg actually paints
           where downstream views can see it (the file-pane itself sits
           behind .content, so an inline `background-color` here was
           invisible). Striped rows use a translucent overlay, so the
           tint shows through them too. */
        --color-pane-bg: var(--color-bg-primary);
    }

    .header {
        /* Generous above, tighter below: the strip reads as its own band without
           drifting away from the column header it sits on. Horizontal stays
           `--spacing-sm`, matching the list gutter. */
        padding: var(--spacing-sm) var(--spacing-sm) var(--spacing-xs);
        background-color: var(--color-bg-secondary);
        font-size: var(--font-size-sm);
        white-space: nowrap;
        display: flex;
        align-items: center;
    }

    .disk-usage-bar-wrapper {
        flex-shrink: 0;
    }

    .disk-usage-bar {
        height: 2px;
        background-color: var(--color-disk-track);
    }

    .disk-usage-fill {
        height: 100%;
        transition: none;
        pointer-events: none;
        border-radius: 0 var(--radius-xs) var(--radius-xs) 0;
    }

    .path {
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        flex: 1;
        min-width: 0;
    }

    /* Clickable ancestor segments: bare inline buttons (no chrome), inheriting
       the breadcrumb's color, brightening to the primary text color on hover so
       the click affordance is visible. Cursor stays the app-wide default (only
       `LinkButton` opts into a pointer). */
    .path :global(.path-segment) {
        font: inherit;
        color: inherit;
        background: none;
        border: none;
        padding: 0;
        margin: 0;
        cursor: default;
    }

    .path :global(.path-segment:hover) {
        color: var(--color-text-primary);
    }

    /* Segments inside a `.git/...` portal pick up the dedicated git-portal
       token so the user can see at a glance they're "in history-land." */
    .path :global(.git-portal) {
        color: var(--color-git-portal-text);
    }

    /* Git-portal segments brighten to the bolder git-portal token on hover,
       keeping the "history-land" hue instead of jumping to the primary color. */
    .path :global(.path-segment.git-portal:hover) {
        color: var(--color-git-portal);
    }

    .path :global(.path-sep) {
        color: var(--color-text-tertiary);
    }

    .content {
        flex: 1;
        overflow: hidden;
        display: flex;
        flex-direction: column;
        /* Anchor for the type-to-jump indicator (absolutely positioned, bottom-right). */
        position: relative;
        /* The pane's single bg layer. `.content` is the only ancestor
           mounted continuously across every dynamic state (loading, error,
           MTP, file list, etc.), so painting it once here guarantees a
           stable backdrop with no transition frame where the parent's
           color leaks through. Downstream views (FullList / BriefList /
           ErrorPane / …) keep their interior elements transparent so this
           stays the single base layer. Highlights (selection, cursor) sit
           on top intentionally. `--color-pane-bg` tracks the user's per-volume
           tint (set inline on `.file-pane`); without a tint it resolves
           to `--color-bg-primary`. */
        background-color: var(--color-pane-bg);
    }

    .error-message {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: var(--color-error);
        text-align: center;
        padding: var(--spacing-lg);
    }
</style>
