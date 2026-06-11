<script lang="ts">
    /**
     * SearchBar: the unified search input.
     *
     * One input drives all three modes (AI, filename, regex). The placeholder updates per mode
     * so the user can see at a glance what kind of input the bar expects. Switching mode preserves
     * the typed query; this component is presentational, the parent owns `query` and `mode`.
     *
     * The right gutter shows two things, both managed by the parent dialog:
     *   - A subtle "Press Enter to search" hint when auto-apply is off (or AI mode) and the query
     *     has changed since the last run. Visible state, not interactive.
     *   - A small ⏎ run button. Always present; clicking it is equivalent to pressing Enter.
     *
     * IME composition is also surfaced: `oncompositionstart` and `oncompositionend` let the parent
     * suppress auto-apply mid-composition and fire exactly once on completion.
     *
     * Keyboard contract (handled by the parent dialog, not here):
     *   - Enter runs the search in the active mode.
     *   - ⌘Enter runs an AI search regardless (only when AI is enabled).
     *   - ⌘1/⌘2/⌘3 switch modes (numbering changes when AI is off).
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import type { SearchMode } from './query-filter-state.svelte'

    interface Props {
        /** Bindable ref to the input element so the parent can manage focus. */
        inputElement: HTMLInputElement | undefined
        query: string
        mode: SearchMode
        disabled: boolean
        aiHighlight: boolean
        /** True when the bar should show the "Press Enter to search" hint. Owned by the parent. */
        showRunHint?: boolean
        /**
         * D8: when true, the Search button surfaces the `⏎` shortcut hint. The dialog
         * owns the ⏎ ownership swap; when this is false, the hint moves to the
         * footer's "Go to file" button.
         */
        showEnterHint?: boolean
        onInput: (value: string) => void
        /** Click handler for the ⏎ run button. Equivalent to pressing Enter in the input. */
        onRun: () => void
        /** IME composition entry: parent suppresses auto-apply between start and end. */
        onCompositionStart?: () => void
        /** IME composition exit: parent fires exactly one debounced search after this. */
        onCompositionEnd?: () => void
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        inputElement = $bindable(),
        query,
        mode,
        disabled,
        aiHighlight,
        showRunHint = false,
        showEnterHint = true,
        onInput,
        onRun,
        onCompositionStart,
        onCompositionEnd,
    }: Props = $props()
    /* eslint-enable prefer-const */

    /** Placeholder text per mode. Filenames are the workhorse, so we name the wildcards there. */
    const placeholder = $derived.by(() => {
        if (mode === 'ai') return "Describe what you're looking for"
        if (mode === 'regex') return 'Regular expression pattern'
        return 'Filename pattern (use * and ? as wildcards)'
    })

    const ariaLabel = $derived.by(() => {
        if (mode === 'ai') return 'Natural language search query'
        if (mode === 'regex') return 'Regex search pattern'
        return 'Filename search pattern'
    })

    /** AI mode runs only on explicit Enter / ⌘Enter / Run-button click. Show the hint title to match. */
    const runTitle = $derived(mode === 'ai' ? 'Run AI search' : 'Run search')
</script>

<div class="search-bar" class:is-disabled={disabled}>
    <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <circle cx="6.5" cy="6.5" r="5" stroke="currentColor" stroke-width="1.5" />
        <line x1="10.5" y1="10.5" x2="14.5" y2="14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
    <input
        bind:this={inputElement}
        type="text"
        class="query-input"
        class:ai-highlight={aiHighlight}
        {placeholder}
        value={query}
        oninput={(e: Event) => {
            onInput((e.target as HTMLInputElement).value)
        }}
        oncompositionstart={() => {
            onCompositionStart?.()
        }}
        oncompositionend={() => {
            onCompositionEnd?.()
        }}
        {disabled}
        aria-label={ariaLabel}
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
    />
    {#if showRunHint}
        <span class="run-hint" aria-hidden="true">Press Enter to search</span>
    {/if}
    <!-- Button reads "Search ⏎" when ⏎ owns the run action; just "Search" when the
         footer's Go-to-file owns ⏎. Exactly one of the two surfaces the hint. The
         shortcut belongs in the suffix slot (matching "Go to file ⏎" and "All
         searches… ⌘H"); no leading icon. -->
    <button
        type="button"
        class="run-button"
        {disabled}
        onclick={onRun}
        use:tooltip={{ text: runTitle, shortcut: '⏎' }}
        aria-label={runTitle}
    >
        <span class="run-label">Search</span>
        {#if showEnterHint}<ShortcutChip key="⏎" size="sm" />{/if}
    </button>
</div>

<style>
    .search-bar {
        display: flex;
        align-items: center;
        padding: var(--spacing-lg);
        background: var(--color-bg-primary);
        gap: var(--spacing-sm);
    }

    .search-icon {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .query-input {
        flex: 1;
        font-size: var(--font-size-xl);
        border: 1px solid transparent;
        background: transparent;
        color: var(--color-text-primary);
        outline: none;
        min-width: 0;
    }

    .query-input:focus {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .query-input::placeholder {
        color: var(--color-text-tertiary);
        opacity: 1; /* Override browser default dimming for a11y contrast */
    }

    .query-input.ai-highlight {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    .run-hint {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        white-space: nowrap;
    }

    .run-button {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        /* --spacing-xs gap between "Search" and "⏎" matches the visual rhythm of
           "All searches… ⌘H" and "Go to file ⏎" elsewhere in the dialog. */
        gap: var(--spacing-xs);
        justify-content: center;
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: transparent;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        cursor: default;
        line-height: 1;
        font-size: var(--font-size-md);
    }

    .run-label {
        line-height: 1;
    }

    .run-button:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .run-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .run-button:disabled {
        opacity: 0.5;
        cursor: default;
    }
</style>
