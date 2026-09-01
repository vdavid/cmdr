<script lang="ts">
    /**
     * A shortcut pill in the Keyboard shortcuts section: the chip of a
     * `KeyboardShortcutsSection` row and of `$lib/downloads/GlobalShortcutRow.svelte`, which
     * sits in the same list and has to look the same. Editable by default (a button that
     * starts recording); `readOnly` renders the same shape as a plain span for rows Cmdr
     * can't rebind. The prose-side sibling is `$lib/ui/ShortcutChip`, which only displays.
     */
    import type { Snippet } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        /** Shown, not editable: a plain span, no hover, no click (macOS-native and fixed-key rows). */
        readOnly?: boolean
        /** Recording a combo: accent fill. */
        editing?: boolean
        /**
         * Recording, and the captured combo conflicts: warning tint (also on hover) while the
         * banner awaits a decision, so it reads as "in question", not as a saved binding.
         */
        pendingConflict?: boolean
        /** An unbound slot: dashed border, muted text. */
        empty?: boolean
        onclick?: () => void
        /** Adds the hover-only × that removes this binding. Omit for slots that can't be removed. */
        remove?: { tooltip: string; onRemove: () => void }
        /** Test hook, rendered as `data-test`. */
        dataTest?: string
        children: Snippet
    }

    const { readOnly = false, editing = false, pendingConflict = false, empty = false, onclick, remove, dataTest, children }: Props =
        $props()

    function handleRemove(event: Event): void {
        // The × sits inside the pill button: stop the press from also starting a recording.
        event.stopPropagation()
        remove?.onRemove()
    }
</script>

{#if readOnly}
    <span class="shortcut-pill static">{@render children()}</span>
{:else}
    <button class="shortcut-pill" class:editing class:pending-conflict={pendingConflict} class:empty data-test={dataTest} {onclick}>
        {@render children()}
        {#if remove}
            <span
                class="remove-shortcut"
                use:tooltip={remove.tooltip}
                role="button"
                tabindex="-1"
                onclick={handleRemove}
                onkeydown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') handleRemove(e)
                }}>×</span
            >
        {/if}
    </button>
{/if}

<style>
    .shortcut-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-primary);
        cursor: default;
        min-width: 40px;
        text-align: center;
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .shortcut-pill.editing:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    .shortcut-pill.editing.pending-conflict,
    .shortcut-pill.editing.pending-conflict:hover {
        background: var(--color-warning-bg);
        color: var(--color-text-primary);
        border-color: var(--color-warning);
    }

    .shortcut-pill.empty {
        color: var(--color-text-tertiary);
        border-style: dashed;
    }

    .shortcut-pill.static {
        color: var(--color-text-secondary);
    }

    .remove-shortcut {
        width: 12px;
        height: 12px;
        border-radius: var(--radius-full);
        background: var(--color-text-tertiary);
        color: var(--color-bg-primary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        cursor: default;
        display: none;
        align-items: center;
        justify-content: center;
        line-height: var(--font-line-height-flat);
        flex-shrink: 0;
    }

    .shortcut-pill:hover .remove-shortcut {
        display: flex;
    }
</style>
