<!--
  The per-thread footer, carrying two numbers that answer two different questions:

  - **Cost**: what the whole thread has SPENT (cumulative tokens + estimated money), read from
    the backend meter (plan §5). Honest miss-path: a local-only chat reads "free, on-device",
    an unpriced model reads "cost unknown", and only a fully-priced chat shows an estimated
    amount — never a silent $0.
  - **The gauge**: how much of ONE prompt's room the last turn used. It goes down when history
    is set aside; the cost only ever goes up.

  So the two figures differ on purpose and each says which it is (the gauge's tooltip spells
  out "N of M tokens used"). Each half hides independently until it has something true to say.
-->
<script lang="ts">
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getAppLogger } from '$lib/logging/logger'
    import { askCmdrConversationCost, type ConversationCost } from '$lib/tauri-commands'
    import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
    import { contextUsageState } from './ask-cmdr-context-usage'
    import { formatUsdMicros, isLocalOnly, totalTokens } from './ask-cmdr-cost'
    import { askCmdrState } from './ask-cmdr-trigger.svelte'

    const log = getAppLogger('askCmdr')

    let cost = $state<ConversationCost | null>(null)

    // Refetch when the active thread changes or a turn finishes streaming (the meter is
    // updated per completed turn), so the footer tracks the newest total. A brand-new,
    // unsaved thread (id null) has no metered turn yet.
    $effect(() => {
        const id = askCmdrState.conversationId
        const streaming = askCmdrState.streaming
        if (id === null) {
            cost = null
            return
        }
        // Read after streaming ends (the `done` event flipped `streaming` false and the
        // meter row is written by then).
        if (streaming) return
        void askCmdrConversationCost(id).then(
            (c) => {
                cost = c
            },
            (e: unknown) => {
                log.warn('reading chat cost failed: {error}', { error: String(e) })
            },
        )
    })

    const tokens = $derived(cost ? totalTokens(cost) : 0)
    const hasUsage = $derived(cost !== null && tokens > 0)
    // The gauge appears the moment a turn has been measured, independently of the cost meter:
    // a local-only thread has a real context reading and no money to report.
    const hasGauge = $derived(contextUsageState(askCmdrState.contextUsage) !== 'unmeasured')

    // The cost half of the footer, honest about the miss-path.
    const costText = $derived.by(() => {
        if (!cost) return ''
        if (isLocalOnly(cost.providers)) return tString('askCmdr.cost.free')
        if (!cost.fullyPriced) return tString('askCmdr.cost.unknown')
        return tString('askCmdr.cost.estimate', { amount: formatUsdMicros(cost.costMicros) })
    })
</script>

{#if hasUsage || hasGauge}
    <div class="cost-footer">
        {#if hasUsage}
            <span class="cost" aria-label={tString('askCmdr.cost.label')}>
                <span>{tString('askCmdr.cost.tokens', { count: tokens, countText: formatInteger(tokens) })}</span>
                <span class="dot" aria-hidden="true">·</span>
                <span>{costText}</span>
            </span>
        {/if}
        {#if hasGauge}
            <AskCmdrContextGauge usage={askCmdrState.contextUsage} />
        {/if}
    </div>
{/if}

<style>
    .cost-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-xxs);
        padding: var(--spacing-xxs) var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        border-top: 1px solid var(--color-border-subtle);
    }

    .cost {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    .dot {
        opacity: 0.6;
    }
</style>
