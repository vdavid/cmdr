<!--
    The viewer's search bar: the query field, the case and regex toggles, the match counter,
    the stop / previous / next / close buttons, and the progress bar under them.

    Presentational only. Every value and action belongs to the page's `createViewerSearch`
    composable, passed in whole; the page shows the bar while `search.searchVisible`. The
    `.search-bar` class is load-bearing: the page's copy handler lets a copy inside it
    through to the native selection.
-->

<script lang="ts">
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import TextInput from '#lib/ui/TextInput.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import type { createViewerSearch } from './viewer-search.svelte'

    interface Props {
        /** The page's search composable: the query, its matches and status, and the actions. */
        search: ReturnType<typeof createViewerSearch>
        /** The query field gained or lost focus, which decides whether Edit > Cut / Paste look live. */
        onFocusChange: (focused: boolean) => void
    }

    const { search, onFocusChange }: Props = $props()
</script>

<div class="search-bar" role="search">
    <TextInput
        bind:inputElement={search.searchInputRef}
        bind:value={search.searchQuery}
        type="search"
        radius="sm"
        containerStyle="flex: 1; max-width: 300px"
        placeholder={tString('viewer.search.placeholder')}
        ariaLabel={tString('viewer.search.ariaLabel')}
        autocomplete="off"
        autocapitalize="off"
        spellcheck={false}
        onfocus={() => { onFocusChange(true); }}
        onblur={() => { onFocusChange(false); }}
    />
    <button
        type="button"
        class="search-toggle"
        class:active={search.caseSensitive}
        aria-pressed={search.caseSensitive}
        aria-label={tString('viewer.search.caseSensitive')}
        onclick={() => { search.toggleCaseSensitive(); }}
        use:tooltip={{ text: tString('viewer.search.caseSensitive'), shortcut: '⌘⌥C' }}
    >
        <!-- eslint-disable-next-line cmdr/no-raw-user-facing-string -- typographic glyph for the case-sensitive toggle, not copy (the a11y label + tooltip carry the copy) -->
        <span>Aa</span>
    </button>
    <button
        type="button"
        class="search-toggle"
        class:active={search.useRegex}
        aria-pressed={search.useRegex}
        aria-label={tString('viewer.search.regex')}
        onclick={() => { search.toggleUseRegex(); }}
        use:tooltip={{ text: tString('viewer.search.regex'), shortcut: '⌘⌥R' }}
    >
        .*
    </button>
    <span class="match-count" aria-live="polite">
        {#if search.searchStatus === 'invalidQuery'}
            <span class="search-error" role="alert">{search.searchError}</span>
        {:else if search.searchStatus === 'running'}
            <span class="search-spinner"><Spinner size="sm" /></span>
            {#if search.searchMatches.length > 0}
                {tString('viewer.search.matchPosition', {
                    current: search.currentMatchIndex + 1,
                    total: search.searchMatches.length,
                    more: search.searchLimitReached ? 'yes' : 'no',
                })}
                &middot; {Math.round(search.searchProgress * 100)}%
            {:else}
                {tString('viewer.search.searching')} {Math.round(search.searchProgress * 100)}%
            {/if}
        {:else if search.searchMatches.length > 0}
            {tString('viewer.search.matchPosition', {
                current: search.currentMatchIndex + 1,
                total: search.searchMatches.length,
                more: search.searchLimitReached ? 'yes' : 'no',
            })}
            {#if search.searchStatus === 'cancelled'}
                {tString('viewer.search.partial')}
            {/if}
        {:else if search.searchQuery && (search.searchStatus === 'done' || search.searchStatus === 'cancelled')}
            {tString('viewer.search.noMatches')}{search.searchStatus === 'cancelled'
                ? ' ' + tString('viewer.search.partial')
                : ''}
        {/if}
    </span>
    {#if search.searchStatus === 'running'}
        <button
            onclick={() => {
                search.stopSearch()
            }}
            aria-label={tString('viewer.search.stop')}
            use:tooltip={tString('viewer.search.stopTooltip')}>&#x25A0;</button
        >
    {/if}
    <button
        onclick={() => {
            search.findPrev()
        }}
        disabled={search.searchMatches.length === 0}
        aria-label={tString('viewer.search.previous')}
        use:tooltip={{ text: tString('viewer.search.previous'), shortcut: '⇧Enter' }}>&#x25B2;</button
    >
    <button
        onclick={() => {
            search.findNext()
        }}
        disabled={search.searchMatches.length === 0}
        aria-label={tString('viewer.search.next')}
        use:tooltip={{ text: tString('viewer.search.next'), shortcut: 'Enter' }}>&#x25BC;</button
    >
    <button
        onclick={() => {
            search.closeSearch()
        }}
        aria-label={tString('viewer.search.close')}
        use:tooltip={{ text: tString('viewer.search.closeTooltip'), shortcut: 'Esc' }}>&#x2715;</button
    >
    {#if search.searchStatus === 'running'}
        <div
            class="search-progress"
            role="progressbar"
            aria-valuenow={Math.round(search.searchProgress * 100)}
            aria-valuemin={0}
            aria-valuemax={100}
        >
            <div class="search-progress-fill" style="width: {search.searchProgress * 100}%"></div>
        </div>
    {/if}
</div>

<style>
    .search-bar {
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        flex-shrink: 0;
    }

    .match-count {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        min-width: 70px;
        white-space: nowrap;
    }

    .search-spinner {
        vertical-align: text-bottom;
        margin-right: var(--spacing-xxs);
    }

    .search-progress {
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 2px;
        background: var(--color-bg-tertiary);
        overflow: hidden;
    }

    .search-progress-fill {
        height: 100%;
        background: var(--color-accent);
        transition: width var(--transition-base);
    }

    @media (prefers-reduced-motion: reduce) {
        .search-progress-fill {
            transition: none;
        }
    }

    .search-bar button {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        padding: var(--spacing-xxs) var(--spacing-xs);
        line-height: var(--font-line-height-flat);
        transition: all var(--transition-base);
    }

    .search-bar button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .search-bar button:hover:not(:disabled) {
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
    }

    .search-bar button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    /* Search-mode toggles (Aa, .*). Use the same chrome as other search-bar
       buttons but switch background + text colour when active so the toggle
       state is visible at a glance. */
    .search-toggle {
        font-family: var(--font-mono);
        min-width: 2.2em;
    }

    .search-toggle.active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-accent-text);
    }

    .search-error {
        color: var(--color-error);
        font-size: var(--font-size-sm);
    }
</style>
