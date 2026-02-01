<script lang="ts">
    /**
     * Progress dialog for MTP file operations (download/upload).
     * Shows progress and allows cancellation.
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import { onMtpTransferProgress, type MtpTransferProgress, type UnlistenFn } from '$lib/tauri-commands'

    interface Props {
        /** Unique operation ID for tracking */
        operationId: string
        /** Type of operation */
        operationType: 'download' | 'upload' | 'delete'
        /** Total number of items */
        totalItems: number
        /** Callback when operation completes */
        onComplete: () => void
        /** Callback when user cancels */
        onCancel: () => void
        /** Callback when operation fails */
        onError: (error: string) => void
    }

    // Note: onComplete and onError are part of the interface but currently unused.
    // The parent component handles completion/error through its own state management.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { operationId, operationType, totalItems, onComplete, onCancel, onError }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let currentFile = $state('')
    let bytesDone = $state(0)
    let bytesTotal = $state(0)
    const itemsDone = $state(0)
    let unlistenProgress: UnlistenFn | undefined

    const progress = $derived(bytesTotal > 0 ? (bytesDone / bytesTotal) * 100 : 0)

    function formatBytes(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }

    function getOperationTitle(): string {
        switch (operationType) {
            case 'download':
                return 'Downloading from device'
            case 'upload':
                return 'Uploading to device'
            case 'delete':
                return 'Deleting from device'
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape') {
            onCancel()
        }
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()

        // Subscribe to transfer progress events
        unlistenProgress = await onMtpTransferProgress((progress: MtpTransferProgress) => {
            if (progress.operationId === operationId) {
                currentFile = progress.currentFile
                bytesDone = progress.bytesDone
                bytesTotal = progress.bytesTotal
            }
        })
    })

    onDestroy(() => {
        unlistenProgress?.()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="progress-dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="progress-dialog">
        <h2 id="progress-dialog-title">{getOperationTitle()}</h2>

        <div class="progress-info">
            {#if operationType === 'delete'}
                <p class="status">Deleting {String(itemsDone)} of {String(totalItems)} items...</p>
            {:else}
                <p class="current-file" title={currentFile}>{currentFile || 'Starting...'}</p>
                <div class="progress-bar-container">
                    <div class="progress-bar" style="width: {progress}%"></div>
                </div>
                <p class="progress-text">
                    {formatBytes(bytesDone)} / {formatBytes(bytesTotal)}
                    {#if totalItems > 1}
                        ({String(itemsDone + 1)} of {String(totalItems)} items)
                    {/if}
                </p>
            {/if}
        </div>

        <div class="button-row">
            <button class="secondary" onclick={onCancel}>Cancel</button>
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

    .progress-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        width: 400px;
        padding: 20px 24px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 16px;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }

    .progress-info {
        margin-bottom: 16px;
    }

    .status {
        margin: 0;
        font-size: 13px;
        color: var(--color-text-secondary);
        text-align: center;
    }

    .current-file {
        margin: 0 0 8px;
        font-size: 13px;
        color: var(--color-text-primary);
        text-align: center;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .progress-bar-container {
        height: 8px;
        background: var(--color-bg-tertiary);
        border-radius: 4px;
        overflow: hidden;
        margin-bottom: 8px;
    }

    .progress-bar {
        height: 100%;
        background: var(--color-accent);
        border-radius: 4px;
        transition: width 0.2s ease;
    }

    .progress-text {
        margin: 0;
        font-size: 12px;
        color: var(--color-text-secondary);
        text-align: center;
    }

    .button-row {
        display: flex;
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
