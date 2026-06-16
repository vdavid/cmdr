<script lang="ts">
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'

    interface Props {
        openingFolder?: boolean
        loadedCount?: number
        finalizingCount?: number
        showCancelHint?: boolean
    }

    const { openingFolder = false, loadedCount, finalizingCount, showCancelHint = false }: Props = $props()

    const formatNumber = formatInteger
</script>

{#snippet escKeyChip(children: Snippet)}<ShortcutChip key="Esc" />{@render children()}{/snippet}

<div class="loading-container">
    <Spinner size="lg" />
    {#if finalizingCount !== undefined}
        <div class="loading-text">
            {tString('ui.loadingIcon.finalizing', { countText: formatNumber(finalizingCount), count: finalizingCount })}
        </div>
    {:else if loadedCount !== undefined}
        <div class="loading-text">
            {tString('ui.loadingIcon.loaded', { countText: formatNumber(loadedCount), count: loadedCount })}
        </div>
    {:else if openingFolder}
        <div class="loading-text">{tString('ui.loadingIcon.openingFolder')}</div>
    {:else}
        <div class="loading-text">{tString('ui.loadingIcon.loading')}</div>
    {/if}
    {#if showCancelHint}
        <div class="cancel-hint">
            <Trans key="ui.loadingIcon.cancelHint" snippets={{ key: escKeyChip }} />
        </div>
    {/if}
</div>

<style>
    .loading-container {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-lg);
        width: 100%;
        height: 100%;
        animation: fadeIn 400ms ease-in;
    }

    @keyframes fadeIn {
        0%,
        50% {
            opacity: 0;
        }
        100% {
            opacity: 1;
        }
    }

    .loading-text {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    .cancel-hint {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
