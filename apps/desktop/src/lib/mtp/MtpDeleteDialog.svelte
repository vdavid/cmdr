<script lang="ts">
    /**
     * Dialog to confirm deletion of files/folders on an MTP device.
     */
    import { onMount, tick } from 'svelte'

    interface Props {
        /** Names of items to delete */
        itemNames: string[]
        /** Total count of files (excluding directories) */
        fileCount: number
        /** Total count of directories */
        folderCount: number
        /** Callback when user confirms deletion */
        onConfirm: () => void
        /** Callback when user cancels */
        onCancel: () => void
    }

    const { itemNames, fileCount, folderCount, onConfirm, onCancel }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()

    const totalCount = $derived(fileCount + folderCount)

    function getSummaryText(): string {
        if (totalCount === 1) {
            return itemNames[0]
        }
        const parts: string[] = []
        if (folderCount > 0) {
            parts.push(`${String(folderCount)} folder${folderCount > 1 ? 's' : ''}`)
        }
        if (fileCount > 0) {
            parts.push(`${String(fileCount)} file${fileCount > 1 ? 's' : ''}`)
        }
        return parts.join(' and ')
    }

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape') {
            onCancel()
        } else if (event.key === 'Enter') {
            onConfirm()
        }
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="delete-dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="delete-dialog">
        <h2 id="delete-dialog-title">Delete from device?</h2>
        <p class="message">
            {#if totalCount === 1}
                Delete <strong>{getSummaryText()}</strong> from your device?
            {:else}
                Delete <strong>{getSummaryText()}</strong> from your device?
            {/if}
        </p>
        <p class="warning">This can't be undone.</p>

        <div class="button-row">
            <button class="secondary" onclick={onCancel}>Cancel</button>
            <button class="danger" onclick={onConfirm}>Delete</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }

    .delete-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        width: 360px;
        padding: 20px 24px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 12px;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }

    .message {
        margin: 0 0 8px;
        font-size: 13px;
        color: var(--color-text-secondary);
        text-align: center;
        word-break: break-word;
    }

    .message strong {
        color: var(--color-text-primary);
    }

    .warning {
        margin: 0 0 16px;
        font-size: 12px;
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
    }

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    .danger {
        background: var(--color-error);
        color: white;
        border: none;
    }

    .danger:hover {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
