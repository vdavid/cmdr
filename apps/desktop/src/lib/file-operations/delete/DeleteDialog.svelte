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
        type CloudOnlineOnlyExtent,
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
    import S3CostLine from '../S3CostLine.svelte'
    import { costRequestFor } from '../s3-cost-line'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import { withTimeout } from '$lib/utils/timing'
    import Trans from '$lib/intl/Trans.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'
    import type { DeleteConfirmer } from '$lib/file-explorer/pane/dialog-props'

    const log = getAppLogger('deleteDialog')

    interface Props {
        sourceItems: DeleteSourceItem[]
        sourcePaths: string[]
        sourceFolderPath: string
        isPermanent: boolean
        supportsTrash: boolean
        /** Source is inside a zip: deletes are permanent (no Trash inside an archive). */
        isArchive?: boolean
        /** A selected item in a cloud-storage folder is online-only, so the trash was
         *  routed here as a permanent delete rather than downloading it. Swaps the
         *  no-trash banner for one that says so, worded for how much of the selection
         *  is online-only. `null` when none of it is. */
        cloudOnlineOnly?: CloudOnlineOnlyExtent | null
        /** A selected FOLDER sits in a cloud drive, so the scan walk below may still
         *  turn up an online-only file and flip this dialog. Confirm waits for that
         *  answer; see `onlineOnlyAnswer`. */
        cloudFolderMayHoldOnlineOnly?: boolean
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
        /** Takes this dialog's own confirm for as long as it's mounted, so an MCP
         *  `dialog confirm` presses the same button a person does. Returns the
         *  unregister. */
        registerConfirmer?: (confirm: DeleteConfirmer) => () => void
        onCancel: () => void
    }

    const {
        sourceItems,
        sourcePaths,
        sourceFolderPath,
        isPermanent: initialIsPermanent,
        supportsTrash,
        isArchive = false,
        cloudOnlineOnly = null,
        cloudFolderMayHoldOnlineOnly = false,
        isFromCursor,
        sortColumn,
        sortOrder,
        sourceVolumeId,
        autoConfirm = false,
        onConfirm,
        registerConfirmer,
        onCancel,
    }: Props = $props()

    /** The scan walk met an online-only file inside a selected folder. Flips the
     *  dialog live: the banner appears, the trash switch goes, and the confirm
     *  button becomes the permanent delete, all before anything is pressed.
     *
     *  ❗ Always `'mixed'`, never `'all'`. The walk reports one boolean for the
     *  whole folder, so a hit says "something in here is online-only" and says
     *  nothing about the rest — and the folder holding one evicted file beside a
     *  hundred ordinary ones is the ordinary case. The mixed wording is true
     *  either way; the other one would tell someone every file they picked is
     *  online-only on the strength of a single hit. */
    let onlineOnlyFoundByWalk = $state<CloudOnlineOnlyExtent | null>(null)
    /** How much of the selection is online-only, however we learned it. */
    const onlineOnlyExtent = $derived(cloudOnlineOnly ?? onlineOnlyFoundByWalk)
    /** Online-only content is in play, however we learned it. */
    const routedForOnlineOnly = $derived(onlineOnlyExtent !== null)
    /** The trash is only on the table while nothing evicted is in the selection. */
    const trashAvailable = $derived(supportsTrash && !routedForOnlineOnly)

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
    const isPermanent = $derived(switchIsPermanent || routedForOnlineOnly || (shiftUpgradesToPermanent && shiftHeld))

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
    const hasWarningBanner = $derived(isArchive || routedForOnlineOnly || !supportsTrash)

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
    /** True while a confirm is waiting on the online-only answer below. Shows the
     *  spinner in the confirm button and swallows a second press. */
    let confirming = $state(false)
    /** The walk answered while a press was in flight and turned this dialog from a
     *  trash into a permanent delete, so the press was handed back. Without the
     *  line this renders, the dialog just sits there and nothing says the press
     *  didn't take. Cleared by the next press. */
    let handedBackForOnlineOnly = $state(false)
    /** Resolves once the walk has said whether anything in the selection is
     *  online-only: on the first progress tick reporting one (a hit is the whole
     *  answer, so a huge folder needn't finish counting), on the completion that
     *  reports none, or on any terminal scan event. Awaited ONLY when
     *  `cloudFolderMayHoldOnlineOnly`; everywhere else confirm stays instant. */
    let settleOnlineOnlyAnswer: () => void = () => {}
    const onlineOnlyAnswer = new Promise<void>((resolve) => {
        settleOnlineOnlyAnswer = resolve
    })
    /** ❗ The safe default when no answer ever lands is the TRASH, which is
     *  today's behavior. The scan watchdog already bounds the walk by inactivity,
     *  so this only covers a scan whose events never reach us at all; it's
     *  generous because a real walk of a big tree legitimately takes a while, and
     *  giving up early would run exactly the download we're trying to avoid. */
    const ONLINE_ONLY_ANSWER_TIMEOUT_MS = 90_000
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
    // Priced as a permanent delete only: a trash is unpriced, so it gets no line.
    const costRequest = $derived(
        costRequestFor({
            operation: isPermanent ? 'delete' : 'trash',
            scanComplete,
            previewId,
            sourceVolumeId,
            destinationVolumeId: null,
        }),
    )
    let unlisteners: UnlistenFn[] = []
    /** Set first thing in `onDestroy`. A plain `let`, read by `startScan`, which can outlive the dialog. */
    let destroyed = false

    /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
    function isOurScanEvent(eventPreviewId: string): boolean {
        if (!previewId) previewId = eventPreviewId
        return eventPreviewId === previewId
    }

    /** Keeps a scan listener for `cleanup`, or hands it straight back when the dialog closed while it registered. */
    function keepListener(unlisten: UnlistenFn): void {
        if (destroyed) unlisten()
        else unlisteners.push(unlisten)
    }

    /**
     * Starts the scan preview to count files/dirs/bytes.
     *
     * It can outlive the dialog: a quick Escape or an MCP close unmounts it while the listeners
     * are still registering. So it reads its props BEFORE the first await, because both parents
     * (`DialogManager`, `DialogGallery`) null their props object on close and a prop read after
     * that throws. Past each await, a closed dialog keeps nothing: late listeners go straight
     * back, no scan starts, and a preview whose id lands after teardown is freed here, since
     * teardown had no id to free.
     */
    async function startScan() {
        const request = { sourcePaths, sortColumn, sortOrder, sourceVolumeId }
        // Subscribe to events BEFORE starting scan (avoid missing fast completions)
        keepListener(
            await onScanPreviewProgress((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesFound
                dirsFound = event.dirsFound
                bytesFound = event.bytesFound
                currentDir = event.currentDir ?? null
                if (event.onlineOnlyFound) {
                    onlineOnlyFoundByWalk = 'mixed'
                    settleOnlineOnlyAnswer()
                }
                const r = throughput.push({
                    timestampMs: Date.now(),
                    files: event.filesFound,
                    bytes: event.bytesFound,
                })
                filesPerSec = r.filesPerSecond
                bytesPerSec = r.bytesPerSecond
            }),
        )
        keepListener(
            await onScanPreviewComplete((event) => {
                if (!isOurScanEvent(event.previewId)) return
                filesFound = event.filesTotal
                dirsFound = event.dirsTotal
                bytesFound = event.bytesTotal
                if (event.onlineOnlyFound) onlineOnlyFoundByWalk = 'mixed'
                isScanning = false
                scanComplete = true
                // A finished walk is the definitive answer either way.
                settleOnlineOnlyAnswer()
            }),
        )
        keepListener(
            await onScanPreviewError((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
                // Keep showing whatever stats we have. A walk that stopped can't
                // answer, so confirm falls back to the trash.
                settleOnlineOnlyAnswer()
            }),
        )
        keepListener(
            await onScanPreviewCancelled((event) => {
                if (!isOurScanEvent(event.previewId)) return
                isScanning = false
                settleOnlineOnlyAnswer()
            }),
        )

        if (destroyed) return

        // Start the scan
        isScanning = true
        const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
        const result = await startScanPreview(
            request.sourcePaths,
            request.sortColumn,
            request.sortOrder,
            progressIntervalMs,
            request.sourceVolumeId,
        )
        if ('refusal' in result) {
            // No volume answers for the source (an unplugged phone): nothing was
            // walked and no preview exists. Same as a walk that stopped; the
            // delete itself is refused typed if it's confirmed.
            isScanning = false
            settleOnlineOnlyAnswer()
            return
        }
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- may have changed during await
        if (destroyed) {
            void cancelScanPreview(result.previewId)
            return
        }
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
        destroyed = true
        unregisterConfirmer?.()
        // Nothing may await an answer that can no longer arrive.
        settleOnlineOnlyAnswer()
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
        if (confirming) return
        confirming = true
        handedBackForOnlineOnly = false
        log.info('Delete confirmed: isPermanent={isPermanent}, items={count}', {
            isPermanent,
            count: sourceItems.length,
        })
        // The scan-preview IPC only mints an id and spawns the walk, so it
        // answers promptly even on a wedged share. See `scanStarted`.
        await scanStarted

        if (cloudFolderMayHoldOnlineOnly && !routedForOnlineOnly) {
            // A selected folder in a cloud drive: the walk decides whether this is
            // a trash or a delete, and pressing before it answers must not settle
            // it by luck. Everywhere else this block never runs.
            await withTimeout(onlineOnlyAnswer, ONLINE_ONLY_ANSWER_TIMEOUT_MS, undefined)
            if (destroyed) return
            if (onlineOnlyFoundByWalk !== null && !autoConfirm) {
                // The answer just turned this button from "Move to trash" into a
                // permanent delete, and the banner beside it changed too. Hand the
                // dialog back rather than run the one operation nothing can undo
                // off a press that meant something else, and say so: the dialog
                // stays open either way, so silence reads as a dead button. An MCP
                // auto-confirm asked for the delete outright, so it goes through.
                confirming = false
                handedBackForOnlineOnly = true
                return
            }
        }

        confirmed = true
        onConfirm(previewId, isPermanent)
    }

    // An MCP `dialog confirm` is the Confirm button: same preview, same mode.
    // Registered during init, so a confirm that lands while the scan is still
    // starting finds it and waits for the id like a fast Enter does.
    const unregisterConfirmer = registerConfirmer?.(() => {
        void handleConfirm()
    })

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
             a selection holding online-only cloud content says so in its own words,
             since the person pressed Trash and got a delete; other no-trash volumes
             get the generic banner. -->
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
        {:else if onlineOnlyExtent !== null}
            <div class="warning-banner" role="alert">
                <span class="warning-icon" aria-hidden="true">
                    <Icon name="triangle-alert" size={18} />
                </span>
                <p id="delete-warning-text">
                    <!-- Two whole sentences, not a shared stem plus a fork: they differ
                         mid-paragraph and in which remedies they can offer, and only one
                         of them can say "deselect the online-only files". -->
                    {#if onlineOnlyExtent === 'all'}
                        <Trans key="fileOperations.delete.cloudOnlineOnlyAllWarning" snippets={{ strong }} />
                    {:else}
                        <Trans key="fileOperations.delete.cloudOnlineOnlyMixedWarning" snippets={{ strong }} />
                    {/if}
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
        <S3CostLine request={costRequest} />

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
                    <span class="scan-throughput-sep" aria-hidden="true">·</span>
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

        <!-- The press that didn't take. Sits last in the body so it's the line
             directly above the button whose meaning just changed under the
             person's finger; `role="status"` announces it without stealing focus. -->
        {#if handedBackForOnlineOnly}
            <p class="handed-back" role="status" data-test="delete-handed-back">
                {tString('fileOperations.delete.cloudOnlineOnlyHandedBack')}
            </p>
        {/if}
    </div>

    <!-- Trash (on, the safe default) vs. permanent delete (off). Rides the footer
         row so it reads as a modifier on the confirm button beside it. Holding Shift
         flips it too, for as long as the key is down. -->
    {#snippet footerLeading()}
        {#if trashAvailable}
            <Switch checked={!isPermanent} onCheckedChange={(toTrash) => (switchIsPermanent = !toTrash)}
                >{tString('fileOperations.delete.trashSwitch')}</Switch
            >
        {/if}
    {/snippet}

    {#snippet footer()}
        <Button variant="secondary" onclick={handleCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant={confirmVariant} onclick={handleConfirm}>
            {#if confirming}<span class="confirm-spinner"><Spinner size="sm" /></span>{/if}{confirmLabel}
        </Button>
    {/snippet}
</ModalDialog>

{#snippet size(children: import('svelte').Snippet)}<Size bytes={bytesPerSec ?? 0} />{@render children()}{/snippet}
{#snippet strong(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}

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

    /* Says a press was handed back, right above the button that changed meaning.
       Warning-colored, since it's the same news the banner above carries. */
    .handed-back {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning);
    }

    /* Rides inside the confirm button while a cloud folder's walk is still
       deciding between a trash and a delete. */
    .confirm-spinner {
        display: inline-flex;
        align-items: center;
        margin-right: var(--spacing-xs);
        vertical-align: text-bottom;
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
