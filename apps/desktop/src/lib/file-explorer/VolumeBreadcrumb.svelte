<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { listVolumes, findContainingVolume, listen, type UnlistenFn } from '$lib/tauri-commands'
    import type { VolumeInfo, LocationCategory } from './types'

    interface Props {
        volumeId: string
        currentPath: string
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
    }

    const { volumeId, currentPath, onVolumeChange }: Props = $props()

    let volumes = $state<VolumeInfo[]>([])
    let isOpen = $state(false)
    let highlightedIndex = $state(-1)
    let dropdownRef: HTMLDivElement | undefined = $state()
    let unlistenMount: UnlistenFn | undefined
    let unlistenUnmount: UnlistenFn | undefined

    // The ID of the actual volume that contains the current path
    // This is used to show the checkmark on the correct volume, not on favorites
    let containingVolumeId = $state<string | null>(null)

    // Current volume info derived from volumes list (the actual containing volume)
    // Special case: 'network' is a virtual volume, not from the backend
    const currentVolume = $derived(
        volumeId === 'network'
            ? { id: 'network', name: 'Network', path: 'smb://', category: 'network' as const, isEjectable: false }
            : volumes.find((v) => v.id === containingVolumeId),
    )
    const currentVolumeName = $derived(currentVolume?.name ?? 'Volume')
    const currentVolumeIcon = $derived(getIconForVolume(currentVolume))

    // Group volumes by category for display
    const groupedVolumes = $derived(groupByCategory(volumes))

    // Flat list of all volumes for keyboard navigation
    const allVolumes = $derived(groupedVolumes.flatMap((g) => g.items))

    // When dropdown opens, initialize highlight to current volume
    $effect(() => {
        if (isOpen) {
            const currentIdx = allVolumes.findIndex((v) => shouldShowCheckmark(v))
            highlightedIndex = currentIdx >= 0 ? currentIdx : 0
        } else {
            highlightedIndex = -1
        }
    })

    // Get appropriate icon for a volume (use cloud icon for cloud drives)
    function getIconForVolume(volume: VolumeInfo | undefined): string | undefined {
        if (!volume) return undefined
        // Cloud drives use the cloud icon
        if (volume.category === 'cloud_drive') {
            return '/icons/sync-online-only.svg'
        }
        // Network uses globe/network emoji as fallback
        if (volume.category === 'network' && !volume.icon) {
            return undefined // Will use placeholder
        }
        return volume.icon
    }

    function groupByCategory(vols: VolumeInfo[]): { category: LocationCategory; label: string; items: VolumeInfo[] }[] {
        const categoryOrder: { category: LocationCategory; label: string }[] = [
            { category: 'favorite', label: 'Favorites' },
            { category: 'main_volume', label: 'Volumes' },
            { category: 'attached_volume', label: '' }, // No label, continues main volumes
            { category: 'cloud_drive', label: 'Cloud' },
            { category: 'network', label: 'Network' },
        ]

        const groups: { category: LocationCategory; label: string; items: VolumeInfo[] }[] = []

        for (const { category, label } of categoryOrder) {
            if (category === 'network') {
                // Network section: show a single "Network" item that opens NetworkBrowser
                // Also include any pre-mounted network volumes (mounted shares)
                const networkVolumes = vols.filter((v) => v.category === 'network')

                // Create the single "Network" entry that opens NetworkBrowser
                const networkItem: VolumeInfo = {
                    id: 'network',
                    name: 'Network',
                    path: 'smb://', // Virtual path
                    category: 'network' as const,
                    icon: undefined, // Will use 🌐 placeholder
                    isEjectable: false,
                }

                // Show network entry plus any mounted shares
                const allItems = [networkItem, ...networkVolumes]
                groups.push({ category, label, items: allItems })
            } else {
                const items = vols.filter((v) => v.category === category)
                if (items.length > 0) {
                    // Merge attached_volume into the previous group (main_volume)
                    if (category === 'attached_volume' && groups.length > 0) {
                        const lastGroup = groups[groups.length - 1]
                        if (lastGroup.category === 'main_volume') {
                            lastGroup.items.push(...items)
                            continue
                        }
                    }
                    groups.push({ category, label, items })
                }
            }
        }

        return groups
    }

    async function loadVolumes() {
        volumes = await listVolumes()
    }

    async function updateContainingVolume(path: string) {
        const containing = await findContainingVolume(path)
        containingVolumeId = containing?.id ?? volumeId
    }

    async function handleVolumeSelect(volume: VolumeInfo) {
        isOpen = false

        // Check if this is a favorite (shortcut) or an actual volume
        if (volume.category === 'favorite') {
            // For favorites, find the actual containing volume
            const containingVolume = await findContainingVolume(volume.path)
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
    }

    // Export for keyboard shortcut access
    export function toggle() {
        isOpen = !isOpen
    }

    // Export to check if dropdown is open
    export function getIsOpen(): boolean {
        return isOpen
    }

    // Export keyboard handler for parent components to call
    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (!isOpen) return false

        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                highlightedIndex = Math.min(highlightedIndex + 1, allVolumes.length - 1)
                return true
            case 'ArrowUp':
                e.preventDefault()
                highlightedIndex = Math.max(highlightedIndex - 1, 0)
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
                return true
            case 'End':
                e.preventDefault()
                highlightedIndex = allVolumes.length - 1
                return true
            default:
                return false
        }
    }

    // Handle mouse hover to sync with keyboard navigation
    function handleVolumeHover(volume: VolumeInfo) {
        const idx = allVolumes.indexOf(volume)
        if (idx >= 0) {
            highlightedIndex = idx
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
            const found = volumes.find((v) => v.id === volumeId)
            if (!found && volumes.length > 0) {
                // Volume not found but we have a list - might be a newly mounted volume
                void loadVolumes()
            }
        }
    })

    onMount(async () => {
        await loadVolumes()
        await updateContainingVolume(currentPath)

        // Listen for volume mount/unmount events
        unlistenMount = await listen<{ volumeId: string }>('volume-mounted', () => {
            void loadVolumes()
        })

        unlistenUnmount = await listen<{ volumeId: string }>('volume-unmounted', () => {
            void loadVolumes()
        })

        // Close on click outside
        document.addEventListener('click', handleClickOutside)
        document.addEventListener('keydown', handleDocumentKeyDown)
    })

    onDestroy(() => {
        unlistenMount?.()
        unlistenUnmount?.()
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
        <span class="chevron">▾</span>
    </span>

    {#if isOpen && groupedVolumes.length > 0}
        <div class="volume-dropdown">
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
                        class:is-selected={shouldShowCheckmark(volume)}
                        class:is-highlighted={allVolumes.indexOf(volume) === highlightedIndex}
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
                        {:else if volume.category === 'network'}
                            <span class="volume-icon-placeholder">🌐</span>
                        {:else if volume.icon}
                            <img class="volume-icon" src={volume.icon} alt="" />
                        {:else}
                            <span class="volume-icon-placeholder">📁</span>
                        {/if}
                        <span class="volume-label">{volume.name}</span>
                    </div>
                {/each}
            {/each}
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
        border-radius: 4px;
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
        font-size: 14px;
        line-height: 1;
    }

    .chevron {
        font-size: 10px;
        opacity: 0.7;
    }

    .path-separator {
        color: var(--color-text-muted);
        margin: 0 4px;
        font-size: 10px;
    }

    .volume-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: 4px;
        min-width: 220px;
        max-height: calc(100vh - 30px);
        overflow-y: auto;
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
        z-index: 1000;
        padding: 4px 0;
    }

    .category-label {
        font-size: 11px;
        font-weight: 500;
        color: var(--color-text-muted);
        padding: 8px 12px 4px;
        text-transform: uppercase;
        /*noinspection CssNonIntegerLengthInPixels*/
        letter-spacing: 0.5px;
    }

    .category-separator {
        height: 1px;
        background-color: var(--color-border-primary);
        margin: 4px 8px;
    }

    .volume-item {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 6px 12px;
        cursor: default;
    }

    .volume-item:hover,
    .volume-item.is-highlighted {
        background-color: var(--color-selection-bg);
    }

    .volume-icon {
        width: 16px;
        height: 16px;
        object-fit: contain;
        flex-shrink: 0;
    }

    .volume-icon-placeholder {
        font-size: 14px;
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
        font-size: 12px;
        flex-shrink: 0;
    }

    .checkmark-placeholder {
        width: 14px;
        flex-shrink: 0;
    }
</style>
