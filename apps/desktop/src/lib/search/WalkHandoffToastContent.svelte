<script lang="ts">
    /**
     * The toast for a search that kept walking after "Open in pane": what it has
     * found so far, the way back into the dialog, and the way to stop it.
     *
     * ❌ Prop-free on purpose. A toast replaced in place keeps the props it was
     * created with (`ui/toast/toast-store.svelte.ts`), so counters passed in would
     * freeze at the values they had when the pane opened. Reading the module means
     * the numbers move.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { getWalkHandoff, reopenHandedOffSearch, stopHandedOffWalk } from './walk-handoff-state.svelte'

    const handoff = $derived(getWalkHandoff())

    /**
     * The counts, in the order someone reads them: what they got, then how hard
     * Cmdr is still looking. No percentage and no ETA — the total is unknowable
     * (Decision 14), and the dialog's own strip says the same thing.
     */
    const counts = $derived(
        handoff
            ? tString('search.walkHandoff.counts', {
                  matchCount: handoff.view.matchCount,
                  matchText: formatInteger(handoff.view.matchCount),
                  folderCount: handoff.view.dirsFound,
                  folderText: formatInteger(handoff.view.dirsFound),
              })
            : '',
    )
</script>

{#if handoff}
    <div class="content">
        <span class="headline">
            <Spinner size="sm" />
            <span>{tString('search.walkHandoff.running', { label: handoff.label })}</span>
        </span>
        <span class="counts">{counts}</span>
        <div class="actions">
            <Button variant="primary" size="mini" onclick={reopenHandedOffSearch}>
                {tString('search.walkHandoff.reopen')}
            </Button>
            <Button variant="secondary" size="mini" onclick={stopHandedOffWalk}>
                {tString('search.walkHandoff.stop')}
            </Button>
        </div>
    </div>
{/if}

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .headline {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-primary);
    }

    .counts {
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
    }

    .actions {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
    }
</style>
