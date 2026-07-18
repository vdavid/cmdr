<script lang="ts">
    import type { FileEntry } from '../types'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { getIsCmdrGold } from '$lib/settings/reactive-settings.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import type { IconName } from '$lib/ui/icons/icon-map'

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

    // Which icon ids get the Cmdr-gold recolor filter. All are folders macOS
    // renders as a plain (accent-tinted) folder shape, so the grayscale-first
    // filter re-tints them to gold cleanly regardless of the system accent:
    //   - `dir` / `symlink-dir`: the generic folder (~99% of rows).
    //   - `special:*`: the standard folders macOS badges with a white glyph
    //     (Downloads, Desktop, Documents, Movies, Music, Pictures, Public,
    //     Trash, home). Without this they keep the raw OS bitmap, whose folder
    //     tint is the system accent, so they'd leak through non-gold.
    // Deliberately EXCLUDED: `pkg:*` (full-color `.app`/bundle icons the filter
    // would destroy) and `path:*` (folders with a user-assigned custom icon we
    // must not override).
    const isFolderIcon = $derived(
        file.iconId === 'dir' || file.iconId === 'symlink-dir' || file.iconId.startsWith('special:'),
    )
    const recolorToGold = $derived(isFolderIcon && getIsCmdrGold())

    // Git portal icons resolve in the frontend via Lucide instead of going
    // through the OS icon provider. The four IDs are reserved by the
    // `FileEntry` schema (`git:`-prefixed) and rendered as Lucide
    // components here.
    const isGitIcon = $derived(file.iconId.startsWith('git:'))

    // Maps the reserved `git:`-prefixed icon ids to the shared registry glyph names.
    const gitIconName = $derived.by((): IconName => {
        switch (file.iconId) {
            case 'git:branch':
                return 'git-branch'
            case 'git:tag':
                return 'tag'
            case 'git:commit':
                return 'git-commit-horizontal'
            default:
                return 'git-fork'
        }
    })
</script>

<span class="icon-wrapper">
    {#if isGitIcon}
        <span class="git-icon">
            <Icon name={gitIconName} size={16} />
        </span>
    {:else if getIconUrl(file)}
        <img class="icon" class:gold-folder={recolorToGold} src={getIconUrl(file)} alt="" width="16" height="16" />
    {:else}
        <!-- Cache miss (cold first launch, or briefly after a theme/accent change clears the cache):
             show the bundled macOS default folder/file icon (from `static/icons/default-*.png`,
             extracted from the system GenericFolderIcon/GenericDocumentIcon) so the placeholder is
             always the real OS-shaped icon, never an emoji. It swaps seamlessly to the live
             (accent-tinted) OS icon once `get_icons` populates the cache. -->
        <img
            class="icon"
            class:gold-folder={recolorToGold}
            src={isFolderIcon ? '/icons/default-folder.png' : '/icons/default-file.png'}
            alt=""
            width="16"
            height="16"
        />
    {/if}
    {#if file.isSymlink}
        <span class="symlink-badge" class:has-sync={!!syncIcon}><Icon name="link" size={10} /></span>
    {/if}
    {#if syncIcon}
        <img class="sync-badge" src={syncIcon} alt="" width="10" height="10" />
    {/if}
</span>

<style>
    .icon-wrapper {
        position: relative;
        width: var(--spacing-icon-size);
        height: var(--spacing-icon-size);
        flex-shrink: 0;
    }

    .icon {
        width: var(--spacing-icon-size);
        height: var(--spacing-icon-size);
        object-fit: contain;
    }

    .gold-folder {
        filter: grayscale(1) sepia(1) hue-rotate(3deg) saturate(2.5) brightness(0.95);
    }

    .git-icon {
        display: inline-flex;
        width: var(--spacing-icon-size);
        height: var(--spacing-icon-size);
        align-items: center;
        justify-content: center;
        color: var(--color-git-portal-text);
    }

    .symlink-badge {
        position: absolute;
        bottom: -2px;
        right: -2px;
        display: inline-flex;
        /* Higher-contrast accent (darker than the accent in light mode, lighter in dark mode) so the
           badge stays legible over gold/accent-tinted icons in both schemes. */
        color: var(--color-accent-pop);
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
