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
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
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

<!-- Inline-chip/link snippets for the <Trans> lines. The tag names (space, key,
     enter, settingsLink) intentionally differ from any interpolation param. -->
{#snippet spaceChip(_children: Snippet)}<ShortcutChip key="Space" />{/snippet}
{#snippet quickLookChip(_children: Snippet)}<ShortcutChip key={quickLookKey} />{/snippet}
{#snippet enterChip(_children: Snippet)}<ShortcutChip key="Enter" />{/snippet}
{#snippet settingsLink(children: Snippet)}<LinkButton onclick={handleOpenSettings}>{@render children()}</LinkButton>{/snippet}

<div class="content">
    <p>
        <Trans key="fileExplorer.quickLookHint.spaceSelects" snippets={{ space: spaceChip }} />
    </p>
    <p>
        <Trans key="fileExplorer.quickLookHint.finderComparison" snippets={{ space: spaceChip }} />
    </p>
    <p>
        <Trans key="fileExplorer.quickLookHint.quickView" snippets={{ key: quickLookChip }} />
    </p>
    <p>
        <Trans key="fileExplorer.quickLookHint.enterOpens" snippets={{ enter: enterChip }} />
    </p>
    <p class="settings-line">
        <Trans key="fileExplorer.quickLookHint.configurable" snippets={{ settingsLink }} />
    </p>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDontShowAgain}
            >{tString('fileExplorer.quickLookHint.dontShowAgain')}</Button
        >
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
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
