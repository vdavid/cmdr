<!--
  The per-thread footer, carrying two numbers that answer two different questions:

  - **Cost**: what the whole thread has SPENT (cumulative tokens + estimated money), read from
    the backend meter (plan §5). Honest miss-path: a local-only chat reads "free, on-device",
    an unpriced model reads "cost unknown", and only a fully-priced chat shows an estimated
    amount — never a silent $0. A chat with nothing metered yet reads zero, priced the way its
    first call would be (`costReading`).
  - **The gauge**: how much of ONE prompt's room the last call used. It goes down when history
    is set aside; the cost only ever goes up. Before any call it shows the baseline: the prefix
    every call carries.

  So the two figures differ on purpose and each says which it is (the gauge's tooltip spells
  out "N of M tokens used"). Shown whenever the chat is; both move after every model call.
-->
<script lang="ts">
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { askCmdrConversationCost, type ConversationCost } from '#lib/tauri-commands/index.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
    import { gaugeReading } from './ask-cmdr-context-usage'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { costEstimateText, costReading, formatTokenCount, tokensTooltip } from './ask-cmdr-cost'
    import { slotStatusState } from './ask-cmdr-slot-status.svelte'
    import { askCmdrState } from './ask-cmdr-trigger.svelte'

    const log = getAppLogger('askCmdr')

    // Keyed by the thread it was read for, so a thread switch hides the old total at once
    // instead of showing it under the new thread until the new read lands.
    let fetched = $state<{ conversationId: number; cost: ConversationCost } | null>(null)
    const threadCost = $derived(
        fetched !== null && fetched.conversationId === askCmdrState.conversationId ? fetched.cost : null,
    )

    // Each read is numbered, so only the newest one lands: an older read of the same thread
    // answering last would show a lower total, and one for a thread already left the wrong one.
    let readSeq = 0

    // Refetch when the active thread changes, after each metered call (`costRevision`, so the
    // total moves mid-turn), and when a turn ends (the backstop for a turn whose events this
    // window missed). A brand-new, unsaved thread (id null) has nothing metered.
    $effect(() => {
        const id = askCmdrState.conversationId
        dependOn(askCmdrState.costRevision)
        dependOn(askCmdrState.streaming)
        const seq = ++readSeq
        if (id === null) return
        void askCmdrConversationCost(id).then(
            (c) => {
                if (seq === readSeq) fetched = { conversationId: id, cost: c }
            },
            (e: unknown) => {
                log.warn('reading chat cost failed: {error}', { error: e })
            },
        )
    })

    const reading = $derived(costReading(threadCost, slotStatusState.status?.nextCall ?? null))
    const gauge = $derived(gaugeReading(askCmdrState.contextUsage, slotStatusState.status?.baseline ?? null))

    // The cost half of the footer, honest about the miss-path.
    const priceText = $derived.by(() => {
        const price = reading.price
        if (!price) return ''
        if (price.kind === 'free') return tString('askCmdr.cost.free')
        if (price.kind === 'unknown') return tString('askCmdr.cost.unknown')
        return costEstimateText(price.micros)
    })
    // Only an estimate carries the "USD, before caching" caveat; free and unknown say it all.
    const priceTooltip = $derived(reading.price?.kind === 'estimate' ? tString('askCmdr.cost.estimateTooltip') : undefined)

    // Compact from 2,000 up ("194.8K tokens"); the exact count and the cached share wait in the tooltip.
    const tokensText = $derived(
        tString('askCmdr.cost.tokens', { count: reading.tokens, countText: formatTokenCount(reading.tokens) }),
    )
    const exactTokens = $derived(tokensTooltip(reading.tokens, reading.cacheReadTokens))
</script>

<div class="cost-footer">
    <span class="cost" aria-label={tString('askCmdr.cost.label')}>
        <span use:tooltip={exactTokens}>{tokensText}</span>
        {#if priceText}
            <span class="dot" aria-hidden="true">·</span>
            <span use:tooltip={priceTooltip}>{priceText}</span>
        {/if}
    </span>
    {#if gauge}
        <AskCmdrContextGauge usage={gauge.usage} baseline={gauge.baseline} />
    {/if}
</div>

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
