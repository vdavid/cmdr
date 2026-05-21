<script lang="ts">
    /**
     * SearchBar: the unified search input.
     *
     * One input drives all three modes (AI, filename, regex). The placeholder updates per mode
     * so the user can see at a glance what kind of input the bar expects. Switching mode preserves
     * the typed query; this component is presentational, the parent owns `query` and `mode`.
     *
     * Keyboard contract (handled by the parent dialog, not here):
     *   - Enter runs the search in the active mode.
     *   - ⌘Enter runs an AI search regardless (only when AI is enabled).
     *   - ⌘1/⌘2/⌘3 switch modes (numbering changes when AI is off).
     */
    import type { SearchMode } from './search-state.svelte'

    interface Props {
        /** Bindable ref to the input element so the parent can manage focus. */
        inputElement: HTMLInputElement | undefined
        query: string
        mode: SearchMode
        disabled: boolean
        aiHighlight: boolean
        onInput: (value: string) => void
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { inputElement = $bindable(), query, mode, disabled, aiHighlight, onInput }: Props = $props()
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
        {disabled}
        aria-label={ariaLabel}
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
    />
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
        font-size: var(--font-size-lg);
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
</style>
