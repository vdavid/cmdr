<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import {
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
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import Size from '$lib/ui/Size.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { ScanThroughput } from '../scan-throughput'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'

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
        /** Source volume ID. Routes the scan preview through the Volume trait
         *  (`run_volume_scan_preview`) for non-local volumes like MTP, so the
         *  confirmation dialog gets a live climbing tally instead of a silently
         *  failed local-FS walk. */
        sourceVolumeId: string
        /** When true, dialog auto-confirms without user interaction (MCP). */
        autoConfirm?: boolean
        onConfirm: (previewId: string | null, isPermanent: boolean) => void
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
        sourceVolumeId,
        autoConfirm = false,
        onConfirm,
        onCancel,
    }: Props = $props()

    // User-facing toggle. Forced to permanent on volumes that don't support trash.
    let isPermanent = $state(initialIsPermanent || !supportsTrash)

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
    // True once the user confirms. On confirm the delete/trash op (or the
    // progress dialog) takes over the same scan and consumes the cached result,
    // so teardown must NOT free it then.
    let confirmed = false
    let filesFound = $state(0)
    let dirsFound = $state(0)
    let bytesFound = $state(0)
    let isScanning = $state(false)
    let scanComplete = $state(false)
    let currentDir = $state<string | null>(null)
    const throughput = new ScanThroughput()
    let filesPerSec = $state<number | null>(null)
    let bytesPerSec = $state<number | null>(null)
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
                currentDir = event.currentDir ?? null
                const r = throughput.push({
                    timestampMs: Date.now(),
                    files: event.filesFound,
                    bytes: event.bytesFound,
                })
                filesPerSec = r.filesPerSecond
                bytesPerSec = r.bytesPerSecond
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
        const result = await startScanPreview(sourcePaths, sortColumn, sortOrder, progressIntervalMs, sourceVolumeId)
        previewId = result.previewId
    }

    function cleanup() {
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
    }

    onMount(async () => {
        void startScan()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            handleConfirm()
        }
    })

    onDestroy(() => {
        // Free the scan preview unless the user confirmed (the op then consumes
        // the cached result). Regardless of `isScanning`: `cancelScanPreview`
        // also evicts the cached `CachedScanResult`, so a dismiss AFTER the scan
        // completed doesn't leak the cache until quit.
        if (previewId && !confirmed) {
            void cancelScanPreview(previewId)
        }
        cleanup()
    })

    function handleConfirm() {
        confirmed = true
        log.info('Delete confirmed: isPermanent={isPermanent}, items={count}', {
            isPermanent,
            count: sourceItems.length,
        })
        onConfirm(previewId, isPermanent)
    }

    function handleCancel() {
        // Free the scan preview (cancels an in-flight scan and evicts any cached
        // result). Regardless of `isScanning`.
        if (previewId) {
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

    /** Formats item size for display. Folders show recursive info when available.
     *  Always uses logical (content) sizes (not worth plumbing the display mode setting
     *  through the delete dialog infrastructure for a transient confirmation dialog). */
    function itemSizeBytes(item: DeleteSourceItem): number | null {
        // Group A wire-format: IPC sends `null` for absent fields, not `undefined`.
        return item.isDirectory ? (item.recursiveSize ?? null) : (item.size ?? null)
    }

    function itemFileCountLabel(item: DeleteSourceItem): string {
        if (!item.isDirectory) return ''
        const fileCount = item.recursiveFileCount
        if (fileCount == null) return ''
        return `${formatNumber(fileCount)} ${fileCount === 1 ? 'file' : 'files'}`
    }
</script>

<ModalDialog
    titleId="delete-dialog-title"
    onkeydown={handleKeydown}
    dialogId="delete-confirmation"
    role={dialogRole}
    onclose={handleCancel}
    ariaDescribedby={isPermanent ? 'delete-warning-text' : undefined}
    containerStyle="width: 500px"
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
                <Icon name="triangle-alert" size={18} />
            </span>
            <p id="delete-warning-text">
                <strong>This volume doesn't support trash.</strong> Files will be permanently deleted.
            </p>
        </div>
    {/if}

    <!-- Trash/Delete toggle -->
    {#if supportsTrash}
        <div class="operation-toggle">
            <button
                class="toggle-option"
                class:active={!isPermanent}
                onclick={() => (isPermanent = false)}>Trash</button
            >
            <button
                class="toggle-option toggle-option-danger"
                class:active={isPermanent}
                onclick={() => (isPermanent = true)}>Delete</button
            >
        </div>
    {/if}

    <!-- Scrollable file list -->
    <div class="file-list-container">
        <div class="file-list" role="list">
            {#each visibleItems as item (item.name)}
                <div class="file-list-item" role="listitem">
                    <span class="item-icon" aria-hidden="true">{item.isDirectory ? '\u25B8' : ''}</span>
                    <span class="item-name">{item.name}</span>
                    <span class="item-size">
                        {#if itemSizeBytes(item) != null}<Size bytes={itemSizeBytes(item)} />{/if}
                        {#if itemFileCountLabel(item)}{#if itemSizeBytes(item) != null}&nbsp;&nbsp;&nbsp;{/if}{itemFileCountLabel(item)}{/if}
                    </span>
                </div>
            {/each}
            {#if overflowCount > 0}
                <div class="file-list-overflow" role="listitem">
                    ... and {formatNumber(overflowCount)} more {overflowCount === 1 ? 'item' : 'items'}
                </div>
            {/if}
        </div>
    </div>

    <!-- Symlink notice -->
    {#if symlinkNotice}
        <div class="symlink-notice">
            <span class="symlink-icon" aria-hidden="true">
                <Icon name="triangle-alert" size={14} />
            </span>
            <span>{symlinkNotice}</span>
        </div>
    {/if}

    <!-- Scan stats (live counting) -->
    <div class="scan-stats">
        <div class="scan-stat">
            <span class="scan-value"><Size bytes={bytesFound} /></span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{formatNumber(filesFound)}</span>
            <span class="scan-label">{filesFound === 1 ? 'file' : 'files'}</span>
        </div>
        <span class="scan-divider">/</span>
        <div class="scan-stat">
            <span class="scan-value">{formatNumber(dirsFound)}</span>
            <span class="scan-label">{dirsFound === 1 ? 'dir' : 'dirs'}</span>
        </div>
        {#if isScanning}
            <Spinner size="sm" />
        {:else if scanComplete}
            <span class="scan-checkmark">&#10003;</span>
        {/if}
    </div>

    <!-- Throughput -->
    {#if isScanning && filesPerSec !== null && filesPerSec > 0}
        <div class="scan-throughput">
            <span class="scan-throughput-value">{formatNumber(Math.round(filesPerSec))} files/s</span>
            {#if bytesPerSec !== null && bytesPerSec > 0}
                <span class="scan-throughput-sep">·</span>
                <span class="scan-throughput-value"><Size bytes={bytesPerSec} />/s</span>
            {/if}
        </div>
    {/if}

    <!-- Current directory being scanned -->
    {#if isScanning && currentDir}
        <div class="scan-current-dir" use:useShortenMiddle={{ text: currentDir, preferBreakAt: '/' }}></div>
    {/if}

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
        padding: var(--spacing-xs) var(--spacing-md);
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
        padding: var(--spacing-xs) var(--spacing-md);
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
        gap: var(--spacing-xs);
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

    .scan-checkmark {
        color: var(--color-allow);
        font-size: var(--font-size-md);
        font-weight: 600;
        margin-left: var(--spacing-xs);
    }

    .scan-throughput {
        display: flex;
        justify-content: center;
        gap: var(--spacing-xs);
        padding: 0 var(--spacing-xl);
        margin-bottom: var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .scan-throughput-value {
        font-variant-numeric: tabular-nums;
    }

    .scan-throughput-sep {
        opacity: 0.6;
    }

    .scan-current-dir {
        padding: var(--spacing-xs) var(--spacing-md);
        margin: 0 var(--spacing-xl) var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    /* Buttons */
    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    /* Trash/Delete segmented control */
    .operation-toggle {
        display: flex;
        justify-content: center;
        gap: 0;
        padding: 0 var(--spacing-xl) var(--spacing-md);
    }

    .toggle-option {
        padding: var(--spacing-xs) var(--spacing-lg);
        font-size: var(--font-size-sm);
        font-weight: 500;
        border: 1px solid var(--color-border-strong);
        background: transparent;
        color: var(--color-text-secondary);
        transition: all var(--transition-base);
        min-width: 60px;
    }

    .toggle-option:first-child {
        border-radius: var(--radius-md) 0 0 var(--radius-md);
        border-right: none;
    }

    .toggle-option:last-child {
        border-radius: 0 var(--radius-md) var(--radius-md) 0;
    }

    .toggle-option.active {
        background: var(--color-accent);
        border-color: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .toggle-option-danger.active {
        background: var(--color-error-bg);
        border-color: var(--color-error);
        color: var(--color-error-text);
    }

    .toggle-option:not(.active):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
