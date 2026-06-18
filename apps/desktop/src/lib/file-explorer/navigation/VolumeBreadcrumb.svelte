<script lang="ts">
    import { onMount, onDestroy, tick, untrack } from 'svelte'
    import {
        ejectVolume,
        getIpcErrorMessage,
        onVolumeContextAction,
        resolvePathVolume,
        showVolumeRowContextMenu,
        upgradeToSmbVolume,
        systemHasSavedSmbPassword,
        upgradeToSmbVolumeUsingSavedPassword,
        type UpgradeResult,
    } from '$lib/tauri-commands'
    import type { UnlistenFn } from '@tauri-apps/api/event'
    import { ask } from '@tauri-apps/plugin-dialog'
    import { triggerNetworkDiscovery } from '../network/lazy-trigger'
    import { addToast, dismissToast } from '$lib/ui/toast'
    import { getDiskUsageLevel, getUsedPercent, formatDiskSpaceShort } from '../disk-space-utils'
    import {
        getFileSizeFormat,
        getNetworkEnabled,
        getUseAppIconsForDocuments,
    } from '$lib/settings/reactive-settings.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getCachedIcon, iconCacheVersion, prefetchIcons } from '$lib/icon-cache'
    import { isRestricted } from '$lib/stores/restricted-paths-store.svelte'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { describeUsbSpeed, type VolumeInfo } from '../types'
    import { isVolumeEjectable } from './eject-predicate'
    import { buildFavoriteTooltip } from './favorite-tooltip'
    import { tString } from '$lib/intl/messages.svelte'

    const favoriteTooltip = (volume: VolumeInfo): string => buildFavoriteTooltip(volume.path, isMacOS())

    /** "USB 3.2 Gen 1 (Max. 625 MB/s)" - shared between the chip tooltip and the dropdown subline. */
    function usbSpeedDisplay(volume: VolumeInfo | undefined): string {
        if (!volume?.usbSpeed) return ''
        const { label, maxMBps } = describeUsbSpeed(volume.usbSpeed)
        const mbps = maxMBps >= 10 ? String(Math.round(maxMBps)) : maxMBps.toFixed(1)
        return tString('fileExplorer.navigation.usbSpeed', { label, mbps })
    }

    import { restrictedFolderTooltip } from '$lib/system-strings.svelte'
    const RESTRICTED_FOLDER_TOOLTIP = $derived(restrictedFolderTooltip())
    import {
        getVolumes,
        getVolumesTimedOut,
        isVolumesRefreshing,
        isVolumeRetryFailed,
        requestVolumeRefresh,
    } from '$lib/stores/volume-store.svelte'
    import { isVolumeBusy } from '$lib/stores/volume-busy-store.svelte'

    /** Tooltip shown on a disabled Eject control while a transfer touches the volume. */
    const EJECT_BUSY_TOOLTIP = $derived(tString('fileExplorer.navigation.ejectBusyTooltip'))
    import { groupByCategory, getIconForVolume } from './volume-grouping'
    import { createVolumeSpaceManager } from './volume-space-manager.svelte'
    import { createFavoritesController } from './favorites-controller.svelte'
    import {
        createBreadcrumbPopupController,
        createKeyboardModeTracker,
        createSubmenuController,
        getConnectionTooltip,
        handleDropdownKey,
        handleSubmenuKey,
        shouldShowCheckmark,
    } from './volume-breadcrumb-handlers.svelte'

    interface Props {
        volumeId: string
        currentPath: string
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
        /** Called when the upgrade flow needs the user to enter SMB credentials. */
        onSmbUpgradeLogin?: (info: UpgradeResult & { status: 'credentialsNeeded' }, volumeId: string) => void
    }

    const { volumeId, currentPath, onVolumeChange, onSmbUpgradeLogin }: Props = $props()

    // Volumes come from the shared store (pushed by backend)
    const volumes = $derived(getVolumes())
    const volumesTimedOut = $derived(getVolumesTimedOut())
    const volumesRefreshing = $derived(isVolumesRefreshing())
    const volumeRetryFailed = $derived(isVolumeRetryFailed())

    let isOpen = $state(false)
    let highlightedIndex = $state(-1)
    let dropdownRef: HTMLDivElement | undefined = $state()
    // Keyboard mode: when true, CSS :hover is suppressed to avoid double-highlight
    const keyboardMode = createKeyboardModeTracker()

    // The ID of the actual volume that contains the current path
    // This is used to show the checkmark on the correct volume, not on favorites
    let containingVolumeId = $state<string | null>(null)

    // Submenu state for "Connect directly" option on os_mount volumes
    const submenu = createSubmenuController()
    let submenuRef: HTMLDivElement | undefined = $state()

    // Breadcrumb inline popup state (for yellow indicator in closed breadcrumb)
    const breadcrumbPopup = createBreadcrumbPopupController()
    let breadcrumbPopupRef: HTMLSpanElement | undefined = $state()

    const spaceManager = createVolumeSpaceManager()
    const {
        volumeSpaceMap,
        spaceTimedOutSet,
        spaceRetryingSet,
        spaceRetryFailedSet,
        spaceRetryAttemptedSet,
        spaceAutoRetryingSet,
    } = spaceManager

    // Favorites interaction layer (rename, pointer-drag + keyboard reorder, remove, optimistic order).
    // Instantiated at top level so its reconciliation `$effect` registers during component init.
    // `effectiveVolumes` / `favorites` below read `fav.optimisticFavoriteIds`, so they re-derive on a
    // local-first reorder before the backend round-trip lands.
    let renameInputRef: HTMLInputElement | undefined = $state()
    const fav = createFavoritesController({
        getFavorites: () => favorites,
        getVolumes: () => volumes,
        getDropdownRef: () => dropdownRef,
        getRenameInputRef: () => renameInputRef,
        navigate: (volume) => { void handleVolumeSelect(volume) },
    })

    // Current volume info derived from volumes list (the actual containing volume)
    // Special case: 'network' is a virtual volume, not from the backend
    // For MTP volumes, look up by volumeId directly; for filesystem volumes, use containingVolumeId
    const currentVolume = $derived(
        volumeId === 'network'
            ? { id: 'network', name: tString('fileExplorer.navigation.networkVolume'), path: 'smb://', category: 'network' as const, isEjectable: false }
            : volumeId === 'search-results'
              ? {
                    // R3 B6: the volume selector reads "Search results", a
                    // generic noun matching every other volume's slot. The
                    // search-specific label (the AI title / pattern) moved to
                    // the path slot in `FilePane.svelte::breadcrumbDisplayPath`.
                    id: 'search-results',
                    name: tString('fileExplorer.navigation.searchResultsVolume'),
                    path: 'search-results://',
                    category: 'network' as const,
                    isEjectable: false,
                }
              : volumes.find((v) => v.id === volumeId && v.category === 'mobile_device')
                ?? volumes.find((v) => v.id === containingVolumeId),
    )

    /**
     * The snapshot's friendly label is rendered as the trailing path text
     * (in `FilePane.svelte`'s `breadcrumbDisplayPath`). The volume selector
     * here reads the static "Search results" label so the volume-selector
     * slot describes the KIND of volume (matching every other volume:
     * "Network", "Macintosh HD", an MTP device name) and the path slot
     * carries the QUERY-specific label. Don't invert these (label in the
     * volume slot, empty path).
     */
    const currentVolumeName = $derived(currentVolume?.name ?? tString('fileExplorer.navigation.volumeFallback'))
    const currentVolumeIcon = $derived(getIconForVolume(currentVolume))

    // Generic macOS folder icon used as fallback when a volume has no icon (for example,
    // FDA-gated favorites whose icons aren't fetched yet to avoid TCC popups). The `dir`
    // icon is sampled from `~`, which isn't TCC-protected, so prefetching is always safe.
    // Reading `$iconCacheVersion` re-evaluates the derived value once the icon lands.
    const dirIconFallback = $derived.by(() => {
        void $iconCacheVersion
        return getCachedIcon('dir')
    })

    // `volumes` with favorites reordered per the optimistic override (each favorite SLOT keeps its
    // position; only which favorite fills it changes). Everything below derives from this, so an
    // optimistic reorder shows without waiting for the backend round-trip. The override itself lives
    // in `fav` (the favorites controller); a local-first reorder there re-derives this synchronously.
    const effectiveVolumes = $derived.by(() => {
        const order = fav.optimisticFavoriteIds
        if (!order) return volumes
        const rank = new Map(order.map((id, i) => [id, i]))
        const orderedFavs = volumes
            .filter((v) => v.category === 'favorite')
            .slice()
            .sort((a, b) => (rank.get(a.id) ?? Number.POSITIVE_INFINITY) - (rank.get(b.id) ?? Number.POSITIVE_INFINITY))
        let fi = 0
        return volumes.map((v) => (v.category === 'favorite' ? orderedFavs[fi++] : v))
    })

    // Group volumes by category for display. The grouping helper renames the synthetic
    // "Network" entry to "Network (disabled)" when networking is off; the click handler
    // checks `getNetworkEnabled()` and routes to settings instead of navigating.
    const groupedVolumes = $derived(groupByCategory(effectiveVolumes, { networkEnabled: getNetworkEnabled() }))

    // Flat list of all volumes for keyboard navigation
    const allVolumes = $derived(groupedVolumes.flatMap((g) => g.items))

    // When dropdown opens, initialize highlight to current volume and fit to viewport.
    $effect(() => {
        if (isOpen) {
            // Init ONCE on open. Read the volume list untracked: otherwise a later `volumes-changed`
            // refresh (for example right after a favorite reorder) re-runs this effect and resets the
            // highlight to the current volume, stealing it from the just-moved favorite.
            untrack(() => {
                const currentIdx = allVolumes.findIndex((v) => shouldShowCheckmark(v, containingVolumeId))
                highlightedIndex = currentIdx >= 0 ? currentIdx : 0
                void fitDropdownToViewport()
            })
        } else {
            highlightedIndex = -1
            keyboardMode.reset()
        }
    })

    // Clear cached space info when the volume list changes (mount/unmount/MTP connect)
    // and re-fetch if the dropdown is open
    let prevVolumeIds = ''
    $effect(() => {
        const ids = volumes.map((v) => v.id).join(',')
        if (prevVolumeIds && ids !== prevVolumeIds) {
            spaceManager.clearAll()
            if (isOpen) {
                void spaceManager.fetchVolumeSpaces(volumes)
            }
        }
        prevVolumeIds = ids
    })

    async function fitDropdownToViewport() {
        await tick()
        const dropdown = dropdownRef?.querySelector('.volume-dropdown') as HTMLElement | null
        const anchor = dropdownRef?.querySelector('.volume-name, .breadcrumb-options-trigger') as HTMLElement | null
        if (dropdown && anchor) {
            const rect = anchor.getBoundingClientRect()
            const top = rect.bottom + 4 // spacing below the breadcrumb
            dropdown.style.top = `${String(top)}px`
            dropdown.style.left = `${String(rect.left)}px`
            dropdown.style.maxHeight = `${String(window.innerHeight - top - 8)}px`
        }
    }

    // Re-fit dropdown on window resize so it adapts to the available space
    function handleResize() {
        if (isOpen) {
            void fitDropdownToViewport()
        }
    }

    async function updateContainingVolume(path: string) {
        const { volume: containing } = await resolvePathVolume(path)
        containingVolumeId = containing?.id ?? volumeId
    }

    async function handleVolumeSelect(volume: VolumeInfo) {
        isOpen = false

        // "Network (disabled)" entry → don't navigate, deep-link to the toggle in Settings
        // so the user can flip it on. Identified by the synthetic id, not the label, so
        // future label tweaks don't break this branch.
        if (volume.id === 'network' && !getNetworkEnabled()) {
            void openSettingsWindow(['File systems', 'SMB/Network shares'])
            return
        }

        // Check if this is a favorite (shortcut) or an actual volume
        if (volume.category === 'favorite') {
            // For favorites, find the actual containing volume
            const { volume: containingVolume } = await resolvePathVolume(volume.path)
            if (containingVolume) {
                // Navigate to the favorite's path, but set the volume to the containing volume
                onVolumeChange?.(containingVolume.id, containingVolume.path, volume.path)
            } else {
                // Fallback: use root volume
                onVolumeChange?.('root', '/', volume.path)
            }
        } else {
            // For actual volumes, navigate to the volume's root
            onVolumeChange?.(volume.id, volume.path, volume.path)
        }
    }

    function setOpen(value: boolean) {
        isOpen = value
        if (value) void spaceManager.fetchVolumeSpaces(volumes)
    }

    function handleToggle() {
        setOpen(!isOpen)
    }

    /** Exported for keyboard shortcut access from parent. */
    export function toggle() {
        setOpen(!isOpen)
    }
    export function getIsOpen(): boolean {
        return isOpen
    }
    export function close() {
        isOpen = false
    }
    export function open() {
        setOpen(true)
    }

    // Export keyboard handler for parent components to call
    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (!isOpen) return false

        // While renaming a favorite, the inline `<input>` owns every key: arrows,
        // Home/End, etc. move the text cursor, not the dropdown highlight. Bail so
        // the dropdown's list navigation doesn't steal them from the textbox. Enter
        // / Escape never reach here (the input's own handler stops propagation), so
        // commit / cancel still work.
        if (fav.renamingFavoriteId !== null) return false

        // Keyboard reorder of the highlighted favorite (Alt+Up / Alt+Down). The rows
        // aren't DOM-focused (the dropdown navigates by a virtual `highlightedIndex`),
        // so this must run here, BEFORE `handleDropdownKey` consumes the bare arrows.
        if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
            const highlighted =
                highlightedIndex >= 0 && highlightedIndex < allVolumes.length
                    ? allVolumes[highlightedIndex]
                    : undefined
            if (highlighted && highlighted.category === 'favorite') {
                e.preventDefault()
                const delta = e.key === 'ArrowUp' ? -1 : 1
                // Synchronous + local-first: `fav.reorderHighlighted` sets the optimistic order, so
                // `favorites` / `allVolumes` re-derive immediately and a rapid next press computes
                // against the fresh order (no stale-state race). Favorites lead `allVolumes` in order,
                // so the favorite's new index IS its new list index; set the highlight to it directly
                // so repeated Alt+Down keeps walking the same item. The open-effect's `untrack` keeps
                // the later refresh from resetting it.
                const newFavIndex = fav.reorderHighlighted(highlighted, delta)
                if (newFavIndex !== null) {
                    highlightedIndex = newFavIndex
                    enterKeyboardMode()
                }
                return true
            }
        }

        const submenuResult = handleSubmenuKey(e.key, {
            isOpen: () => submenu.volumeId !== null,
            close: () => { submenu.close(); },
            activate: () => {
                void handleSubmenuAction()
            },
        })
        if (submenuResult !== null) {
            e.preventDefault()
            return submenuResult
        }

        const handled = handleDropdownKey(e.key, {
            moveHighlight: (delta) => {
                highlightedIndex = (highlightedIndex + delta + allVolumes.length) % allVolumes.length
                enterKeyboardMode()
            },
            goHome: () => {
                highlightedIndex = 0
                enterKeyboardMode()
            },
            goEnd: () => {
                highlightedIndex = allVolumes.length - 1
                enterKeyboardMode()
            },
            activate: () => {
                if (highlightedIndex >= 0 && highlightedIndex < allVolumes.length) {
                    void handleVolumeSelect(allVolumes[highlightedIndex])
                }
            },
            close: () => {
                isOpen = false
            },
            highlightedSupportsSubmenu: () =>
                highlightedIndex >= 0 && allVolumes[highlightedIndex]?.smbConnectionState === 'os_mount',
            openSubmenuAtHighlight: () => {
                const el = dropdownRef?.querySelector(
                    `.volume-item[data-index="${String(highlightedIndex)}"]`,
                ) as HTMLElement | null
                if (el) submenu.open(allVolumes[highlightedIndex].id, el, true)
            },
        })
        if (handled) e.preventDefault()
        return handled
    }

    function enterKeyboardMode() {
        keyboardMode.enter()
        void scrollHighlightedIntoView()
    }

    async function scrollHighlightedIntoView() {
        await tick()
        const el = dropdownRef?.querySelector(
            `.volume-item[data-index="${String(highlightedIndex)}"]`,
        ) as HTMLElement | null
        el?.scrollIntoView({ block: 'nearest' })
    }

    // Handle mouse hover to sync with keyboard navigation
    function handleVolumeHover(volume: VolumeInfo) {
        if (keyboardMode.isKeyboardMode) return
        const idx = allVolumes.indexOf(volume)
        if (idx >= 0) highlightedIndex = idx
    }

    function handleDropdownMouseMove(e: MouseEvent) {
        const idx = keyboardMode.onMouseMove(e)
        if (idx !== null) highlightedIndex = idx
    }

    function handleClickOutside(event: MouseEvent) {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
            isOpen = false
        }
    }

    // Document-level keyboard handler for Escape when dropdown is open
    function handleDocumentKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape' && isOpen) isOpen = false
    }

    // Update containing volume when current path changes
    $effect(() => {
        void updateContainingVolume(currentPath)
    })

    onMount(() => {
        void updateContainingVolume(currentPath)

        // Make sure the generic dir icon is cached for the fallback below.
        if (!getCachedIcon('dir')) {
            void prefetchIcons(['dir'], getUseAppIconsForDocuments())
        }

        // Close on click outside
        document.addEventListener('click', handleClickOutside)
        document.addEventListener('click', handleBreadcrumbPopupClickOutside)
        document.addEventListener('keydown', handleDocumentKeyDown)
        document.addEventListener('keydown', handleBreadcrumbPopupKeyDown)
        window.addEventListener('resize', handleResize)

        // Native row context menu (Rename / Remove a favorite) routes its pick back here.
        void onVolumeContextAction(handleVolumeContextAction).then((unlisten) => {
            unlistenVolumeContext = unlisten
        })
    })

    let unlistenVolumeContext: UnlistenFn | undefined

    onDestroy(() => {
        spaceManager.destroy()
        fav.destroy()
        unlistenVolumeContext?.()
        document.removeEventListener('click', handleClickOutside)
        document.removeEventListener('click', handleBreadcrumbPopupClickOutside)
        document.removeEventListener('keydown', handleDocumentKeyDown)
        document.removeEventListener('keydown', handleBreadcrumbPopupKeyDown)
        window.removeEventListener('resize', handleResize)
    })

    async function handleSubmenuAction(overrideVolumeId?: string) {
        const vid = overrideVolumeId ?? submenu.volumeId
        submenu.close()
        breadcrumbPopup.close()
        if (!vid) return

        // Direct smb2 upgrade opens a TCP socket to a private IP, which triggers macOS's
        // Local Network prompt on its own, so this is the right moment to also kick off
        // mDNS discovery for the rest of the network UI.
        triggerNetworkDiscovery()

        const connectingToastId = addToast(tString('fileExplorer.navigation.connectingDirectly'), { dismissal: 'persistent' })

        try {
            const result = await upgradeToSmbVolume(vid)
            dismissToast(connectingToastId)

            if (result.status === 'success') {
                addToast(tString('fileExplorer.pane.connectedDirectlyToast'), { level: 'success' })
                requestVolumeRefresh()
            } else if (result.status === 'credentialsNeeded') {
                // Before asking the user to type a password, see if macOS/Finder already
                // saved one for this share (prompt-free probe). If so, offer to reuse it.
                if (await tryUseSavedPassword(vid, result.displayName)) return
                onSmbUpgradeLogin?.(result, vid)
            } else {
                addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: result.message }), { level: 'error' })
            }
        } catch (e) {
            dismissToast(connectingToastId)
            addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: String(e) }), { level: 'error' })
        }
    }

    /**
     * If macOS/Finder already saved a password for this share, offer to reuse it (so the
     * user doesn't retype it). A prompt-free probe decides whether to offer; on "Use
     * saved password" we prime the user (the macOS Keychain consent dialog comes next,
     * and we can't customize its text) then read+connect. Returns `true` when it fully
     * handled the connection (connected, or the saved password was absent/denied/failed
     * and we routed to the login form), so the caller skips its own login-form trigger.
     * Returns `false` when there's nothing saved or the user chose to type it instead.
     */
    async function tryUseSavedPassword(vid: string, displayName: string): Promise<boolean> {
        if (!(await systemHasSavedSmbPassword(vid))) return false

        const useSaved = await ask(tString('fileExplorer.navigation.useSavedPasswordMessage', { displayName }), {
            title: tString('fileExplorer.navigation.useSavedPasswordTitle'),
            kind: 'info',
            okLabel: tString('fileExplorer.navigation.useSavedPasswordConfirm'),
            cancelLabel: tString('fileExplorer.navigation.useSavedPasswordCancel'),
        })
        if (!useSaved) return false

        const savedToastId = addToast(tString('fileExplorer.navigation.connectingWithSavedPassword'), { dismissal: 'persistent' })
        try {
            const r = await upgradeToSmbVolumeUsingSavedPassword(vid)
            dismissToast(savedToastId)
            if (r.status === 'success') {
                addToast(tString('fileExplorer.pane.connectedDirectlyToast'), { level: 'success' })
                requestVolumeRefresh()
                return true
            }
            if (r.status === 'credentialsNeeded') {
                // Saved password was absent/denied/wrong — fall to the login form.
                onSmbUpgradeLogin?.(r, vid)
                return true
            }
            addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: r.message }), { level: 'error' })
            return true
        } catch (e) {
            dismissToast(savedToastId)
            addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: String(e) }), { level: 'error' })
            return true
        }
    }

    // Per-row right-click context menu. Favorites get Rename / Remove; ejectable
    // volumes get Eject ({name}); anything else has no menu. Uses the NATIVE (muda)
    // menu via `showVolumeRowContextMenu`, matching the breadcrumb / tab menus. While
    // the native menu tracks, the webview is frozen, so the dropdown highlight can't
    // drift onto another row under the cursor or arrow keys — it stays pinned to the
    // right-clicked one. The picked action returns via the `volume-context-action`
    // event: eject is handled in `DualPaneExplorer`; rename / remove land in
    // `handleVolumeContextAction` below (the open dropdown owns them).
    function openRowMenu(volume: VolumeInfo, event: MouseEvent) {
        event.preventDefault()
        event.stopPropagation()
        const isFavorite = volume.category === 'favorite'
        const ejectable = isVolumeEjectable(volume)
        if (!isFavorite && !ejectable) return
        void showVolumeRowContextMenu(volume.id, volume.name, isFavorite, ejectable)
    }

    // Rename / remove a favorite when the user picks it from the native row menu.
    // Both panes' breadcrumbs receive this global event, but only the one whose
    // dropdown is open owns the menu it spawned (favorites are global, so the id
    // alone can't tell the panes apart; `isOpen` can). Eject is handled elsewhere.
    function handleVolumeContextAction(payload: { action: string; volumeId: string }) {
        if (!isOpen) return
        if (payload.action !== 'rename-favorite' && payload.action !== 'remove-favorite') return
        const volume = favorites.find((f) => f.id === payload.volumeId)
        if (!volume) return
        if (payload.action === 'rename-favorite') fav.startRename(volume)
        else void fav.remove(volume)
    }

    // ── Favorites: remove, rename, reorder ───────────────────────────────
    // Favorites arrive as VolumeInfo with `category: 'favorite'` and `id: 'fav-<favId>'`.
    // The mutate commands take the bare id (strip the `fav-` prefix). Each mutation re-emits
    // `volumes-changed`, so the list below re-derives with no manual refresh.
    const favorites = $derived(effectiveVolumes.filter((v) => v.category === 'favorite'))

    async function handleEjectClick(volume: VolumeInfo, event?: MouseEvent) {
        event?.stopPropagation()
        // Guard: the eject controls are disabled while the volume is busy, but a
        // keyboard / edge path could still reach here. Don't tear down a volume
        // mid-transfer.
        if (isVolumeBusy(volume.id)) return
        breadcrumbPopup.close()
        // Keep the dropdown open so several drives can be ejected in a row; the ejected
        // volume disappears on its own via `volume-unmounted` / `mtp-device-disconnected`.
        try {
            await ejectVolume(volume.id)
            // Success: the volume disappears via `volume-unmounted` (disk) or
            // `mtp-device-disconnected` (MTP). No toast needed — the change is
            // visible. Panes redirect to root via the existing listeners.
        } catch (e) {
            addToast(tString('fileExplorer.pane.ejectFailedToast', { volumeName: volume.name, message: getIpcErrorMessage(e) }), {
                level: 'error',
            })
        }
    }

    function handleBreadcrumbPopupClickOutside(event: MouseEvent) {
        if (breadcrumbPopupRef && !breadcrumbPopupRef.contains(event.target as Node)) {
            breadcrumbPopup.close()
        }
    }

    function handleBreadcrumbPopupKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape' && breadcrumbPopup.isOpen) {
            breadcrumbPopup.close()
        }
    }

    // Close submenu when the dropdown closes (covers click-outside too)
    $effect(() => {
        if (!isOpen) submenu.close()
    })

</script>

<div class="volume-breadcrumb" bind:this={dropdownRef}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="volume-name" class:is-open={isOpen} onclick={handleToggle}>
        {#if currentVolume && isRestricted(currentVolume.path) && dirIconFallback}
            <!-- TCC-denied paths: `NSWorkspace.iconForFile` returns a confusing "no
                 access" placeholder. Use the generic Aqua folder icon instead. -->
            <img class="icon" src={dirIconFallback} alt="" />
        {:else if currentVolumeIcon}
            <img class="icon" src={currentVolumeIcon} alt="" />
        {:else if volumeId === 'network'}
            <span class="icon-emoji">🌐</span>
        {:else if dirIconFallback}
            <img class="icon" src={dirIconFallback} alt="" />
        {/if}
        {currentVolumeName}
        {#if currentVolume?.isReadOnly}
            <span class="read-only-indicator" use:tooltip={tString('fileExplorer.navigation.readOnlyTooltip')}>🔒</span>
        {/if}
        <span class="chevron"></span>
    </span>
    {#if currentVolume?.usbSpeed}
        <span
            class="usb-speed-indicator breadcrumb-usb-speed-indicator usb-speed-indicator-{describeUsbSpeed(currentVolume.usbSpeed).tier}"
            use:tooltip={`${usbSpeedDisplay(currentVolume)}\n${tString('fileExplorer.navigation.usbSpeedNegotiated')}`}
        ></span>
    {/if}
    {#if currentVolume?.smbConnectionState === 'direct'}
        <span
            class="smb-indicator breadcrumb-smb-indicator smb-indicator-direct"
            use:tooltip={getConnectionTooltip('direct')}
        ></span>
    {:else if currentVolume?.smbConnectionState === 'os_mount'}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
            class="breadcrumb-options-trigger"
            class:is-open={breadcrumbPopup.isOpen}
            bind:this={breadcrumbPopupRef}
            use:tooltip={breadcrumbPopup.isOpen ? '' : tString('fileExplorer.navigation.volumeOptionsTooltip')}
            onclick={(e: MouseEvent) => {
                e.stopPropagation()
                isOpen = false
                breadcrumbPopup.toggle()
            }}
        >
            <span class="smb-indicator smb-indicator-os_mount"></span>
            <span class="chevron"></span>
        </span>
        {#if breadcrumbPopup.isOpen}
            <div class="breadcrumb-popup">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                    class="breadcrumb-popup-item"
                    onclick={(e: MouseEvent) => {
                        e.stopPropagation()
                        void handleSubmenuAction(currentVolume.id)
                    }}
                >
                    {tString('fileExplorer.navigation.connectDirectly')}
                </div>
            </div>
        {/if}
    {/if}
    {#if currentVolume && isVolumeEjectable(currentVolume)}
        <button
            type="button"
            class="eject-button breadcrumb-eject-button"
            aria-label={isVolumeBusy(currentVolume.id)
                ? EJECT_BUSY_TOOLTIP
                : tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name: currentVolume.name })}
            disabled={isVolumeBusy(currentVolume.id)}
            use:tooltip={isVolumeBusy(currentVolume.id)
                ? EJECT_BUSY_TOOLTIP
                : tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name: currentVolume.name })}
            onclick={(e: MouseEvent) => { void handleEjectClick(currentVolume, e) }}
        >
            <Icon name="eject" size={14} aria-hidden="true" />
        </button>
    {/if}

    {#if isOpen && (groupedVolumes.length > 0 || volumesTimedOut)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="volume-dropdown" class:keyboard-mode={keyboardMode.isKeyboardMode} onmousemove={handleDropdownMouseMove}>
            {#each groupedVolumes as group, groupIndex (group.category)}
                {#if group.label && groupIndex > 0}
                    <div class="category-separator"></div>
                {/if}
                {#if group.label}
                    <div class="category-label">{group.label}</div>
                {/if}
                {#if group.category === 'favorite' && group.items.length === 0}
                    <!-- Empty state: the user removed every favorite. A disabled, non-focusable,
                         non-clickable placeholder so the section reads as a real (empty) state. -->
                    <div class="favorites-empty" aria-disabled="true">{tString('fileExplorer.navigation.favoritesEmpty')}</div>
                {/if}
                {#each group.items as volume (volume.id)}
                    {@const isFavorite = volume.category === 'favorite'}
                    {@const favIndex = isFavorite ? favorites.findIndex((f) => f.id === volume.id) : -1}
                    <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                    <div
                        class="volume-item"
                        class:favorite-item={isFavorite}
                        class:is-dragging={isFavorite && fav.draggingFavoriteId === volume.id}
                        class:is-drag-over={isFavorite && fav.dragOverIndex === favIndex && fav.draggingFavoriteId !== volume.id}
                        class:is-drag-over-end={isFavorite &&
                            fav.draggingFavoriteId !== volume.id &&
                            fav.dragOverIndex === favorites.length &&
                            favIndex === favorites.length - 1}
                        class:is-under-cursor={shouldShowCheckmark(volume, containingVolumeId)}
                        class:is-focused-and-under-cursor={allVolumes.indexOf(volume) === highlightedIndex && !submenu.volumeId}
                        class:is-restricted={isRestricted(volume.path)}
                        data-index={allVolumes.indexOf(volume)}
                        data-fav-id={isFavorite ? volume.id : undefined}
                        use:tooltip={isRestricted(volume.path)
                            ? RESTRICTED_FOLDER_TOOLTIP
                            : isFavorite
                              ? favoriteTooltip(volume)
                              : ''}
                        onclick={() => {
                            // Favorites navigate from the pointer mouseup handler (it decides
                            // click-vs-drag), so skip the click path for them to avoid a double-fire.
                            if (isFavorite || fav.renamingFavoriteId === volume.id) return
                            void handleVolumeSelect(volume)
                        }}
                        oncontextmenu={(e: MouseEvent) => { openRowMenu(volume, e); }}
                        onmousedown={isFavorite ? (e: MouseEvent) => { fav.handleMouseDown(volume, e) } : undefined}
                        onmouseover={(e: MouseEvent) => {
                            handleVolumeHover(volume)
                            if (volume.smbConnectionState === 'os_mount') {
                                submenu.open(volume.id, e.currentTarget as HTMLElement)
                            } else if (submenu.volumeId) {
                                submenu.close()
                            }
                        }}
                    >
                        {#if shouldShowCheckmark(volume, containingVolumeId)}
                            <span class="checkmark">✓</span>
                        {:else}
                            <span class="checkmark-placeholder"></span>
                        {/if}
                        {#if volume.category === 'cloud_drive'}
                            <img class="volume-icon" src="/icons/sync-online-only.svg" alt="" />
                        {:else if volume.category === 'mobile_device'}
                            <img class="volume-icon" src="/icons/mobile-device.svg" alt="" />
                        {:else if volume.category === 'network'}
                            <span class="volume-icon-placeholder">🌐</span>
                        {:else if isRestricted(volume.path) && dirIconFallback}
                            <!-- TCC-denied paths: `NSWorkspace.iconForFile` returns a confusing "no
                                 access" placeholder. Use the generic Aqua folder icon instead. -->
                            <img class="volume-icon" src={dirIconFallback} alt="" />
                        {:else if volume.icon}
                            <img class="volume-icon" src={volume.icon} alt="" />
                        {:else if dirIconFallback}
                            <img class="volume-icon" src={dirIconFallback} alt="" />
                        {:else}
                            <span class="volume-icon-placeholder">📁</span>
                        {/if}
                        {#if fav.renamingFavoriteId === volume.id}
                            <input
                                class="favorite-rename-input"
                                bind:this={renameInputRef}
                                bind:value={fav.renameDraft}
                                onclick={(e: MouseEvent) => { e.stopPropagation() }}
                                onkeydown={(e: KeyboardEvent) => { fav.handleRenameKeyDown(e, volume) }}
                                onblur={() => { void fav.commitRename(volume) }}
                                aria-label={tString('fileExplorer.navigation.renameFavoriteAriaLabel')}
                            />
                        {:else}
                            <span class="volume-label">{volume.name}</span>
                        {/if}
                        {#if isRestricted(volume.path)}
                            <span class="restricted-indicator" aria-hidden="true">
                                <Icon name="info" size={12} />
                            </span>
                        {/if}
                        {#if volume.isReadOnly}
                            <span class="read-only-indicator" use:tooltip={tString('fileExplorer.navigation.readOnlyTooltip')}>🔒</span>
                        {/if}
                        {#if volume.smbConnectionState}
                            <span
                                class="smb-indicator smb-indicator-{volume.smbConnectionState}"
                                use:tooltip={getConnectionTooltip(volume.smbConnectionState)}
                            ></span>
                            {#if volume.smbConnectionState === 'os_mount'}
                                <span class="submenu-trigger"></span>
                            {/if}
                        {/if}
                        {#if volume.usbSpeed}
                            <span
                                class="usb-speed-indicator usb-speed-indicator-{describeUsbSpeed(volume.usbSpeed).tier}"
                                use:tooltip={`${usbSpeedDisplay(volume)}\n${tString('fileExplorer.navigation.usbSpeedNegotiated')}`}
                            ></span>
                        {/if}
                        {#if isVolumeEjectable(volume)}
                            <button
                                type="button"
                                class="eject-button"
                                aria-label={isVolumeBusy(volume.id)
                                    ? EJECT_BUSY_TOOLTIP
                                    : tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name: volume.name })}
                                disabled={isVolumeBusy(volume.id)}
                                use:tooltip={isVolumeBusy(volume.id)
                                    ? EJECT_BUSY_TOOLTIP
                                    : tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name: volume.name })}
                                onclick={(e: MouseEvent) => { void handleEjectClick(volume, e) }}
                            >
                                <Icon name="eject" size={14} aria-hidden="true" />
                            </button>
                        {/if}
                    </div>
                    {#if volumeSpaceMap.has(volume.id)}
                        {@const space = volumeSpaceMap.get(volume.id)}
                        {#if space}
                            <!-- svelte-ignore a11y_click_events_have_key_events -->
                            <!-- svelte-ignore a11y_no_static_element_interactions -->
                            <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                            <div
                                class="volume-space-info"
                                onclick={() => { void handleVolumeSelect(volume) }}
                                onmouseover={() => { handleVolumeHover(volume) }}
                            >
                                <div class="volume-space-bar">
                                    <div
                                        class="volume-space-fill"
                                        style:width="{getUsedPercent(space)}%"
                                        style:background-color="var({getDiskUsageLevel(getUsedPercent(space)).cssVar})"
                                    ></div>
                                </div>
                                <span class="volume-space-text">{formatDiskSpaceShort(space, (b) => formatFileSizeWithFormat(b, getFileSizeFormat()))}</span>
                            </div>
                        {/if}
                    {:else if spaceRetryingSet.has(volume.id)}
                        <div
                            class="volume-space-info volume-space-timeout"
                            use:tooltip={spaceAutoRetryingSet.has(volume.id)
                                ? tString('fileExplorer.navigation.spaceRetryingAuto')
                                : tString('fileExplorer.navigation.spaceRetrying')}
                        >
                            <div class="volume-space-bar volume-space-bar-timeout">
                                <Spinner size="sm" />
                            </div>
                            <span class="volume-space-text volume-space-text-timeout"
                                >{tString('fileExplorer.navigation.spaceRetryingText')}</span
                            >
                        </div>
                    {:else if spaceTimedOutSet.has(volume.id)}
                        <!-- svelte-ignore a11y_click_events_have_key_events -->
                        <!-- svelte-ignore a11y_no_static_element_interactions -->
                        <div
                            class="volume-space-info volume-space-timeout"
                            class:space-shake={spaceRetryFailedSet.has(volume.id)}
                            use:tooltip={spaceRetryAttemptedSet.has(volume.id)
                                ? tString('fileExplorer.navigation.spaceStillUnavailable')
                                : tString('fileExplorer.navigation.spaceFetchFailed')}
                            onclick={(e: MouseEvent) => {
                                e.stopPropagation()
                                spaceManager.retryVolumeSpace(volume)
                            }}
                        >
                            <div class="volume-space-bar volume-space-bar-timeout">
                                <span class="volume-space-timeout-icon">?</span>
                            </div>
                            <span class="volume-space-text volume-space-text-timeout"
                                >{tString('fileExplorer.navigation.spaceUnavailableText')}</span
                            >
                        </div>
                    {/if}
                {/each}
            {/each}
            {#if volumesTimedOut}
                <div class="category-separator"></div>
                <div class="timeout-warning-row" class:retry-failed={volumeRetryFailed}>
                    <span class="timeout-warning-text"
                        >{volumeRetryFailed
                            ? tString('fileExplorer.navigation.volumesStillUnreachable')
                            : tString('fileExplorer.navigation.volumesMayBeMissing')}</span
                    >
                    <button
                        class="timeout-retry-button"
                        disabled={volumesRefreshing}
                        use:tooltip={tString('fileExplorer.navigation.refreshVolumeList')}
                        onclick={() => {
                            requestVolumeRefresh()
                        }}
                    >
                        <span class="timeout-retry-icon" class:is-retrying={volumesRefreshing}>↻</span>
                    </button>
                </div>
            {/if}
        </div>
        {#if submenu.volumeId && submenu.position}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
                class="connection-submenu"
                bind:this={submenuRef}
                style:top="{submenu.position.top}px"
                style:left="{submenu.position.left}px"
                onmouseleave={() => {
                    submenu.setHighlighted(false)
                    submenu.close()
                }}
            >
                <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                <div
                    class="connection-submenu-item"
                    class:is-highlighted={submenu.highlighted}
                    onmouseover={() => {
                        submenu.setHighlighted(true)
                    }}
                    onclick={(e: MouseEvent) => {
                        e.stopPropagation()
                        void handleSubmenuAction()
                    }}
                >
                    {tString('fileExplorer.navigation.connectDirectly')}
                </div>
            </div>
        {/if}
    {/if}
</div>

<span class="path-separator">▸</span>

<style>
    .volume-breadcrumb {
        position: relative;
        display: inline-flex;
        align-items: center;
    }

    .volume-name {
        cursor: default;
        font-weight: 500;
        color: var(--color-text-primary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        transition: background-color var(--transition-fast);
    }

    .volume-name:hover {
        background-color: var(--color-bg-tertiary);
    }

    .volume-name.is-open {
        background-color: var(--color-bg-tertiary);
    }

    .icon {
        width: calc(14px * var(--font-scale));
        height: calc(14px * var(--font-scale));
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-md);
        line-height: 1;
    }

    .chevron {
        /* CSS triangle: consistent size across fonts. Uses currentcolor
           so the parent element controls the color via hover/active states. */
        display: inline-block;
        width: 0;
        height: 0;
        border-left: 4px solid transparent;
        border-right: 4px solid transparent;
        border-top: 5px solid currentcolor;
        vertical-align: middle;
        color: var(--color-text-tertiary);
    }

    .volume-name:hover .chevron,
    .volume-name.is-open .chevron,
    .breadcrumb-options-trigger:hover .chevron,
    .breadcrumb-options-trigger.is-open .chevron {
        color: var(--color-text-primary);
    }

    .path-separator {
        color: var(--color-text-tertiary);
        margin: 0 var(--spacing-xs);
        font-size: var(--font-size-xs);
    }

    .volume-dropdown {
        position: fixed;
        min-width: 220px;
        max-height: calc(100vh - 30px); /* Fallback, overridden dynamically by fitDropdownToViewport() */
        overflow-y: auto;
        /* Frosted-glass material: shared tokens with the tooltip / filter-chip popover so the
           whole app reads as one glass. See `app.css` § Frosted-glass material. The translucent
           fill flips to opaque under `prefers-reduced-transparency` via the `--color-bg-glass`
           token (in `app.css`); the blur is dropped at the rule site below. */
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        z-index: var(--z-overlay); /* Above function key bar and other pane elements */
        padding: var(--spacing-xs) 0;
    }

    .category-label {
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-tertiary);
        padding: var(--spacing-sm) var(--spacing-md) var(--spacing-xs);
        text-transform: uppercase;
        /*noinspection CssNonIntegerLengthInPixels*/
        letter-spacing: 0.5px;
    }

    .category-separator {
        height: 1px;
        background-color: var(--color-border-strong);
        margin: var(--spacing-xs) var(--spacing-sm);
    }

    .volume-item {
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-md);
        cursor: default;
    }

    /* Show hover only when NOT in keyboard mode */
    /*noinspection CssUnusedSymbol*/
    .volume-dropdown:not(.keyboard-mode) .volume-item:hover,
    .volume-item.is-focused-and-under-cursor {
        background-color: var(--color-accent-subtle);
    }

    .volume-icon {
        width: var(--spacing-icon-size);
        height: var(--spacing-icon-size);
        object-fit: contain;
        flex-shrink: 0;
    }

    .volume-icon-placeholder {
        font-size: var(--font-size-md);
        width: var(--spacing-icon-size);
        text-align: center;
        flex-shrink: 0;
    }

    .volume-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* ── Favorites section ──────────────────────────────────────────── */

    .favorites-empty {
        padding: var(--spacing-sm) var(--spacing-md);
        color: var(--color-text-tertiary);
        font-style: italic;
        cursor: default;
        user-select: none;
    }

    .favorite-item {
        cursor: grab;
    }

    /*noinspection CssUnusedSymbol*/
    .favorite-item.is-dragging {
        cursor: grabbing;
        opacity: 0.5;
    }

    /* Drop-line cue: a top border marking the gap the pointer is over. */
    /*noinspection CssUnusedSymbol*/
    .favorite-item.is-drag-over {
        box-shadow: inset 0 2px 0 0 var(--color-accent);
    }

    /* Drop at the very end of the list: bottom border on the last favorite. */
    /*noinspection CssUnusedSymbol*/
    .favorite-item.is-drag-over-end {
        box-shadow: inset 0 -2px 0 0 var(--color-accent);
    }

    .favorite-rename-input {
        flex: 1;
        min-width: 0;
        font: inherit;
        color: var(--color-text-primary);
        background-color: var(--color-bg-primary);
        border: 1px solid var(--color-accent);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }

    .favorite-rename-input:focus {
        outline: none;
    }

    /* TCC-restricted entries: faded text + (i) icon. The tooltip explains the
       restriction and points to System Settings. See `restricted-paths-store`. */
    .volume-item.is-restricted .volume-label {
        font-style: italic;
        opacity: 0.6;
    }

    .restricted-indicator {
        display: inline-flex;
        align-items: center;
        opacity: 0.6;
        font-size: var(--font-size-sm);
        flex-shrink: 0;
    }

    .checkmark {
        width: calc(14px * var(--font-scale));
        font-size: var(--font-size-sm);
        flex-shrink: 0;
    }

    .checkmark-placeholder {
        width: 14px;
        flex-shrink: 0;
    }

    .read-only-indicator {
        font-size: var(--font-size-sm);
        margin-left: auto;
        opacity: 0.7;
    }

    .volume-space-info {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- left pad aligns to a computed icon+gap offset; 14px/16px are measured widths */
        padding: 0 var(--spacing-md) var(--spacing-xs) calc(14px + var(--spacing-sm) + 16px + var(--spacing-sm));
    }

    .volume-space-bar {
        flex: 1;
        height: 2px;
        background-color: var(--color-disk-track);
        border-radius: var(--radius-sm);
    }

    .volume-space-fill {
        height: 100%;
        border-radius: var(--radius-sm);
    }

    .volume-space-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        flex-shrink: 0;
    }

    /* Volume space timeout placeholder */
    .volume-space-timeout {
        cursor: default;
    }

    .volume-space-bar-timeout {
        border: 1px dashed var(--color-border);
        background-color: transparent;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 8px;
    }

    .volume-space-timeout-icon {
        font-size: var(--font-size-xs);
        color: var(--color-warning);
        line-height: 1;
        transition: opacity var(--transition-base);
    }

    /* Shake on retry failure */
    /*noinspection CssUnusedSymbol*/
    .space-shake {
        animation: shake 300ms ease;
    }

    @keyframes shake {
        0%,
        100% {
            transform: translateX(0);
        }
        25% {
            transform: translateX(-3px);
        }
        75% {
            transform: translateX(3px);
        }
    }

    .volume-space-text-timeout {
        color: var(--color-warning);
    }

    /* Volumes timeout warning row */
    .timeout-warning-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-md);
    }

    .timeout-warning-text {
        font-size: var(--font-size-xs);
        color: var(--color-warning);
        flex: 1;
    }

    .timeout-retry-button {
        background: none;
        border: none;
        padding: 0 var(--spacing-xs);
        cursor: default;
        color: var(--color-warning-text);
        font-size: var(--font-size-md);
        line-height: 1;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-base);
    }

    .timeout-retry-button:hover {
        background-color: var(--color-bg-tertiary);
    }

    .timeout-retry-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .timeout-retry-button:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    /*noinspection CssUnusedSymbol*/
    .timeout-retry-icon.is-retrying {
        display: inline-block;
        animation: spin 0.8s linear infinite;
    }

    /*noinspection CssUnusedSymbol*/
    .timeout-warning-row.retry-failed {
        animation: flash-warning 0.3s ease;
    }

    @keyframes flash-warning {
        0%,
        100% {
            background-color: transparent;
        }
        50% {
            background-color: var(--color-warning-bg);
        }
    }

    @media (prefers-reduced-motion: reduce) {
        /*noinspection CssUnusedSymbol*/
        .timeout-retry-icon.is-retrying {
            animation: none;
        }

        /*noinspection CssUnusedSymbol*/
        .timeout-warning-row.retry-failed {
            animation: none;
        }

        /* Reduced motion: opacity flash instead of shake */
        /*noinspection CssUnusedSymbol*/
        .space-shake {
            animation: flash-warning 300ms ease;
        }
    }

    /* ── SMB connection indicators ───────────────────────────────── */

    .smb-indicator {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
        opacity: 0.8;
    }

    /*noinspection CssUnusedSymbol*/
    .smb-indicator-direct {
        background-color: var(--color-allow);
    }

    /*noinspection CssUnusedSymbol*/
    .smb-indicator-os_mount {
        background-color: var(--color-warning);
    }

    /* In the dropdown, push the indicator to the far right */
    .volume-item .smb-indicator {
        margin-left: auto;
    }

    /* If read-only badge is also present, don't double-auto-margin */
    .volume-item .read-only-indicator + .smb-indicator {
        margin-left: var(--spacing-sm);
    }

    /* ── USB speed indicators (MTP volumes) ──────────────────────────
       Same shape as the SMB dot, with a 5-tier rainbow keyed to the
       negotiated USB generation: red → orange → yellow → light green →
       dark green. Dark green matches `--color-allow`, the same shade
       SMB uses for a healthy direct session. */

    .usb-speed-indicator {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
        opacity: 0.8;
    }

    /*noinspection CssUnusedSymbol*/
    .usb-speed-indicator-low {
        background-color: var(--color-apple-red);
    }

    /*noinspection CssUnusedSymbol*/
    .usb-speed-indicator-full {
        background-color: var(--color-apple-orange);
    }

    /*noinspection CssUnusedSymbol*/
    .usb-speed-indicator-high {
        background-color: var(--color-apple-yellow);
    }

    /*noinspection CssUnusedSymbol*/
    .usb-speed-indicator-super {
        background-color: var(--color-apple-green);
    }

    /*noinspection CssUnusedSymbol*/
    .usb-speed-indicator-super_plus {
        background-color: var(--color-allow);
    }

    /* In the dropdown, push the indicator to the far right (same as SMB) */
    .volume-item .usb-speed-indicator {
        margin-left: auto;
    }

    /* If another right-aligned badge is already present, don't double-auto-margin */
    .volume-item .smb-indicator + .usb-speed-indicator,
    .volume-item .submenu-trigger + .usb-speed-indicator,
    .volume-item .read-only-indicator + .usb-speed-indicator {
        margin-left: var(--spacing-sm);
    }

    .breadcrumb-usb-speed-indicator {
        margin-left: var(--spacing-xs);
    }

    .submenu-trigger {
        /* CSS right-pointing triangle (matches macOS submenu arrow) */
        display: inline-block;
        width: 0;
        height: 0;
        border-top: 4px solid transparent;
        border-bottom: 4px solid transparent;
        border-left: 5px solid var(--color-text-tertiary);
        flex-shrink: 0;
        margin-left: auto;
        padding: 0;
    }

    .connection-submenu {
        position: fixed;
        min-width: 220px;
        /* Same frosted glass as the dropdown it extends. See `.volume-dropdown`. */
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        /* Must be above the dropdown (--z-overlay: 200) */
        z-index: calc(var(--z-overlay) + 1);
        padding: var(--spacing-xs) 0;
    }

    .connection-submenu-item {
        padding: var(--spacing-sm) var(--spacing-md);
        cursor: default;
        white-space: nowrap;
    }

    /*noinspection CssUnusedSymbol*/
    .connection-submenu-item.is-highlighted,
    .connection-submenu-item:hover {
        background-color: var(--color-accent-subtle);
    }

    /* ── Breadcrumb inline popup ───────────────────────────────── */

    .breadcrumb-options-trigger {
        color: var(--color-text-tertiary);
        cursor: default;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin-left: var(--spacing-xxs);
        transition:
            background-color var(--transition-fast),
            color var(--transition-fast);
    }

    .breadcrumb-options-trigger:hover,
    .breadcrumb-options-trigger.is-open {
        color: var(--color-text-primary);
        background-color: var(--color-bg-tertiary);
    }

    .breadcrumb-smb-indicator {
        margin-left: var(--spacing-xs);
    }

    .breadcrumb-popup {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: var(--spacing-xs);
        min-width: 220px;
        /* Same frosted glass as the dropdown. See `.volume-dropdown`. */
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        z-index: var(--z-dropdown);
        padding: var(--spacing-xs) 0;
    }

    .breadcrumb-popup-item {
        padding: var(--spacing-sm) var(--spacing-md);
        cursor: default;
        white-space: nowrap;
    }

    .breadcrumb-popup-item:hover {
        background-color: var(--color-accent-subtle);
    }

    /* ── Eject button ────────────────────────────────────────────────
       Right-aligned next to the SMB / USB badges. Same flex-shrink and
       margin rules as the other right-aligned indicators so the badges
       and the button line up cleanly when both are present. */

    .eject-button {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        background: none;
        border: none;
        padding: var(--spacing-xxs);
        margin: 0;
        cursor: default;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        flex-shrink: 0;
        font: inherit;
        line-height: 1;
        transition: background-color var(--transition-base), color var(--transition-base);
    }

    .eject-button:hover:not(:disabled) {
        background-color: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .eject-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    /* Busy: a write op is reading from / writing to this volume, so ejecting is
       blocked. Greyed out, no hover affordance; the tooltip explains why. */
    .eject-button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    /* In the dropdown row, push the button to the far right when it's the only
       right-aligned element; otherwise sit next to whatever badge precedes it. */
    .volume-item .eject-button {
        margin-left: auto;
    }

    /* If a badge (smb / usb / submenu / read-only) is right before us, the
       badge already carries the auto margin — we just need a small gap. */
    .volume-item .smb-indicator + .eject-button,
    .volume-item .usb-speed-indicator + .eject-button,
    .volume-item .submenu-trigger + .eject-button,
    .volume-item .read-only-indicator + .eject-button {
        margin-left: var(--spacing-xs);
    }

    /* Closed-state (header) eject button: small left margin so it sits next
       to the SMB / USB badges instead of jamming against them. */
    .breadcrumb-eject-button {
        margin-left: var(--spacing-xs);
    }

    /* Reduced transparency: the `--color-bg-glass` token already flips to opaque
       (in `app.css`), so here we only drop the blur on the three glass surfaces. */
    @media (prefers-reduced-transparency: reduce) {
        .volume-dropdown,
        .connection-submenu,
        .breadcrumb-popup {
            -webkit-backdrop-filter: none;
            backdrop-filter: none;
        }
    }
</style>
