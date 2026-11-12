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

    import type { Snippet } from 'svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { setSetting } from '#lib/settings/index.js'
    import { commandHasShortcut } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { openShortcutCustomization } from '#lib/settings/settings-window.js'

    import { QUICK_LOOK_HINT_TOAST_ID } from './quick-look-hint-id'

    // The Quick Look chip is live: it follows a rebind, and its line goes away once no
    // key runs `file.quickLook`. ❌ No fall back to ⇧Space: a default the user removed
    // runs nothing, and naming it would lie. The chip is non-clickable: the toast
    // already offers the Settings link below.
    const quickLookBound = $derived(commandHasShortcut('file.quickLook'))

    function handleOpenSettings() {
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
        // Deep-link straight to the Quick Look command's row in Keyboard shortcuts
        // (scrolled into view + flashed), the first consumer of the deep-link path.
        void openShortcutCustomization('quick-look-toast', 'file.quickLook')
    }

    function handleDontShowAgain() {
        setSetting('fileExplorer.suppressQuickLookHint', true)
        dismissToast(QUICK_LOOK_HINT_TOAST_ID)
    }
</script>

<!-- Inline-chip/link snippets for the <Trans> lines. The tag names (space, key,
     enter, settingsLink) intentionally differ from any interpolation param. -->
{#snippet spaceChip(_children: Snippet)}<ShortcutChip key="Space" />{/snippet}
{#snippet quickLookChip(_children: Snippet)}<ShortcutChip commandId="file.quickLook" clickable={false} />{/snippet}
{#snippet enterChip(_children: Snippet)}<ShortcutChip key="Enter" />{/snippet}
{#snippet settingsLink(children: Snippet)}<LinkButton onclick={handleOpenSettings}>{@render children()}</LinkButton>{/snippet}

<div class="content">
    <p>
        <Trans key="fileExplorer.quickLookHint.spaceSelects" snippets={{ space: spaceChip }} />
    </p>
    <p>
        <Trans key="fileExplorer.quickLookHint.finderComparison" snippets={{ space: spaceChip }} />
    </p>
    {#if quickLookBound}
        <p>
            <Trans key="fileExplorer.quickLookHint.quickView" snippets={{ key: quickLookChip }} />
        </p>
    {/if}
    <p>
        <Trans key="fileExplorer.quickLookHint.enterOpens" snippets={{ enter: enterChip }} />
    </p>
    <p class="settings-line">
        <Trans key="fileExplorer.quickLookHint.configurable" snippets={{ settingsLink }} />
    </p>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleDontShowAgain}
            >{tString('fileExplorer.quickLookHint.dontShowAgain')}</Button
        >
    </ToastActions>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        /* Toasts default to a narrow column; give this one a bit more room so
           lines don't break awkwardly mid-sentence. */
        max-width: 28rem;
    }

    .settings-line {
        margin-top: var(--spacing-sm);
    }
</style>
