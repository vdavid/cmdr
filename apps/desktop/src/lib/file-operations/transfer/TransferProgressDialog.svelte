<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { DEFAULT_VOLUME_ID,
    type Initiator, type ProgressAtStop } from '#lib/tauri-commands/index.js'
    import type {
        TransferOperationType,
        WriteOperationError,
        ConflictResolution,
    } from '#lib/file-explorer/types.js'
    import type { OpKind, RollbackSupport, SpaceShortfall } from '#lib/ipc/bindings.js'
    import type { TransferCompletePayload } from '#lib/file-explorer/pane/dialog-props.js'
    import { getVolumes } from '#lib/stores/volume-store.svelte.js'
    import DirectionIndicator from './DirectionIndicator.svelte'
    import { conflictDialogWidthStyle, deriveTransferLabel } from './transfer-dialog-utils'
    import ScanPhaseBody from './ScanPhaseBody.svelte'
    import TransferConflictDialog from './TransferConflictDialog.svelte'
    import { createTransferProgressState } from './transfer-progress-state.svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import Icon from '#lib/ui/Icon.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import TransferTree from '../transfer-tree/TransferTree.svelte'
    import { createTransferTreeMemory } from '../transfer-tree/transfer-tree-view.svelte'
    import { treeOperationFor } from '../transfer-tree/tree-operation'
    import RollbackConfirmDialog from '../RollbackConfirmDialog.svelte'
    import AlertDialog from '#lib/ui/AlertDialog.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { stallNoticeFor, waitLineFor } from './transfer-stall'
    import TransferStallNotice from './TransferStallNotice.svelte'
    import { archivePhaseLabel, isIndeterminateProgressPhase, progressCountKind } from '../progress-readout'
    import { rollbackConfirmVariant, reversalTitleKey } from '../reversal-wording'
    import TransferRollbackControls from './TransferRollbackControls.svelte'
    import { createTransferRollback } from './transfer-rollback.svelte'
    import { conflictPolicyCaption } from './conflict-policy-labels'
    import { opKindForTransferType } from '../op-kind'
    import { getMainWindowOperationRows } from '#lib/file-operations/queue/main-window-operations.svelte.js'
    import { hasOtherQueuedWork } from '#lib/file-operations/queue/queue-backlog.js'
    import { loadProgressDialogHeight, saveProgressDialogHeight } from '#lib/app-status-store.js'

    interface Props {
        /** An operation already running that this dialog WATCHES instead of
         *  starting one (Show, from the operation queue). It arrives with the
         *  four things the registry snapshot knows — the id, the type, and the
         *  two paths below — and nothing else: the dispatch props marked
         *  "started only" are absent, because nobody dispatches on this path. */
        adoptOperationId?: string
        /** Adopted only. Set when the adopted operation IS the reversal of a
         *  finished one, to the kind of the operation it reverses. It retitles
         *  the dialog by what the reversal will DO; `operationType` names only
         *  the syscall it runs as, which for a reversal misleads (undoing a copy
         *  runs as a delete). ❌ Never inferred from the `rolling_back` phase:
         *  a CANCELLED copy wears that phase too, and it really is deleting. */
        reverses?: OpKind | null
        operationType: TransferOperationType
        /** Started only. */
        sourcePaths?: string[]
        sourceFolderPath: string
        /** Destination path (not applicable for delete/trash) */
        destinationPath?: string
        destinationName?: string
        /** Transfer direction (not applicable for delete/trash, and unknown for
         *  an adopted operation: the snapshot names paths, not panes). */
        direction?: 'left' | 'right'
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
        /** Copy only: `proceed` when the person chose "Copy anyway" after a space shortfall. */
        spaceShortfall?: SpaceShortfall
        /** Per-item sizes for trash progress (from scan or drive index, optional) */
        itemSizes?: number[]
        /** Started only: the top-level files and folders among `sourcePaths`, so the
         *  queue row can say "2 folders and 1 file". */
        fileCount?: number
        folderCount?: number
        /** Who triggered this operation (`aiClient` for MCP-originated writes). */
        initiator?: Initiator
        /** Rename mode: the one source moves into `destinationPath` under this name. */
        newName?: string
        onComplete: (payload: TransferCompletePayload) => void
        onCancelled: (filesProcessed: number) => void
        onError: (error: WriteOperationError, progressAtStop: ProgressAtStop | null) => void
        /** Send this operation to the background: unmount the modal but keep the
         *  op running, managed in the queue window. Fired by the Queue button, the
         *  dialog-scoped F2, and the auto-queue path (an op admitted as Queued).
         *  Optional so existing callers/tests that don't background stay valid. */
        onQueue?: (operationId: string) => void
        /** MCP round-trip id, present only for an auto-confirmed MCP op. Passed to
         *  the state machine so it replies with the spawned operationId. */
        mcpRequestId?: string
    }

    // The "started only" defaults are never read on the path that omits them:
    // an adopted dialog dispatches nothing, so the whole dispatch config sits
    // unused. They exist so this component keeps one flat prop list.
    const {
        adoptOperationId,
        reverses = null,
        operationType,
        sourcePaths = [],
        sourceFolderPath,
        destinationPath,
        destinationName,
        direction,
        previewId = null,
        sourceVolumeId = DEFAULT_VOLUME_ID,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        spaceShortfall,
        itemSizes,
        fileCount,
        folderCount,
        initiator,
        newName,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    }: Props = $props()

    /** The tree's height before the user resizes the dialog, in rows. */
    const TREE_ROWS = 8

    /** The conflict prompt and a reversal take the tree's place; this brings it back as the
     *  user left it (toggles, following, scroll, height) rather than fresh. */
    const treeMemory = createTransferTreeMemory()
    /** The tree on screen, which hears the dialog's F (`handleKeydown`). */
    let treeView: { handleFollowKey(event: KeyboardEvent): boolean } | undefined = $state()

    /** Wide enough that the shared readout's fixed columns (amount, percent,
     *  rate, time left) still leave the bars a readable width. It's also the
     *  resize floor: the columns don't shrink, so anything narrower squeezes the
     *  bars out of existence rather than the numbers. */
    const DIALOG_WIDTH_PX = 580
    const DIALOG_WIDTH_STYLE = `width: ${String(DIALOG_WIDTH_PX)}px; min-width: ${String(DIALOG_WIDTH_PX)}px`

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
        volumes.find((v) => v.id === destVolumeId)?.connectionState === 'os_mount',
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
     *  Stands in for the registry row's `rollbackSupport` until it lands. */
    const isSameVolumeMove = $derived(
        operationType === 'move' &&
            sourceVolumeId !== DEFAULT_VOLUME_ID &&
            sourceVolumeId === (destVolumeId ?? sourceVolumeId),
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
        destinationName,
        previewId,
        sourceVolumeId,
        destVolumeId,
        conflictResolution,
        preKnownConflicts,
        spaceShortfall,
        itemSizes,
        fileCount,
        folderCount,
        initiator,
        newName,
        onComplete,
        onCancelled,
        onError,
        onQueue,
        mcpRequestId,
    })

    const opKind = $derived(opKindForTransferType(operationType))

    // Local aliases over the factory getters so the markup reads the same names
    // it always has. Each tracks reactive state (the view's own, or the
    // session's through it), so the template updates exactly as before.
    const phase = $derived(progress.phase)
    const isRollingBack = $derived(progress.isRollingBack)

    /** The dialog is showing a reversal launched from the operation history, and
     *  `reverses` says what the operation it undoes DID. Null on an in-flight
     *  rollback (a cancelled copy cleaning up after itself), which keeps its own
     *  title: there the bar drains backwards and "Rolling back…" is honest. */
    const reversalVariant = $derived(reverses === null ? null : rollbackConfirmVariant(reverses))
    /** Whether this operation can reverse, and if not, why. Any reason the
     *  registry row gives wins; otherwise the same-volume-move rule blocks, which
     *  covers the frames before the first snapshot lands. */
    const rollbackSupport = $derived.by((): RollbackSupport | null => {
        const fromRegistry = progress.rollbackSupport
        if (fromRegistry !== null && fromRegistry !== 'supported') return fromRegistry
        return isSameVolumeMove ? 'sameVolumeMove' : fromRegistry
    })

    /** Rollback is asked about before it happens: it undoes everything the
     *  operation has written or moved, and a file it overwrote has no backup
     *  either way, so one mis-click beside a harmless Cancel is unrecoverable.
     *  The button and the conflict body both ask through `rollback.request`. */
    const rollback = createTransferRollback({
        opKind: () => opKind,
        phase: () => phase,
        rollbackSupport: () => rollbackSupport,
        operationSettled: () => progress.operationSettled,
        rollBack: () => void progress.handleCancel(true),
    })
    const conflictPolicy = $derived(progress.conflictPolicy)
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
    /** What to say instead of an ETA while bytes are on their way but none has
     *  landed: the first file still opening, or a response still arriving. */
    const waitLine = $derived(waitLineFor(progress.activity))

    /** The operation is counting, not yet writing. One flag for the whole scan
     *  phase: the preview the backend waits on and its own foolproof re-scan
     *  both arrive as `write-progress` in `phase: 'scanning'`. */
    const isScanning = $derived(phase === 'scanning')
    const isIndeterminate = $derived(isIndeterminateProgressPhase(phase) && !isScanning)
    const archivePhaseText = $derived.by(() => {
        const label = archivePhaseLabel(phase, progress.step)
        return label === null ? null : tString(label.key, label.params)
    })

    /** This view is watching an operation that hasn't said where it is yet: only
     *  an adopted one can be here, and only in a window that has heard nothing
     *  about it at all (a reload, with the operation paused so no tick is
     *  coming). Bars would read 0% about a copy that may be nearly done, so it
     *  shows what it honestly has until the operation speaks. */
    const phaseUnknown = $derived(phase === null)

    /** The notice renders at the foot of the body, outside the branch that owns
     *  the bars, so exclude every indeterminate phase: a scan writes nothing to
     *  be stalled about, compression finishing has no measured transfer, and a
     *  view with no phase yet knows too little to accuse anything of being stuck. */
    const showStall = $derived(stall !== null && !isIndeterminateProgressPhase(phase) && !phaseUnknown)

    /** The live tree of the selection, by the rule the queue rows share (`treeOperationFor`):
     *  a copy, move, or compress past its scan, and never during a reversal. */
    const treeOperation = $derived(
        treeOperationFor({ operationType, phase, reversing: isRollingBack || reversalVariant !== null }),
    )
    /** The tree also needs an id to ask about. */
    const treeOperationId = $derived(treeOperation !== null ? progress.operationId : null)

    /** With an empty queue you're not queueing behind anything, you're sending
     *  this out of sight, so the button says "Background" instead. It reads the
     *  main window's operations store, the same live rows the corner chip reads,
     *  so the word follows the queue as operations come and go. */
    const queueHasOtherWork = $derived(hasOtherQueuedWork(getMainWindowOperationRows(), progress.operationId))

    /** Any command modifier or Shift: `⇧F2` and `⌘F2` are other combos, not Queue. */
    function hasModifier(event: KeyboardEvent): boolean {
        return event.metaKey || event.ctrlKey || event.altKey || event.shiftKey
    }

    /** Where the files are going, shown for copy and move. It's also the dialog's
     *  description, so a screen reader hears it beside the title on opening, when
     *  focus lands on Pause rather than on anything that says what's running. */
    const showDirection = $derived(!isDeleteOrTrash && Boolean(destinationPath))
    const DIRECTION_ID = 'progress-dialog-direction'

    /** Escape is the Cancel button (keeps what's written, asks nothing). Once
     *  Cancel is spent, it's the close instead, so Escape always leads out. */
    function handleEscape(): void {
        if (isCancelling || operationSettled) progress.detach()
        else void progress.handleCancel(false)
    }

    /** The conflict body's answer keys, reached through this dialog's keydowns. */
    let conflictBody: { handleKeydown(event: KeyboardEvent): boolean } | undefined = $state()

    function handleKeydown(event: KeyboardEvent) {
        // A clash's keys (Enter = Skip, Escape = Cancel). With a clash up there's
        // no `onclose`, so `ModalDialog` passes Escape on to here unhandled.
        if (conflictBody?.handleKeydown(event)) return

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

        // F resumes the tree's following from anywhere in the dialog, not only from its rows: a
        // wheel pauses the tree without moving focus into it (`transfer-tree/DETAILS.md` § Keyboard).
        if (treeView?.handleFollowKey(event)) return

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

    /** The height the user last dragged the dialog to while the tree showed (#428),
     *  which the tree opens at again; `null` until one has been dragged. While the
     *  tree is hidden the dialog keeps its natural height (`ModalDialog`). */
    let rememberedHeight = $state<number | null>(null)

    /** A drag that changed the height while the tree showed is the one worth keeping:
     *  with the tree hidden the dialog has no height of its own to remember. */
    function rememberHeight(size: { height: number | null }): void {
        if (size.height === null || treeOperationId === null || conflictEvent) return
        void saveProgressDialogHeight(size.height)
    }

    onMount(() => {
        progress.start()
        void loadProgressDialogHeight().then((height) => {
            rememberedHeight = height
        })
    })

    onDestroy(() => {
        progress.destroy()
    })
</script>

<!-- The action buttons, in `ModalDialog`'s footer, which keeps them on one row in any
     language (the panel widens to fit them). Withdrawn while a clash is up: its body
     carries its own buttons for every way out.
     Once `operationSettled` is true (write-complete / write-cancelled / write-error
     arrived) the backend state is gone, so a Rollback click can't be honored; disable
     both buttons during the MIN_DISPLAY_MS hold-open window. Without this, the user can
     click Rollback after the copy completed and silently get nothing. -->
{#snippet actions()}
    <!-- Manage controls: Pause/Resume keeps the op alive but parked;
         Queue sends it to the background and opens the queue window (also
         F2 while this dialog is focused). Both show only during the active
         copy/move/delete phases (`canPauseOrQueue`). -->
    {#if canPauseOrQueue}
        <!-- Offered during the scan as much as during the write: the
             walk parks on the same gate the drivers do
             (`crates/cmdr-operations/src/scan_bridge.rs`), and the scan is where
             somebody realizes they picked the wrong destination.
             Focused when it appears, so Space pauses and resumes with no
             shortcut of its own: the focus ring shows what the key will do. -->
        <Button
            variant="secondary"
            onclick={progress.handlePauseResume}
            disabled={pauseInFlight}
            autoFocus
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
        <!-- One button, two words: "Queue" when there's something to
             queue behind, "Background" when there isn't. The action, the
             tooltip, and F2 are the same either way. No aria-label: the
             name is the visible word (WCAG 2.5.3, label in name). -->
        <span use:tooltip={tString('fileOperations.transferProgress.queueTooltip')}>
            <Button variant="secondary" onclick={progress.handleQueue} keyHint="F2" aria-keyshortcuts="F2">
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
        <Button variant="secondary" onclick={progress.dismiss}>{tString('fileOperations.transferProgress.close')}</Button>
    {/if}
    <!-- ❌ No Rollback affordance on a reversal, not even the disabled
         one: this operation IS the undo, and its registry row says
         `rollbackSupport: 'isReversal'`. A red button here would offer to
         re-apply what the person just chose to undo. -->
    {#if (isCopy || isMove) && reversalVariant === null}
        <TransferRollbackControls {rollback} {isRollingBack} disabled={isCancelling || operationSettled || isScanning} />
    {/if}
{/snippet}

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
              // ❌ Never a cancel. Closing this dialog (×, or an MCP close)
              // detaches it from the operation; only Cancel and Escape stop it.
              progress.detach()
          }}
    onescape={conflictEvent ? undefined : handleEscape}
    ariaDescribedby={showDirection && !conflictEvent ? DIRECTION_ID : undefined}
    containerStyle={conflictEvent ? conflictDialogWidthStyle(DIALOG_WIDTH_PX) : DIALOG_WIDTH_STYLE}
    resizable={treeOperationId !== null && !conflictEvent ? true : 'horizontal'}
    restoreHeight={rememberedHeight}
    onResizeEnd={rememberHeight}
    footer={!isDeleteOrTrash && conflictEvent ? undefined : actions}
>
    {#snippet title()}
        {#if reversalVariant !== null}
            <!-- Named by what the reversal DOES, with the scope the journal
                 already counted, so the title agrees with the confirmation the
                 user answered a moment ago. It outranks every phase below: a
                 reversal is `rolling_back` from its first frame, and it has no
                 scan. -->
            {tString(reversalTitleKey(reversalVariant), {
                count: filesTotal,
                countText: formatInteger(filesTotal),
            })}
        {:else if isRollingBack}
            {tString('fileOperations.transferProgress.titleRollingBack')}
        {:else if isCancelling || cancelEventReceived}
            {#if settleSlow}
                {tString('fileOperations.transferProgress.titleCancellingSlow')}
            {:else}
                {tString('fileOperations.transferProgress.titleCancelling')}
            {/if}
        {:else if conflictEvent}
            {tString('fileOperations.transferProgress.titleConflict')}
        {:else if isPaused}
            <!-- Before scanning, not after: a paused scan is stopped, so
                 "Scanning to copy..." over it would describe a walk that isn't
                 walking. -->
            {tString('fileOperations.transferProgress.titlePaused')}
        {:else if isScanning}
            <!-- After cancelling and rolling back, not before: the phase stays
                 `scanning` while a cancel issued mid-count winds down, and the
                 title has to name what the dialog is doing NOW. -->
            {scanTitle}
        {:else if archivePhaseText !== null}
            {archivePhaseText}
        {:else if phase === 'flushing'}
            {tString('fileOperations.transferProgress.titleFlushing')}
        {:else if isMove && phase === 'deleting'}
            <!-- The closing stage of a move BETWEEN disks: the files are all at
                 the destination and the originals are going. It has its own bar
                 (over the top-level sources), so leaving the title on "Moving..."
                 would be the one phase whose name says nothing about what the
                 numbers under it are counting. -->
            {tString('fileOperations.transferProgress.titleRemovingOriginals')}
        {:else}
            {tString('fileOperations.transferProgress.titleActive', { gerund: gerundKind })}
        {/if}
    {/snippet}

    {#if !isDeleteOrTrash && conflictEvent}
        <TransferConflictDialog
            bind:this={conflictBody}
            {conflictEvent}
            {isCopy}
            {isMove}
            {rollbackSupport}
            {isCancelling}
            {isResolvingConflict}
            onResolve={(resolution: ConflictResolution, applyToAll: boolean) => {
                void progress.handleConflictResolution(resolution, applyToAll)
            }}
            onCancel={(wantsRollback: boolean) => {
                if (wantsRollback) {
                    rollback.request()
                    return
                }
                void progress.handleCancel(false)
            }}
        />
    {:else}
        <!-- A column filling the body, so a dialog the user made taller hands the
             slack to the tree, the one child that grows. -->
        <div class="progress-body">
        <!-- Where the files are going (copy/move only). An adopted operation
             brings its two paths and no pane-relative direction, and naming
             neither end would be worse than naming both without the arrow
             pointing at a pane. -->
        {#if showDirection && destinationPath}
            <DirectionIndicator
                id={DIRECTION_ID}
                sourcePath={sourceFolderPath}
                {destinationPath}
                {direction}
                {sourceLabel}
                {destinationLabel}
            />
        {/if}
        <!-- What happens to a file that's already there, in the setup dialog's
             own words. Off the operation's registry row, so an adopted dialog
             says it too and a mid-run "… all" answer changes it. -->
        {#if (isCopy || isMove) && conflictPolicy !== null}
            <p class="conflict-policy-caption">{conflictPolicyCaption(conflictPolicy)}</p>
        {/if}

        {#if isScanning}
            <!-- Scanning phase: what's happening, then tallies, throughput, and
                 the current dir/file. There's no matching banner for the active
                 phase: the bars, their labels, and the dialog title already say
                 what's going on, so a "Copying" chip under a "Copying..." title
                 is just a second copy of the word. -->
            <!-- The spinner is the claim that the walk is moving, so a paused
                 scan doesn't get one: it is stopped, and the title says so. -->
            <div class="phase-banner">
                {#if !isPaused}
                    <Spinner size="sm" />
                {/if}
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
                    paused={isPaused}
                    {waitLine}
                />
            </div>
        {:else if isIndeterminate && archivePhaseText !== null}
            <div class="phase-banner" role="status">
                {#if !isPaused}
                    <Spinner size="sm" />
                {/if}
                <span>{archivePhaseText}</span>
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
                    {waitLine}
                    countKind={progressCountKind(opKind, phase)}
                />
            </div>

            <!-- Current file (active phase only; scanning shows it inside scanPhaseBody).
                 The tree names every file at work, so it takes this line's place. -->
            {#if currentFile && treeOperationId === null}
                <div class="current-file" use:useShortenMiddle={{ text: currentFile, preferBreakAt: '/' }}>
                </div>
            {/if}
        {/if}

        {#if treeOperation !== null && treeOperationId !== null}
            <!-- Keyed by id: a tree's operation is fixed for its life. -->
            {#key treeOperationId}
                <TransferTree
                    bind:this={treeView}
                    operationId={treeOperationId}
                    tick={progress.progressTick}
                    operation={treeOperation}
                    defaultRows={TREE_ROWS}
                    memory={treeMemory}
                />
            {/key}
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
            <TransferStallNotice {stall} {operationType} />
        {/if}
        </div>
    {/if}
</ModalDialog>

<!-- Stacked over the progress dialog, which is the dialog that raised it: same
     subtree, so DOM order puts it on top and the focus trap it mounts takes
     over until it goes (`#lib/ui/DETAILS.md` § ModalDialog). Withdrawn once the
     operation settles, because there is nothing left to undo — and the moment
     Rollback is blocked, since a move that reaches its source-deletion phase with
     the question up would otherwise still be promising a journey home. -->
{#if rollback.confirming}
    <RollbackConfirmDialog variant={rollback.variant} onConfirm={rollback.confirm} onCancel={rollback.dismiss} />
{/if}

<!-- A cancel pressed here, once the operation has fully stopped: the dialog stays
     behind it with its last numbers, so the person sees how far it got, and OK
     closes both (`progress.acknowledgeCancel`). Same stacking as the question above. -->
{#if progress.cancelAcknowledgement !== null}
    <AlertDialog
        title={tString('fileOperations.transferProgress.canceledAlertTitle', {
            type: progress.cancelAcknowledgement.operationType,
        })}
        message={tString('fileOperations.transferProgress.canceledAlertMessage', {
            type: progress.cancelAcknowledgement.operationType,
        })}
        onClose={progress.acknowledgeCancel}
    />
{/if}

<style>
    /* Fills the body (which a resized dialog makes taller than its content) as a
       column, so the tree's `flex: 1 1 auto` takes the slack. At its natural height
       it's exactly as tall as what's in it. */
    .progress-body {
        display: flex;
        flex-direction: column;
        height: 100%;
        min-height: 0;
    }

    /* Scan wait section (wraps the ScanPhaseBody child during the scan phases) */
    .scan-wait-section {
        padding: var(--spacing-md) 0 var(--spacing-lg);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    /* A quiet aside under the from → to line: static, secondary to everything
       that moves. */
    .conflict-policy-caption {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
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

    /* Native SMB note */
    .smb-native-note {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-warning-text);
        background: var(--color-warning-bg);
        border-radius: var(--radius-sm);
    }

    /* Icon + label inside the Pause/Resume and Queue buttons. */
    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
