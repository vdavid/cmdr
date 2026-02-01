<script lang="ts">
    /**
     * Dialog for copying files between MTP device and local filesystem.
     * Handles both download (MTP -> local) and upload (local -> MTP).
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import {
        downloadMtpFile,
        uploadToMtp,
        onMtpTransferProgress,
        type MtpTransferProgress,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { FileEntry } from '$lib/file-explorer/types'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('mtp')

    interface Props {
        /** Type of copy operation */
        operationType: 'download' | 'upload'
        /** Source files to copy (MTP entries for download, local paths for upload) */
        sourceFiles: FileEntry[] | string[]
        /** Destination path (local path for download, MTP path for upload) */
        destinationPath: string
        /** MTP device ID */
        deviceId: string
        /** MTP storage ID */
        storageId: number
        /** Current MTP path (for getting inner paths) */
        mtpBasePath: string
        /** Callback when copy completes successfully */
        onComplete: (filesProcessed: number, bytesTransferred: number) => void
        /** Callback when copy is cancelled */
        onCancel: () => void
        /** Callback when copy fails */
        onError: (error: string) => void
    }

    const {
        operationType,
        sourceFiles,
        destinationPath,
        deviceId,
        storageId,
        mtpBasePath,
        onComplete,
        onCancel,
        onError,
    }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let currentFile = $state('')
    let bytesDone = $state(0)
    let bytesTotal = $state(0)
    let itemsDone = $state(0)
    let isRunning = $state(true)
    let unlistenProgress: UnlistenFn | undefined
    let abortController: AbortController | undefined

    const totalItems = $derived(sourceFiles.length)
    const progress = $derived(bytesTotal > 0 ? (bytesDone / bytesTotal) * 100 : 0)

    function formatBytes(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }

    function getTitle(): string {
        if (operationType === 'download') {
            return `Copying from device (${String(itemsDone)} of ${String(totalItems)})`
        }
        return `Copying to device (${String(itemsDone)} of ${String(totalItems)})`
    }

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape') {
            handleCancel()
        }
    }

    function handleCancel() {
        isRunning = false
        abortController?.abort()
        onCancel()
    }

    async function startTransfer() {
        abortController = new AbortController()

        try {
            // Set up progress listener
            unlistenProgress = await onMtpTransferProgress((progress: MtpTransferProgress) => {
                currentFile = progress.currentFile
                bytesDone = progress.bytesDone
                bytesTotal = progress.bytesTotal
            })

            let totalBytesTransferred = 0

            if (operationType === 'download') {
                // Download from MTP to local
                for (const source of sourceFiles as FileEntry[]) {
                    if (!isRunning) break

                    const operationId = crypto.randomUUID()
                    const innerPath = mtpBasePath ? `${mtpBasePath}/${source.name}` : source.name
                    const localPath = `${destinationPath}/${source.name}`

                    currentFile = source.name

                    const result = await downloadMtpFile(deviceId, storageId, innerPath, localPath, operationId)
                    totalBytesTransferred += result.bytesTransferred
                    itemsDone++
                    log.info('Downloaded: {file}', { file: source.name })
                }
            } else {
                // Upload from local to MTP
                for (const localPath of sourceFiles as string[]) {
                    if (!isRunning) break

                    const operationId = crypto.randomUUID()
                    const fileName = localPath.split('/').pop() || localPath
                    currentFile = fileName

                    await uploadToMtp(deviceId, storageId, localPath, mtpBasePath, operationId)
                    itemsDone++
                    log.info('Uploaded: {file}', { file: fileName })
                }
            }

            if (isRunning) {
                onComplete(itemsDone, totalBytesTransferred)
            }
        } catch (e) {
            if (isRunning) {
                const errorMessage = e instanceof Error ? e.message : String(e)
                log.error('Transfer failed: {error}', { error: errorMessage })
                onError(errorMessage)
            }
        }
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()
        void startTransfer()
    })

    onDestroy(() => {
        isRunning = false
        unlistenProgress?.()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="copy-dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="copy-dialog">
        <h2 id="copy-dialog-title">{getTitle()}</h2>

        <div class="progress-info">
            <p class="current-file" title={currentFile}>{currentFile || 'Starting...'}</p>
            <div class="progress-bar-container">
                <div class="progress-bar" style="width: {progress}%"></div>
            </div>
            <p class="progress-text">
                {formatBytes(bytesDone)} / {formatBytes(bytesTotal)}
            </p>
        </div>

        <div class="button-row">
            <button class="secondary" onclick={handleCancel}>Cancel</button>
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

    .copy-dialog {
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
