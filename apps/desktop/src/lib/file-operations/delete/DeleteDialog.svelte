<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import {
        formatBytes,
        startScanPreview,
        cancelScanPreview,
        onScanPreviewProgress,
        onScanPreviewComplete,
        onScanPreviewError,
        onScanPreviewCancelled,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { SortColumn, SortOrder } from '$lib/file-explorer/types'
    import { getSetting } from '$lib/settings'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import {
        generateDeleteTitle,
        abbreviatePath,
        getSymlinkNotice,
        MAX_VISIBLE_ITEMS,
        type DeleteSourceItem,
    } from './delete-dialog-utils'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('deleteDialog')

    interface Props {
        sourceItems: DeleteSourceItem[]
        sourcePaths: string[]
        sourceFolderPath: string
        isPermanent: boolean
        supportsTrash: boolean
        isFromCursor: boolean
        /** Current sort column on source pane (for scan preview ordering) */
        sortColumn: SortColumn
        /** Current sort order on source pane */
        sortOrder: SortOrder
        onConfirm: (previewId: string | null) => void
        onCancel: () => void
    }

    const {
        sourceItems,
        sourcePaths,
        sourceFolderPath,
        isPermanent: initialIsPermanent,
        supportsTrash,
        isFromCursor,
        sortColumn,
        sortOrder,
        onConfirm,
        onCancel,
    }: Props = $props()

    // Force permanent when trash not supported
    const isPermanent = $derived(initialIsPermanent || !supportsTrash)

    const dialogTitle = $derived(generateDeleteTitle(sourceItems, isFromCursor))
    const abbreviatedPath = $derived(abbreviatePath(sourceFolderPath))
    const symlinkNotice = $derived(getSymlinkNotice(sourceItems))

    const visibleItems = $derived(sourceItems.slice(0, MAX_VISIBLE_ITEMS))
    const overflowCount = $derived(Math.max(0, sourceItems.length - MAX_VISIBLE_ITEMS))

    const confirmLabel = $derived(isPermanent ? 'Delete permanently' : 'Move to trash')
    const confirmVariant = $derived<'primary' | 'danger'>(isPermanent ? 'danger' : 'primary')
    const dialogRole = $derived<'dialog' | 'alertdialog'>(isPermanent ? 'alertdialog' : 'dialog')

    // Scan preview state
    let previewId = $state<string | null>(null)
    let filesFound = $state(0)
    let dirsFound = $state(0)
    let bytesFound = $state(0)
    let isScanning = $state(false)
    let scanComplete = $state(false)
    let unlisteners: UnlistenFn[] = []

    /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
    function isOurScanEvent(eventPreviewId: string): boolean {
        if (!previewId) previewId = eventPreviewId
        return eventPreviewId === previewId
    }

    /** Starts the scan preview to count files/dirs/bytes. */
    async function startScan() {
        // Subscribe to events BEFORE starting scan (avoid missing fast completions)
        unlisteners.push(
            await onScanPreviewProgress((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesFound
                dirsFound = event.dirsFound
                bytesFound = event.bytesFound
            }),
        )
        unlisteners.push(
            await onScanPreviewComplete((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesTotal
                dirsFound = event.dirsTotal
                bytesFound = event.bytesTotal
                isScanning = false
                scanComplete = true
            }),
        )
        unlisteners.push(
            await onScanPreviewError((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
                // Keep showing whatever stats we have
            }),
        )
        unlisteners.push(
            await onScanPreviewCancelled((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
            }),
        )

        // Start the scan
        isScanning = true
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const result = await startScanPreview(sourcePaths, sortColumn, sortOrder, progressIntervalMs)
        previewId = result.previewId
    }

    function cleanup() {
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
    }

    onMount(() => {
        void startScan()
    })

    onDestroy(() => {
        if (previewId && isScanning) {
            void cancelScanPreview(previewId)
        }
        cleanup()
    })

    function handleConfirm() {
        log.info('Delete confirmed: isPermanent={isPermanent}, items={count}', {
            isPermanent,
            count: sourceItems.length,
        })
        onConfirm(previewId)
    }

    function handleCancel() {
        if (previewId && isScanning) {
            void cancelScanPreview(previewId)
        }
        cleanup()
        onCancel()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            handleConfirm()
        }
    }

    /** Formats item size for display. Folders show recursive info when available. */
    function formatItemSize(item: DeleteSourceItem): string {
        if (item.isDirectory) {
            const size = item.recursiveSize
            const fileCount = item.recursiveFileCount
            const parts: string[] = []
            if (size !== undefined) parts.push(formatFileSize(size))
            if (fileCount !== undefined) {
                parts.push(`${String(fileCount)} ${fileCount === 1 ? 'file' : 'files'}`)
            }
            return parts.length > 0 ? parts.join('   ') : ''
        }
        return item.size !== undefined ? formatFileSize(item.size) : ''
    }
</script>

<ModalDialog
    titleId="delete-dialog-title"
    onkeydown={handleKeydown}
    dialogId="delete-confirmation"
    role={dialogRole}
    onclose={handleCancel}
    ariaDescribedby={isPermanent ? 'delete-warning-text' : undefined}
    containerStyle="min-width: 420px; max-width: 500px"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <!-- Source path -->
    <div class="source-path">
        From: {abbreviatedPath}
    </div>

    <!-- No-trash warning banner -->
    {#if !supportsTrash}
        <div class="warning-banner" role="alert">
            <span class="warning-icon" aria-hidden="true">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M12 2L1 21h22L12 2z" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
                    <line
                        x1="12"
                        y1="9"
                        x2="12"
                        y2="15"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    />
                    <circle cx="12" cy="18" r="1" fill="currentColor" />
                </svg>
            </span>
            <p id="delete-warning-text">
                <strong>This volume doesn't support trash.</strong> Files will be permanently deleted.
            </p>
        </div>
    {/if}

    <!-- Scrollable file list -->
    <div class="file-list-container">
        <div class="file-list" role="list">
            {#each visibleItems as item (item.name)}
                <div class="file-list-item" role="listitem">
                    <span class="item-icon" aria-hidden="true">{item.isDirectory ? '\u25B8' : ''}</span>
                    <span class="item-name">{item.name}</span>
                    <span class="item-size">{formatItemSize(item)}</span>
                </div>
            {/each}
            {#if overflowCount > 0}
                <div class="file-list-overflow" role="listitem">
                    ... and {overflowCount} more {overflowCount === 1 ? 'item' : 'items'}
                </div>
            {/if}
        </div>
    </div>

    <!-- Symlink notice -->
    {#if symlinkNotice}
        <div class="symlink-notice">
            <span class="symlink-icon" aria-hidden="true">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M12 2L1 21h22L12 2z" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
                    <line
                        x1="12"
                        y1="9"
                        x2="12"
                        y2="15"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    />
                    <circle cx="12" cy="18" r="1" fill="currentColor" />
                </svg>
            </span>
            <span>{symlinkNotice}</span>
        </div>
    {/if}

    <!-- Scan stats (live counting) -->
    <div class="scan-stats">
        <div class="scan-stat">
            <span class="scan-value">{formatBytes(bytesFound)}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{filesFound}</span>
            <span class="scan-label">{filesFound === 1 ? 'file' : 'files'}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{dirsFound}</span>
            <span class="scan-label">{dirsFound === 1 ? 'dir' : 'dirs'}</span>
        </div>
        {#if isScanning}
            <span class="scan-spinner"></span>
        {:else if scanComplete}
            <span class="scan-checkmark">&#10003;</span>
        {/if}
    </div>

    <!-- Buttons -->
    <div class="button-row">
        <Button variant="secondary" onclick={handleCancel}>Cancel</Button>
        <Button variant={confirmVariant} onclick={handleConfirm}>{confirmLabel}</Button>
    </div>
</ModalDialog>

<style>
    .source-path {
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    /* No-trash warning banner */
    .warning-banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: 0 var(--spacing-xl) var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
    }

    .warning-icon {
        flex-shrink: 0;
        color: var(--color-warning);
        margin-top: 1px;
    }

    .warning-banner p {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    /* Scrollable file list */
    .file-list-container {
        margin: 0 var(--spacing-xl) var(--spacing-md);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .file-list {
        max-height: 250px;
        overflow-y: auto;
    }

    .file-list-item {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        padding: 4px var(--spacing-md);
        font-size: var(--font-size-sm);
        border-bottom: 1px solid var(--color-border);
    }

    .file-list-item:last-child {
        border-bottom: none;
    }

    .item-icon {
        flex-shrink: 0;
        width: 12px;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .item-name {
        flex: 1;
        color: var(--color-text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .item-size {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    .file-list-overflow {
        padding: 4px var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
        text-align: center;
    }

    /* Symlink notice */
    .symlink-notice {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-warning);
        line-height: 1.4;
    }

    .symlink-icon {
        flex-shrink: 0;
        margin-top: 1px;
    }

    /* Scan stats */
    .scan-stats {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    .scan-stat {
        display: flex;
        align-items: baseline;
        gap: 4px;
    }

    .scan-value {
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
        font-weight: 500;
    }

    .scan-label {
        color: var(--color-text-tertiary);
    }

    .scan-divider {
        color: var(--color-text-tertiary);
    }

    .scan-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-accent);
        border-top-color: transparent;
        border-radius: var(--radius-full);
        animation: spin 0.8s linear infinite;
        margin-left: 4px;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .scan-checkmark {
        color: var(--color-allow);
        font-size: var(--font-size-md);
        font-weight: bold;
        margin-left: 4px;
    }

    /* Buttons */
    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) 20px;
    }
</style>
