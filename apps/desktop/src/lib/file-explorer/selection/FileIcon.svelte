<script lang="ts">
    import type { FileEntry } from '../types'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { getFallbackEmoji } from '../views/file-list-utils'
    import { getIsCmdrGold } from '$lib/settings/reactive-settings.svelte'
    import IconGitBranch from '~icons/lucide/git-branch'
    import IconTag from '~icons/lucide/tag'
    import IconGitCommit from '~icons/lucide/git-commit-horizontal'
    import IconGitFork from '~icons/lucide/git-fork'

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

    // Git portal icons resolve in the frontend via Lucide instead of going
    // through the OS icon provider. The four IDs are reserved by the M1
    // schema and rendered here in M2.
    const isGitIcon = $derived(file.iconId.startsWith('git:'))
</script>

<span class="icon-wrapper">
    {#if isGitIcon}
        <span class="git-icon">
            {#if file.iconId === 'git:branch'}
                <IconGitBranch width="16" height="16" />
            {:else if file.iconId === 'git:tag'}
                <IconTag width="16" height="16" />
            {:else if file.iconId === 'git:commit'}
                <IconGitCommit width="16" height="16" />
            {:else}
                <IconGitFork width="16" height="16" />
            {/if}
        </span>
    {:else if getIconUrl(file)}
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

    .git-icon {
        display: inline-flex;
        width: 16px;
        height: 16px;
        align-items: center;
        justify-content: center;
        color: var(--color-git-portal-text);
    }

    .symlink-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- below type scale, tiny badge */
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
