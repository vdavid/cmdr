<script lang="ts">
    import type { FileEntry } from '../types'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { getFallbackEmoji } from '../views/file-list-utils'
    import { getIsCmdrGold } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        file: FileEntry
        syncIcon?: string
    }

    const { file, syncIcon }: Props = $props()

    // Subscribe to cache version - this makes getIconUrl reactive
    const _cacheVersion = $derived($iconCacheVersion)

    function getIconUrl(f: FileEntry): string | undefined {
        void _cacheVersion // Track cache version for reactivity
        return getCachedIcon(f.iconId)
    }

    const isFolderIcon = $derived(file.iconId === 'dir' || file.iconId === 'symlink-dir')
    const recolorToGold = $derived(isFolderIcon && getIsCmdrGold())
</script>

<span class="icon-wrapper">
    {#if getIconUrl(file)}
        <img class="icon" class:gold-folder={recolorToGold} src={getIconUrl(file)} alt="" width="16" height="16" />
    {:else}
        <span class="icon-emoji">{getFallbackEmoji(file)}</span>
    {/if}
    {#if file.isSymlink}
        <span class="symlink-badge" class:has-sync={!!syncIcon}>🔗</span>
    {/if}
    {#if syncIcon}
        <img class="sync-badge" src={syncIcon} alt="" width="10" height="10" />
    {/if}
</span>

<style>
    .icon-wrapper {
        position: relative;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
    }

    .icon {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    .gold-folder {
        filter: grayscale(1) sepia(1) hue-rotate(3deg) saturate(2.5) brightness(0.95);
    }

    .icon-emoji {
        font-size: var(--font-size-sm);
        width: 16px;
        text-align: center;
        display: block;
    }

    .symlink-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        font-size: 8px;
        line-height: 1;
    }

    .symlink-badge.has-sync {
        bottom: auto;
        right: auto;
        top: -2px;
        left: -2px;
    }

    .sync-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        width: 10px;
        height: 10px;
    }
</style>
