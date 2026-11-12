<script lang="ts">
    // A volume as its own entity in a fact line: the volume picker's icon and name in a soft
    // pill, so "Testnas" reads as a drive and not as the first folder of the path beside it.
    //
    // The icon comes from the same `getIconForVolume` the picker uses, looked up by id in the
    // live volume list. A volume that isn't listed (a saved share nobody connected) keeps the
    // name the backend gave and the generic folder icon.
    import Icon from '#lib/ui/Icon.svelte'
    import { getVolumes } from '#lib/stores/volume-store.svelte.js'
    import { getIconForVolume } from '#lib/file-explorer/navigation/volume-grouping.js'
    import { getCachedIcon, iconCacheVersion } from '#lib/icon-cache.js'
    import { dependOn } from '#lib/utils/reactivity.js'

    interface Props {
        /** The volume's id, or `null` when only its name is known. */
        volumeId: string | null
        /** The name to show when the volume isn't in the live list. */
        name: string | null
    }

    const { volumeId, name }: Props = $props()

    const volume = $derived(volumeId === null ? undefined : getVolumes().find((v) => v.id === volumeId))
    const label = $derived(volume?.name ?? name)
    const icon = $derived.by(() => {
        dependOn($iconCacheVersion)
        return getIconForVolume(volume) ?? getCachedIcon('dir')
    })
</script>

{#if label}
    <span class="volume-pill">
        {#if icon}
            <img class="volume-icon" src={icon} alt="" />
        {:else}
            <span class="volume-glyph"><Icon name="server" size={14} aria-hidden="true" /></span>
        {/if}
        <span class="volume-name">{label}</span>
    </span>
{/if}

<style>
    .volume-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin-right: var(--spacing-xs);
        padding: 0 var(--spacing-sm) 0 var(--spacing-xs);
        border-radius: var(--radius-full);
        background: var(--color-bg-tertiary);
        vertical-align: middle;
        white-space: nowrap;
    }

    .volume-icon {
        width: 14px;
        height: 14px;
        object-fit: contain;
    }

    .volume-glyph {
        display: inline-flex;
        color: var(--color-text-secondary);
    }

    .volume-name {
        font-weight: 500;
    }
</style>
