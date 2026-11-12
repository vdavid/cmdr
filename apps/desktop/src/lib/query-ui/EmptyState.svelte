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
     *
     * `aiBlocked` (the AI mode is Cloud and "Allow cloud AI" is off): the AI prompts would only
     * be refused, so they give way to one line saying why and a button to the switch. The
     * non-AI examples stay, and the AI mode chip stays visible so the feature stays findable.
     */
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { openCloudConsentSettings } from '#lib/ai/cloud-consent.svelte.js'
    import type { SearchMode } from './query-filter-state.svelte'
    import { getFirstShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { toDisplayShortcut } from '#lib/shortcuts/key-capture.js'

    interface ExampleChip {
        label: string
        mode: SearchMode
        query: string
    }

    interface Props {
        /** True when AI mode is available (provider set and index ready). */
        aiEnabled: boolean
        /** True when the AI mode is Cloud and the user hasn't allowed cloud AI yet. */
        aiBlocked?: boolean
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

    const { aiEnabled, aiBlocked = false, indexEntryCount, examples: providedExamples, onPick }: Props = $props()

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

    const baseExamples = $derived(
        providedExamples && providedExamples.length > 0
            ? providedExamples
            : aiEnabled && !aiBlocked
              ? AI_EXAMPLES
              : FILENAME_EXAMPLES,
    )
    const examples = $derived(aiBlocked ? baseExamples.filter((chip) => chip.mode !== 'ai') : baseExamples)
    const formattedCount = $derived(formatNumber(indexEntryCount))
    // Typographic glyph marking a keyword (non-AI) example: the same in every language.
    const KEYWORD_BADGE = 'Aa'
    // The tip's keys, live: both commands are fixed, so they're always bound.
    const tipKeys = $derived({
        newCombo: toDisplayShortcut(getFirstShortcutReactive('search.newQuery') ?? ''),
        historyCombo: toDisplayShortcut(getFirstShortcutReactive('search.toggleRecent') ?? ''),
    })
</script>

<!--
  One ShortcutChip per `<tag>` in the keyboard-tip message, drawing its command's live key in this
  platform's spelling (`search.newQuery`, `search.toggleRecent`: ⌘N / ⌘Y on macOS, Ctrl+N / Ctrl+H
  elsewhere). The message gets the same keys as placeholders, so a translation places them; the chip
  draws the key itself, so `children` is ignored.
-->
{#snippet newChip()}<ShortcutChip commandId="search.newQuery" clickable={false} />{/snippet}
{#snippet historyChip()}<ShortcutChip commandId="search.toggleRecent" clickable={false} />{/snippet}

<div class="empty-state">
    {#if aiBlocked}
        <div class="cloud-off" role="note">
            <p>{tString('queryUi.ai.cloudOff.body')}</p>
            <Button size="mini" variant="secondary" onclick={() => { openCloudConsentSettings('query-ai-cloud-off'); }}>
                {tString('queryUi.ai.openSettings')}
            </Button>
        </div>
    {/if}
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
                <span class="chip-badge"
                    >{chip.mode === 'ai' ? tString('queryUi.empty.aiBadge') : KEYWORD_BADGE /* eslint-disable-line cmdr/no-raw-user-facing-string -- a glyph, not copy */}</span
                >
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
        <Trans key="queryUi.empty.keysTip" snippets={{ newChip, historyChip }} params={tipKeys} />
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

    .cloud-off {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .cloud-off p {
        margin: 0;
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
        line-height: var(--font-line-height-flat);
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
        line-height: var(--font-line-height-flat);
    }

    .chip-label {
        line-height: var(--font-line-height-flat);
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
