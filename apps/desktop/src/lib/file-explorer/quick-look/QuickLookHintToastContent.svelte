<script lang="ts">
    /**
     * Educational toast for Finder converts: explains that Cmdr uses Space
     * for selection and ⇧Space for Quick Look.
     *
     * Two dismissal paths:
     *
     * - **X (frame's close button)**: just closes this instance. The next
     *   Space press will show the toast again — see `quick-look-hint.ts`.
     * - **"Don't show again" button below**: closes this instance AND flips
     *   `fileExplorer.suppressQuickLookHint` to `true`, so future Space
     *   presses never show the hint until the user turns it back off in
     *   Settings > Advanced.
     */

    import { dismissToast } from '$lib/ui/toast'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { setSetting } from '$lib/settings'
    import { getEffectiveShortcuts } from '$lib/shortcuts'
    import { openShortcutCustomization } from '$lib/settings/settings-window'

    import { QUICK_LOOK_HINT_TOAST_ID } from './quick-look-hint-id'

    // Snapshot the Quick Look binding at toast creation. Toasts don't rewrite
    // themselves when the user rebinds mid-display (the next toast picks up the
    // change); a literal-mode chip with this fixed string is the right shape.
    // Falls back to ⇧Space (the default) so the hint always shows a key. The
    // chip is non-clickable: the toast already offers the Settings link below.
    const quickLookKey = getEffectiveShortcuts('file.quickLook')[0] ?? '⇧Space'

    function handleOpenSettings() {
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
        // Deep-link straight to the Quick Look command's row in Keyboard shortcuts
        // (scrolled into view + flashed), the first consumer of the deep-link path.
        void openShortcutCustomization('file.quickLook')
    }

    function handleDontShowAgain() {
        setSetting('fileExplorer.suppressQuickLookHint', true)
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
    }
</script>

<div class="content">
    <p>
        In Cmdr, <ShortcutChip key="Space" /> selects the file under the cursor by default.
    </p>
    <p>
        If you come from Finder, this might be unusual because Finder triggers a "Quick preview" action on
        <ShortcutChip key="Space" />.
    </p>
    <p>
        In Cmdr, you can trigger quick view via <ShortcutChip key={quickLookKey} />. (<ShortcutChip
            key={quickLookKey}
        /> works in Finder, too, btw!)
    </p>
    <p>
        You can also use <ShortcutChip key="Enter" /> to open files in the default app.
    </p>
    <p class="settings-line">
        All of this is configurable in
        <LinkButton onclick={handleOpenSettings}>Settings &gt; Keyboard shortcuts</LinkButton>.
    </p>
    <div class="actions">
        <button class="dont-show-again-button" type="button" onclick={handleDontShowAgain}>Don't show again</button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        line-height: 1.4;
        color: var(--color-text-primary);
        /* Toasts default to a narrow column; give this one a bit more room so
           lines don't break awkwardly mid-sentence. */
        max-width: 28rem;
    }

    p {
        margin: 0;
    }

    .settings-line {
        margin-top: var(--spacing-xs);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-xs);
    }

    /* Muted text-link style — matches the MTP toast's "Disable MTP..." link.
       Intentionally not using the LinkButton accent style: "Don't show again"
       is a soft opt-out, not a primary action competing with the inline
       Settings link above. */
    .dont-show-again-button {
        background: none;
        border: none;
        padding: 0;
        font-family: inherit;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .dont-show-again-button:hover {
        color: var(--color-text-secondary);
    }
</style>
