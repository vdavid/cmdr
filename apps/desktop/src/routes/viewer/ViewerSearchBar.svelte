<!--
    The viewer's search bar: the house `FindBar` plus the viewer's own pieces, the case and
    regex toggles, the running counter (spinner, percent, "(partial)", an invalid regex),
    the Stop button, and the progress bar under the row.

    Presentational only. Every value and action belongs to the page's `createViewerSearch`
    composable, passed in whole; the page shows the bar while `search.searchVisible`. The
    viewer hears Enter / ⇧Enter / Escape at the window (`viewer-keyboard.ts`), so the bar
    takes no keys of its own. `FindBar`'s `.search-bar` root class is load-bearing: the
    page's copy handler lets a copy inside it through to the native selection.
-->

<script lang="ts">
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import FindBar from '#lib/ui/FindBar.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import type { createViewerSearch } from './viewer-search.svelte'

    interface Props {
        /** The page's search composable: the query, its matches and status, and the actions. */
        search: ReturnType<typeof createViewerSearch>
        /** The query field gained or lost focus, which decides whether Edit > Cut / Paste look live. */
        onFocusChange: (focused: boolean) => void
    }

    const { search, onFocusChange }: Props = $props()

    const position = $derived(
        tString('viewer.search.matchPosition', {
            current: search.currentMatchIndex + 1,
            total: search.searchMatches.length,
            more: search.searchLimitReached ? 'yes' : 'no',
        }),
    )
</script>

{#snippet toggles()}
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
{/snippet}

{#snippet status()}
    {#if search.searchStatus === 'invalidQuery'}
        <span class="search-error" role="alert">{search.searchError}</span>
    {:else if search.searchStatus === 'running'}
        <span class="search-spinner"><Spinner size="sm" /></span>
        {#if search.searchMatches.length > 0}
            {position} &middot; {Math.round(search.searchProgress * 100)}%
        {:else}
            {tString('viewer.search.searching')} {Math.round(search.searchProgress * 100)}%
        {/if}
    {:else if search.searchMatches.length > 0}
        {position}
        {#if search.searchStatus === 'cancelled'}
            {tString('viewer.search.partial')}
        {/if}
    {:else if search.searchQuery && (search.searchStatus === 'done' || search.searchStatus === 'cancelled')}
        {tString('viewer.search.noMatches')}{search.searchStatus === 'cancelled'
            ? ' ' + tString('viewer.search.partial')
            : ''}
    {/if}
{/snippet}

{#snippet stop()}
    {#if search.searchStatus === 'running'}
        <button
            type="button"
            onclick={() => { search.stopSearch(); }}
            aria-label={tString('viewer.search.stop')}
            use:tooltip={tString('viewer.search.stopTooltip')}>&#x25A0;</button
        >
    {/if}
{/snippet}

{#snippet progress()}
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
{/snippet}

<FindBar
    bind:inputElement={search.searchInputRef}
    bind:query={search.searchQuery}
    placeholder={tString('viewer.search.placeholder')}
    ariaLabel={tString('viewer.search.ariaLabel')}
    total={search.searchMatches.length}
    {status}
    options={toggles}
    actions={stop}
    footer={progress}
    onPrevious={() => { search.findPrev(); }}
    onNext={() => { search.findNext(); }}
    onClose={() => { search.closeSearch(); }}
    onfocus={() => { onFocusChange(true); }}
    onblur={() => { onFocusChange(false); }}
    previousLabel={tString('viewer.search.previous')}
    nextLabel={tString('viewer.search.next')}
    closeLabel={tString('viewer.search.close')}
    closeTooltip={tString('viewer.search.closeTooltip')}
/>

<style>
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

    /* Search-mode toggles (Aa, .*). The bar's button chrome, with the background and text
       switching when active so the toggle state is visible at a glance. */
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
