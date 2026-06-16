<script lang="ts">
    /**
     * AiTransparencyStrip: shows what the agent did with the user's natural-language prompt.
     *
     * Sits between the mode chips and the filter chips. Visible only after an AI search has run
     * this session; the parent clears the AI state on ⌘N or when a non-AI search runs, which hides
     * the strip.
     *
     * Three rows, top to bottom:
     *   1. The echoed prompt (what the user asked).
     *   2. "Here's what the agent did:" plus a plain-language summary: the produced pattern
     *      (labelled Glob / Regex) and the filters the agent set (Size, Modified, Type). This
     *      summary is a human-readable MIRROR of the structured filter state the AI produced; the
     *      live filter chips below are the editable source of truth, never this text (see
     *      `query-ui/CLAUDE.md`). The structured `summary` is built by the pure `buildAiSummary`.
     *   3. The AI's caveat, if any ("I ignored the file size you mentioned because…").
     *
     * Voice (David-decided): the strip MAY speak as the in-app agent in first person ("Here's what
     * the agent did:"). This is a SANCTIONED exception to the no-first-person app-copy rule
     * (alongside onboarding / About): the product's mental model is an agent acting on the user's
     * behalf, and the language can reflect that even though the agentic loop isn't built yet. Keep
     * it warm and honest, not overclaiming.
     *
     * The "Refine…" button is intentionally visible-disabled with a tooltip: it signals a coming
     * chat-back feature without overpromising. No keyboard shortcut is wired.
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { patternRowLabel, type AiSummary } from './ai-summary'

    interface Props {
        /** The natural-language prompt the user typed, before AI translated it. */
        aiPrompt: string
        /** Optional caveat returned by the AI translator. Empty string hides the caveat row. */
        caveat: string
        /** Structured mirror of what the agent set (pattern + filters). Built by `buildAiSummary`. */
        summary: AiSummary
    }

    const { aiPrompt, caveat, summary }: Props = $props()

    const hasSummary = $derived(summary.pattern !== null || summary.filters.length > 0)
</script>

<div class="ai-transparency-strip" aria-label={tString('queryUi.ai.stripAria')}>
    <div class="strip-text">
        <p class="ai-prompt">{aiPrompt}</p>
        <div class="ai-summary">
            <span class="ai-summary-lead">{tString('queryUi.ai.lead')}</span>
            {#if hasSummary}
                <ul class="ai-summary-list">
                    {#if summary.pattern !== null}
                        <li>
                            <span class="ai-summary-label">{patternRowLabel(summary.patternKind)}:</span>
                            <span class="ai-summary-value ai-summary-pattern">{summary.pattern}</span>
                        </li>
                    {/if}
                    {#each summary.filters as filter (filter.label)}
                        <li>
                            <span class="ai-summary-label">{filter.label}:</span>
                            <span class="ai-summary-value">{filter.value}</span>
                        </li>
                    {/each}
                </ul>
            {:else}
                <span class="ai-summary-empty">{tString('queryUi.ai.empty')}</span>
            {/if}
        </div>
        {#if caveat}
            <p class="ai-caveat">{caveat}</p>
        {/if}
    </div>
    <button
        type="button"
        class="refine-button"
        disabled
        aria-label={tString('queryUi.ai.refineAria')}
        use:tooltip={tString('queryUi.ai.refineTooltip')}
    >
        {tString('queryUi.ai.refine')}
    </button>
</div>

<style>
    .ai-transparency-strip {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-primary);
    }

    .strip-text {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .ai-prompt {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
        line-height: 1.3;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .ai-summary {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        font-size: var(--font-size-md);
        line-height: 1.3;
    }

    .ai-summary-lead {
        color: var(--color-text-tertiary);
    }

    .ai-summary-list {
        margin: 0;
        padding: 0;
        list-style: none;
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xxs) var(--spacing-md);
    }

    .ai-summary-list li {
        display: inline-flex;
        align-items: baseline;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .ai-summary-label {
        color: var(--color-text-tertiary);
    }

    .ai-summary-value {
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        max-width: 40ch;
    }

    .ai-summary-pattern {
        font-family: var(--font-mono);
        color: var(--color-text-primary);
    }

    .ai-summary-empty {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .ai-caveat {
        margin: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        font-style: italic;
        line-height: 1.3;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .refine-button {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-md);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .refine-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }
</style>
