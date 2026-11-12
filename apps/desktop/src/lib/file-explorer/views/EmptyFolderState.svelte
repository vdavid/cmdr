<script lang="ts">
    /**
     * The empty state both file lists lay over their row area: "Empty folder", or how
     * many hidden items the folder holds and how to reveal them (`empty-folder-text.ts`).
     *
     * ❗ The host places it as a SIBLING of its listbox, never a child: the text is not an
     * option, and a listbox holding a non-option child is an aria-required-children
     * violation. Its host's `.row-area` is the positioned box it fills.
     */
    import { getFirstShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { emptyFolderParagraphs } from './empty-folder-text'

    interface Props {
        /** Hidden items the pane hides while it shows no rows (zero for a truly empty folder). */
        hiddenCount: number
    }

    const { hiddenCount }: Props = $props()

    const paragraphs = $derived(emptyFolderParagraphs(hiddenCount, getFirstShortcutReactive('view.showHidden')))
</script>

<div class="empty-folder-state">
    {#each paragraphs as paragraph, i (i)}
        <p>{paragraph}</p>
    {/each}
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
