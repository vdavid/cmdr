<script lang="ts">
    /**
     * SearchFooterActions: the right-edge action buttons in the dialog footer.
     *
     * Per search-redesign-plan §3.9:
     *   - "Open in pane" (primary): visible whenever results exist. M7 ships this button
     *     as a STUB — clicking closes the dialog and shows a "coming in M8" toast. M8
     *     wires the snapshot store + virtual-volume push, at which point the toast goes
     *     away and the handler does the real navigation.
     *   - "Open in Finder" (macOS) / "Open in file manager" (Linux): opens the parent
     *     folder of the cursor row in the platform's file manager. Wired here via the
     *     existing `showInFinder` IPC (which already calls `open -R` on macOS and
     *     `xdg-open` on the parent on Linux — exactly what the spec wants).
     *   - Both buttons are HIDDEN, not just disabled, when there are no results, because
     *     they have nothing to act on.
     */
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { isMacOS } from '$lib/shortcuts/key-capture'

    interface Props {
        /** Number of results currently displayed. When 0, the footer renders nothing. */
        resultCount: number
        /**
         * Disabled state mirrors the dialog's `inputsDisabled` flag (index not ready,
         * etc.). Keeps the buttons visible-but-disabled instead of yanking them, which
         * would otherwise jump the layout while the user is mid-thought.
         */
        disabled: boolean
        /** Click handler for "Open in pane". Parent owns the (currently stub) navigation. */
        onOpenInPane: () => void
        /** Click handler for "Open in Finder / file manager". Parent owns the IPC. */
        onOpenInFileManager: () => void
    }

    const { resultCount, disabled, onOpenInPane, onOpenInFileManager }: Props = $props()

    // Spec §5.6: macOS "Open in Finder"; Linux "Open in file manager". Sentence case.
    const fileManagerLabel = $derived(isMacOS() ? 'Open in Finder' : 'Open in file manager')
</script>

{#if resultCount > 0}
    <div class="footer-actions" role="group" aria-label="Search result actions">
        <Button
            variant="secondary"
            size="mini"
            {disabled}
            onclick={onOpenInFileManager}
            aria-label={fileManagerLabel}
        >
            <span use:tooltip={'Reveal the cursor row in the system file manager'}>{fileManagerLabel}</span>
        </Button>
        <Button
            variant="primary"
            size="mini"
            {disabled}
            onclick={onOpenInPane}
            aria-label="Open in pane"
        >
            <span use:tooltip={'Open the results in a pane (coming soon)'}>Open in pane</span>
        </Button>
    </div>
{/if}

<style>
    .footer-actions {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-primary);
        border-top: 1px solid var(--color-border-subtle);
    }
</style>
