<script lang="ts">
    /**
     * A text field's history, for the pointer: its key, ⌥⇧↓, as a quiet chip at the far end of the
     * field's label row (named "History" for screen readers and in its tooltip), shown only while
     * the field is hovered or focused (the parent's `.field` rule), so the sheet stays calm. Just
     * the chip, so it fits beside the narrow extension mask's label too. Out of the Tab order: the keyboard has ⌥⇧↓. It goes after the input, so
     * the `<label>` around both still labels the input.
     */
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        onOpen: () => void
    }

    const { onOpen }: Props = $props()
</script>

<button
    type="button"
    class="history-hint"
    tabindex="-1"
    aria-label={tString('multiRename.history')}
    use:tooltip={{ text: tString('multiRename.history') }}
    onmousedown={(e: MouseEvent) => { e.preventDefault() }}
    onclick={onOpen}
>
    <span class="key" aria-hidden="true">
        <ShortcutChip commandId="multiRename.fieldHistory" clickable={false} size="sm" />
    </span>
</button>

<style>
    /* Level with the field's label, at its far end. */
    .history-hint {
        position: absolute;
        top: 0;
        right: 0;
        display: flex;
        align-items: center;
        padding: 0;
        border: none;
        background: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .history-hint:hover {
        color: var(--color-text-secondary);
    }

    .key {
        display: flex;
    }

    /* The key hint stays quiet, as the sheet's option keys do. */
    .key :global(.shortcut-chip) {
        color: inherit;
        background: transparent;
    }
</style>
