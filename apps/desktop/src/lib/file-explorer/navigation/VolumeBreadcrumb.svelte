<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { listVolumes, findContainingVolume, listen, type UnlistenFn } from '$lib/tauri-commands'
    import { getDiskUsageLevel, getUsedPercent, formatDiskSpaceShort } from '../disk-space-utils'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { VolumeInfo } from '../types'
    import { getMtpVolumes, initialize as initMtpStore, scanDevices as scanMtpDevices, type MtpVolume } from '$lib/mtp'
    import { groupByCategory, getIconForVolume } from './volume-grouping'
    import { createVolumeSpaceManager } from './volume-space-manager.svelte'

    interface Props {
        volumeId: string
        currentPath: string
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
    }

    const { volumeId, currentPath, onVolumeChange }: Props = $props()

    let volumes = $state<VolumeInfo[]>([])
    let mtpVolumes = $state<MtpVolume[]>([])
    let isOpen = $state(false)
    let highlightedIndex = $state(-1)
    let dropdownRef: HTMLDivElement | undefined = $state()
    // Keyboard mode: when true, CSS :hover is suppressed to avoid double-highlight
    let isKeyboardMode = $state(false)
    let lastMousePos = $state<{ x: number; y: number } | null>(null)
    let unlistenMount: UnlistenFn | undefined
    let unlistenUnmount: UnlistenFn | undefined
    let unlistenMtpDetected: UnlistenFn | undefined
    let unlistenMtpConnected: UnlistenFn | undefined
    let unlistenMtpRemoved: UnlistenFn | undefined

    // The ID of the actual volume that contains the current path
    // This is used to show the checkmark on the correct volume, not on favorites
    let containingVolumeId = $state<string | null>(null)

    // Timeout tracking for listVolumes
    let volumesTimedOut = $state(false)
    let isRetryingVolumes = $state(false)
    let volumeRetryFailed = $state(false)
    let retryFailedRevertTimer: ReturnType<typeof setTimeout> | null = null

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
    // Special case: MTP volumes are handled via mtp:// paths
    const currentVolume = $derived(
        volumeId === 'network'
            ? { id: 'network', name: 'Network', path: 'smb://', category: 'network' as const, isEjectable: false }
            : volumeId.startsWith('mtp-')
              ? (() => {
                    // Try to find the exact MTP volume (device:storage format)
                    const mtpVolume = mtpVolumes.find((v) => v.id === volumeId || v.deviceId === volumeId)
                    return mtpVolume
                        ? {
                              id: mtpVolume.id,
                              name: mtpVolume.name,
                              path: mtpVolume.path,
                              category: 'mobile_device' as const,
                              isEjectable: true,
                              isReadOnly: mtpVolume.isReadOnly,
                          }
                        : undefined
                })()
              : volumes.find((v) => v.id === containingVolumeId),
    )
    const currentVolumeName = $derived(currentVolume?.name ?? 'Volume')
    const currentVolumeIcon = $derived(getIconForVolume(currentVolume))

    // Group volumes by category for display
    const groupedVolumes = $derived(groupByCategory(volumes, mtpVolumes))

    // Flat list of all volumes for keyboard navigation
    const allVolumes = $derived(groupedVolumes.flatMap((g) => g.items))

    // When dropdown opens, initialize highlight to current volume and fit to viewport
    $effect(() => {
        if (isOpen) {
            const currentIdx = allVolumes.findIndex((v) => shouldShowCheckmark(v))
            highlightedIndex = currentIdx >= 0 ? currentIdx : 0
            void fitDropdownToViewport()
        } else {
            highlightedIndex = -1
            isKeyboardMode = false
            lastMousePos = null
        }
    })

    async function fitDropdownToViewport() {
        await tick()
        const dropdown = dropdownRef?.querySelector('.volume-dropdown') as HTMLElement | null
        if (dropdown) {
            const top = dropdown.getBoundingClientRect().top
            dropdown.style.maxHeight = `${String(window.innerHeight - top - 8)}px`
        }
    }

    async function loadVolumes() {
        const result = await listVolumes()
        volumes = result.data
        volumesTimedOut = result.timedOut
    }

    async function retryLoadVolumes() {
        isRetryingVolumes = true
        volumeRetryFailed = false
        if (retryFailedRevertTimer) clearTimeout(retryFailedRevertTimer)
        try {
            await loadVolumes()
            if (volumesTimedOut) {
                volumeRetryFailed = true
                retryFailedRevertTimer = setTimeout(() => {
                    volumeRetryFailed = false
                }, 3000)
            }
        } finally {
            isRetryingVolumes = false
        }
    }

    async function updateContainingVolume(path: string) {
        const { data: containing } = await findContainingVolume(path)
        containingVolumeId = containing?.id ?? volumeId
    }

    async function handleVolumeSelect(volume: VolumeInfo) {
        isOpen = false

        // Check if this is a favorite (shortcut) or an actual volume
        if (volume.category === 'favorite') {
            // For favorites, find the actual containing volume
            const { data: containingVolume } = await findContainingVolume(volume.path)
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

    function handleToggle() {
        isOpen = !isOpen
        if (isOpen) {
            void spaceManager.fetchVolumeSpaces(volumes)
        }
    }

    // Export for keyboard shortcut access
    export function toggle() {
        isOpen = !isOpen
        if (isOpen) {
            void spaceManager.fetchVolumeSpaces(volumes)
        }
    }

    // Export to check if dropdown is open
    export function getIsOpen(): boolean {
        return isOpen
    }

    // Export to explicitly close the dropdown
    export function close() {
        isOpen = false
    }

    // Export to explicitly open the dropdown
    export function open() {
        isOpen = true
        void spaceManager.fetchVolumeSpaces(volumes)
    }

    // Export keyboard handler for parent components to call
    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (!isOpen) return false

        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                highlightedIndex = Math.min(highlightedIndex + 1, allVolumes.length - 1)
                enterKeyboardMode()
                return true
            case 'ArrowUp':
                e.preventDefault()
                highlightedIndex = Math.max(highlightedIndex - 1, 0)
                enterKeyboardMode()
                return true
            case 'Enter':
                e.preventDefault()
                if (highlightedIndex >= 0 && highlightedIndex < allVolumes.length) {
                    void handleVolumeSelect(allVolumes[highlightedIndex])
                }
                return true
            case 'Escape':
                e.preventDefault()
                isOpen = false
                return true
            case 'Home':
                e.preventDefault()
                highlightedIndex = 0
                enterKeyboardMode()
                return true
            case 'End':
                e.preventDefault()
                highlightedIndex = allVolumes.length - 1
                enterKeyboardMode()
                return true
            default:
                return false
        }
    }

    function enterKeyboardMode() {
        isKeyboardMode = true
        lastMousePos = null // Will be captured on next mousemove
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
        if (isKeyboardMode) return // Don't update highlight while in keyboard mode
        const idx = allVolumes.indexOf(volume)
        if (idx >= 0) {
            highlightedIndex = idx
        }
    }

    // Handle mouse movement to exit keyboard mode after 5px threshold
    function handleDropdownMouseMove(e: MouseEvent) {
        if (!isKeyboardMode) return

        if (!lastMousePos) {
            // Capture position on first move after entering keyboard mode
            lastMousePos = { x: e.clientX, y: e.clientY }
            return
        }

        const dx = Math.abs(e.clientX - lastMousePos.x)
        const dy = Math.abs(e.clientY - lastMousePos.y)
        if (dx > 5 || dy > 5) {
            isKeyboardMode = false
            lastMousePos = null
            // Update highlight to the item under the mouse cursor
            const volumeItem = (e.target as HTMLElement).closest('.volume-item')
            if (volumeItem) {
                const idx = parseInt(volumeItem.getAttribute('data-index') ?? '-1', 10)
                if (idx >= 0) {
                    highlightedIndex = idx
                }
            }
        }
    }

    function handleClickOutside(event: MouseEvent) {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
            isOpen = false
        }
    }

    // Document-level keyboard handler for Escape when dropdown is open
    function handleDocumentKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape' && isOpen) {
            isOpen = false
        }
    }

    // Update containing volume when current path changes
    $effect(() => {
        void updateContainingVolume(currentPath)
    })

    // Refresh volumes if the current volumeId is not in our list
    // This handles the race condition where we navigate to a newly mounted volume
    // before the mount event is received
    $effect(() => {
        if (volumeId && volumeId !== 'network') {
            // Skip check for MTP volumes - they're tracked separately in mtpVolumes
            if (volumeId.startsWith('mtp-')) {
                return
            }
            const found = volumes.find((v) => v.id === volumeId)
            if (!found && volumes.length > 0) {
                // Volume not found but we have a list - might be a newly mounted volume
                void loadVolumes()
            }
        }
    })

    async function loadMtpVolumes() {
        // Initialize the MTP store if needed, scan for devices, then get volumes
        await initMtpStore()
        await scanMtpDevices()
        mtpVolumes = getMtpVolumes()
    }

    async function refreshMtpVolumes() {
        // Small delay to let mtp-store's event handler finish scanning first
        await new Promise((resolve) => setTimeout(resolve, 100))
        await initMtpStore()
        mtpVolumes = getMtpVolumes()
    }

    onMount(async () => {
        await loadVolumes()
        await loadMtpVolumes()
        await updateContainingVolume(currentPath)

        // Listen for volume mount/unmount events
        unlistenMount = await listen<{ volumeId: string }>('volume-mounted', () => {
            spaceManager.clearAll()
            void loadVolumes()
        })

        unlistenUnmount = await listen<{ volumeId: string }>('volume-unmounted', () => {
            spaceManager.clearAll()
            void loadVolumes()
        })

        // Listen for MTP device hotplug events
        // Use refreshMtpVolumes() to avoid race with mtp-store's event handler
        unlistenMtpDetected = await listen<{ deviceId: string }>('mtp-device-detected', () => {
            void refreshMtpVolumes()
        })

        // Listen for MTP device connection (this is when isReadOnly is determined via probe)
        unlistenMtpConnected = await listen<{ deviceId: string }>('mtp-device-connected', () => {
            void refreshMtpVolumes()
        })

        unlistenMtpRemoved = await listen<{ deviceId: string }>('mtp-device-removed', () => {
            void refreshMtpVolumes()
        })

        // Close on click outside
        document.addEventListener('click', handleClickOutside)
        document.addEventListener('keydown', handleDocumentKeyDown)
    })

    onDestroy(() => {
        unlistenMount?.()
        unlistenUnmount?.()
        unlistenMtpDetected?.()
        unlistenMtpConnected?.()
        unlistenMtpRemoved?.()
        spaceManager.destroy()
        document.removeEventListener('click', handleClickOutside)
        document.removeEventListener('keydown', handleDocumentKeyDown)
    })

    // Helper: check if a volume should show the checkmark
    // For favorites, never show checkmark
    // For actual volumes, show if it's the containing volume for the current path
    function shouldShowCheckmark(volume: VolumeInfo): boolean {
        if (volume.category === 'favorite') {
            return false
        }
        return volume.id === containingVolumeId
    }
</script>

<div class="volume-breadcrumb" bind:this={dropdownRef}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="volume-name" class:is-open={isOpen} onclick={handleToggle}>
        {#if currentVolumeIcon}
            <img class="icon" src={currentVolumeIcon} alt="" />
        {:else if volumeId === 'network'}
            <span class="icon-emoji">🌐</span>
        {/if}
        {currentVolumeName}
        {#if currentVolume?.isReadOnly}
            <span class="read-only-indicator" use:tooltip={'Read-only'}>🔒</span>
        {/if}
        <span class="chevron">▾</span>
    </span>

    {#if isOpen && (groupedVolumes.length > 0 || volumesTimedOut)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="volume-dropdown" class:keyboard-mode={isKeyboardMode} onmousemove={handleDropdownMouseMove}>
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
                        class:is-under-cursor={shouldShowCheckmark(volume)}
                        class:is-focused-and-under-cursor={allVolumes.indexOf(volume) === highlightedIndex}
                        data-index={allVolumes.indexOf(volume)}
                        onclick={() => {
                            void handleVolumeSelect(volume)
                        }}
                        onmouseover={() => {
                            handleVolumeHover(volume)
                        }}
                    >
                        {#if shouldShowCheckmark(volume)}
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
                        {:else if volume.icon}
                            <img class="volume-icon" src={volume.icon} alt="" />
                        {:else}
                            <span class="volume-icon-placeholder">📁</span>
                        {/if}
                        <span class="volume-label">{volume.name}</span>
                        {#if volume.isReadOnly}
                            <span class="read-only-indicator" use:tooltip={'Read-only'}>🔒</span>
                        {/if}
                    </div>
                    {#if volumeSpaceMap.has(volume.id)}
                        {@const space = volumeSpaceMap.get(volume.id)}
                        {#if space}
                            <div class="volume-space-info">
                                <div class="volume-space-bar">
                                    <div
                                        class="volume-space-fill"
                                        style:width="{getUsedPercent(space)}%"
                                        style:background-color="var({getDiskUsageLevel(getUsedPercent(space)).cssVar})"
                                    ></div>
                                </div>
                                <span class="volume-space-text">{formatDiskSpaceShort(space, formatFileSize)}</span>
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
                            ? 'Still unreachable — try again later'
                            : 'Some volumes may be missing'}</span
                    >
                    <button
                        class="timeout-retry-button"
                        disabled={isRetryingVolumes}
                        use:tooltip={'Refresh volume list'}
                        onclick={() => {
                            void retryLoadVolumes()
                        }}
                    >
                        <span class="timeout-retry-icon" class:is-retrying={isRetryingVolumes}>↻</span>
                    </button>
                </div>
            {/if}
        </div>
    {/if}
</div>

<span class="path-separator">▸</span>

<style>
    .volume-breadcrumb {
        position: relative;
        display: inline-block;
    }

    .volume-name {
        cursor: default;
        font-weight: 500;
        color: var(--color-text-primary);
        padding: 2px 4px;
        border-radius: var(--radius-sm);
        display: inline-flex;
        align-items: center;
        gap: 4px;
    }

    .volume-name:hover {
        background-color: var(--color-bg-tertiary);
    }

    .volume-name.is-open {
        background-color: var(--color-bg-tertiary);
    }

    .icon {
        width: 14px;
        height: 14px;
        object-fit: contain;
    }

    .icon-emoji {
        font-size: var(--font-size-md);
        line-height: 1;
    }

    .chevron {
        font-size: var(--font-size-xs);
        opacity: 0.7;
    }

    .path-separator {
        color: var(--color-text-tertiary);
        margin: 0 4px;
        font-size: var(--font-size-xs);
    }

    .volume-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: 4px;
        min-width: 220px;
        max-height: calc(100vh - 30px); /* Fallback — overridden dynamically by fitDropdownToViewport() */
        overflow-y: auto;
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        z-index: var(--z-dropdown);
        padding: 4px 0;
    }

    .category-label {
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-tertiary);
        padding: 8px 12px 4px;
        text-transform: uppercase;
        /*noinspection CssNonIntegerLengthInPixels*/
        letter-spacing: 0.5px;
    }

    .category-separator {
        height: 1px;
        background-color: var(--color-border-strong);
        margin: 4px 8px;
    }

    .volume-item {
        display: flex;
        align-items: center;
        gap: 8px;
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
        width: 16px;
        height: 16px;
        object-fit: contain;
        flex-shrink: 0;
    }

    .volume-icon-placeholder {
        font-size: var(--font-size-md);
        width: 16px;
        text-align: center;
        flex-shrink: 0;
    }

    .volume-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .checkmark {
        width: 14px;
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
        color: var(--color-warning);
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

    @keyframes spin {
        from {
            transform: rotate(0deg);
        }
        to {
            transform: rotate(360deg);
        }
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
</style>
