<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { listVolumes, listen, type UnlistenFn } from '$lib/tauri-commands'
    import type { VolumeInfo, LocationCategory } from './types'

    interface Props {
        volumeId: string
        onVolumeChange?: (volumeId: string, volumePath: string) => void
    }

    const { volumeId, onVolumeChange }: Props = $props()

    let volumes = $state<VolumeInfo[]>([])
    let isOpen = $state(false)
    let dropdownRef: HTMLDivElement | undefined = $state()
    let unlistenMount: UnlistenFn | undefined
    let unlistenUnmount: UnlistenFn | undefined

    // Current volume info derived from volumes list
    const currentVolume = $derived(volumes.find((v) => v.id === volumeId))
    const currentVolumeName = $derived(currentVolume?.name ?? 'Volume')
    const currentVolumeIcon = $derived(getIconForVolume(currentVolume))

    // Group volumes by category for display
    const groupedVolumes = $derived(groupByCategory(volumes))

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

        return groups
    }

    async function loadVolumes() {
        volumes = await listVolumes()
    }

    function handleVolumeSelect(volume: VolumeInfo) {
        isOpen = false
        if (volume.id !== volumeId) {
            onVolumeChange?.(volume.id, volume.path)
        }
    }

    function handleToggle() {
        isOpen = !isOpen
    }

    // Export for keyboard shortcut access
    export function toggle() {
        isOpen = !isOpen
    }

    function handleClickOutside(event: MouseEvent) {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
            isOpen = false
        }
    }

    function handleKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            isOpen = false
        }
    }

    onMount(async () => {
        await loadVolumes()

        // Listen for volume mount/unmount events
        unlistenMount = await listen<{ volumeId: string }>('volume-mounted', () => {
            void loadVolumes()
        })

        unlistenUnmount = await listen<{ volumeId: string }>('volume-unmounted', () => {
            void loadVolumes()
        })

        // Close on click outside
        document.addEventListener('click', handleClickOutside)
        document.addEventListener('keydown', handleKeyDown)
    })

    onDestroy(() => {
        unlistenMount?.()
        unlistenUnmount?.()
        document.removeEventListener('click', handleClickOutside)
        document.removeEventListener('keydown', handleKeyDown)
    })
</script>

<div class="volume-breadcrumb" bind:this={dropdownRef}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="volume-name" class:is-open={isOpen} onclick={handleToggle}>
        {#if currentVolumeIcon}
            <img class="icon" src={currentVolumeIcon} alt="" />
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
                    <div
                        class="volume-item"
                        class:is-selected={volume.id === volumeId}
                        onclick={() => {
                            handleVolumeSelect(volume)
                        }}
                    >
                        {#if volume.id === volumeId}
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
        transition: background-color 0.15s ease;
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
        transition: background-color 0.1s ease;
    }

    .volume-item:hover {
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
