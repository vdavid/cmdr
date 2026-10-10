<script lang="ts">
    /**
     * The chevron at a Multi-Rename text field's end: it opens the field's history (what it held
     * in earlier renames), as a combobox's trigger opens its list. The same 16 px tertiary glyph
     * as `Combobox`'s, in the field's trailing slot, so it reads as the field's own affordance
     * and stays apart from the small ▾ markers under `[C…]` tokens. Disabled while the field has
     * no history. Out of the Tab order: in the field, ↓ opens the same list.
     */
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The field has no history yet. */
        disabled: boolean
        /** The list is open under this field. */
        expanded: boolean
        onOpen: () => void
    }

    const { disabled, expanded, onOpen }: Props = $props()
</script>

<button
    type="button"
    class="history-chevron"
    tabindex="-1"
    aria-label={tString('multiRename.history')}
    aria-haspopup="menu"
    aria-expanded={expanded}
    {disabled}
    onmousedown={(e: MouseEvent) => { e.preventDefault() }}
    onclick={onOpen}
>
    <Icon name="chevron-down" size={16} aria-hidden="true" />
</button>

<style>
    .history-chevron {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        align-self: stretch;
        padding: 0;
        border: none;
        background: transparent;
        color: var(--color-text-tertiary);
    }

    .history-chevron:hover:not(:disabled) {
        color: var(--color-text-secondary);
    }

    .history-chevron:disabled {
        opacity: 0.5;
    }
</style>
