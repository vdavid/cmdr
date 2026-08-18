<script lang="ts">
    // The status-corner indicator: how many suggestions are waiting, and a way into the review.
    //
    // A plain inline box, because the corner owns placement (`status-corner/CLAUDE.md`). It
    // hides at zero rather than showing an empty badge: an always-present control for a feature
    // that has nothing to say is noise in the one corner reserved for work in progress.
    import Icon from '$lib/ui/Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { suggestedOpsBadge } from './suggested-ops-badge.svelte'
    import { openSuggestedOps } from './suggested-ops-trigger.svelte'

    const count = $derived(suggestedOpsBadge.pendingGroupCount)
    const visible = $derived(count > 0)
    const label = $derived(
        tString('suggestedOps.indicatorTooltip', {
            groups: count,
            countText: formatInteger(suggestedOpsBadge.pendingOpCount),
            count: suggestedOpsBadge.pendingOpCount,
        }),
    )
</script>

{#if visible}
    <button class="indicator" onclick={() => void openSuggestedOps()} use:tooltip={label} aria-label={label}>
        <Icon name="sparkles" size={14} />
        <span class="count">{formatInteger(count)}</span>
    </button>
{/if}

<style>
    .indicator {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        padding: 0 var(--spacing-xs);
        height: 20px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
    }

    .indicator:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .count {
        font-size: var(--font-size-xs);
        font-variant-numeric: tabular-nums;
    }
</style>
