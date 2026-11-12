<script lang="ts">
    /**
     * The empty state both file lists lay over their row area: "Empty folder", or how
     * many hidden items the folder holds and how to reveal them (`empty-folder-text.ts`).
     * The `view.showHidden` key is a live `ShortcutChip`, so its glyphs (`⌘⇧.`) never read
     * as the sentence's punctuation; with the command unbound the hint names only the menu.
     *
     * ❗ The host places it as a SIBLING of its listbox, never a child: the text is not an
     * option, and a listbox holding a non-option child is an aria-required-children
     * violation. Its host's `.row-area` is the positioned box it fills.
     */
    import type { Snippet } from 'svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import { commandHasShortcut } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { hiddenItemsText, showHiddenMenuPath } from './empty-folder-text'

    interface Props {
        /** Hidden items the pane hides while it shows no rows (zero for a truly empty folder). */
        hiddenCount: number
    }

    const { hiddenCount }: Props = $props()

    const shortcutBound = $derived(commandHasShortcut('view.showHidden'))
</script>

<!-- Non-clickable: the overlay takes no pointer events, and a focusable chip in it would be a stray tab stop. -->
{#snippet showHiddenChip(_children: Snippet)}<ShortcutChip commandId="view.showHidden" clickable={false} />{/snippet}

<div class="empty-folder-state">
    {#if hiddenCount <= 0}
        <p>{tString('fileExplorer.list.empty')}</p>
    {:else}
        <p>{hiddenItemsText(hiddenCount)}</p>
        {#if shortcutBound}
            <p>
                <Trans
                    key="fileExplorer.list.hiddenItemsHowToShortcut"
                    snippets={{ key: showHiddenChip }}
                    params={showHiddenMenuPath()}
                />
            </p>
        {:else}
            <p>{tString('fileExplorer.list.hiddenItemsHowToMenu', showHiddenMenuPath())}</p>
        {/if}
    {/if}
</div>

<style>
    .empty-folder-state {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-lg);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        text-align: center;
        pointer-events: none;
    }

    p {
        margin: 0;
        max-width: 36em;
    }
</style>
