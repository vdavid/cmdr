<script lang="ts">
    /**
     * AiTransparencyStrip: shows what the user actually asked the AI, plus the AI's caveat if any,
     * and a placeholder "Refine…" button for the future chat-back feature.
     *
     * Sits between the search bar (and mode chips) and the filter chips. Visible only after an AI
     * search has run this session; the parent (`SearchDialog.svelte`) clears the state on ⌘N or when
     * a non-AI search runs, which hides the strip.
     *
     * Why this exists: when the AI translates a natural-language prompt, the result populates
     * `query` and `mode` so the user can see (and iterate on) the translated pattern. The original
     * prompt would otherwise vanish into the user's memory. The strip surfaces it again, alongside
     * any caveat the AI returned ("I ignored the file size you mentioned because…"). This is the
     * "radical transparency" principle from the redesign plan (§2.6).
     *
     * The "Refine…" button is intentionally **visible-disabled** with a tooltip. It signals that
     * a chat-back feature is coming without overpromising. Consistent with the Content mode chip's
     * disabled-with-tooltip treatment; neither has a keyboard shortcut wired.
     */
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        /** The natural-language prompt the user typed, before AI translated it. */
        aiPrompt: string
        /** Optional caveat returned by the AI translator. Empty string hides the caveat row. */
        caveat: string
    }

    const { aiPrompt, caveat }: Props = $props()
</script>

<div class="ai-transparency-strip" aria-label="Last AI search prompt">
    <div class="strip-text">
        <p class="ai-prompt">{aiPrompt}</p>
        {#if caveat}
            <p class="ai-caveat">{caveat}</p>
        {/if}
    </div>
    <button
        type="button"
        class="refine-button"
        disabled
        aria-label="Refine the AI search (coming soon)"
        use:tooltip={'Coming soon: chat back to the agent'}
    >
        Refine…
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
        font-size: var(--font-size-sm);
        line-height: 1.3;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .ai-caveat {
        margin: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        font-style: italic;
        line-height: 1.3;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .refine-button {
        flex-shrink: 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
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
