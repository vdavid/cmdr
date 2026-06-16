<script lang="ts">
    /**
     * EmptyState: shown in the results area before the user has searched.
     *
     * Three "Try…" chips with real, working queries appropriate for the active provider
     * (AI prompts when AI is on, filename patterns when AI is off). Clicking any chip loads
     * the query into the bar and runs it. AI chips count the click as the user's explicit
     * "yes, please run this" — same as `Enter` in the bar — so the search fires immediately.
     *
     * Below the chips, two muted lines show the current index size (locale-formatted) and a
     * short hint about the in-dialog keyboard shortcuts.
     *
     * Hidden whenever the index isn't ready: `SearchResults.svelte` owns the "Drive index
     * not ready" surface and we don't want to compete with it.
     */
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import type { SearchMode } from './query-filter-state.svelte'

    interface ExampleChip {
        label: string
        mode: SearchMode
        query: string
    }

    interface Props {
        /** True when AI mode is available (provider set and index ready). */
        aiEnabled: boolean
        /** Total entries in the loaded search index (status line). 0 hides the line (Selection). */
        indexEntryCount: number
        /**
         * Consumer-provided example chips. When omitted (or empty), the component
         * falls back to the Search-flavoured defaults below so existing call sites
         * keep working. Selection passes its own examples via
         * `config.emptyState.examples` so users see "all image files" instead of
         * Search's "PDFs from the last 7 days".
         */
        examples?: ExampleChip[]
        /** Fired when the user activates a chip. The parent loads + runs the query. */
        onPick: (chip: ExampleChip) => void
    }

    const { aiEnabled, indexEntryCount, examples: providedExamples, onPick }: Props = $props()

    /**
     * Search-flavoured fallback examples. Locked in `docs/notes/ai-search-eval-history.md`
     * so the spec, the eval catalog, and this component stay in lockstep. Consumers
     * that want a different set pass `examples` explicitly.
     */
    const AI_EXAMPLES: ExampleChip[] = [
        { label: 'large files modified this week', mode: 'ai', query: 'large files modified this week' },
        { label: 'screenshots', mode: 'ai', query: 'screenshots' },
        { label: 'PDFs from the last 7 days', mode: 'ai', query: 'PDFs from the last 7 days' },
    ]

    const FILENAME_EXAMPLES: ExampleChip[] = [
        { label: '*.pdf', mode: 'filename', query: '*.pdf' },
        { label: '*.dmg', mode: 'filename', query: '*.dmg' },
        { label: 'screenshot*', mode: 'filename', query: 'screenshot*' },
    ]

    const examples = $derived(
        providedExamples && providedExamples.length > 0
            ? providedExamples
            : aiEnabled
              ? AI_EXAMPLES
              : FILENAME_EXAMPLES,
    )
    const formattedCount = $derived(formatNumber(indexEntryCount))
</script>

<!--
  One ShortcutChip per `<tag>` in the keyboard-tip message. The chip renders the
  fixed key glyph (`key=`), not the tag''s inner content, so the rendered chip is a
  literal-mode key chip regardless of the message text; the glyph also lives in the
  message so translators see it in context. `children` is intentionally ignored.
-->
{#snippet newChip()}<ShortcutChip key="⌘N" />{/snippet}
{#snippet historyChip()}<ShortcutChip key="⌘H" />{/snippet}
{#snippet aiChip()}<ShortcutChip key="⌘Enter" />{/snippet}

<div class="empty-state">
    <p class="try-line">{tString('queryUi.empty.tryLine')}</p>
    <div class="example-row">
        {#each examples as chip (chip.label)}
            <button
                type="button"
                class="example-chip"
                onclick={() => {
                    onPick(chip)
                }}
            >
                <span class="chip-badge">{chip.mode === 'ai' ? 'AI' : 'Aa'}</span>
                <span class="chip-label">{chip.label}</span>
            </button>
        {/each}
    </div>
    {#if indexEntryCount > 0}
        <p class="index-status">
            {tString('queryUi.empty.indexReady', { countText: formattedCount, count: indexEntryCount })}
        </p>
    {/if}
    <p class="tip">
        {#if aiEnabled}
            <Trans
                key="queryUi.empty.tipAi"
                snippets={{ newKey: newChip, historyKey: historyChip, aiKey: aiChip }}
            />
        {:else}
            <Trans key="queryUi.empty.tip" snippets={{ newKey: newChip, historyKey: historyChip }} />
        {/if}
    </p>
</div>

<style>
    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-2xl) var(--spacing-lg);
        color: var(--color-text-secondary);
        text-align: center;
        min-height: 240px;
    }

    .try-line {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }

    .example-row {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
    }

    .example-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .example-chip:hover {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
    }

    .chip-badge {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }

    .chip-label {
        line-height: 1;
    }

    .index-status {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
        margin-top: var(--spacing-sm);
    }

    .tip {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        margin: 0;
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xxs);
    }
</style>
