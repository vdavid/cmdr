<script lang="ts">
    import { onDestroy, onMount, tick, untrack } from 'svelte'
    import type {
        FileEntry,
        FriendlyError,
        ListingStats,
        NetworkHost,
        SortColumn,
        SortOrder,
        SyncStatus,
    } from '../types'
    import type { ListingCompleteEvent } from '$lib/tauri-commands'
    import {
        cancelListing,
        findFileIndex,
        getFileRange,
        pathExistsChecked,
        getFileAt,
        getListingStats,
        getPathsAtIndices,
        getSyncStatus,
        getTotalCount,
        listDirectoryEnd,
        listDirectoryStart,
        onListingOpening,
        onListingProgress,
        onListingReadComplete,
        onListingComplete,
        onListingError,
        onListingCancelled,
        onMtpDeviceDisconnected,
        openFile,
        refreshListingIndexSizes,
        showFileContextMenu,
        showParentRowContextMenu,
        trackEvent,
        type Location,
        type UnlistenFn,
        updateMenuContext,
    } from '$lib/tauri-commands'
    import { resolveLocationOrToast } from '../navigation/navigate-and-select'
    import { sweepListingTags } from './tag-sweep'
    import { renderListingError } from '$lib/errors/listing-error'
    import { updateIndexSizesInPlace } from '../views/file-list-utils'
    import { evictPerPathIconsForDir } from '$lib/icon-cache'
    import { classifySelectionDialogKey } from './selection-dialog-keys'
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
    import { createSmbViewState } from './smb-view-state.svelte'
    import { createVolumeSpace } from './volume-space.svelte'
    import { isVirtualGitPath } from '../git/path-detection'
    import { getSetting, setSetting } from '$lib/settings'
    import { isFileListBackgroundClick } from './pane-background-dblclick'
    import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'
    import ErrorPane from './ErrorPane.svelte'
    import VolumeUnreachableBanner from './VolumeUnreachableBanner.svelte'
    import SmbReauthView from './SmbReauthView.svelte'
    import NetworkMountView from './NetworkMountView.svelte'
    import SearchResultsView from './SearchResultsView.svelte'
    import type { SearchResultsViewAPI } from './types'
    import { getSnapshot } from '$lib/search/snapshot-store.svelte'
    import MtpConnectionView from './MtpConnectionView.svelte'
    import SmbReconnectingView from './SmbReconnectingView.svelte'
    import { smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
    import NetworkLoginForm from '../network/NetworkLoginForm.svelte'
    import { createSelectionState } from './selection-state.svelte'
    import { createPaneMcpSync } from './pane-mcp-sync.svelte'
    import { initListingDiffSync } from './listing-diff-sync.svelte'
    import { createRenameState } from '../rename/rename-state.svelte'
    import { cancelClickToRename } from '../rename/rename-activation'
    import { type DirectorySortMode } from '$lib/settings'
    import { addToast, dismissTransientToasts } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { maybeShowQuickLookHint } from '../quick-look/quick-look-hint'
    import { createRenameFlow } from './rename-flow.svelte'
    import ExtensionChangeDialog from '../rename/ExtensionChangeDialog.svelte'
    import RenameConflictDialog from '../rename/RenameConflictDialog.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { createDebounce, createThrottle } from '$lib/utils/timing'

    const log = getAppLogger('fileExplorer')
    import { isMtpVolumeId, getMtpDisplayPath } from '$lib/mtp'
    import { getPaneTintBg, getPaneTintName } from './volume-tint.svelte'
    import * as benchmark from '$lib/benchmark'
    import { createCursorNavKeys } from './cursor-nav-keys'
    import { createSearchPaneKeys } from './search-pane-keys'
    import { computeHasParent } from './has-parent'
    import { firstSelectedIndex } from './first-selected-index'
    import { capabilitiesFor } from './volume-capabilities'
    import { resolveValidPath } from '../navigation/path-resolution'
    import { isVolumeEjectable } from '../navigation/eject-predicate'
    import { homeDir } from '@tauri-apps/api/path'
    import { basenameOf, type CanonicalPath, parentOf, toCanonical } from '$lib/path/canonical'
    import { showBreadcrumbContextMenu } from '$lib/tauri-commands'
    import { getEffectiveShortcuts } from '$lib/shortcuts/shortcuts-store'
    import { getVolumes as getStoreVolumes } from '$lib/stores/volume-store.svelte'
    import type { UnreachableState } from '../tabs/tab-types'
    import { getDiskUsageLevel, getUsedPercent, formatBarTooltip } from '../disk-space-utils'
    import { getFileSizeFormat, getTypeToJumpResetDelay } from '$lib/settings/reactive-settings.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'

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
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
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
        /** Volume resolution timed out for this tab: show banner instead of file list */
        unreachable?: UnreachableState | null
        /** Called when user clicks "Retry" on the unreachable banner */
        onRetryUnreachable?: () => void
        /** Called when user clicks "Open home folder" on the unreachable banner */
        onOpenHome?: () => void
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
        showHiddenFiles = true,
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
        unreachable = null,
        onRetryUnreachable,
        onOpenHome,
        onCommand,
    }: Props = $props()

    let currentPath = $state(untrack(() => initialPath))

    // New architecture: store listingId and totalCount, not files
    let listingId = $state('')
    // The directory path of the currently active listing. Plain bookkeeping (not
    // reactive): used to evict this directory's per-path icons (`path:*` / `pkg:*`)
    // when the listing ends, so a folder re-iconed while away is re-detected next
    // time it's shown rather than served stale from the session cache.
    let loadedPath = ''
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

    // File under the cursor fetched separately for SelectionInfo
    let entryUnderCursor = $state<FileEntry | null>(null)

    // Listing stats for SelectionInfo (selection summary in Full mode, totals display)
    let listingStats = $state<ListingStats | null>(null)

    // Volume root path from listing-complete event (accurate for MTP and all volume types)
    let volumeRootFromEvent = $state<string | undefined>(undefined)


    import type { ListViewAPI, VolumeBreadcrumbAPI, NetworkMountViewAPI, NetworkCursorEntry } from './types'
    import type { DragAutoScrollFrameResult, DragAutoScrollPointer } from '../drag/drag-auto-scroll'

    // Component refs for keyboard navigation
    let fullListRef: ListViewAPI | undefined = $state()
    let briefListRef: ListViewAPI | undefined = $state()
    let volumeBreadcrumbRef: VolumeBreadcrumbAPI | undefined = $state()
    let networkMountViewRef: NetworkMountViewAPI | undefined = $state()
    let searchResultsViewRef: SearchResultsViewAPI | undefined = $state()

    /**
     * This pane's volume capabilities, the single A6 source of truth for "what
     * can a pane on this KIND do". Resolved once from `volumeId` (the two virtual
     * ids short-circuit in `volumeKindOf` before the store lookup; real ids read
     * `fsType`/`category` from the volume store). The view-selection discriminant,
     * the named view deriveds below, and the per-feature gates all read off this.
     */
    const caps = $derived(capabilitiesFor(volumeId))

    // Check if we're viewing the network (special virtual volume). Sourced from
    // the kind, not a `volumeId === 'network'` string compare (A6).
    const isNetworkView = $derived(caps.kind === 'network')

    /**
     * Check if we're viewing a search-results snapshot (the other virtual volume,
     * `search-results://<id>`). Behaves like the network view: no backend listing,
     * no file watcher, no git lookups, no pane-state-to-MCP sync. The pane renders
     * `SearchResultsView` which pulls the snapshot from the in-memory store.
     * Most code paths that gate on `isNetworkView` also gate on this; the few
     * exceptions are noted at each call site. Sourced from the kind, not a
     * `volumeId === 'search-results'` string compare (A6).
     */
    const isSearchResultsView = $derived(caps.kind === 'search-results')

    /**
     * The phone-storage caveat for the disk-space readout, only on MTP volumes
     * (keyed on `caps.kind`, not a volume-id string, per A6). Over USB a phone
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

    // Display path shown in the breadcrumb after the volume name.
    // For the root volume: replaces the home dir prefix with "~", otherwise shows absolute path.
    // For other volumes: shows path relative to the volume root.
    const breadcrumbDisplayPath = $derived.by(() => {
        // R3 B6: the search-results pane shows the snapshot's friendly label
        // (the AI title / filename pattern / regex pattern) AS the path. The
        // volume selector itself reads the generic "Search results" so the
        // slots map cleanly: volume-kind on the left, query-specific label
        // on the right. Don't invert this (label on the left, no path on
        // the right) — see `lib/search/CLAUDE.md` § "Search-specific UI
        // behavior".
        if (isSearchResultsView) {
            return searchSnapshot?.label ?? 'Search'
        }
        if (isMtpVolumeId(volumeId)) return getMtpDisplayPath(currentPath)

        // For non-root volumes, strip the volume path prefix
        if (volumePath !== '/') {
            return currentPath.startsWith(volumePath) ? currentPath.slice(volumePath.length) || '/' : currentPath
        }

        // Root volume: paths starting with ~ are already user-friendly
        if (currentPath.startsWith('~')) return currentPath

        // Root volume with absolute path: replace home dir prefix with ~
        if (userHomePath && currentPath.startsWith(userHomePath)) {
            const rest = currentPath.slice(userHomePath.length)
            return rest ? '~' + rest : '~'
        }

        // Root volume, outside home dir: show absolute path as-is
        return currentPath
    })

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
            ? [{ text: breadcrumbDisplayPath, gitPortal: false }]
            : splitPathSegments(breadcrumbDisplayPath),
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

    /** Navigate to a breadcrumb ancestor. Errors surface via the pane's error pipeline. */
    function handleBreadcrumbSegmentClick(target: string): void {
        void navigateToPath(target).catch(() => {})
    }

    /**
     * Double-clicking the empty file-list background navigates up one folder
     * (Directory Opus-style), gated by `behavior.doubleClickPaneNavigatesToParent`.
     * On the very first trigger we raise a one-time INFO toast explaining it, and
     * flip the hidden `behavior.doubleClickOnPaneNotificationSeen` so it shows once.
     */
    function handlePaneBackgroundDblClick(e: MouseEvent): void {
        if (!getSetting('behavior.doubleClickPaneNavigatesToParent')) return
        if (!isFileListBackgroundClick(e.target)) return
        if (!hasParent) return // nothing above (volume root / search-results pane)
        void navigateToParent()
        if (!getSetting('behavior.doubleClickOnPaneNotificationSeen')) {
            setSetting('behavior.doubleClickOnPaneNotificationSeen', true)
            addToast(DoubleClickPaneHintToastContent, {
                level: 'info',
                dismissal: 'persistent',
                id: 'double-click-pane-hint',
            })
        }
    }

    // Check if we're viewing an MTP device
    const isMtpView = $derived(isMtpVolumeId(volumeId))

    // Check if this is a device-only MTP ID (needs connection)
    // Device-only IDs start with "mtp-" but don't contain ":" (no storage ID)
    const isMtpDeviceOnly = $derived(isMtpView && volumeId.startsWith('mtp-') && !volumeId.includes(':'))

    /**
     * The KIND-structural alt-view selector for the `{#if}` chain below. It picks
     * which non-list view a pane renders purely as a function of `caps.kind` (plus
     * the MTP device-only connection sub-state, which the kind table doesn't carry
     * — it's a runtime connection state, not a kind). This is NOT a new component
     * (A8): it's a derived discriminant the existing chain branches on.
     *
     * Only the KIND-driven branches live here. The runtime-state branches
     * (`unreachable`, SMB reconnecting / gave-up, the inline SMB upgrade login,
     * `loading` / `friendlyError` / `error`) stay per-feature and gate IN FRONT of
     * this in the chain, with byte-identical precedence to before (L10): a runtime
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
        loadDirectory: (path: string) => void loadDirectory(path),
        navigateToFallback,
    })

    // Live per-pane disk space: the readout, the fetch, the backend live-update
    // listener, and the watch/unwatch registration live in a `*.svelte.ts` factory.
    // The pane keeps a one-line `refreshVolumeSpace` delegate (a FilePaneAPI export)
    // and drives watch/unwatch across mount, volume-switch, and destroy.
    const diskSpace = createVolumeSpace({
        paneId,
        getVolumeId: () => volumeId,
        getCurrentPath: () => currentPath,
        getIsDiskImage: () => isDiskImageVolume,
    })

    // Network browsing state - tracked here for history navigation integration
    let currentNetworkHost = $state<NetworkHost | null>(null)
    // Pending share to auto-mount on the network host. Set by "Copy path between
    // panes" when the source pane has the cursor on a share. Cleared on volume leave.
    let pendingAutoMountShare = $state<string | undefined>(undefined)

    // Clear the selected network host whenever the pane leaves the network
    // volume so that re-entering Network always lands on the host list, not on
    // a stale ShareBrowser for whichever host was open last. Without this,
    // `NetworkMountView` re-mounts with the old `initialNetworkHost` and the
    // user sees the previous share list when they expected the host list.
    //
    // Previously this only got cleared by an explicit "Back" click inside
    // `ShareBrowser` (which calls `onNetworkHostChange(null)`). Volume-switches
    // via the picker, the breadcrumb, history navigation, or MCP didn't trip
    // that path, so the host stayed pinned. The matching gotcha in
    // `file-explorer/network/CLAUDE.md` documented this as the cause of E2E
    // test 436 ("unicode shares render") and several SMB share-count tests.
    $effect(() => {
        if (!isNetworkView && currentNetworkHost !== null) {
            currentNetworkHost = null
        }
        if (!isNetworkView && pendingAutoMountShare !== undefined) {
            pendingAutoMountShare = undefined
        }
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
     * Used by `moveCursor` (and any other callers that need a stable
     * `listingId`) to avoid the race where the FE has set a fresh `listingId`
     * but `list_directory_start_streaming` hasn't yet inserted the listing
     * into the backend's `LISTING_CACHE`. Wraps the existing
     * `pendingLoadResolve` hook so we don't introduce a second promise track:
     * if no load is in flight, resolves immediately.
     */
    export function whenLoadSettles(): Promise<void> {
        if (!loading) return Promise.resolve()
        return new Promise<void>((resolve) => {
            // Chain onto the existing resolver / rejecter so we don't disturb
            // a pending `navigateToPath` caller already waiting on the load.
            const prevResolve = pendingLoadResolve
            const prevReject = pendingLoadReject
            pendingLoadResolve = () => {
                prevResolve?.()
                resolve()
            }
            pendingLoadReject = (reason: string) => {
                prevReject?.(reason)
                resolve() // We treat reject as "load is no longer in flight"; caller checks isLoading.
            }
        })
    }

    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getFilenameUnderCursor(): string | undefined {
        return entryUnderCursor?.name
    }

    /**
     * Absolute path of the entry under the cursor (or `undefined` when the listing is empty
     * or hasn't resolved the entry yet). Reads the reactive `entryUnderCursor` $state, so
     * Quick Look's cursor-follow $effect in `DualPaneExplorer.svelte` stays subscribed
     * across cursor moves, listing swaps, and pane switches.
     */
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function getPathUnderCursor(): string | undefined {
        return entryUnderCursor?.path
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
        return entryUnderCursor
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
        // fetchEntryUnderCursor is handled by the $effect tracking cursorIndex
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
            cursorNav.applyNavigation(cursorIndex + 1, listRef, false)
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
    export async function getEntriesSnapshot(): Promise<FileEntry[]> {
        if (isSearchResultsView) {
            // Adapt SearchResultEntry → FileEntry. The snapshot's entry.name is the
            // friendly full path (per the search-results virtual volume contract);
            // we preserve that so the Selection matcher's accessor sees what the
            // user sees in the pane.
            const sn = searchSnapshot
            if (!sn) return []
            return sn.entries.map(
                (e): FileEntry => ({
                    name: e.name,
                    path: e.path,
                    parentPath: e.parentPath,
                    isDirectory: e.isDirectory,
                    isSymlink: false,
                    size: e.size ?? undefined,
                    modifiedAt: e.modifiedAt ?? undefined,
                    permissions: 0,
                    owner: '',
                    group: '',
                    iconId: e.iconId,
                    extendedMetadataLoaded: true,
                }),
            )
        }
        const canonical = canonicalPath
        if (!listingId || totalCount === 0) {
            // Synthetic `..` entry (when present) keeps the index alignment.
            const synthetic = canonical ? createParentEntry(canonical) : null
            return hasParent && synthetic ? [synthetic] : []
        }
        try {
            const fetched = await getFileRange(listingId, 0, totalCount, showHiddenFiles)
            if (hasParent) {
                const synthetic = canonical ? createParentEntry(canonical) : null
                return synthetic ? [synthetic, ...fetched] : fetched
            }
            return fetched
        } catch {
            return []
        }
    }

    /** Cursor index inside the entries-snapshot returned by `getEntriesSnapshot()`. */
    // noinspection JSUnusedGlobalSymbols -- consumed by DualPaneExplorer.getFocusedPaneEntries
    export function getEntriesCursorIndex(): number {
        return cursorIndex
    }

    /** Snapshots the current selection as file names for diff-driven adjustment during operations. */
    export async function snapshotSelectionForOperation(): Promise<void> {
        if (selection.isAllSelected(hasParent, effectiveTotalCount)) {
            operationSelectedNames = 'all'
            return
        }

        const indices = selection.getSelectedIndices()
        const names: string[] = []
        for (const frontendIndex of indices) {
            const backendIndex = hasParent ? frontendIndex - 1 : frontendIndex
            if (backendIndex < 0) continue
            const entry = await getFileAt(listingId, backendIndex, includeHidden)
            if (entry) names.push(entry.name)
        }
        operationSelectedNames = names
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
        getListingId: () => listingId,
        getTotalCount: () => totalCount,
        getIncludeHidden: () => includeHidden,
        getCurrentPath: () => currentPath,
        getCursorIndex: () => cursorIndex,
        getShowHiddenFiles: () => showHiddenFiles,
        getVolumeId: () => volumeId,
        getEntryUnderCursor,
        onRequestFocus: () => onRequestFocus?.(),
    })

    // Destructure handlers: factory methods don't use `this`, safe to destructure
    /* eslint-disable @typescript-eslint/unbound-method -- factory return, no `this` */
    const {
        handleRenameInput,
        handleRenameSubmit,
        handleRenameCancel,
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

    export function startRename(): void {
        // Type-to-jump must not linger over the inline rename editor.
        jump.clear()
        renameFlow.startRename()
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
        // Re-enrich backend cache entries so fetchListingStats sees fresh recursive_size values
        if (listingId) {
            void refreshListingIndexSizes(listingId).then(() => fetchListingStats())
        }
        // Refresh the cursor entry too so SelectionInfo's Brief size readout (and
        // its "size updating" hourglass) tracks the storm live, not just on cursor moves.
        void fetchEntryUnderCursor()
        // Mirror the refreshed sizes (and the `recursiveSizePending` hourglass flag)
        // into the MCP pane state so agents see `[size-pending]` update live during
        // an index storm, not just on cursor/nav changes. Debounced (300ms), so a
        // burst of index-dir-updated refreshes coalesces into one sync.
        debouncedSyncMcp.call()
    }

    export function getSwapState(): SwapState {
        return {
            currentPath,
            listingId,
            totalCount,
            cursorIndex,
            selectedIndices: selection.getSelectedIndices(),
            lastSequence,
        }
    }

    export function adoptListing(state: SwapState): void {
        // Cancel any in-flight loads
        loadGeneration++

        // Set currentPath first so the initialPath $effect sees newPath === curPath and skips reload
        currentPath = state.currentPath

        // Adopt the listing identity
        listingId = state.listingId
        totalCount = state.totalCount
        lastSequence = state.lastSequence

        // Restore cursor and selection
        cursorIndex = state.cursorIndex
        selection.setSelectedIndices(state.selectedIndices)

        // Force virtual list to re-fetch visible range from (now-swapped) cache
        cacheGeneration++

        // Clear loading/error state
        loading = false
        error = null

        // Re-fetch entry under cursor and listing stats for SelectionInfo
        void fetchEntryUnderCursor()
        void fetchListingStats()

        // Sync state to MCP
        debouncedSyncMcp.call()

        // Scroll to cursor position
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            listRef?.scrollToIndex(cursorIndex)
        })
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
        currentNetworkHost = host
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
        pendingAutoMountShare = shareName
    }

    /** Navigates up and selects the folder we came from. Returns false if already at root. */
    export async function navigateToParent(): Promise<boolean> {
        if (currentPath === '/' || currentPath === volumePath) {
            return false // Already at root
        }
        const canonical = canonicalPath
        if (!canonical) return false // userHomePath not resolved yet
        const currentFolderName = basenameOf(canonical)
        const parentPath = parentOf(canonical)

        currentPath = parentPath
        // Note: onPathChange is called in listing-complete handler after successful load
        await loadDirectory(parentPath, currentFolderName)
        return true
    }

    // Track the current load operation to cancel outdated ones
    let loadGeneration = 0
    // Set on unmount so the background Finder-tag sweep stops (it's a detached
    // async loop the listing-cancel machinery doesn't reach).
    let isDestroyed = false
    // Track last sequence for file watcher diffs
    let lastSequence = 0
    // Streaming event listeners
    let unlistenOpening: UnlistenFn | undefined
    let unlistenProgress: UnlistenFn | undefined
    let unlistenComplete: UnlistenFn | undefined
    let unlistenError: UnlistenFn | undefined
    let unlistenCancelled: UnlistenFn | undefined
    // Opening folder state (before read_dir starts - slow for network folders)
    let openingFolder = $state(false)
    // Loading progress state for streaming
    let loadingCount = $state<number | undefined>(undefined)
    // Finalizing state (read_dir done, now sorting/caching)
    let finalizingCount = $state<number | undefined>(undefined)
    let unlistenReadComplete: UnlistenFn | undefined
    function resetLoadingState(errorMessage?: string, preserveTotalCount = false, friendly?: FriendlyError | null) {
        if (errorMessage) error = errorMessage
        friendlyError = friendly ?? null
        listingId = ''
        if (!preserveTotalCount) totalCount = 0
        loading = false
        openingFolder = false
        loadingCount = undefined
        finalizingCount = undefined
        // Reject pending load promise on error/cancel
        if (errorMessage) {
            rejectPendingLoad(errorMessage)
        } else {
            rejectPendingLoad('Loading cancelled')
        }
    }

    // Sync status map for visible files
    let syncStatusMap = $state<Record<string, SyncStatus>>({})
    const syncPollIntervalMs = 3000
    let syncPollInterval: ReturnType<typeof setInterval>
    // Pending retry timer for timed-out sync status fetches (max 1 retry)
    let syncRetryTimer: ReturnType<typeof setTimeout> | undefined
    const syncRetryDelayMs = 5000
    // Poll to detect when the current directory is deleted externally (FSEvents doesn't notify)
    const dirExistsPollMs = 2000
    let dirExistsPollInterval: ReturnType<typeof setInterval>
    let dirNotExistsCount = 0 // Consecutive "not exists" results: require 2 before navigating away

    // Derive includeHidden from showHiddenFiles prop
    const includeHidden = $derived(showHiddenFiles)

    // MCP state-sync factory: mirrors this pane into the `PaneState` store. Deps
    // pass reactive reads via getters so the factory lives in a plain `.svelte.ts`.
    const mcpSync = createPaneMcpSync({
        paneId,
        // The network + search-results skip folds into the kind's `syncsToMcp`
        // capability (false for both), read off the pane's derived `caps` rather
        // than the two `volumeId ===` deriveds (A6).
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
    const debouncedFetchEntry = createDebounce(() => void fetchEntryUnderCursor(), 16)
    const throttledFetchStats = createThrottle(() => void fetchListingStats(), 150)
    const debouncedMenuContext = createDebounce(() => {
        if (entryUnderCursor && entryUnderCursor.name !== '..') {
            void updateMenuContext(entryUnderCursor.path, entryUnderCursor.name)
        }
    }, 100)
    const debouncedSyncMcp = createDebounce(() => void syncPaneStateToMcp(), 300)

    /** Handle visible range change from list components */
    function handleVisibleRangeChange(start: number, end: number) {
        visibleRangeStart = start
        visibleRangeEnd = end
        debouncedSyncMcp.call()
    }

    // Create ".." entry for parent navigation
    function createParentEntry(path: CanonicalPath): FileEntry | null {
        if (path === '/') return null
        return {
            name: '..',
            path: parentOf(path),
            isDirectory: true,
            isSymlink: false,
            permissions: 0o755,
            owner: '',
            group: '',
            iconId: 'dir',
            extendedMetadataLoaded: true,
        }
    }

    // Check if current directory has a parent (not at filesystem root AND not at volume root)
    // Prefer volumeRoot from the listing event (accurate for MTP), fall back to prop (for initial state)
    const effectiveVolumeRoot = $derived(volumeRootFromEvent ?? volumePath)
    // Search-results panes have NO `..` row: the snapshot is a flat result set, not a directory.
    // Without this gate, the path comparison was true (search-results://sr-N never matches a real
    // volume root), causing `hasParent` to be `true`, which made `selectAll` skip index 0 (P6).
    // R3 T1: the derivation lives in `has-parent.ts` so the regression test
    // (`has-parent.test.ts`) can pin the integration with `selection.selectAll`
    // without spinning up the whole `FilePane` component.
    const hasParent = $derived(
        computeHasParent({
            // The snapshot no-`..` rule comes from the kind capability, not a
            // `volumeId === 'search-results'` string compare (A6), read off the
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

    // Pending load completion resolver: used by navigateToPath to signal when listing is done.
    // Set at the start of loadDirectory, resolved by handleListingComplete / error / cancel handlers.
    let pendingLoadResolve: (() => void) | null = null
    let pendingLoadReject: ((reason: string) => void) | null = null

    function resolvePendingLoad() {
        pendingLoadResolve?.()
        pendingLoadResolve = null
        pendingLoadReject = null
    }

    function rejectPendingLoad(reason: string) {
        pendingLoadReject?.(reason)
        pendingLoadResolve = null
        pendingLoadReject = null
    }

    /**
     * Navigates to a fallback path after the current path became invalid.
     * If the resolved path is outside the current volume (~ or /), switches
     * to the root volume instead of trying to list it on a non-root volume.
     */
    function navigateToFallback(validPath: string | null) {
        const target = validPath ?? '~'
        const isOutsideVolume = volumeId !== 'root' && (target === '~' || target === '/')
        if (isOutsideVolume && onVolumeChange) {
            // The volume root was unreachable: switch to the root volume
            log.info('Volume root unreachable, switching to root volume with path: {target}', { target })
            onVolumeChange('root', '/', target)
        } else {
            currentPath = target
            void loadDirectory(target)
        }
    }

    async function loadDirectory(path: string, selectName?: string) {
        // Cancel any active rename when navigating
        rename.cancel()
        cancelClickToRename()
        dismissTransientToasts()
        // Directory change invalidates in-flight type-to-jump buffer (per plan § 6).
        jump.clear()

        // Reset benchmark epoch for this navigation
        benchmark.resetEpoch()
        benchmark.logEventValue('loadDirectory CALLED', path)

        // Debug logging for diagnosing concurrent list_directory calls
        log.debug(
            '[FilePane] loadDirectory called: paneId={paneId}, volumeId={volumeId}, path={path}, selectName={selectName}, currentLoading={loading}, currentListingId={listingId}',
            { paneId, volumeId, path, selectName: selectName ?? 'none', loading, listingId },
        )

        // Reject any pending load from a previous navigation
        rejectPendingLoad('Superseded by new navigation')

        // Increment generation to cancel any in-flight requests
        const thisGeneration = ++loadGeneration
        log.debug('[FilePane] loadDirectory: generation={generation}', { generation: thisGeneration })

        // Cancel any abandoned listing from previous navigation
        if (listingId) {
            log.debug('[FilePane] loadDirectory: cancelling previous listing {listingId}', { listingId })
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
            // Evict the closed directory's per-path icons (no longer visible).
            evictPerPathIconsForDir(loadedPath)
            listingId = ''
            loadedPath = ''
            lastSequence = 0
        }

        // Clean up previous event listeners
        unlistenOpening?.()
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()

        // Set loading state BEFORE starting IPC call
        // This ensures the UI shows the loading spinner immediately
        loading = true
        openingFolder = false
        loadingCount = undefined
        finalizingCount = undefined
        error = null
        friendlyError = null
        syncStatusMap = {}
        clearTimeout(syncRetryTimer)
        syncRetryTimer = undefined
        selection.clearSelection()
        totalCount = 0 // Reset to show empty list immediately
        entryUnderCursor = null // Clear old under-the-cursor entry info

        // Store path and selectName for use in event handlers
        const loadPath = path
        const loadSelectName = selectName

        // Loading state is set synchronously above; Svelte will render it on the next
        // microtask. The IPC call below is non-blocking (spawns a background task and
        // returns immediately), so no double-RAF paint wait is needed.
        await tick()

        try {
            // Generate listingId first and set up listeners BEFORE starting the streaming
            // This prevents a race condition where fast folders complete before listeners are ready
            const newListingId = crypto.randomUUID()
            listingId = newListingId
            loadedPath = path
            lastSequence = 0

            // Register all event listeners in parallel (no ordering dependency between them)
            ;[
                unlistenOpening,
                unlistenProgress,
                unlistenReadComplete,
                unlistenComplete,
                unlistenError,
                unlistenCancelled,
            ] = await Promise.all([
                onListingOpening((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        openingFolder = true
                    }
                }),
                onListingProgress((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        loadingCount = payload.loadedCount
                    }
                }),
                onListingReadComplete((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        finalizingCount = payload.totalCount
                    }
                }),
                onListingComplete((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        void handleListingComplete(payload, loadPath, loadSelectName)
                    }
                }),
                onListingError((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        // For MTP volumes, trigger fallback on error (device likely disconnected)
                        if (isMtpView) {
                            resetLoadingState(payload.message)
                            log.warn('MTP listing error, triggering fallback: {error}', {
                                error: payload.message,
                            })
                            onMtpFatalError?.(payload.message)
                            return
                        }

                        // For local volumes, check if the path was deleted.
                        // Use the checked variant so a connection-blip "false" doesn't get treated as
                        // "deleted": show the error pane in that case instead of walking up.
                        void pathExistsChecked(loadPath).then(({ data: exists, timedOut }) => {
                            if (!exists && !timedOut) {
                                // Path is gone: auto-navigate to nearest valid parent
                                log.info('Listing error for deleted path, navigating to valid parent: {path}', {
                                    path: loadPath,
                                })
                                void resolveValidPath(loadPath, { volumeRoot: volumePath }).then((validPath) => {
                                    navigateToFallback(validPath)
                                })
                            } else {
                                // Path exists, or we couldn't tell: show the original listing error
                                const rendered = payload.error ? renderListingError(payload.error) : undefined
                                resetLoadingState(payload.message, false, rendered)
                                // Record the failed path in history so Cmd+[ goes back one step,
                                // not two. The success path pushes via the `onPathChange` call in
                                // `handleListingComplete`; without this call, an error pane would
                                // be visually displayed but absent from history, so Back would
                                // skip over it. `pushPath` deduplicates same-path retries.
                                onPathChange?.(loadPath)
                            }
                        })
                    }
                }),
                onListingCancelled((payload) => {
                    if (payload.listingId === newListingId && thisGeneration === loadGeneration) {
                        // Cancellation handled by onCancelLoading callback
                        resetLoadingState(undefined, true)
                    }
                }),
            ])

            // Now start streaming listing - listeners are already set up
            benchmark.logEvent('IPC listDirectoryStart CALL')
            log.debug(
                '[FilePane] calling listDirectoryStart: volumeId={volumeId}, path={loadPath}, listingId={listingId}',
                { volumeId, loadPath, listingId: newListingId },
            )
            const result = await listDirectoryStart(
                volumeId,
                path,
                includeHidden,
                sortBy,
                sortOrder,
                newListingId,
                directorySortMode,
            )
            benchmark.logEventValue('IPC listDirectoryStart RETURNED', result.listingId)
            log.debug('[FilePane] listDirectoryStart returned: status={status}', {
                status: JSON.stringify(result.status),
            })

            // Check if this load was cancelled while we were starting
            if (thisGeneration !== loadGeneration) {
                // Cancel the abandoned listing
                void cancelListing(newListingId)
                return
            }
        } catch (e) {
            if (thisGeneration !== loadGeneration) return
            resetLoadingState(e instanceof Error ? e.message : String(e))
        }
    }

    // Handle listing completion event
    async function handleListingComplete(
        payload: ListingCompleteEvent,
        loadPath: string,
        loadSelectName: string | undefined,
    ) {
        benchmark.logEventValue('listing-complete received, totalCount', payload.totalCount)
        totalCount = payload.totalCount
        volumeRootFromEvent = payload.volumeRoot

        // Determine initial cursor position
        if (loadSelectName) {
            const foundIndex = await findFileIndex(listingId, loadSelectName, includeHidden)
            const adjustedIndex = hasParent ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
            cursorIndex = adjustedIndex >= 0 ? adjustedIndex : 0
        } else {
            cursorIndex = 0
        }

        loading = false
        openingFolder = false
        loadingCount = undefined
        finalizingCount = undefined
        benchmark.logEvent('loading = false (UI can render)')

        // NOW push to history (only on successful completion)
        onPathChange?.(loadPath)

        // PII-free analytics: a navigation landed. Only the volume KIND enum crosses; never the path.
        void trackEvent('pane_navigated', { volume_kind: caps.kind })

        // Fetch entry under the cursor for SelectionInfo
        void fetchEntryUnderCursor()

        // Fetch listing stats for SelectionInfo
        void fetchListingStats()

        // Resolve pending load promise (for MCP round-trips waiting on directory load)
        resolvePendingLoad()

        // Sync state to MCP for context tools
        debouncedSyncMcp.call()

        // Backfill Finder tags for the WHOLE listing (not just the visible range)
        // so scrolling shows dots instantly and a future sort/filter sees them.
        // Cancelable via `isStale`: stops on unmount, a newer load, or a listing
        // swap. The detached loop's logic lives in `tag-sweep.ts` (testable).
        if (getSetting('listing.showTags') && caps.hasBackendListing) {
            const sweepGen = loadGeneration
            const sweepListingId = listingId
            void sweepListingTags({
                listingId: sweepListingId,
                totalCount: payload.totalCount,
                includeHidden,
                isStale: () => isDestroyed || sweepGen !== loadGeneration || sweepListingId !== listingId,
            })
        }

        // Scroll to cursor after DOM updates
        void tick().then(() => {
            const listRef = viewMode === 'brief' ? briefListRef : fullListRef
            listRef?.scrollToIndex(cursorIndex)
        })
    }

    // Handle cancellation during loading (called from DualPaneExplorer on ESC)
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleCancelLoading() {
        if (!loading || !listingId) return

        // Cancel the Rust-side operation
        void cancelListing(listingId)

        // Extract the folder name we were trying to enter, so parent can select it when reloading
        const folderName = currentPath.split('/').pop()

        // Tell parent to navigate back (passes the path we were loading so parent can decide where to go)
        onCancelLoading?.(currentPath, folderName)
    }

    // Navigate to a specific path with optional item selection (used when cancelling navigation).
    // Returns a Promise that resolves when the directory listing completes, or rejects on error.
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function navigateToPath(path: string, selectName?: string): Promise<void> {
        currentPath = path
        // Start loadDirectory first: it rejects any previous pending load
        void loadDirectory(path, selectName)
        // Then set up our promise (after the previous one was rejected)
        return new Promise<void>((resolve, reject) => {
            pendingLoadResolve = resolve
            pendingLoadReject = (reason: string) => {
                reject(new Error(reason))
            }
        })
    }

    // Fetch the entry currently under the cursor for SelectionInfo
    async function fetchEntryUnderCursor() {
        if (!listingId) {
            entryUnderCursor = null
            return
        }

        // Handle ".." entry specially
        if (hasParent && cursorIndex === 0) {
            entryUnderCursor = canonicalPath ? createParentEntry(canonicalPath) : null
            return
        }

        // Empty listing at a volume root (no ".." synthetic entry, no real entries):
        // calling getFileAt(0) here would log a spurious FE/BE index-mismatch error.
        if (totalCount === 0) {
            entryUnderCursor = null
            return
        }

        // Adjust index for ".." entry
        const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex

        try {
            entryUnderCursor = await getFileAt(listingId, backendIndex, includeHidden)
        } catch {
            entryUnderCursor = null
        }

        // Overlay the per-folder `recursiveSizePending` flag (and refresh the
        // recursive size) onto the cursor entry. It lives only on `DirStats`, not
        // on `get_file_range`, so SelectionInfo's Brief readout couldn't show the
        // "size updating" hourglass without this. Reuses the same enrichment the
        // list rows get; no-op for files. Fire-and-forget (mutates in place, so
        // Svelte reactivity updates SelectionInfo); re-runs on `index-dir-updated`
        // via `refreshIndexSizes`. Skips "..", whose entry path is the *parent*
        // folder, so enriching it would fetch the wrong folder's stats.
        if (entryUnderCursor?.isDirectory && entryUnderCursor.name !== '..') {
            void updateIndexSizesInPlace([entryUnderCursor])
        }
    }

    // Fetch listing stats for SelectionInfo (totals and selection stats)
    async function fetchListingStats() {
        if (!listingId) {
            listingStats = null
            return
        }

        try {
            // Convert selected indices to backend indices (adjust for ".." entry)
            const backendIndices =
                selection.selectedIndices.size > 0
                    ? Array.from(selection.selectedIndices).map((i) => (hasParent ? i - 1 : i))
                    : undefined

            listingStats = await getListingStats(listingId, includeHidden, backendIndices)
        } catch {
            listingStats = null
        }
    }

    // Fetch sync status for visible entries (called by List components)
    async function fetchSyncStatusForPaths(paths: string[]) {
        if (paths.length === 0) return

        // Cancel any pending retry: a new fetch supersedes it
        clearTimeout(syncRetryTimer)
        syncRetryTimer = undefined

        try {
            const { data: statuses, timedOut } = await getSyncStatus(paths)
            syncStatusMap = { ...syncStatusMap, ...statuses }

            if (timedOut) {
                // Schedule a single retry after a short delay
                syncRetryTimer = setTimeout(() => {
                    syncRetryTimer = undefined
                    void getSyncStatus(paths)
                        .then(({ data: retryStatuses }) => {
                            syncStatusMap = { ...syncStatusMap, ...retryStatuses }
                        })
                        .catch(() => {
                            // Give up silently on retry failure
                        })
                }, syncRetryDelayMs)
            }
        } catch {
            // Silently ignore - sync status is optional
        }
    }

    function handleSelect(index: number, shiftKey = false, metaKey = false) {
        if (shiftKey) {
            // Shift wins over Cmd when both are held (matches Finder).
            selection.handleShiftMouseNavigation(index, cursorIndex, hasParent)
        } else if (metaKey) {
            // Cmd+click toggles the clicked item. `..` is a no-op inside toggleAt.
            selection.toggleAt(index, hasParent)
            selection.clearRangeState()
        } else {
            selection.clearRangeState()
        }
        cursorIndex = index
        onRequestFocus?.()
        void fetchEntryUnderCursor()
    }

    async function handleContextMenu(entry: FileEntry) {
        if (entry.name === '..') {
            // The `..` row gets its own one-item menu: "Add to favorites" (favorites the
            // parent dir `entry.path`). The full file menu (Copy / Move / Delete) makes no
            // sense on `..`. On a snapshot pane there's no real parent to favorite, so skip.
            jump.clear()
            if (volumeId === 'search-results') return
            await showParentRowContextMenu(entry.path)
            return
        }
        // Spec: opening a context menu cancels in-flight type-to-jump.
        jump.clear()
        // Match Finder: if the right-clicked entry is part of the current selection,
        // actions apply to the whole selection. Otherwise they apply to just this entry.
        let paths = [entry.path]
        if (listingId && selection.selectedIndices.size > 0) {
            const indices = Array.from(selection.selectedIndices)
            try {
                const selectedPaths = await getPathsAtIndices(listingId, indices, includeHidden, hasParent)
                if (selectedPaths.includes(entry.path)) {
                    paths = selectedPaths
                }
            } catch {
                // Selection lookup failed: fall back to single-file action.
            }
        }
        await showFileContextMenu(entry.path, entry.name, entry.isDirectory, paths, false, listingId)
    }

    async function handleNavigate(entry: FileEntry) {
        // `redirectToPath` is set by the backend on virtual entries that
        // should open elsewhere (worktree and submodule working dirs).
        if (entry.redirectToPath) {
            // On the search-results pane, opening a real entry must switch to the
            // entry's real volume FIRST (resolve a `Location`, route through
            // `navigate()`). Otherwise the pane keeps `volumeId === 'search-results'`
            // with a real `path` and `SearchResultsView` shows "Search results no
            // longer available" (the path doesn't start with `search-results://`).
            if (isSearchResultsView) {
                await goToRealEntry(entry.redirectToPath)
                return
            }
            currentPath = entry.redirectToPath
            await loadDirectory(entry.redirectToPath)
            return
        }
        if (entry.isDirectory) {
            // Same as the redirect branch: a real directory opened from the
            // search-results rows switches to its real volume first.
            if (isSearchResultsView) {
                await goToRealEntry(entry.path)
                return
            }
            // When navigating to parent (..), remember current folder name to select it
            const isGoingUp = entry.name === '..'
            const currentFolderName = isGoingUp && canonicalPath ? basenameOf(canonicalPath) : undefined

            currentPath = entry.path
            // Note: onPathChange is called in listing-complete handler after successful load
            await loadDirectory(entry.path, currentFolderName)
        } else {
            // Open file with default application
            try {
                await openFile(entry.path)
            } catch {
                // Silently fail - file open errors are expected sometimes
            }
        }
    }

    /**
     * Leave the search-results pane for a real entry: resolve `realPath` to a
     * `Location` (volume id + path), then bubble it via `onGoToLocation` so
     * `navigate()` switches to the real volume before loading the path. An
     * unresolvable path (its drive is gone) shows the shared friendly toast rather
     * than navigating to the wrong volume.
     */
    async function goToRealEntry(realPath: string): Promise<void> {
        const location = await resolveLocationOrToast(realPath)
        if (!location) return
        onGoToLocation?.(location)
    }

    function handlePaneClick() {
        onRequestFocus?.()
    }

    function handleBreadcrumbContextMenu(e: MouseEvent) {
        e.preventDefault()
        onRequestFocus?.()
        const shortcuts = getEffectiveShortcuts('file.copyCurrentDirectoryPath')
        // Pass eject info when the pane's volume is ejectable so the menu can
        // include an "Eject ({name})" item. Same gate as the row/header eject
        // buttons; the volume-context-action listener in DualPaneExplorer
        // dispatches the click to `ejectVolume`.
        const v = currentVolumeInfo
        const ejectable = v && isVolumeEjectable(v)
        void showBreadcrumbContextMenu(
            shortcuts[0] ?? '',
            ejectable ? v.id : undefined,
            ejectable ? v.name : undefined,
        )
    }

    function handleVolumeChangeFromBreadcrumb(newVolumeId: string, newVolumePath: string, targetPath: string) {
        // Navigate to the target path (may differ from volume root for favorites)
        // Note: We intentionally don't call onPathChange here - the volume change handler
        // in DualPaneExplorer takes care of saving both the old volume's path and the new path.
        // Calling onPathChange would save the new path under the OLD volume ID (race condition).
        currentPath = targetPath
        onVolumeChange?.(newVolumeId, newVolumePath, targetPath)

        // Don't load directory for network views (they handle their own data)
        // or device-only MTP views (they need connection first via auto-connect effect)
        // But DO load for connected MTP views (storage-specific volume ID contains ":")
        const isDeviceOnlyMtp = isMtpVolumeId(newVolumeId) && !newVolumeId.includes(':')
        if (newVolumeId !== 'network' && !isDeviceOnlyMtp) {
            void loadDirectory(targetPath)
            diskSpace.unwatch()
            // Disk images have no meaningful free space: skip the poll, the bottom bar, and the
            // SelectionInfo free/total text. Read the flag off the NEW volume directly — the
            // `volumeId` prop (and so `isDiskImageVolume`) hasn't updated yet this tick.
            const newIsDiskImage = getStoreVolumes().find((v) => v.id === newVolumeId)?.isDiskImage === true
            if (newIsDiskImage) {
                diskSpace.clear()
            } else {
                void diskSpace.refresh()
                diskSpace.watch(newVolumeId, targetPath)
            }
        } else {
            // Leaving a physical volume: stop watching
            diskSpace.unwatch()
        }
    }

    // Handle network host change from NetworkMountView
    function handleNetworkHostChange(host: NetworkHost | null) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

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
        extendSelection: (fromIndex, toIndex, overflow, parent) =>
            { selection.handleShiftKeyboardNavigation(fromIndex, toIndex, overflow, parent); },
        getHasParent: () => hasParent,
        getEffectiveTotalCount: () => effectiveTotalCount,
        getBriefListRef: () => briefListRef,
        getFullListRef: () => fullListRef,
    })

    /**
     * Bare `+` / `-` open the Selection dialog. Dispatch lives at the FilePane
     * keyboard level (not menu-driven on macOS, since menu accelerators always carry
     * ⌘). The pure classifier in `selection-dialog-keys.ts` pins the exact event
     * filter: no `metaKey` / `altKey` / `ctrlKey`; `shiftKey` is intentionally NOT
     * filtered (Shift+= on US QWERTY produces `event.key === '+'`).
     */
    function handleSelectionDialogKey(e: KeyboardEvent): boolean {
        const action = classifySelectionDialogKey(e)
        if (!action) return false
        e.preventDefault()
        e.stopPropagation()
        onCommand?.(action === 'open-add' ? 'selection.selectFiles' : 'selection.deselectFiles')
        return true
    }

    // Helper: Handle selection-related key events
    function handleSelectionKeys(e: KeyboardEvent): boolean {
        // Space - toggle selection at cursor. `Shift+Space` is the Quick Look
        // accelerator: AppKit consumes the menu shortcut before the webview
        // sees the keydown, so we shouldn't observe it here in practice. We
        // still gate `!e.shiftKey` defensively — AppKit can release modifier
        // keydowns to the webview in edge cases (menu rebuild during shortcut
        // customization, focus mid-flight), and we don't want Shift+Space to
        // ever silently toggle selection.
        if (e.key === ' ' && !e.shiftKey) {
            e.preventDefault()
            // Stop propagation so the document-level centralized dispatch doesn't
            // re-fire `selection.toggle` (whose case in command-dispatch.ts exists
            // for palette/MCP triggers).
            e.stopPropagation()
            selection.toggleAt(cursorIndex, hasParent)
            // Finder-convert education: the first time the user presses Space
            // in the file list, explain that Cmdr uses Space for selection and
            // ⇧Space for Quick Look. The selection toggle above still applies
            // normally — the toast is purely additive. Subsequent presses are
            // no-ops (the helper reads its own "shown once" persisted flag).
            maybeShowQuickLookHint()

            return true
        }
        // Insert - toggle selection at cursor and move cursor down (Total Commander style)
        if (e.key === 'Insert') {
            e.preventDefault()
            // See Space note above re: stopPropagation.
            e.stopPropagation()
            toggleSelectionAndMoveDownAtCursor()
            return true
        }
        // Cmd+A - select all (Cmd+Shift+A - deselect all)
        if (e.key === 'a' && e.metaKey) {
            e.preventDefault()
            if (e.shiftKey) {
                selection.deselectAll()
            } else {
                selection.selectAll(hasParent, effectiveTotalCount)
            }
            return true
        }
        return false
    }

    /** Gets the file entry under the cursor from the current list view */
    function getEntryUnderCursor(): FileEntry | undefined {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        return listRef?.getEntryAt(cursorIndex)
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
        extendSelection: (fromIndex, toIndex, overflow) =>
            { selection.handleShiftKeyboardNavigation(fromIndex, toIndex, overflow, false); },
        toggleSelectionAt: (index: number) => selection.toggleAt(index, false),
        openCursorItem: () => void openCursorItem(),
    })

    /**
     * Open / parent keys, view-independent (handled before the Brief/Full split).
     * Returns true if the key was consumed.
     *
     * - Enter / ⌘↓ → open the entry under the cursor (Finder parity, mirror of ⌘↑).
     * - Backspace / ⌘↑ → go to the parent directory.
     *
     * `stopPropagation` is load-bearing for the ⌘-variants: ⌘↓ (`nav.open`) and ⌘↑
     * (`nav.parent`) are ALSO bound in the registry, so without stopping here the
     * document-level dispatcher would run the command a second time (⌘↑ → grandparent,
     * ⌘↓ → double-open). ⌘Backspace is deliberately excluded from the parent branch:
     * it deletes via `file.delete` (⌘⌫), so it falls through to the document dispatcher.
     */
    function handleOpenOrParentKey(e: KeyboardEvent): boolean {
        if (e.key === 'Enter' || (e.key === 'ArrowDown' && e.metaKey)) {
            const entry = getEntryUnderCursor()
            if (entry) {
                e.preventDefault()
                if (e.metaKey) e.stopPropagation()
                void handleNavigate(entry)
                return true
            }
            // ⌘↓ with nothing under the cursor: swallow it so it can't fall through
            // to cursor-move or the document dispatcher.
            if (e.metaKey) {
                e.preventDefault()
                e.stopPropagation()
                return true
            }
            return false
        }

        if (((e.key === 'Backspace' && !e.metaKey) || (e.key === 'ArrowUp' && e.metaKey)) && hasParent) {
            e.preventDefault()
            e.stopPropagation()
            void navigateToParent()
            return true
        }

        return false
    }

    // Exported so DualPaneExplorer can forward keyboard events
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyDown(e: KeyboardEvent) {
        // When rename is active, suppress all app-level shortcuts.
        // The InlineRenameEditor handles its own keyboard events via stopPropagation.
        // This guard handles any edge cases where events still bubble.
        if (rename.active) return

        // Any keyboard action cancels a pending click-to-rename timer
        cancelClickToRename()

        if (isNetworkView) {
            networkMountViewRef?.handleKeyDown(e)
            return
        }

        // Search-results pane: route Enter to the cursor row's activation, arrow keys
        // through the SearchResultsView's setCursorIndex. The view embeds FullList but
        // owns its own bind ref; FilePane's `fullListRef` doesn't apply here. The
        // cursor state itself still lives on `cursorIndex` so we can clamp uniformly.
        if (isSearchResultsView) {
            searchPaneKeys.handleSearchResultsKeyDown(e)
            return
        }

        // Open (Enter / ⌘↓) and parent (Backspace / ⌘↑) — handled above the
        // view-mode split so every view gets them. See `handleOpenOrParentKey`.
        if (handleOpenOrParentKey(e)) return

        // Bare `+` / `-` open the Selection dialog (Total Commander parity).
        if (handleSelectionDialogKey(e)) return

        // Handle selection keys
        if (handleSelectionKeys(e)) return

        // Delegate to view-mode-specific handler
        if (viewMode === 'brief') {
            cursorNav.handleBriefModeKeys(e)
        } else {
            cursorNav.handleFullModeKeys(e)
        }
    }

    // Handle key release - terminates the mouse Shift+click anchor gesture so the next
    // gesture starts fresh. Keyboard Shift+nav is stateless and doesn't need this.
    // noinspection JSUnusedGlobalSymbols -- Used dynamically
    export function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            selection.clearRangeState()
        }
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

    // When includeHidden changes, cancel rename and refetch total count
    $effect(() => {
        if (listingId && !loading) {
            // Cancel rename on hidden files toggle (spec: sort change / toggle hidden = cancel)
            untrack(() => {
                rename.cancel()
            })
            // Read cursor state without tracking to avoid infinite re-triggers
            const nameToFollow = untrack(() => entryUnderCursor?.name)
            const currentCursor = untrack(() => cursorIndex)
            void getTotalCount(listingId, includeHidden).then(async (count) => {
                totalCount = count
                const total = hasParent ? count + 1 : count
                // Try to keep cursor on the same file
                if (nameToFollow) {
                    const foundIndex = await findFileIndex(listingId, nameToFollow, includeHidden)
                    if (foundIndex !== null) {
                        const adjustedIndex = hasParent ? foundIndex + 1 : foundIndex
                        await setCursorIndex(adjustedIndex)
                        return
                    }
                }
                // File not found (was hidden) or no file: clamp cursor
                if (currentCursor >= total) {
                    await setCursorIndex(Math.max(0, total - 1))
                }
            })
        }
    })

    // Track previous unreachable state to detect when volume becomes reachable (retry success).
    // Only triggers when the path stays the same (retry case). The "Open home folder" case
    // changes the path, which the initialPath effect below handles instead.
    let prevUnreachable = $state(unreachable)

    $effect(() => {
        const wasUnreachable = prevUnreachable !== null
        const isNowReachable = unreachable === null
        const pathUnchanged = initialPath === untrack(() => currentPath)

        if (wasUnreachable && isNowReachable && pathUnchanged) {
            log.info('Tab became reachable (retry succeeded), loading directory: {path}', { path: initialPath })
            void loadDirectory(initialPath)
            void refreshVolumeSpace()
        }
        prevUnreachable = unreachable
    })

    // Track the previous volumeId to detect MTP connection completion
    let prevVolumeId = $state(volumeId)

    // Reactive path loading: handles persistence restore AND MTP connection completion.
    // One effect to avoid duplicate loadDirectory calls from overlapping triggers.
    $effect(() => {
        const newPath = initialPath // Track this
        const curPath = untrack(() => currentPath) // Don't track: user navigation changes this
        const currentVolumeId = volumeId

        // Case 1: MTP device just connected (device-only → storage-specific)
        // This takes priority: the device just became browsable, always load.
        const wasDeviceOnly = isMtpVolumeId(prevVolumeId) && !prevVolumeId.includes(':')
        const isNowConnected = isMtpVolumeId(currentVolumeId) && currentVolumeId.includes(':')

        if (wasDeviceOnly && isNowConnected) {
            log.info('MTP volume connected, loading directory: {path}', { path: newPath })
            currentPath = newPath
            void loadDirectory(newPath)
            prevVolumeId = currentVolumeId
            return // Don't also fire the initialPath branch
        }

        prevVolumeId = currentVolumeId

        // Case 2: initialPath changed for a loadable view (local volumes, connected MTP).
        // Search-results panes get their data from the snapshot store, not a real listing,
        // so we only sync `currentPath` without triggering a backend `list_directory`.
        if (isSearchResultsView) {
            if (newPath !== curPath) currentPath = newPath
            return
        }
        if (!isNetworkView && !isMtpDeviceOnly && newPath !== curPath) {
            log.debug(
                '[FilePane] initialPath effect: triggering loadDirectory, paneId={paneId}, newPath={newPath}, curPath={curPath}',
                { paneId, newPath, curPath },
            )
            currentPath = newPath
            void loadDirectory(newPath)
        }

        // Case 3: Device-only MTP: just sync path, don't load (auto-connect handles transition)
        if (isMtpDeviceOnly && newPath !== curPath) {
            log.debug('[FilePane] initialPath effect (MTP device-only): updating path only, paneId={paneId}', {
                paneId,
            })
            currentPath = newPath
        }
    })

    // Update global menu context when cursor position or focus changes (debounced: only matters for right-click)
    $effect(() => {
        if (!isFocused) return
        if (entryUnderCursor && entryUnderCursor.name !== '..') {
            debouncedMenuContext.call()
        }
    })

    // Re-fetch entry under the cursor when cursorIndex changes (debounced: status bar info can lag one frame).
    // Also sync to MCP so cmdr://state reflects keyboard nav (arrows, Insert, PageUp/Down, Home/End, click-to-position).
    // Previously, only listing changes and visible-range scrolls triggered the sync, so cursor moves within an
    // already-rendered window stayed invisible to MCP-driven agents.
    $effect(() => {
        void cursorIndex // Track
        if (listingId && !loading) {
            debouncedFetchEntry.call()
            debouncedSyncMcp.call()
        }
    })

    /**
     * Search-results pane: mirror the snapshot row under the cursor into
     * `entryUnderCursor` so SelectionInfo (if ever surfaced) and other consumers
     * see a real `FileEntry`. The cursor index changes via FilePane's keyboard
     * handler, and the snapshot itself is immutable, so the read here is cheap
     * and synchronous. No-op for non-search panes; the regular effect above
     * handles those.
     */
    $effect(() => {
        if (!isSearchResultsView) return
        const snap = searchSnapshot
        if (!snap) {
            entryUnderCursor = null
            return
        }
        // TS doesn't model array bounds (no `noUncheckedIndexedAccess`), but the
        // cursor can briefly point past the snapshot's entries after a delete-
        // sync mutation. Keep the guard at runtime.
         
        const e = snap.entries[cursorIndex]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime bounds guard; cursor can point past entries after delete-sync (see comment above)
        if (!e) {
            entryUnderCursor = null
            return
        }
        entryUnderCursor = {
            name: e.name,
            path: e.path,
            isDirectory: e.isDirectory,
            isSymlink: false,
            size: e.size ?? undefined,
            modifiedAt: e.modifiedAt ?? undefined,
            permissions: 0o644,
            owner: '',
            group: '',
            iconId: e.iconId,
            extendedMetadataLoaded: true,
            parentPath: e.parentPath,
        }
    })

    // Re-fetch listing stats when selection changes (throttled: shows live count at steady cadence)
    $effect(() => {
        void selection.selectedIndices.size // Track selection changes
        if (listingId && !loading) {
            throttledFetchStats.call()
        }
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
        fetchEntryUnderCursor: () => void fetchEntryUnderCursor(),
        fetchListingStats: () => void fetchListingStats(),
        onRequestFocus,
        navigateToFallback,
    })

    // Listen for MTP device removal events
    // When the device is disconnected, trigger fallback to previous volume
    //
    // IMPORTANT: We capture reactive values (volumeId, isMtpView) in the effect body
    // so Svelte tracks them as dependencies. This ensures the listener is re-created
    // when volumeId changes, avoiding stale closures in the callback.
    $effect(() => {
        // Capture current values - this makes Svelte track volumeId as a dependency
        const currentVolumeId = volumeId
        const currentIsMtpView = isMtpView

        // Extract device ID from volume ID (like "mtp-2097152:65537" -> "mtp-2097152").
        // Split on the LAST colon: the storage id is the trailing numeric segment,
        // and a serial-based device id can itself contain a colon (mirrors the Rust
        // `mtp::identity::device_id_of_volume`).
        const deviceIdFromVolume =
            currentIsMtpView && currentVolumeId.includes(':')
                ? currentVolumeId.slice(0, currentVolumeId.lastIndexOf(':'))
                : null

        // Only set up listener if we're viewing an MTP volume with a storage ID
        if (!currentIsMtpView || !deviceIdFromVolume) {
            return
        }

        const listenerPromise = onMtpDeviceDisconnected((event) => {
            // Check if the disconnected device matches our current MTP volume
            if (event.deviceId === deviceIdFromVolume) {
                log.warn('MTP device disconnected while viewing: {deviceId}, triggering fallback', {
                    deviceId: event.deviceId,
                })
                onMtpFatalError?.('Device disconnected')
            }
        })

        return () => {
            void listenerPromise
                .then((unsub) => {
                    unsub()
                })
                .catch(() => {})
        }
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
            void loadDirectory(currentPath)
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

        // Poll sync status so iCloud/Dropbox icons update while idle
        syncPollInterval = setInterval(() => {
            const paths = Object.keys(syncStatusMap)
            if (!listingId || paths.length === 0) return
            void fetchSyncStatusForPaths(paths)
        }, syncPollIntervalMs)

        // Poll to detect externally deleted directories (macOS FSEvents doesn't notify)
        dirExistsPollInterval = setInterval(() => {
            // Network / search-results panes have no real `currentPath` on disk
            // to poll — that folds into `!caps.hasBackendListing`. `isMtpView`
            // STAYS: MTP has a backend listing (`hasBackendListing: true`) but no
            // real on-disk path for `pathExists` to stat, so it's an
            // MTP-path-specific skip, not a capability question.
            if (!listingId || loading || !caps.hasBackendListing || isMtpView) return
            // Virtual `.git/<category>/...` paths don't exist on disk, so
            // `pathExists` always returns false and the poll would evict
            // the user back to `.git/`. The git watcher keeps these
            // listings fresh via `git-state-changed` and the
            // `directory-diff` events from `invalidate_virtual_listings`.
            if (isVirtualGitPath(currentPath)) return
            void pathExistsChecked(currentPath).then(({ data: exists, timedOut }) => {
                // `timedOut` covers both a 2s syscall timeout and an SMB volume in
                // `Disconnected` state: in both cases we don't know whether the path
                // exists. Reset the counter and wait for the connection to recover.
                if (timedOut) {
                    dirNotExistsCount = 0
                    return
                }
                if (exists) {
                    dirNotExistsCount = 0
                    return
                }

                // Require 2 consecutive confirmed "not exists" before navigating away.
                dirNotExistsCount++
                if (dirNotExistsCount < 2) return

                // If on an external volume, check whether the volume root itself is gone.
                // If so, skip: the volume unmount handler will manage the transition.
                if (volumePath !== '/') {
                    void pathExistsChecked(volumePath).then(
                        ({ data: volumeExists, timedOut: volumeTimedOut }) => {
                            // If we couldn't tell whether the volume is there, don't walk up.
                            if (volumeTimedOut) return
                            if (!volumeExists) return
                            log.info(
                                'Directory {dir} no longer exists, navigating to nearest valid parent under {volume}',
                                { dir: currentPath, volume: volumePath },
                            )
                            void resolveValidPath(currentPath, { volumeRoot: volumePath }).then((validPath) => {
                                navigateToFallback(validPath)
                            })
                        },
                    )
                } else {
                    log.info('Directory {dir} no longer exists, navigating to nearest valid parent', {
                        dir: currentPath,
                    })
                    void resolveValidPath(currentPath, { volumeRoot: volumePath }).then((validPath) => {
                        navigateToFallback(validPath)
                    })
                }
            })
        }, dirExistsPollMs)
    })

    onDestroy(() => {
        // Stop the background Finder-tag sweep if one is mid-flight.
        isDestroyed = true
        // Clean up listing
        if (listingId) {
            void cancelListing(listingId)
            void listDirectoryEnd(listingId)
            evictPerPathIconsForDir(loadedPath)
        }
        clearInterval(syncPollInterval)
        clearTimeout(syncRetryTimer)
        clearInterval(dirExistsPollInterval)
        debouncedFetchEntry.cancel()
        throttledFetchStats.cancel()
        debouncedMenuContext.cancel()
        debouncedSyncMcp.cancel()
        // Stop type-to-jump timers so they can't fire after the FilePane is gone
        // (otherwise orphan setTimeouts mutate $state slots on the dead instance).
        jump.dispose()
        unlistenOpening?.()
        unlistenProgress?.()
        unlistenReadComplete?.()
        unlistenComplete?.()
        unlistenError?.()
        unlistenCancelled?.()
        // Drop the disk-space live listener + this pane's space watch.
        diskSpace.cleanup()
        // Drop the git subscriptions (setting listeners + repo watcher) on unmount.
        gitBrowser.cleanup()
    })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    class="file-pane"
    class:is-focused={isFocused}
    onclick={handlePaneClick}
    ondblclick={handlePaneBackgroundDblClick}
    onkeydown={() => {}}
    role="region"
    aria-label={tString('fileExplorer.pane.filePaneAriaLabel', { side: paneId })}
    style={paneTintBg ? `--color-pane-bg: ${paneTintBg}` : undefined}
    data-pane-tint={paneTintName ?? undefined}
>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="header" oncontextmenu={handleBreadcrumbContextMenu}>
        <VolumeBreadcrumb
            bind:this={volumeBreadcrumbRef}
            {volumeId}
            {currentPath}
            onVolumeChange={handleVolumeChangeFromBreadcrumb}
            onSmbUpgradeLogin={smbView.handleSmbUpgradeLogin}
        />
        <span class="path">{#each clickableBreadcrumbSegments as seg, i (i)}{#if i > 0 && seg.text !== ''}<span class="path-sep">/</span>{/if}{#if seg.target !== null}<button
                    type="button"
                    class="path-segment"
                    class:git-portal={seg.gitPortal}
                    use:tooltip={tString('fileExplorer.breadcrumb.navigateTooltip', { path: seg.displayPath })}
                    onclick={() => { handleBreadcrumbSegmentClick(seg.target as string); }}
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
                initialNetworkHost={currentNetworkHost}
                initialAutoMountShare={pendingAutoMountShare}
                {onVolumeChange}
                onNetworkHostChange={handleNetworkHostChange}
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
                onSelect={(idx: number, shiftKey?: boolean, metaKey?: boolean) => {
                    // Reuse the regular pane's click semantics so shift-range
                    // and cmd-toggle behave identically. The snapshot pane has
                    // no `..` row, so `hasParent` is always false; `handleSelect`
                    // honours it via the bound `hasParent` state. M8d.
                    handleSelect(idx, shiftKey ?? false, metaKey ?? false)
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
            <ErrorPane friendly={friendlyError} folderPath={currentPath} onRetry={() => navigateToPath(currentPath)} />
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
                {syncStatusMap}
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
                onSyncStatusRequest={fetchSyncStatusForPaths}
                onSortChange={onSortChange
                    ? (column: SortColumn) => {
                          onSortChange(column)
                      }
                    : undefined}
                onVisibleRangeChange={handleVisibleRangeChange}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
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
                {syncStatusMap}
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
                onSyncStatusRequest={fetchSyncStatusForPaths}
                onRenameInput={handleRenameInput}
                onRenameSubmit={handleRenameSubmit}
                onRenameCancel={handleRenameCancel}
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
        <SelectionInfo
            {viewMode}
            {volumeId}
            entry={entryUnderCursor}
            currentDirModifiedAt={undefined}
            stats={listingStats}
            selectedCount={selection.selectedIndices.size}
            volumeSpace={diskSpace.volumeSpace}
            {mtpSpaceHint}
        />
        <!--suppress HtmlWrongAttributeValue -- We know this is not a valid ARIA role, it's fine -->
        {#if !isDiskImageVolume}
        <div
            class="disk-usage-bar-wrapper"
            use:tooltip={diskSpace.volumeSpace
                ? { text: formatBarTooltip(diskSpace.volumeSpace, (b) => formatFileSizeWithFormat(b, getFileSizeFormat()), mtpSpaceHint) }
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
    {/if}
</div>

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
            size: entryUnderCursor?.size ?? 0,
            modifiedAt: entryUnderCursor?.modifiedAt,
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
        padding: var(--spacing-xxs) var(--spacing-sm);
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
