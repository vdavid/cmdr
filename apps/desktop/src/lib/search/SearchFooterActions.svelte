<script lang="ts">
    /**
     * SearchFooterActions: the right-edge action buttons in the dialog footer.
     *
     * Two affordances:
     *   - "Go to file": closes the dialog and navigates the active pane to the cursor
     *     row's parent folder, focusing the file (pushes a new history entry).
     *     Replaces the previous "Open in Finder" button per search-fixup-brief
     *     clarification 3. The parent owns the navigation; this component only fires
     *     the callback.
     *   - "Show all in main window" (⌥A): the primary action. Promotes the current
     *     result set into a `search-results://<id>` virtual-volume pane and closes the
     *     dialog. Per search-fixup-brief item 10 + clarification 1.
     *
     * Both buttons are HIDDEN (not just disabled) when there are no results, because
     * they have nothing to act on. Shortcut hints render inline in tertiary text so
     * keyboard users can discover them without hovering.
     */
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        /**
         * Number of results currently displayed. Per round-2 D6, the buttons stay
         * VISIBLE on 0 results and just render disabled — yanking them would
         * jump the layout while the user is mid-thought.
         */
        resultCount: number
        /**
         * Disabled state mirrors the dialog's `inputsDisabled` flag (index not ready,
         * etc.). When true, both buttons render disabled even with results.
         */
        disabled: boolean
        /** Click handler for "Show all in main window". Parent creates the snapshot,
         *  navigates the active pane to `search-results://<id>`, and closes the dialog. */
        onShowAllInMainWindow: () => void
        /** Click handler for "Go to file". Parent closes the dialog and navigates the
         *  active pane to the cursor row's parent folder, focusing the file. */
        onGoToFile: () => void
        /**
         * Per D8: which button currently owns the `⏎` shortcut hint. Exactly one
         * of "Go to file" / the bar's Search button surfaces `⏎` at a time. The
         * footer button reads `Go to file ⏎` when `enterAction === 'go-to-file'`
         * (results visible and the last event was results arrival or cursor move);
         * otherwise the hint moves to the bar's Search button and we drop it here.
         */
        enterAction: 'go-to-file' | 'run-search'
    }

    const { resultCount, disabled, onShowAllInMainWindow, onGoToFile, enterAction }: Props = $props()

    /** True when both buttons should render disabled (no results or inputs gated). */
    const effectivelyDisabled = $derived(disabled || resultCount === 0)
</script>

<!-- D6: always render both buttons; disable when there's nothing to act on. -->
<div class="footer-actions" role="group" aria-label="Search result actions">
    <Button
        variant="secondary"
        size="mini"
        disabled={effectivelyDisabled}
        onclick={onGoToFile}
        aria-label="Go to file"
    >
        <span use:tooltip={'Open the file in the active pane'}>
            Go to file{#if enterAction === 'go-to-file'}<span class="shortcut-hint" aria-hidden="true">⏎</span>{/if}
        </span>
    </Button>
    <Button
        variant="primary"
        size="mini"
        disabled={effectivelyDisabled}
        onclick={onShowAllInMainWindow}
        aria-label="Show all in main window"
    >
        <!-- R3: ⌥⏎ replaces the old ⌥A (which now belongs to mode chip AI). -->
        <span use:tooltip={'Open the search results in the active pane'}>
            Show all in main window<span class="shortcut-hint shortcut-on-primary" aria-hidden="true">⌥⏎</span>
        </span>
    </Button>
</div>

<style>
    /* No background / border-top here: the parent `.dialog-footer` owns the
       single uniform footer surface and the hairline above it (search-fixup
       brief item 1). */
    .footer-actions {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-lg);
    }

    /* Inline shortcut hint inside a button label. Stays quiet (tertiary text on
       secondary buttons, muted accent-fg on primary) so the action verb leads. */
    .shortcut-hint {
        margin-left: var(--spacing-xs);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        opacity: 0.8;
    }

    .shortcut-hint.shortcut-on-primary {
        color: var(--color-accent-fg);
        opacity: 0.8;
    }
</style>
