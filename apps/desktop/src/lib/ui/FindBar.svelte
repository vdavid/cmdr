<!--
    The house find bar: a query field, the match counter, and previous / next / close.
    Presentational only: the owner keeps the query, the matches, and the keys (where
    Enter / ⇧Enter / Escape are heard differs per surface), and reads the field back
    through `bind:inputElement` to focus it.

    Two owners today: the file viewer (`routes/viewer/ViewerSearchBar.svelte`, which adds
    its case and regex toggles, a Stop button, and a progress bar through the snippets)
    and the Ask Cmdr rail's in-chat find (`lib/ask-cmdr/AskCmdrFind.svelte`).

    The root's `.search-bar` class is a contract: the viewer's copy handler lets a copy
    inside it through to the native selection, and the viewer's E2E specs find the field
    by it.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import TextInput from './TextInput.svelte'

    interface Props {
        /** The query. Bindable. */
        query?: string
        /** The field element, for the owner's `focus()` / `select()`. Bindable. */
        inputElement?: HTMLInputElement
        /** The field's accessible name ("Find in chat"). */
        ariaLabel: string
        placeholder: string
        /** The bar's landmark name. Optional: one search landmark per window needs none. */
        regionLabel?: string
        /** Matches found, and the 1-based one shown as current (0 for none). Drive the default counter. */
        total?: number
        current?: number
        /** The counter's whole content, replacing the default "3 of 12" / "No matches". */
        status?: Snippet
        /** A quiet line after the counter, such as what the find covers. */
        note?: string
        /** Controls between the field and the counter (the viewer's case and regex toggles). */
        options?: Snippet
        /** Controls between the counter and Previous (the viewer's Stop). */
        actions?: Snippet
        /** Drawn under the row, across the bar (the viewer's progress bar). */
        footer?: Snippet
        /** Previous / Next have a match to go to. Defaults to `total > 0`. */
        canNavigate?: boolean
        onPrevious: () => void
        onNext: () => void
        onClose: () => void
        /** The field's own keys, for an owner that hears them on the field. */
        onkeydown?: (event: KeyboardEvent) => void
        onfocus?: () => void
        onblur?: () => void
        /** Label and tooltip overrides, for an owner with its own (translated) wording. */
        previousLabel?: string
        nextLabel?: string
        closeLabel?: string
        closeTooltip?: string
        previousShortcut?: string
        nextShortcut?: string
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        query = $bindable(''),
        inputElement = $bindable(),
        ariaLabel,
        placeholder,
        regionLabel,
        total = 0,
        current = 0,
        status,
        note,
        options,
        actions,
        footer,
        canNavigate,
        onPrevious,
        onNext,
        onClose,
        onkeydown,
        onfocus,
        onblur,
        previousLabel,
        nextLabel,
        closeLabel,
        closeTooltip,
        previousShortcut = '⇧Enter',
        nextShortcut = 'Enter',
    }: Props = $props()
    /* eslint-enable prefer-const */

    const navigable = $derived(canNavigate ?? total > 0)
    const previousText = $derived(previousLabel ?? tString('ui.findBar.previous'))
    const nextText = $derived(nextLabel ?? tString('ui.findBar.next'))
    const closeText = $derived(closeLabel ?? tString('ui.findBar.close'))
    const closeTip = $derived(closeTooltip ?? closeText)
</script>

<div class="search-bar" role="search" aria-label={regionLabel}>
    <TextInput
        bind:inputElement
        bind:value={query}
        type="search"
        radius="sm"
        containerStyle="flex: 1; min-width: 0; max-width: 300px"
        {placeholder}
        {ariaLabel}
        autocomplete="off"
        autocapitalize="off"
        autocorrect="off"
        spellcheck={false}
        {onkeydown}
        onfocus={() => onfocus?.()}
        onblur={() => onblur?.()}
    />
    {@render options?.()}
    <span class="match-count" aria-live="polite">
        {#if status}
            {@render status()}
        {:else if total > 0}
            {tString('ui.findBar.matchPosition', { currentText: formatInteger(current), totalText: formatInteger(total) })}
        {:else if query.trim() !== ''}
            {tString('ui.findBar.noMatches')}
        {/if}
    </span>
    {@render actions?.()}
    <button
        type="button"
        onclick={onPrevious}
        disabled={!navigable}
        aria-label={previousText}
        use:tooltip={{ text: previousText, shortcut: previousShortcut }}>&#x25B2;</button
    >
    <button
        type="button"
        onclick={onNext}
        disabled={!navigable}
        aria-label={nextText}
        use:tooltip={{ text: nextText, shortcut: nextShortcut }}>&#x25BC;</button
    >
    <button type="button" onclick={onClose} aria-label={closeText} use:tooltip={{ text: closeTip, shortcut: 'Esc' }}
        >&#x2715;</button
    >
    <!-- Last, on a line of its own under the controls: a narrow bar (the rail) has no room beside them. -->
    {#if note}
        <span class="note">{note}</span>
    {/if}
    {@render footer?.()}
</div>

<style>
    .search-bar {
        position: relative;
        display: flex;
        flex-wrap: wrap;
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

    .note {
        flex-basis: 100%;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    /* `:global` reaches the owner's snippet buttons (the viewer's toggles and Stop) too, so
       every control in the bar wears one chrome. */
    .search-bar :global(button) {
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

    .search-bar :global(button:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .search-bar :global(button:hover:not(:disabled)) {
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
    }

    .search-bar :global(button:disabled) {
        opacity: 0.4;
        cursor: default;
    }

    @media (prefers-reduced-motion: reduce) {
        .search-bar :global(button) {
            transition: none;
        }
    }
</style>
