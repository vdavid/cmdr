<script lang="ts">
    /**
     * QueryBar: the unified query field, shaped as a combobox.
     *
     * One field drives all three modes (AI, filename, regex). The placeholder updates per mode
     * so the user can see at a glance what kind of input the bar expects. Switching mode preserves
     * the typed query; this component is presentational, the parent owns `query` and `mode`.
     *
     * The field is the house `TextInput` in its search-pill shape (`radius="full"` + the magnifier
     * `leadingIcon`), matching the Settings sidebar's search field. Its trailing slot holds the
     * recent-items dropdown trigger: a small chevron that opens the recent-items popover. The
     * popover itself lives in `QueryDialog`; this component only owns the trigger and hands the
     * pill element back through `bind:fieldElement` so the popover can anchor to the whole field.
     *
     * To the right of the pill:
     *   - A subtle "Press Enter to search" hint when auto-apply is off (or AI mode) and the query
     *     has changed since the last run. Visible state, not interactive.
     *   - The house `Button` that runs the query. Always present; clicking it is equivalent to
     *     pressing Enter.
     *
     * IME composition is also surfaced: `oncompositionstart` and `oncompositionend` let the parent
     * suppress auto-apply mid-composition and fire exactly once on completion.
     *
     * Keyboard contract (handled by the parent dialog, not here):
     *   - Enter runs the query in the active mode.
     *   - ArrowDown with no results opens the recent-items dropdown.
     *   - ⌘1/⌘2/⌘3 switch modes (numbering changes when AI is off).
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Button from '$lib/ui/Button.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import type { SearchMode } from './query-filter-state.svelte'

    interface Props {
        /** Bindable ref to the input element so the parent can manage focus. */
        inputElement: HTMLInputElement | undefined
        /**
         * Bindable ref to the pill frame. The parent anchors the recent-items popover to it, so
         * the dropdown lines up with the whole field rather than with the chevron.
         */
        fieldElement?: HTMLElement
        query: string
        mode: SearchMode
        disabled: boolean
        aiHighlight: boolean
        /** True when the bar should show the run hint. Owned by the parent. */
        showRunHint?: boolean
        /**
         * The run hint itself. Each dialog names its own verb ("Press Enter to search" for
         * Search, "Press Enter to filter" for Selection), so the bar renders what it's
         * handed rather than a shared string.
         */
        runHintCopy: string
        /**
         * Replaces the run button's tooltip and accessible name. Search sets it so the
         * button says what Enter actually does now: search past the index, into folders
         * that aren't indexed yet, which the auto-apply debounce deliberately won't do
         * (`docs/specs/unindexed-search-plan.md` Decision 7). Omitted → the per-mode
         * default.
         */
        runTitleOverride?: string
        /**
         * D8: when true, the run button surfaces the `⏎` shortcut hint. The dialog
         * owns the ⏎ ownership swap; when this is false, the hint moves to the
         * footer's "Go to file" button.
         */
        showEnterHint?: boolean
        /** True while the recent-items dropdown is open. Drives the trigger's `aria-expanded`. */
        recentOpen?: boolean
        onInput: (value: string) => void
        /** Click handler for the run button. Equivalent to pressing Enter in the input. */
        onRun: () => void
        /** Click handler for the dropdown trigger. Toggles the recent-items popover. */
        onToggleRecent: () => void
        /** Accessible name + tooltip for the dropdown trigger. Consumer copy ("All recent searches"). */
        recentTriggerLabel: string
        recentTriggerTooltip: string
        /** IME composition entry: parent suppresses auto-apply between start and end. */
        onCompositionStart?: () => void
        /** IME composition exit: parent fires exactly one debounced search after this. */
        onCompositionEnd?: () => void
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        inputElement = $bindable(),
        fieldElement = $bindable(),
        query,
        mode,
        disabled,
        aiHighlight,
        showRunHint = false,
        runHintCopy,
        runTitleOverride,
        showEnterHint = true,
        recentOpen = false,
        onInput,
        onRun,
        onToggleRecent,
        recentTriggerLabel,
        recentTriggerTooltip,
        onCompositionStart,
        onCompositionEnd,
    }: Props = $props()
    /* eslint-enable prefer-const */

    /** Placeholder text per mode. Filenames are the workhorse, so we name the wildcards there. */
    const placeholder = $derived.by(() => {
        if (mode === 'ai') return tString('queryUi.bar.placeholder.ai')
        if (mode === 'regex') return tString('queryUi.bar.placeholder.regex')
        return tString('queryUi.bar.placeholder.filename')
    })

    const ariaLabel = $derived.by(() => {
        if (mode === 'ai') return tString('queryUi.bar.aria.ai')
        if (mode === 'regex') return tString('queryUi.bar.aria.regex')
        return tString('queryUi.bar.aria.filename')
    })

    /** AI mode runs only on explicit Enter / ⌘Enter / Run-button click. Show the hint title to match. */
    const runTitle = $derived(
        mode === 'ai'
            ? tString('queryUi.bar.runTitle.ai')
            : (runTitleOverride ?? tString('queryUi.bar.runTitle.default')),
    )
</script>

<!-- `display: contents`: the bar hands its two halves straight to the dialog's 2×2
     control grid, so the field lands in the left column (above the mode chips) and the
     Search button in the right one (above Count only). -->
<div class="query-bar">
    <div class="query-bar__query">
        <!-- The AI flash rides the wrapper as an accent ring rather than the field's own fill: the
             pill's background is opaque, so tinting behind it would never show. -->
        <div class="query-field" class:ai-highlight={aiHighlight} bind:this={fieldElement}>
            <TextInput
                bind:inputElement
                type="text"
                radius="full"
                leadingIcon="search"
                containerStyle="flex: 1 1 auto; min-width: 0;"
                {placeholder}
                value={query}
                {disabled}
                {ariaLabel}
                spellcheck={false}
                autocomplete="off"
                autocapitalize="off"
                oninput={(e: Event) => {
                    onInput((e.target as HTMLInputElement).value)
                }}
                oncompositionstart={() => {
                    onCompositionStart?.()
                }}
                oncompositionend={() => {
                    onCompositionEnd?.()
                }}
            >
                {#snippet trailing()}
                    <button
                        type="button"
                        class="recent-trigger"
                        {disabled}
                        aria-label={recentTriggerLabel}
                        aria-haspopup="dialog"
                        aria-expanded={recentOpen}
                        onclick={onToggleRecent}
                        use:tooltip={{ text: recentTriggerTooltip, shortcut: '⌘H' }}
                    >
                        <Icon name="chevron-down" size={14} aria-hidden="true" />
                    </button>
                {/snippet}
            </TextInput>
        </div>
        {#if showRunHint}
            <span class="run-hint" aria-hidden="true">{runHintCopy}</span>
        {/if}
    </div>
    <!-- Button reads "Search ⏎" when ⏎ owns the run action; just "Search" when the
         footer's Go-to-file owns ⏎. Exactly one of the two surfaces the hint. It's the
         house `Button` in the same secondary family as the footer's actions, so the
         dialog has exactly one button style. -->
    <div class="run-action">
        <Button variant="secondary" {disabled} onclick={onRun} aria-label={runTitle}>
            <span class="run-label" use:tooltip={{ text: runTitle, shortcut: '⏎' }}>
                {tString('queryUi.bar.runLabel')}{#if showEnterHint}<ShortcutChip key="⏎" size="sm" />{/if}
            </span>
        </Button>
    </div>
</div>

<style>
    /* No box of its own: the two children below ARE the grid cells. The dialog's
       `.query-grid` owns the inset (`--spacing-dialog`, matching `ModalDialog`'s title
       bar) and the column widths. `.query-bar` survives as a selector hook: the E2E
       suite and the dialog tests address the field and the run button through it. */
    .query-bar {
        display: contents;
    }

    /* Left cell: the pill takes the room, the run hint rides at its trailing end. */
    .query-bar__query {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    /* The pill's own wrapper. Carries the AI flash ring and hands the parent one element
       to anchor the recent-items dropdown to. */
    .query-field {
        display: flex;
        flex: 1 1 auto;
        min-width: 0;
        border-radius: var(--radius-full);
        transition: box-shadow 1.5s ease-out;
    }

    .query-field.ai-highlight {
        box-shadow: 0 0 0 4px var(--color-accent-subtle);
    }

    /* The dropdown trigger inside the pill's trailing slot. Quiet by default; it's an
       affordance, not a call to action. */
    .recent-trigger {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-xxs);
        background: transparent;
        border: none;
        border-radius: var(--radius-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        line-height: var(--font-line-height-flat);
    }

    .recent-trigger:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .recent-trigger:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .recent-trigger:disabled {
        opacity: 0.5;
    }

    .run-hint {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        white-space: nowrap;
    }

    /* Right cell. The button fills it, so "Search ⏎" and the "Count only" switch below
       it end on the same two edges: that shared width is what makes the four controls
       read as one 2×2 block. */
    .run-action {
        display: flex;
    }

    .run-action :global(button) {
        width: 100%;
    }

    /* --spacing-xs gap between "Search" and "⏎" matches the visual rhythm of the
       footer's "Go to file ⏎". */
    .run-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        line-height: var(--font-line-height-flat);
    }
</style>
