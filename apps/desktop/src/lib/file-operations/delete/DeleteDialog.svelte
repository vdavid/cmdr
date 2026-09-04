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
    import Switch from '$lib/ui/Switch.svelte'
    import {
        generateDeleteTitle,
        abbreviatePath,
        getSymlinkNotice,
        MAX_VISIBLE_ITEMS,
        type DeleteSourceItem,
    } from './delete-dialog-utils'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { formatFilesPerSecond } from '$lib/units'
    import Size from '$lib/ui/Size.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import { ScanThroughput } from '../scan-throughput'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Trans from '$lib/intl/Trans.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('deleteDialog')

    interface Props {
        sourceItems: DeleteSourceItem[]
        sourcePaths: string[]
        sourceFolderPath: string
        isPermanent: boolean
        supportsTrash: boolean
        /** Source is inside a zip: deletes are permanent (no Trash inside an archive). */
        isArchive?: boolean
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
        isArchive = false,
        isFromCursor,
        sortColumn,
        sortOrder,
        sourceVolumeId,
        autoConfirm = false,
        onConfirm,
        onCancel,
    }: Props = $props()

    // The switch's own position. Forced to permanent on volumes that don't support trash.
    let switchIsPermanent = $state(initialIsPermanent || !supportsTrash)
    /** True while Shift is down. Held on the window in the CAPTURE phase, so it also catches a
     *  release outside the dialog. See `SHIFT_LISTENER_PHASE`. */
    let shiftHeld = $state(false)
    /** Shift-hold only upgrades a dialog that opened as a trash: on a Shift+F8 dialog the user is
     *  still holding the key that opened it, so honouring the release would demote a delete they
     *  deliberately asked for. Snapshot at open; neither input changes for the dialog's lifetime. */
    const shiftUpgradesToPermanent = !initialIsPermanent && supportsTrash
    // Shift only ever upgrades: a hold can't demote a permanent delete back to a trash.
    const isPermanent = $derived(switchIsPermanent || (shiftUpgradesToPermanent && shiftHeld))

    const dialogTitle = $derived(generateDeleteTitle(sourceItems, isFromCursor))
    const abbreviatedPath = $derived(abbreviatePath(sourceFolderPath))
    const symlinkNotice = $derived(getSymlinkNotice(sourceItems))

    const visibleItems = $derived(sourceItems.slice(0, MAX_VISIBLE_ITEMS))
    const overflowCount = $derived(Math.max(0, sourceItems.length - MAX_VISIBLE_ITEMS))

    const confirmLabel = $derived(
        isPermanent
            ? tString('fileOperations.delete.confirmDelete')
            : tString('fileOperations.delete.confirmMoveToTrash'),
    )
    const confirmVariant = $derived<'primary' | 'danger'>(isPermanent ? 'danger' : 'primary')
    const dialogRole = $derived<'dialog' | 'alertdialog'>(isPermanent ? 'alertdialog' : 'dialog')
    // `delete-warning-text` only exists while a banner renders, so point
    // `aria-describedby` at the banner's condition, not at `isPermanent`.
    const hasWarningBanner = $derived(isArchive || !supportsTrash)

    // Scan preview state
    let previewId = $state<string | null>(null)
    /** Resolves once `startScanPreview` has answered and `previewId` is set.
     *  Confirm awaits it, for the same reason `TransferDialog` awaits
     *  `scan.scanStarted`: dispatching with a null `previewId` leaves the
     *  operation nothing to claim, so it re-walks the tree CONCURRENTLY with
     *  the preview `startScan` already began, and that orphaned walk has no
     *  owner and nothing to cancel it (teardown's cleanup is gated on
     *  `!confirmed`, and confirming sets that before the id ever arrives). */
    let scanStarted: Promise<void> = Promise.resolve()
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

    /** The walk's speed, through the one files-per-second policy the transfer
     *  bars use, so a slow scan reads "0.4 files/s" instead of the "0 files/s" a
     *  bare `Math.round` produced. `null` once it rounds to nothing, which is
     *  also what hides the line. */
    const scanRate = $derived(filesPerSec === null ? null : formatFilesPerSecond(filesPerSec))
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

    /** Any key event re-reads the modifier state, so a keyup we never saw (window switch,
     *  a native menu eating it) self-heals on the next keystroke instead of leaving the
     *  dialog stuck on "Delete permanently". */
    function syncShiftState(event: KeyboardEvent) {
        shiftHeld = event.shiftKey
    }

    /** Focus left the window: whatever Shift does now, we won't see it come back up. */
    function releaseShift() {
        shiftHeld = false
    }

    /** Capture, NOT bubble: `ModalDialog`'s overlay calls `stopPropagation()` on every keydown to
     *  shield the file explorer, and focus lives inside that overlay while we're open. A
     *  bubble-phase window listener sits downstream of it, so it would never see the Shift keydown
     *  at all. The capture phase runs on `window` first, before anything can stop the event. */
    const SHIFT_LISTENER_PHASE = true

    function watchShift() {
        window.addEventListener('keydown', syncShiftState, SHIFT_LISTENER_PHASE)
        window.addEventListener('keyup', syncShiftState, SHIFT_LISTENER_PHASE)
        window.addEventListener('blur', releaseShift)
    }

    function unwatchShift() {
        window.removeEventListener('keydown', syncShiftState, SHIFT_LISTENER_PHASE)
        window.removeEventListener('keyup', syncShiftState, SHIFT_LISTENER_PHASE)
        window.removeEventListener('blur', releaseShift)
    }

    function cleanup() {
        for (const unlisten of unlisteners) {
            unlisten()
        }
        unlisteners = []
        unwatchShift()
    }

    onMount(async () => {
        if (shiftUpgradesToPermanent) watchShift()
        scanStarted = startScan()

        // Auto-confirm if MCP requested it (after a tick so the dialog is fully initialized)
        if (autoConfirm) {
            await tick()
            void handleConfirm()
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

    async function handleConfirm() {
        confirmed = true
        log.info('Delete confirmed: isPermanent={isPermanent}, items={count}', {
            isPermanent,
            count: sourceItems.length,
        })
        // The scan-preview IPC only mints an id and spawns the walk, so it
        // answers promptly even on a wedged share. See `scanStarted`.
        await scanStarted
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
            void handleConfirm()
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
        return `${formatNumber(fileCount)} ${tString('fileOperations.delete.scanFile', { count: fileCount })}`
    }
</script>

<ModalDialog
    titleId="delete-dialog-title"
    onkeydown={handleKeydown}
    dialogId="delete-confirmation"
    role={dialogRole}
    onclose={handleCancel}
    ariaDescribedby={hasWarningBanner ? 'delete-warning-text' : undefined}
    containerStyle="width: 500px"
    resizable="horizontal"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <div class="dialog-body">
        <!-- Warning banner: archive deletes are permanent (no Trash inside a zip);
             other no-trash volumes get the generic banner. -->
        {#if isArchive}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <strong>{tString('fileOperations.delete.archiveWarningStrong')}</strong>
                    {tString('fileOperations.delete.archiveWarningRest')}
                </p>
            </div>
        {:else if !supportsTrash}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <strong>{tString('fileOperations.delete.noTrashWarningStrong')}</strong>
                    {tString('fileOperations.delete.noTrashWarningRest')}
                </p>
            </div>
        {/if}

        <!-- Source path. The tooltip is unconditional whenever `abbreviatePath` swapped a
             home directory for `~`, since then the line is short AND incomplete; otherwise
             it only steps in once the line runs out of room. -->
        <div class="source-path" use:tooltip={{ text: sourceFolderPath, overflowOnly: abbreviatedPath === sourceFolderPath }}>
            {tString('fileOperations.delete.fromPath', { path: abbreviatedPath })}
        </div>

        <!-- Scrollable file list -->
        <div class="file-list-container">
            <div class="file-list" role="list">
                {#each visibleItems as item, index (item.name)}
                    <div class="file-list-item" role="listitem">
                        <span class="item-icon" aria-hidden="true">
                            <Icon name={item.isDirectory ? 'folder' : 'file'} size={14} />
                        </span>
                        <!-- `sourcePaths` is index-aligned with `sourceItems` at both call
                             sites, so the row's own full path is what hovering reveals. -->
                        <span
                            class="item-name"
                            use:tooltip={{ text: sourcePaths[index] ?? item.name, overflowOnly: true }}>{item.name}</span
                        >
                        <span class="item-size">
                            {#if itemSizeBytes(item) != null}<Size bytes={itemSizeBytes(item)} />{/if}
                            {#if itemFileCountLabel(item)}{#if itemSizeBytes(item) != null}&nbsp;&nbsp;&nbsp;{/if}{itemFileCountLabel(
                                    item,
                                )}{/if}
                        </span>
                    </div>
                {/each}
                {#if overflowCount > 0}
                    <div class="file-list-overflow" role="listitem">
                        {t('fileOperations.delete.overflowMore', {
                            countText: formatNumber(overflowCount),
                            count: overflowCount,
                        })}
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

        <!-- Scan stats (live counting). `data-scan-state` is the race-free
             "counting done" marker for E2E; there's no visual completion badge. -->
        <div class="scan-stats" data-scan-state={scanComplete ? 'done' : 'counting'}>
            <div class="scan-stat">
                <span class="scan-value"><Size bytes={bytesFound} /></span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(filesFound)}</span>
                <span class="scan-label">{t('fileOperations.delete.scanFile', { count: filesFound })}</span>
            </div>
            <span class="scan-divider">/</span>
            <div class="scan-stat">
                <span class="scan-value">{formatNumber(dirsFound)}</span>
                <span class="scan-label">{t('fileOperations.delete.scanDir', { count: dirsFound })}</span>
            </div>
            {#if isScanning}
                <span
                    class="scan-status"
                    role="img"
                    aria-label={tString('fileOperations.shared.scanningTooltip')}
                    use:tooltip={{ text: tString('fileOperations.shared.scanningTooltip') }}
                >
                    <Spinner size="sm" />
                </span>
            {/if}
        </div>

        <!-- Throughput -->
        {#if isScanning && scanRate !== null}
            <div class="scan-throughput">
                <span class="scan-throughput-value"
                    >{tString('fileOperations.shared.fileRate', {
                        count: scanRate.value,
                        rateText: scanRate.text,
                    })}</span
                >
                {#if bytesPerSec !== null && bytesPerSec > 0}
                    <span class="scan-throughput-sep">·</span>
                    <span class="scan-throughput-value"
                        ><Trans key="fileOperations.shared.byteRate" snippets={{ size }} /></span
                    >
                {/if}
            </div>
        {/if}

        <!-- Current directory being scanned -->
        {#if isScanning && currentDir}
            <div class="scan-current-dir" use:useShortenMiddle={{ text: currentDir, preferBreakAt: '/' }}></div>
        {/if}

    </div>

    <!-- Trash (on, the safe default) vs. permanent delete (off). Rides the footer
         row so it reads as a modifier on the confirm button beside it. Holding Shift
         flips it too, for as long as the key is down. -->
    {#snippet footerLeading()}
        {#if supportsTrash}
            <Switch checked={!isPermanent} onCheckedChange={(toTrash) => (switchIsPermanent = !toTrash)}
                >{tString('fileOperations.delete.trashSwitch')}</Switch
            >
        {/if}
    {/snippet}

    {#snippet footer()}
        <Button variant="secondary" onclick={handleCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant={confirmVariant} onclick={handleConfirm}>{confirmLabel}</Button>
    {/snippet}
</ModalDialog>

{#snippet size(children: import('svelte').Snippet)}<Size bytes={bytesPerSec ?? 0} />{@render children()}{/snippet}

<style>
    /* Uniform vertical rhythm: every section is a flex-column child, so a single
       `gap` sets equal spacing between all of them. The side inset is `ModalDialog`'s. */
    .dialog-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .source-path {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    /* No-trash warning banner */
    .warning-banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
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
    }

    /* Scrollable file list */
    .file-list-container {
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
        align-items: center;
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
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        color: var(--color-text-tertiary);
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
    }

    /* Symlink notice */
    .symlink-notice {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-warning);
    }

    .symlink-icon {
        flex-shrink: 0;
        margin-top: 1px;
    }

    /* Scan stats. Right-aligned so the tallies sit under the dialog's right edge
       and don't compete with the left-aligned labels above them. */
    .scan-stats {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-sm);
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

    .scan-status {
        display: inline-flex;
        align-items: center;
    }

    .scan-throughput {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-xs);
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
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

</style>
