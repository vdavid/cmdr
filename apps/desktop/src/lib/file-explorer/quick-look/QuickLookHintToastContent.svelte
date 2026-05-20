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
    import { setSetting } from '$lib/settings'
    import { openSettingsWindow } from '$lib/settings/settings-window'

    import { QUICK_LOOK_HINT_TOAST_ID } from './quick-look-hint-id'

    function handleOpenSettings() {
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
        void openSettingsWindow(['Keyboard shortcuts'])
    }

    function handleDontShowAgain() {
        setSetting('fileExplorer.suppressQuickLookHint', true)
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
    }
</script>

<div class="content">
    <p>
        In Cmdr, <kbd>Space</kbd> selects the file under the cursor by default.
    </p>
    <p>
        If you come from Finder, this might be unusual because Finder triggers a "Quick preview" action on
        <kbd>Space</kbd>.
    </p>
    <p>
        In Cmdr, you can trigger quick view via <kbd>⇧Space</kbd>. (<kbd>⇧Space</kbd> works in Finder, too, btw!)
    </p>
    <p>
        You can also use <kbd>Enter</kbd> to open files in the default app.
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

    kbd {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        white-space: nowrap;
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
