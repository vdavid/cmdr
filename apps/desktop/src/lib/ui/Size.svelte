<script lang="ts">
    import { formatSizeForDisplay } from '$lib/file-explorer/selection/selection-info-utils'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        /** Byte count. `null`/`undefined` renders the fallback. */
        bytes: number | null | undefined
        /** Text shown when `bytes` is null/undefined. Defaults to empty string. */
        fallback?: string
        /** The live form: a tenth below ten, whole units above ("1.7 GB",
         *  "24 GB", not "1.70 GB"). For a LIVE readout, where a number changing
         *  several times a second reads better coarse; a size someone compares
         *  or copies keeps its two decimals. */
        rounded?: boolean
    }

    const { bytes, fallback = '', rounded = false }: Props = $props()
    // The inline `<Size>` component always renders the friendly dynamic form,
    // independent of the user's `listing.sizeUnit` choice. The setting is for
    // the file-list size column where apples-to-apples comparison matters;
    // tooltips, dialogs, breadcrumbs, etc. read more clearly with the
    // self-describing dynamic format.
    const parts = $derived(
        bytes == null
            ? null
            : formatSizeForDisplay(bytes, { unit: 'dynamic', format: getFileSizeFormat(), rounded }),
    )
</script>

{#if parts}{#each parts as p, i (i)}<span class={p.tierClass}>{p.value}</span>{/each}{:else}{fallback}{/if}
