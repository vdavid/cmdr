<script lang="ts">
    interface Props {
        visible?: boolean
        onView?: () => void
        onEdit?: () => void
        onCopy?: () => void
        onMove?: () => void
        onNewFolder?: () => void
    }

    const { visible = true, onView, onEdit, onCopy, onMove, onNewFolder }: Props = $props()
</script>

{#if visible}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="function-key-bar"
        onmousedown={(e) => {
            e.preventDefault()
        }}
    >
        <button onclick={onView} tabindex={-1} aria-label="View file (F3)">
            <kbd>F3</kbd><span>View</span>
        </button>
        <button onclick={onEdit} tabindex={-1} aria-label="Edit file (F4)">
            <kbd>F4</kbd><span>Edit</span>
        </button>
        <button onclick={onCopy} tabindex={-1} aria-label="Copy (F5)">
            <kbd>F5</kbd><span>Copy</span>
        </button>
        <button onclick={onMove} tabindex={-1} aria-label="Move (F6)">
            <kbd>F6</kbd><span>Move</span>
        </button>
        <button onclick={onNewFolder} tabindex={-1} aria-label="New folder (F7)">
            <kbd>F7</kbd><span>New folder</span>
        </button>
        <button disabled tabindex={-1} aria-label="Delete (F8) — not yet available">
            <kbd>F8</kbd><span>Delete</span>
        </button>
    </div>
{/if}

<style>
    .function-key-bar {
        display: flex;
        flex-shrink: 0;
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-primary);
    }

    button {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        border: none;
        border-right: 1px solid var(--color-border-secondary);
        background: transparent;
        padding: var(--spacing-xs) 0;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-xs);
        cursor: pointer;
        color: var(--color-text-primary);
    }

    button:last-child {
        border-right: none;
    }

    button:hover:not(:disabled) {
        background: var(--color-button-hover);
    }

    button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    kbd {
        font-family: var(--font-system) sans-serif;
        font-size: 10px;
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        border-radius: 3px;
        padding: 1px 4px;
    }
</style>
