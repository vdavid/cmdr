<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { DEFAULT_VOLUME_ID,
    type Initiator } from '$lib/tauri-commands'
    import type {
        TransferOperationType,
        WriteOperationError,
        SortColumn,
        SortOrder,
        ConflictResolution,
    } from '$lib/file-explorer/types'
    import type { TransferCompletePayload } from '$lib/file-explorer/pane/dialog-props'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { deriveTransferLabel } from './transfer-dialog-utils'
    import ScanPhaseBody from './ScanPhaseBody.svelte'
    import TransferConflictDialog from './TransferConflictDialog.svelte'
    import { createTransferProgressState } from './transfer-progress-state.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import RollbackConfirmDialog from '../RollbackConfirmDialog.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import { stallNoticeFor } from './transfer-stall'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
    import { hasOtherQueuedWork } from '$lib/file-operations/queue/queue-backlog'

    interface Props {
        /** An operation already running that this dialog WATCHES instead of
         *  starting one (Show, from the operation queue). It arrives with the
         *  four things the registry snapshot knows — the id, the type, and the
         *  two paths below — and nothing else: the dispatch props marked
         *  "started only" are absent, because nobody dispatches on this path. */
        adoptOperationId?: string
        operationType: TransferOperationType
        /** Started only. */
        sourcePaths?: string[]
        sourceFolderPath: string
        /** Destination path (not applicable for delete/trash) */
        destinationPath?: string
        /** Transfer direction (not applicable for delete/trash, and unknown for
         *  an adopted operation: the snapshot names paths, not panes). */
        direction?: 'left' | 'right'
        /** Started only: current sort column on source pane (files will be processed in this order) */
        sortColumn?: SortColumn
        /** Started only: current sort order on source pane */
        sortOrder?: SortOrder
        /** Started only: preview scan ID from TransferDialog (for reusing scan results) */
        previewId?: string | null
        /** Source volume ID (like "root", "mtp-336592896:65537"). Started only:
         *  an adopted operation's Rollback affordance comes from its registry
         *  row instead. */
        sourceVolumeId?: string
        /** Destination volume ID (not applicable for delete/trash) */
        destVolumeId?: string
        /** Conflict resolution policy from TransferDialog (not applicable for delete/trash) */
        conflictResolution?: ConflictResolution
        /** Source filenames known to conflict at dest (from TransferDialog's pre-flight scan).
         *  Forwarded to the BE so it can bulk-skip them upfront under `Skip all`. */
        preKnownConflicts?: string[]
        /** Per-item sizes for trash progress (from scan or drive index, optional) */
        itemSizes?: number[]
        /** Who triggered this operation (`aiClient` for MCP-originated writes). */
        initiator?: Initiator
        onComplete: (payload: TransferCompletePayload) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: WriteOperationError) => void
        /** Send this operation to the background: unmount the modal but keep the
         *  op running, managed in the queue window. Fired by the Queue button, the
         *  dialog-scoped F2, and the auto-queue path (an op admitted as Queued).
         *  Optional so existing callers/tests that don't background stay valid. */
        onQueue?: () => void
        /** MCP round-trip id, present only for an auto-confirmed MCP op. Passed to
         *  the state machine so it replies with the spawned operationId. */
        mcpRequestId?: string
    }

    // The "started only" defaults are never read on the path that omits them:
    // an adopted dialog dispatches nothing, so the whole dispatch config sits
    // unused. They exist so this component keeps one flat prop list.
    const {
        adoptOperationId,
        operationType,
        sourcePaths = [],
        sourceFolderPath,
        destinationPath,
        direction,
        sortColumn = 'name',
        sortOrder = 'ascending',
        previewId = null,
        sourceVolumeId = DEFAULT_VOLUME_ID,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        itemSizes,
        initiator,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    }: Props = $props()

    /** Wide enough that the shared readout's fixed columns (amount, percent,
     *  rate, time left) still leave the bars a readable width. It's also the
     *  resize floor: the columns don't shrink, so anything narrower squeezes the
     *  bars out of existence rather than the numbers. */
    const DIALOG_WIDTH_STYLE = 'width: 580px; min-width: 580px'

    /** The select discriminator the catalog's gerund/verb messages key on. */
    const gerundKind = $derived(operationType)
    const isDeleteOrTrash = $derived(operationType === 'delete' || operationType === 'trash')
    const isCopy = $derived(operationType === 'copy')
    const isMove = $derived(operationType === 'move')

    /** Title for the scanning phase: names the upcoming action so the user
     *  knows why we're walking the tree, not just "scanning for fun". */
    const scanTitleMap: Record<Exclude<TransferOperationType, 'archive_edit'>, MessageKey> = {
        copy: 'fileOperations.transferProgress.scanTitleCopy',
        move: 'fileOperations.transferProgress.scanTitleMove',
        delete: 'fileOperations.transferProgress.scanTitleDelete',
        trash: 'fileOperations.transferProgress.scanTitleTrash',
        compress: 'fileOperations.transferProgress.scanTitleCompress',
    }
    // Archive edits have no scan phase, so no scan title ever renders for them.
    const scanTitle = $derived(operationType === 'archive_edit' ? '' : tString(scanTitleMap[operationType]))
    const volumes = $derived(getVolumes())
    const destUsesNativeSmb = $derived(
        volumes.find((v) => v.id === destVolumeId)?.smbConnectionState === 'os_mount',
    )

    // Source/destination labels for the direction header. At a volume root the
    // path basename isn't a user-meaningful name — for an MTP storage root it's
    // the raw storage id (like "65538"). `deriveTransferLabel` falls back to the
    // volume's display name in that case (like "Virtual Pixel 9 - SD Card").
    const sourceVolume = $derived(volumes.find((v) => v.id === sourceVolumeId))
    const destVolume = $derived(volumes.find((v) => v.id === destVolumeId))
    const sourceLabel = $derived(
        deriveTransferLabel(sourceFolderPath, sourceVolume?.path ?? '/', sourceVolume?.name ?? ''),
    )
    const destinationLabel = $derived(
        deriveTransferLabel(destinationPath ?? '/', destVolume?.path ?? '/', destVolume?.name ?? ''),
    )

    /** A move where source and destination are the SAME non-default volume (one
     *  smb2 share / one MTP device). The backend handles these as a server-side
     *  rename-merge with NO rollback support — it stops without reversing and
     *  reports `rolled_back: false`. Local→local same-FS moves DO have real
     *  rollback (via `MoveTransaction`), so the default local volume is excluded.
     *  Drives the disabled Rollback affordance + tooltip. */
    const isSameVolumeMove = $derived(
        operationType === 'move' &&
            sourceVolumeId !== DEFAULT_VOLUME_ID &&
            sourceVolumeId === (destVolumeId ?? sourceVolumeId),
    )

    const ROLLBACK_UNAVAILABLE_TOOLTIP = $derived(
        tString('fileOperations.transferProgress.rollbackUnavailableTooltip'),
    )

    // This dialog as a VIEW of one operation: the factory dispatches it, binds
    // its session, and owns what belongs to a piece of UI (the anti-flicker
    // floor, dismissal, the Queue handoff). Everything the operation itself
    // knows comes through the session it shares with the queue rows and the
    // corner chip. Lives in a factory so it's testable without rendering; the
    // markup reads it through the aliases below.
    const progress = createTransferProgressState({
        adoptOperationId,
        operationType,
        sourcePaths,
        destinationPath,
        sortColumn,
        sortOrder,
        previewId,
        sourceVolumeId,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        itemSizes,
        initiator,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    })

    /** Rollback is asked about before it happens: it deletes everything the
     *  operation has written, and a file it overwrote has no backup, so one
     *  mis-click on a button that sits beside a harmless Cancel is
     *  unrecoverable. Both entry points (this dialog's own button and the
     *  conflict body's) go through `handleCancel(true)`, so the question hangs
     *  off that one call. `../DETAILS.md` § "Rollback asks first". */
    let rollbackAsked = $state(false)

    // Local aliases over the factory getters so the markup reads the same names
    // it always has. Each tracks reactive state (the view's own, or the
    // session's through it), so the template updates exactly as before.
    const phase = $derived(progress.phase)
    const isRollingBack = $derived(progress.isRollingBack)
    const rollbackUnavailable = $derived(progress.rollbackUnavailable)
    const isCancelling = $derived(progress.isCancelling)
    const cancelEventReceived = $derived(progress.cancelEventReceived)
    const settleSlow = $derived(progress.settleSlow)
    const conflictEvent = $derived(progress.conflict)
    const isPaused = $derived(progress.isPaused)
    const pauseInFlight = $derived(progress.pauseInFlight)
    const canPauseOrQueue = $derived(progress.canPauseOrQueue)
    const operationSettled = $derived(progress.operationSettled)
    const isResolvingConflict = $derived(progress.isResolvingConflict)
    /** The scan-phase readout, computed by the operation's session so this
     *  dialog and a queue row watching the same walk can't disagree. */
    const scan = $derived(progress.scan)
    const currentFile = $derived(progress.currentFile)
    const filesDone = $derived(progress.filesDone)
    const filesTotal = $derived(progress.filesTotal)
    const bytesDone = $derived(progress.bytesDone)
    const bytesTotal = $derived(progress.bytesTotal)
    const bytesPerSecond = $derived(progress.bytesPerSecond)
    const filesPerSecond = $derived(progress.filesPerSecond)
    const etaSecondsDisplay = $derived(progress.etaSecondsDisplay)
    /** Non-null only once the BACKEND says the transfer has stopped moving for
     *  a reason that isn't deliberate. Drives the ETA line off the screen. */
    const stall = $derived(stallNoticeFor(progress.activity))

    /** The operation is counting, not yet writing. One flag for the whole scan
     *  phase: the preview the backend waits on and its own foolproof re-scan
     *  both arrive as `write-progress` in `phase: 'scanning'`. */
    const isScanning = $derived(phase === 'scanning')

    /** This view is watching an operation that hasn't said where it is yet: only
     *  an adopted one can be here, and only in a window that has heard nothing
     *  about it at all (a reload, with the operation paused so no tick is
     *  coming). Bars would read 0% about a copy that may be nearly done, so it
     *  shows what it honestly has until the operation speaks. */
    const phaseUnknown = $derived(phase === null)

    /** The notice renders at the foot of the body, outside the branch that owns
     *  the bars, so it has to re-state the two phases that branch excludes: a
     *  scan writes nothing to be stalled about, and a view with no phase yet
     *  knows too little to accuse anything of being stuck. */
    const showStall = $derived(stall !== null && !isScanning && !phaseUnknown)

    /** With an empty queue you're not queueing behind anything, you're sending
     *  this out of sight, so the button says "Background" instead. It reads the
     *  main window's operations store, the same live rows the corner chip reads,
     *  so the word follows the queue as operations come and go. */
    const queueHasOtherWork = $derived(hasOtherQueuedWork(getMainWindowOperationRows(), progress.operationId))

    /** Any command modifier or Shift: `⇧F2` and `⌘F2` are other combos, not Queue. */
    function hasModifier(event: KeyboardEvent): boolean {
        return event.metaKey || event.ctrlKey || event.altKey || event.shiftKey
    }

    function handleKeydown(event: KeyboardEvent) {
        // Dialog-scoped F2 → "Queue" (send to background). This is Total
        // Commander's copy-dialog-local F2, NOT the global `file.rename` binding:
        // it works ONLY while this dialog is open and intercepts here. The
        // `ModalDialog` overlay `stopPropagation`s every keydown before it can
        // reach the global key handler, so closing the dialog unmounts this
        // handler and F2 falls through to `file.rename` again (no leak). We still
        // `preventDefault` so the key never triggers a default browser action.
        if (event.key === 'F2' && !hasModifier(event) && canPauseOrQueue) {
            event.preventDefault()
            progress.handleQueue()
            return
        }

        if (event.key === 'Tab') {
            // Trap focus within the dialog
            const overlay = event.currentTarget as HTMLElement
            const focusableElements = overlay.querySelectorAll<HTMLElement>(
                'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
            )
            if (focusableElements.length === 0) return

            const firstElement = focusableElements[0]
            const lastElement = focusableElements[focusableElements.length - 1]

            if (event.shiftKey) {
                if (document.activeElement === firstElement) {
                    event.preventDefault()
                    lastElement.focus()
                }
            } else {
                if (document.activeElement === lastElement) {
                    event.preventDefault()
                    firstElement.focus()
                }
            }
        }
    }

    onMount(() => {
        progress.start()
    })

    onDestroy(() => {
        progress.destroy()
    })
</script>

<ModalDialog
    titleId="progress-dialog-title"
    onkeydown={handleKeydown}
    dialogId="transfer-progress"
    onclose={conflictEvent
        ? // No × and no Escape while a clash is on screen: every way out of one
          // is a decision about the user's files, and the conflict body carries
          // its own buttons for all of them. Same rule the main window's
          // conflict prompt follows.
          undefined
        : () => {
              // ❌ Never a cancel. Closing this dialog detaches it from the
              // operation; only the Cancel button asks the operation to stop.
              progress.detach()
          }}
    containerStyle={DIALOG_WIDTH_STYLE}
    resizable="horizontal"
>
    {#snippet title()}
        {#if isRollingBack}
            {tString('fileOperations.transferProgress.titleRollingBack')}
        {:else if isCancelling || cancelEventReceived}
            {#if settleSlow}
                {tString('fileOperations.transferProgress.titleCancellingSlow')}
            {:else}
                {tString('fileOperations.transferProgress.titleCancelling')}
            {/if}
        {:else if conflictEvent}
            {tString('fileOperations.transferProgress.titleConflict')}
        {:else if isScanning}
            <!-- After cancelling and rolling back, not before: the phase stays
                 `scanning` while a cancel issued mid-count winds down, and the
                 title has to name what the dialog is doing NOW. -->
            {scanTitle}
        {:else if isPaused}
            {tString('fileOperations.transferProgress.titlePaused')}
        {:else if phase === 'flushing'}
            {tString('fileOperations.transferProgress.titleFlushing')}
        {:else}
            {tString('fileOperations.transferProgress.titleActive', { gerund: gerundKind })}
        {/if}
    {/snippet}

    {#if !isDeleteOrTrash && conflictEvent}
        <TransferConflictDialog
            {conflictEvent}
            {isCopy}
            {isMove}
            rollbackUnavailable={isSameVolumeMove}
            {isCancelling}
            {isResolvingConflict}
            onResolve={(resolution: ConflictResolution, applyToAll: boolean) => {
                void progress.handleConflictResolution(resolution, applyToAll)
            }}
            onCancel={(rollback: boolean) => {
                if (rollback) {
                    rollbackAsked = true
                    return
                }
                void progress.handleCancel(false)
            }}
        />
    {:else}
        <!-- Where the files are going (copy/move only). An adopted operation
             brings its two paths and no pane-relative direction, and naming
             neither end would be worse than naming both without the arrow
             pointing at a pane. -->
        {#if !isDeleteOrTrash && destinationPath}
            <DirectionIndicator
                sourcePath={sourceFolderPath}
                {destinationPath}
                {direction}
                {sourceLabel}
                {destinationLabel}
            />
        {/if}

        {#if isScanning}
            <!-- Scanning phase: what's happening, then tallies, throughput, and
                 the current dir/file. There's no matching banner for the active
                 phase: the bars, their labels, and the dialog title already say
                 what's going on, so a "Copying" chip under a "Copying..." title
                 is just a second copy of the word. -->
            <div class="phase-banner">
                <Spinner size="sm" />
                <span>{tString('fileOperations.transferProgress.stageScanning')}</span>
            </div>

            <div class="scan-wait-section">
                <ScanPhaseBody
                    {sourceFolderPath}
                    scanFilesFound={scan.filesFound}
                    scanDirsFound={scan.dirsFound}
                    scanBytesFound={scan.bytesFound}
                    scanFilesPerSec={scan.filesPerSecond}
                    scanBytesPerSec={scan.bytesPerSecond}
                    scanCurrentDir={scan.currentDir}
                    {currentFile}
                />
            </div>
        {:else if !phaseUnknown}
            <!-- Dual progress bars (size + count) for the active phase. The
                 operation queue's rows render the same component, so the two
                 surfaces can't drift apart on what a running op looks like. -->
            <div class="progress-section">
                <TransferProgressReadout
                    {bytesDone}
                    {bytesTotal}
                    {filesDone}
                    {filesTotal}
                    {bytesPerSecond}
                    {filesPerSecond}
                    etaSeconds={etaSecondsDisplay}
                    {stall}
                    countKind={operationType === 'trash' ? 'items' : 'files'}
                />
            </div>

            <!-- Current file (active phase only; scanning shows it inside scanPhaseBody) -->
            {#if currentFile}
                <div class="current-file" use:useShortenMiddle={{ text: currentFile, preferBreakAt: '/' }}>
                </div>
            {/if}
        {/if}

        {#if destUsesNativeSmb}
            <p class="smb-native-note">
                {tString('fileOperations.transferProgress.smbNativeNote')}
            </p>
        {/if}

        <!-- The stall notice lives at the foot of the body, directly above the
             actions: it is the reason a person reaches for Cancel, so it sits
             next to the button they'd reach for rather than interrupting the
             readout. Full content width, warning-toned, and built from the house
             card so it can't drift from the conflict block in `TransferDialog`. -->
        {#if showStall && stall}
            <div class="stall-notice">
                <SectionCard tone="warning">
                    <div class="stall-body" role="status">
                        <span class="stall-icon" aria-hidden="true"><Icon name="hourglass" size={16} /></span>
                        <div class="stall-text">
                            <p class="stall-reason">
                                {#if stall.reason === 'destination'}
                                    {tString('fileOperations.transferProgress.stallWaitingDestination')}
                                {:else if stall.reason === 'source'}
                                    {tString('fileOperations.transferProgress.stallWaitingSource')}
                                {:else}
                                    {tString('fileOperations.transferProgress.stallUnknown')}
                                {/if}
                            </p>
                            {#if stall.inFlight > 0}
                                <!-- Why the finished count can read lower than what
                                     the user can see at the destination. -->
                                <p class="stall-detail">
                                    {tString('fileOperations.transferProgress.stallInFlight', {
                                        count: stall.inFlight,
                                    })}
                                </p>
                            {/if}
                            <p class="stall-detail">{tString('fileOperations.transferProgress.stallLogHint')}</p>
                        </div>
                    </div>
                </SectionCard>
            </div>
        {/if}

        <!-- Action buttons -->
        <!-- Once `operationSettled` is true (write-complete / write-cancelled / write-error
             arrived) the backend state is gone, so a Rollback click can't be honored; disable
             both buttons during the MIN_DISPLAY_MS hold-open window. Without this, the user can
             click Rollback after the copy completed and silently get nothing. -->
        <div class="button-row">
            <!-- Manage controls: Pause/Resume keeps the op alive but parked;
                 Queue sends it to the background and opens the queue window (also
                 F2 while this dialog is focused). Both show only during the active
                 copy/move/delete phases (`canPauseOrQueue`). -->
            {#if canPauseOrQueue}
                <!-- Pause parks between files, so there's nothing to park while
                     the operation is still counting: the backend declines a
                     pause in its scan-wait, and offering a button that does
                     nothing is worse than not offering it. Queue stays, and is
                     the whole point of giving a scanning transfer an id. -->
                {#if !isScanning}
                    <Button
                        variant="secondary"
                        onclick={progress.handlePauseResume}
                        disabled={pauseInFlight}
                        aria-label={isPaused
                            ? tString('fileOperations.transferProgress.resumeAria')
                            : tString('fileOperations.transferProgress.pauseAria')}
                    >
                        <span class="btn-inner">
                            <Icon name={isPaused ? 'play' : 'pause'} size={14} />
                            {isPaused
                                ? tString('fileOperations.transferProgress.resume')
                                : tString('fileOperations.transferProgress.pause')}
                        </span>
                    </Button>
                {/if}
                <!-- One button, two words: "Queue" when there's something to
                     queue behind, "Background" when there isn't. The action, the
                     tooltip, and F2 are the same either way. -->
                <span use:tooltip={tString('fileOperations.transferProgress.queueTooltip')}>
                    <Button
                        variant="secondary"
                        onclick={progress.handleQueue}
                        aria-label={queueHasOtherWork
                            ? tString('fileOperations.transferProgress.queueAria')
                            : tString('fileOperations.transferProgress.backgroundAria')}
                    >
                        <span class="btn-inner">
                            <Icon name="list" size={14} />
                            {queueHasOtherWork
                                ? tString('fileOperations.transferProgress.queue')
                                : tString('fileOperations.transferProgress.background')}
                        </span>
                    </Button>
                </span>
            {/if}
            <Button
                variant="secondary"
                onclick={() => progress.handleCancel(false)}
                disabled={isCancelling || operationSettled}>{tString('fileOperations.button.cancel')}</Button
            >
            <!-- The escape hatch. Once cancelling is under way the backend can
                 take its bounded wind-down time, and the person must never be
                 stuck watching it: force-quit was the only way out of the
                 2026-07-31 wedge, and that's what cost two files. -->
            {#if isCancelling || cancelEventReceived}
                <Button variant="secondary" onclick={progress.dismiss}
                    >{tString('fileOperations.transferProgress.close')}</Button
                >
            {/if}
            {#if isCopy || isMove}
                {#if isRollingBack}
                    <Button variant="danger" disabled>{tString('fileOperations.transferProgress.titleRollingBack')}</Button
                    >
                {:else if isSameVolumeMove || rollbackUnavailable}
                    <!-- No backend rollback for this one: either a same-volume
                         move (which the props alone can tell), or the
                         operation's own registry row saying so — the authority
                         wherever it has arrived, and an adopted view's only
                         source. Disabled with an explanatory tooltip; plain
                         Cancel above stays reachable. -->
                    <span use:tooltip={ROLLBACK_UNAVAILABLE_TOOLTIP}>
                        <Button variant="danger" disabled
                            >{tString('fileOperations.transferProgress.conflictRollback')}</Button
                        >
                    </span>
                {:else}
                    <!-- Nothing has been written during the scan, so there is
                         nothing to reverse: disabled rather than hidden, so the
                         button row doesn't reshuffle when counting ends. -->
                    <span use:tooltip={tString('fileOperations.transferProgress.rollbackTooltip')}>
                        <Button
                            variant="danger"
                            onclick={() => {
                                rollbackAsked = true
                            }}
                            disabled={isCancelling || operationSettled || isScanning}
                            >{tString('fileOperations.transferProgress.conflictRollback')}</Button
                        >
                    </span>
                {/if}
            {/if}
        </div>
    {/if}
</ModalDialog>

<!-- Stacked over the progress dialog, which is the dialog that raised it: same
     subtree, so DOM order puts it on top and the focus trap it mounts takes
     over until it goes (`$lib/ui/DETAILS.md` § ModalDialog). Withdrawn once the
     operation settles, because there is nothing left to undo. -->
{#if rollbackAsked && !operationSettled}
    <RollbackConfirmDialog
        variant="stopAndDelete"
        onConfirm={() => {
            rollbackAsked = false
            void progress.handleCancel(true)
        }}
        onCancel={() => {
            rollbackAsked = false
        }}
    />
{/if}

<style>
    /* Scan wait section (wraps the ScanPhaseBody child during the scan phases) */
    .scan-wait-section {
        padding: var(--spacing-md) 0 var(--spacing-lg);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    /* Says what the dialog is busy with while it's something other than the
       obvious. Scanning only — see the markup. */
    .phase-banner {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-md) 0;
        color: var(--color-accent-text);
        font-size: var(--font-size-sm);
    }

    /* Gutter around the shared dual-bar readout; the readout owns its own
       internal widths. */
    .progress-section {
        margin-bottom: var(--spacing-md);
    }

    /* Full content width: no side inset of its own, so the card lines up with
       the readout above it instead of sitting in from both edges. The card
       carries the tint, the border, and its own generous padding; all this
       wrapper owns is the gap below, which the button row's top padding
       already supplies. */
    .stall-notice :global(.section-card-wrap) {
        margin-bottom: 0;
    }

    .stall-body {
        display: flex;
        gap: var(--spacing-md);
        align-items: flex-start;
    }

    /* The one colored mark on the card. A tone colors the SURFACE and text keeps
       its own color (`$lib/ui/DETAILS.md` § SectionCard), so the icon is where
       the warning reads as a warning. `--color-warning-text`, not
       `--color-warning`: the brand orange clocks ~3.3:1 on this tint. */
    .stall-icon {
        color: var(--color-warning-text);
        flex-shrink: 0;
        display: flex;
        align-items: center;
        /* Optically centered on the first line of the reason, whose line box is
           taller than the 16px glyph. */
        padding-top: 1px;
    }

    .stall-text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    /* Matches the conflict card's summary line in `TransferDialog`: the verdict
       reads at the body size, in ordinary primary text on the tint. */
    .stall-reason {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .stall-detail {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Current file */
    .current-file {
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    /* Buttons */
    .smb-native-note {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        border-radius: var(--radius-sm);
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
        flex-wrap: wrap;
        /* Last row in a footerless body: the body's own inset supplies the gap below. */
        padding: var(--spacing-lg) 0 0;
    }

    /* Icon + label inside the Pause/Resume and Queue buttons. */
    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
