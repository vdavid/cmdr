<script lang="ts">
    interface Props {
        visible?: boolean
        onRename?: () => void
        onView?: () => void
        onEdit?: () => void
        onCopy?: () => void
        onMove?: () => void
        onNewFolder?: () => void
    }

    const { visible = true, onRename, onView, onEdit, onCopy, onMove, onNewFolder }: Props = $props()

    let shiftHeld = $state(false)

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            shiftHeld = true
        }
    }

    function handleKeyUp(e: KeyboardEvent) {
        if (e.key === 'Shift') {
            shiftHeld = false
        }
    }
</script>

<svelte:document onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

{#if visible}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="function-key-bar"
        onmousedown={(e) => {
            e.preventDefault()
        }}
    >
        {#if shiftHeld}
            <button disabled tabindex={-1} aria-label="F2 — no shift action">
                <kbd>F2</kbd>
            </button>
            <button disabled tabindex={-1} aria-label="F3 — no shift action">
                <kbd>F3</kbd>
            </button>
            <button disabled tabindex={-1} aria-label="F4 — no shift action">
                <kbd>F4</kbd>
            </button>
            <button disabled tabindex={-1} aria-label="F5 — no shift action">
                <kbd>F5</kbd>
            </button>
            <button onclick={onRename} tabindex={-1} aria-label="Rename (Shift+F6)">
                <kbd>⇧F6</kbd><span>Rename</span>
            </button>
            <button disabled tabindex={-1} aria-label="F7 — no shift action">
                <kbd>F7</kbd>
            </button>
            <button disabled tabindex={-1} aria-label="F8 — no shift action">
                <kbd>F8</kbd>
            </button>
        {:else}
            <button onclick={onRename} tabindex={-1} aria-label="Rename (F2)">
                <kbd>F2</kbd><span>Rename</span>
            </button>
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
        {/if}
    </div>
{/if}

<style>
    .function-key-bar {
        display: flex;
        flex-shrink: 0;
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-strong);
    }

    button {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        border: none;
        border-right: 1px solid var(--color-border-subtle);
        background: transparent;
        padding: var(--spacing-xs) 0;
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        cursor: pointer;
        color: var(--color-text-primary);
    }

    button:last-child {
        border-right: none;
    }

    button:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
    }

    button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    kbd {
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        padding: 1px 4px;
    }
</style>
