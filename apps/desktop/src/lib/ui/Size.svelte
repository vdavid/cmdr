<script lang="ts">
    import { formatSizeForDisplay } from '$lib/file-explorer/selection/selection-info-utils'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        /** Byte count. `null`/`undefined` renders the fallback. */
        bytes: number | null | undefined
        /** Text shown when `bytes` is null/undefined. Defaults to empty string. */
        fallback?: string
    }

    const { bytes, fallback = '' }: Props = $props()
    const parts = $derived(
        bytes == null ? null : formatSizeForDisplay(bytes, { humanFriendly: true, format: getFileSizeFormat() }),
    )
</script>

{#if parts}{#each parts as p, i (i)}<span class={p.tierClass}>{p.value}</span>{/each}{:else}{fallback}{/if}
