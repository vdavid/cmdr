<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        ejectVolume,
        getIpcErrorMessage,
        resolvePathVolume,
        upgradeToSmbVolume,
        type UpgradeResult,
    } from '$lib/tauri-commands'
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
    import InfoIcon from '~icons/lucide/info'
    import { describeUsbSpeed, type VolumeInfo } from '../types'
    import { isVolumeEjectable } from './eject-predicate'

    /** "USB 3.2 Gen 1 (Max. 625 MB/s)" - shared between the chip tooltip and the dropdown subline. */
    function usbSpeedDisplay(volume: VolumeInfo | undefined): string {
        if (!volume?.usbSpeed) return ''
        const { label, maxMBps } = describeUsbSpeed(volume.usbSpeed)
        const mbps = maxMBps >= 10 ? String(Math.round(maxMBps)) : maxMBps.toFixed(1)
        return `${label} (Max. ${mbps} MB/s)`
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
    const EJECT_BUSY_TOOLTIP = "Can't eject while operations are in progress on this device"
    import { groupByCategory, getIconForVolume } from './volume-grouping'
    import { createVolumeSpaceManager } from './volume-space-manager.svelte'
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

    // Current volume info derived from volumes list (the actual containing volume)
    // Special case: 'network' is a virtual volume, not from the backend
    // For MTP volumes, look up by volumeId directly; for filesystem volumes, use containingVolumeId
    const currentVolume = $derived(
        volumeId === 'network'
            ? { id: 'network', name: 'Network', path: 'smb://', category: 'network' as const, isEjectable: false }
            : volumeId === 'search-results'
              ? {
                    // R3 B6: the volume selector reads "Search results", a
                    // generic noun matching every other volume's slot. The
                    // search-specific label (the AI title / pattern) moved to
                    // the path slot in `FilePane.svelte::breadcrumbDisplayPath`.
                    id: 'search-results',
                    name: 'Search results',
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
    const currentVolumeName = $derived(currentVolume?.name ?? 'Volume')
    const currentVolumeIcon = $derived(getIconForVolume(currentVolume))

    // Generic macOS folder icon used as fallback when a volume has no icon (for example,
    // FDA-gated favorites whose icons aren't fetched yet to avoid TCC popups). The `dir`
    // icon is sampled from `~`, which isn't TCC-protected, so prefetching is always safe.
    // Reading `$iconCacheVersion` re-evaluates the derived value once the icon lands.
    const dirIconFallback = $derived.by(() => {
        void $iconCacheVersion
        return getCachedIcon('dir')
    })

    // Group volumes by category for display. The grouping helper renames the synthetic
    // "Network" entry to "Network (disabled)" when networking is off; the click handler
    // checks `getNetworkEnabled()` and routes to settings instead of navigating.
    const groupedVolumes = $derived(groupByCategory(volumes, { networkEnabled: getNetworkEnabled() }))

    // Flat list of all volumes for keyboard navigation
    const allVolumes = $derived(groupedVolumes.flatMap((g) => g.items))

    // When dropdown opens, initialize highlight to current volume and fit to viewport
    $effect(() => {
        if (isOpen) {
            const currentIdx = allVolumes.findIndex((v) => shouldShowCheckmark(v, containingVolumeId))
            highlightedIndex = currentIdx >= 0 ? currentIdx : 0
            void fitDropdownToViewport()
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
        document.addEventListener('click', handleRowMenuClickOutside)
        document.addEventListener('keydown', handleDocumentKeyDown)
        document.addEventListener('keydown', handleBreadcrumbPopupKeyDown)
        document.addEventListener('keydown', handleRowMenuKeyDown)
        window.addEventListener('resize', handleResize)
    })

    onDestroy(() => {
        spaceManager.destroy()
        document.removeEventListener('click', handleClickOutside)
        document.removeEventListener('click', handleBreadcrumbPopupClickOutside)
        document.removeEventListener('click', handleRowMenuClickOutside)
        document.removeEventListener('keydown', handleDocumentKeyDown)
        document.removeEventListener('keydown', handleBreadcrumbPopupKeyDown)
        document.removeEventListener('keydown', handleRowMenuKeyDown)
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

        const connectingToastId = addToast('Connecting directly...', { dismissal: 'persistent' })

        try {
            const result = await upgradeToSmbVolume(vid)
            dismissToast(connectingToastId)

            if (result.status === 'success') {
                addToast('Connected directly for faster access', { level: 'success' })
                requestVolumeRefresh()
            } else if (result.status === 'credentialsNeeded') {
                onSmbUpgradeLogin?.(result, vid)
            } else {
                addToast(`Direct connection failed: ${result.message}`, { level: 'error' })
            }
        } catch (e) {
            dismissToast(connectingToastId)
            addToast(`Direct connection failed: ${String(e)}`, { level: 'error' })
        }
    }

    // Per-volume right-click popup (the small "Eject (name)" menu shown when
    // the user right-clicks a row in the dropdown). Mirrors the breadcrumb-popup
    // pattern used for "Connect directly", but each invocation targets a specific
    // volume rather than the currently-selected one.
    let rowMenuVolumeId: string | null = $state(null)
    let rowMenuPosition: { left: number; top: number } | null = $state(null)
    let rowMenuRef: HTMLDivElement | undefined = $state()

    function openRowMenu(volume: VolumeInfo, event: MouseEvent) {
        event.preventDefault()
        event.stopPropagation()
        if (!isVolumeEjectable(volume)) return
        rowMenuVolumeId = volume.id
        rowMenuPosition = { left: event.clientX, top: event.clientY }
    }

    function closeRowMenu() {
        rowMenuVolumeId = null
        rowMenuPosition = null
    }

    async function handleEjectClick(volume: VolumeInfo, event?: MouseEvent) {
        event?.stopPropagation()
        // Guard: the eject controls are disabled while the volume is busy, but a
        // keyboard / edge path could still reach here. Don't tear down a volume
        // mid-transfer.
        if (isVolumeBusy(volume.id)) return
        closeRowMenu()
        breadcrumbPopup.close()
        isOpen = false
        try {
            await ejectVolume(volume.id)
            // Success: the volume disappears via `volume-unmounted` (disk) or
            // `mtp-device-disconnected` (MTP). No toast needed — the change is
            // visible. Panes redirect to root via the existing listeners.
        } catch (e) {
            addToast(`Couldn't eject ${volume.name}: ${getIpcErrorMessage(e)}`, { level: 'error' })
        }
    }

    function handleRowMenuClickOutside(event: MouseEvent) {
        if (rowMenuRef && !rowMenuRef.contains(event.target as Node)) {
            closeRowMenu()
        }
    }

    function handleRowMenuKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape' && rowMenuVolumeId) {
            closeRowMenu()
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
            <span class="read-only-indicator" use:tooltip={'Read-only'}>🔒</span>
        {/if}
        <span class="chevron"></span>
    </span>
    {#if currentVolume?.usbSpeed}
        <span
            class="usb-speed-indicator breadcrumb-usb-speed-indicator usb-speed-indicator-{describeUsbSpeed(currentVolume.usbSpeed).tier}"
            use:tooltip={`${usbSpeedDisplay(currentVolume)}\nNegotiated for this cable, port, and device`}
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
            use:tooltip={breadcrumbPopup.isOpen ? '' : 'Volume options'}
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
                    Connect directly for faster access
                </div>
            </div>
        {/if}
    {/if}
    {#if currentVolume && isVolumeEjectable(currentVolume)}
        <button
            type="button"
            class="eject-button breadcrumb-eject-button"
            aria-label={isVolumeBusy(currentVolume.id) ? EJECT_BUSY_TOOLTIP : `Eject ${currentVolume.name}`}
            disabled={isVolumeBusy(currentVolume.id)}
            use:tooltip={isVolumeBusy(currentVolume.id) ? EJECT_BUSY_TOOLTIP : `Eject ${currentVolume.name}`}
            onclick={(e: MouseEvent) => { void handleEjectClick(currentVolume, e) }}
        >
            <!-- Inline SVG: Lucide doesn't ship an `eject` icon (as of v0.477).
                 Shape matches the macOS menu-bar eject glyph: filled triangle
                 above a short bar. -->
            <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
                focusable="false"
            >
                <path d="M5 14 L12 5 L19 14 Z" />
                <line x1="5" y1="19" x2="19" y2="19" />
            </svg>
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
                {#each group.items as volume (volume.id)}
                    <!-- svelte-ignore a11y_click_events_have_key_events -->
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                    <div
                        class="volume-item"
                        class:is-under-cursor={shouldShowCheckmark(volume, containingVolumeId)}
                        class:is-focused-and-under-cursor={allVolumes.indexOf(volume) === highlightedIndex && !submenu.volumeId}
                        class:is-restricted={isRestricted(volume.path)}
                        data-index={allVolumes.indexOf(volume)}
                        use:tooltip={isRestricted(volume.path) ? RESTRICTED_FOLDER_TOOLTIP : ''}
                        onclick={() => {
                            void handleVolumeSelect(volume)
                        }}
                        oncontextmenu={(e: MouseEvent) => { openRowMenu(volume, e); }}
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
                        <span class="volume-label">{volume.name}</span>
                        {#if isRestricted(volume.path)}
                            <span class="restricted-indicator" aria-hidden="true">
                                <InfoIcon />
                            </span>
                        {/if}
                        {#if volume.isReadOnly}
                            <span class="read-only-indicator" use:tooltip={'Read-only'}>🔒</span>
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
                                use:tooltip={`${usbSpeedDisplay(volume)}\nNegotiated for this cable, port, and device`}
                            ></span>
                        {/if}
                        {#if isVolumeEjectable(volume)}
                            <button
                                type="button"
                                class="eject-button"
                                aria-label={isVolumeBusy(volume.id) ? EJECT_BUSY_TOOLTIP : `Eject ${volume.name}`}
                                disabled={isVolumeBusy(volume.id)}
                                use:tooltip={isVolumeBusy(volume.id) ? EJECT_BUSY_TOOLTIP : `Eject ${volume.name}`}
                                onclick={(e: MouseEvent) => { void handleEjectClick(volume, e) }}
                            >
                                <!-- Inline SVG: Lucide doesn't ship an `eject` icon (as of v0.477).
                 Shape matches the macOS menu-bar eject glyph: filled triangle
                 above a short bar. -->
            <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
                focusable="false"
            >
                <path d="M5 14 L12 5 L19 14 Z" />
                <line x1="5" y1="19" x2="19" y2="19" />
            </svg>
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
                                ? 'Retrying automatically\u2026'
                                : 'Retrying\u2026'}
                        >
                            <div class="volume-space-bar volume-space-bar-timeout">
                                <span class="volume-space-timeout-icon space-spinner">\u21BB</span>
                            </div>
                            <span class="volume-space-text volume-space-text-timeout">Retrying</span>
                        </div>
                    {:else if spaceTimedOutSet.has(volume.id)}
                        <!-- svelte-ignore a11y_click_events_have_key_events -->
                        <!-- svelte-ignore a11y_no_static_element_interactions -->
                        <div
                            class="volume-space-info volume-space-timeout"
                            class:space-shake={spaceRetryFailedSet.has(volume.id)}
                            use:tooltip={spaceRetryAttemptedSet.has(volume.id)
                                ? 'Still unavailable \u2014 click to retry'
                                : "Couldn't fetch disk space \u2014 click to retry"}
                            onclick={(e: MouseEvent) => {
                                e.stopPropagation()
                                spaceManager.retryVolumeSpace(volume)
                            }}
                        >
                            <div class="volume-space-bar volume-space-bar-timeout">
                                <span class="volume-space-timeout-icon">?</span>
                            </div>
                            <span class="volume-space-text volume-space-text-timeout">Unavailable</span>
                        </div>
                    {/if}
                {/each}
            {/each}
            {#if volumesTimedOut}
                <div class="category-separator"></div>
                <div class="timeout-warning-row" class:retry-failed={volumeRetryFailed}>
                    <span class="timeout-warning-text"
                        >{volumeRetryFailed
                            ? 'Still unreachable. Try again later'
                            : 'Some volumes may be missing'}</span
                    >
                    <button
                        class="timeout-retry-button"
                        disabled={volumesRefreshing}
                        use:tooltip={'Refresh volume list'}
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
                    Connect directly for faster access
                </div>
            </div>
        {/if}
    {/if}

    {#if rowMenuVolumeId && rowMenuPosition}
        {@const rowMenuVolume = volumes.find((v) => v.id === rowMenuVolumeId)}
        {#if rowMenuVolume}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
                class="row-menu"
                bind:this={rowMenuRef}
                style:left="{rowMenuPosition.left}px"
                style:top="{rowMenuPosition.top}px"
            >
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                    class="row-menu-item"
                    class:row-menu-item-disabled={isVolumeBusy(rowMenuVolume.id)}
                    use:tooltip={isVolumeBusy(rowMenuVolume.id) ? EJECT_BUSY_TOOLTIP : ''}
                    onclick={(e: MouseEvent) => { void handleEjectClick(rowMenuVolume, e) }}
                >
                    {isVolumeBusy(rowMenuVolume.id)
                        ? `Eject (${rowMenuVolume.name}) (busy)`
                        : `Eject (${rowMenuVolume.name})`}
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
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
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
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
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

    /* Spinner for retrying state */
    .space-spinner {
        display: inline-block;
        animation: spin 1s linear infinite;
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

        /* Reduced motion: pulsing opacity instead of spinning */
        .space-spinner {
            animation: pulse-opacity 1.5s ease-in-out infinite;
        }

        /* Reduced motion: opacity flash instead of shake */
        /*noinspection CssUnusedSymbol*/
        .space-shake {
            animation: flash-warning 300ms ease;
        }
    }

    @keyframes pulse-opacity {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0.3;
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
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
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
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
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

    /* The inline eject glyph inside the button: sized to match the row height. */
    .eject-button svg {
        width: 14px;
        height: 14px;
    }

    /* ── Per-volume right-click menu (dropdown rows) ──────────────── */

    .row-menu {
        position: fixed;
        min-width: 180px;
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        /* Above the dropdown (--z-overlay: 200) and its submenu. */
        z-index: calc(var(--z-overlay) + 2);
        padding: var(--spacing-xs) 0;
    }

    .row-menu-item {
        padding: var(--spacing-sm) var(--spacing-md);
        cursor: default;
        white-space: nowrap;
    }

    .row-menu-item:hover:not(.row-menu-item-disabled) {
        background-color: var(--color-accent-subtle);
    }

    /* Busy volume: the eject action is blocked while a transfer touches it. */
    .row-menu-item-disabled {
        opacity: 0.4;
    }
</style>
