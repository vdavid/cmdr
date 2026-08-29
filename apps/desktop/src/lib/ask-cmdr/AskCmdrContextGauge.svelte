<!--
  How full the model's view of this chat is: a fill bar, a percentage, and a tooltip with the
  real figures. Sits in the rail footer beside the cost line, which answers a different
  question — cost is what the whole thread has SPENT, this is how much of one prompt's room is
  in use, so the two numbers differ and each says which it is.

  Both numbers are `chars/4` estimates, not a tokenizer's count, and the tooltip says so.
  Hidden until a turn has actually been measured: an empty bar would read as "plenty of room"
  for a thread nobody measured yet.
-->
<script lang="ts">
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { contextUsagePercent, contextUsageState, type ContextUsage } from './ask-cmdr-context-usage'

    type Props = { usage: ContextUsage | null }

    const { usage }: Props = $props()

    const state = $derived(contextUsageState(usage))
    const percent = $derived(contextUsagePercent(usage))
    const tooltipText = $derived(
        usage
            ? tString('askCmdr.context.tooltip', {
                  usedText: formatInteger(usage.estimatedTokens),
                  budgetText: formatInteger(usage.budgetTokens),
              })
            : '',
    )
</script>

{#if state !== 'unmeasured'}
    <div class="gauge" data-state={state} use:tooltip={tooltipText}>
        <!-- The meter carries BOTH the name and the value: an ARIA meter without an accessible
             name is announced as a bare number, so the label lives here, not on the wrapper. -->
        <div
            class="track"
            role="meter"
            aria-label={tString('askCmdr.context.label')}
            aria-valuenow={percent}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuetext={tooltipText}
        >
            <div class="fill" style="width: {percent}%"></div>
        </div>
        <span class="percent">{percent}%</span>
    </div>
{/if}

<style>
    .gauge {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    .track {
        width: 3rem;
        height: 0.25rem;
        overflow: hidden;
        background: var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .fill {
        height: 100%;
        background: var(--color-text-tertiary);
        transition: width var(--transition-fast);
    }

    /* Filling and set-aside earn attention; calm deliberately does not, so the gauge is
       ignorable on the ordinary chats that make up most of them. */
    .gauge[data-state='filling'] .fill {
        background: var(--color-warning);
    }

    .gauge[data-state='setAside'] .fill {
        background: var(--color-error);
    }

    .percent {
        font-variant-numeric: tabular-nums;
    }
</style>
