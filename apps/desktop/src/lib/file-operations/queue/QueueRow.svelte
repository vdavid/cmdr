<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { isInstantOperation, type OperationRow } from './operations-store.svelte'
    import { operationTypeIcon } from './operation-icon'
    import { failureReasonFor } from './failure-reason'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import RollbackConfirmDialog from '../RollbackConfirmDialog.svelte'
    import ScanPhaseBody from '../transfer/ScanPhaseBody.svelte'
    import { stallNoticeFor } from '../transfer/transfer-stall'
    import { bindOperationSession } from '../operation-session/bind-operation-session.svelte'
    import {
        inFlightRollbackTooltipKey,
        inFlightRollbackVariant,
        reversalWindowClosed,
        rollbackConfirmVariant,
        reversalLabelKey,
    } from '../reversal-wording'
    import { opKindForWireType } from '../op-kind'
    import { progressCountKind } from '../progress-readout'
    import { requestForegroundOperation } from '$lib/tauri-commands'

    interface Props {
        row: OperationRow
        selected: boolean
        onToggleSelect: () => void
        /** Stop showing a retained failure. Not a command on the operation (it
         *  has none left): the page drops the row the backend retained. The only
         *  way one leaves the list — no timer, no window close, no next
         *  operation. */
        onDismiss: () => void
    }

    const { row, selected, onToggleSelect, onDismiss }: Props = $props()

    const snapshot = $derived(row.snapshot)
    const progress = $derived(row.progress)
    const status = $derived(snapshot.status)

    /** This row's looking glass onto the operation, and how it talks back. It
     *  owns the one ETA smoother and the one scan-rate estimator for this
     *  operation in this window, so the numbers here can't drift from what any
     *  other view of it shows — and the Pause / Cancel / Rollback it issues are
     *  the same commands, against the same guards, whatever else is watching. */
    const session = bindOperationSession(() => snapshot.operationId)

    /** The session once the binding takes hold: what the operation is, and what
     *  this row can ask of it. Null only for the frame between mount and the
     *  first effect, which is before anyone can click, and no command throws —
     *  so a control is a plain `void op?.thing()`. */
    const op = $derived(session.current)

    /** The bar-is-moving truth is the SNAPSHOT status, never the progress event:
     *  a parked op emits no further ticks, so its last one describes a transfer
     *  that has stopped. Only a `running` op shows the live spinner / animated bar. */
    const isRunning = $derived(status === 'running')
    const isPaused = $derived(status === 'paused')
    const isQueued = $derived(status === 'queued')
    const isActionable = $derived(status === 'running' || status === 'paused' || status === 'queued')

    /** The operation stopped and the backend kept its reason. The row is a
     *  record now: nothing to pause, cancel, roll back, or select in bulk, and
     *  the only control left is Dismiss. */
    const isFailed = $derived(status === 'failed')

    /** Why it stopped, in the error dialog's own words. Null on every live row.
     *  `reason.message` is markup from the pipeline (escaped names, size tiers),
     *  so it goes through `{@html}` exactly as the dialog's body does. */
    const reason = $derived(failureReasonFor(snapshot))

    /** This operation IS the reversal of a finished one, and `reverses` names
     *  what that one DID, so the row can say what this one will do to the files
     *  rather than which syscall it uses. Null on every ordinary operation.
     *  Same variant the confirmation was worded from a moment ago, so the two
     *  can't contradict each other. `../reversal-wording.ts`. */
    const reversalVariant = $derived(
        snapshot.reverses === null ? null : rollbackConfirmVariant(snapshot.reverses),
    )

    /** ❌ Not the op-type glyph a reversal would otherwise wear: undoing a copy
     *  registers as a delete, so the row would fly a trash can over an operation
     *  the user asked to UNDO. The undo arrow is the one the Roll back button
     *  in this same window already uses. */
    const typeIcon = $derived(
        reversalVariant === null ? operationTypeIcon(snapshot.operationType) : 'rotate-ccw',
    )

    /** The backend switched this op to undoing what it wrote. The lifecycle
     *  status stays `running` throughout (rollback is an INTENT, not a
     *  lifecycle state), so the live progress phase is the only signal. */
    const isRollingBack = $derived(progress?.phase === 'rolling_back')

    /** The operation is counting, not yet writing: it holds an `operationId`
     *  and its lanes from the moment the user confirmed, and the backend's own
     *  task is waiting on the `TransferDialog` preview. `supportsRollback` is a
     *  promise about the OPERATION, so the phase is what decides which controls
     *  make sense right now. */
    const isScanning = $derived(progress?.phase === 'scanning')

    /** Rollback is offered on exactly the operations the backend can reverse
     *  (`supportsRollback`), and only while there's still something running to
     *  reverse. A scanning operation has written nothing, so it has nothing to put
     *  back; a move removing its originals has already landed everything, so
     *  nothing can come back (`../reversal-wording.ts`). Same verdict the progress
     *  dialog reaches, same wording. */
    const canRollback = $derived(
        snapshot.supportsRollback &&
            (isRunning || isPaused) &&
            !isRollingBack &&
            !isScanning &&
            !reversalWindowClosed(opKindForWireType(snapshot.operationType), progress?.phase ?? null),
    )

    /** Show hands this operation back to the main window's progress dialog, the
     *  one it was backgrounded from. Offered while the operation is actually
     *  moving (a scan counts) and never on a `queued` one, which has no progress
     *  to fill the dialog with yet. Instant ops emit no progress at all, so
     *  there's nothing to show for them either. DETAILS § Show. */
    const canForeground = $derived((isRunning || isPaused) && !isInstantOperation(snapshot.operationType))

    /** The operation has stopped on a clash nobody has answered yet. The
     *  lifecycle status stays `running` throughout (a clash pauses nothing), so
     *  this is the one thing the snapshot can't tell the row; it comes from the
     *  backend's own wait classification, through the session. */
    const awaitingAnswer = $derived(op?.awaitingAnswer ?? false)

    /** A reversal is named by what it will DO to the files ("Putting files
     *  back"), ❌ never by the operation type it runs as: undoing a move is
     *  journaled as a move and undoing a copy as a delete, so the plain action
     *  word would tell a person their undo is deleting things. */
    const label = $derived(
        reversalVariant === null
            ? tString('queue.row.label', { type: snapshot.operationType })
            : tString(reversalLabelKey(reversalVariant)),
    )

    /** The status cell names the LIFECYCLE, and twice it doesn't: an in-flight
     *  rollback and an unanswered clash both leave the operation `running` while
     *  they do something a person needs to recognize. This column is what
     *  somebody reads down when they're looking for the operation that isn't
     *  moving, so it carries the most specific true thing, with the lifecycle
     *  word as its default vocabulary. Why here rather than in the readout:
     *  DETAILS § "A row parked on a clash".
     *
     *  A reversal launched from history is the exception to the exception: its
     *  LABEL already says it's a reversal, so the status cell is free to keep
     *  the lifecycle word. That's what lets a paused reversal read "Paused"
     *  instead of a "Rolling back..." that hides the pause. */
    const statusLabel = $derived(
        isRollingBack && reversalVariant === null
            ? tString('fileOperations.transferProgress.titleRollingBack')
            : awaitingAnswer
              ? tString('queue.row.statusAwaitingAnswer')
              : tString('queue.row.status', { status }),
    )

    /** The dual-bar readout shows once there's something to fill either bar.
     *  Instant ops (rename, create folder/file) emit no `write-progress` at all,
     *  so their rows stay a single line. A scanning row is excluded on purpose:
     *  `filesTotal` means "what the scan concluded", and during the scan there
     *  is no such thing — the counting line below is what it renders instead. */
    const showReadout = $derived(
        (isRunning || isPaused) && !isScanning && progress !== null && (progress.bytesTotal > 0 || progress.filesTotal > 0),
    )

    /** The scan-phase line, on a `queued` row as well as a running one. An
     *  operation admitted behind another on the same lane keeps counting while
     *  it waits, and on a busy lane that's the common case: "Waiting" over a
     *  bare row reads as a hung queue, where "Waiting" over a moving file count
     *  is exactly what's happening. Costs no new strings — `ScanPhaseBody`
     *  resolves the same catalog keys the progress dialog uses. */
    const showScanLine = $derived(isScanning && (isRunning || isPaused || isQueued))

    /** Non-null once the BACKEND reports this transfer has stopped moving for a
     *  reason that isn't deliberate. Same classifier the copy dialog uses, so
     *  the two windows can't disagree about whether something is stuck. */
    const stall = $derived(stallNoticeFor(progress?.activity))

    /** Speed and ETA come from the session, which drops the two RATES while a
     *  person is deciding (a pause, an unanswered clash: nothing is moving, so
     *  there's no honest speed), keeps the ETA through both, and smooths it.
     *  ❌ Never `progress.etaSeconds` or a rate off the raw tick: every view of
     *  this operation reads the session's numbers, so no two of them can
     *  disagree about how fast it's going or how long is left. */
    const byteRate = $derived(op?.bytesPerSecondDisplay ?? null)
    const fileRate = $derived(op?.filesPerSecondDisplay ?? null)
    const etaSeconds = $derived(op?.etaSecondsDisplay ?? null)

    /** How fast the walk is going, which the backend doesn't measure during a
     *  scan: the session computes it from the same ticks this row renders. */
    const scanRates = $derived(op?.scan ?? null)

    const pauseResumeLabel = $derived(
        isPaused ? tString('queue.row.resume') : tString('queue.row.pause'),
    )
    const pauseResumeAria = $derived(
        isPaused ? tString('queue.row.resumeAria') : tString('queue.row.pauseAria'),
    )

    // Source / destination basenames for a compact summary; the full paths sit in
    // the tooltip. Delete / trash have no destination.
    function basename(path: string | null): string {
        if (!path) return ''
        const trimmed = path.replace(/\/+$/, '')
        const idx = trimmed.lastIndexOf('/')
        return idx >= 0 ? trimmed.slice(idx + 1) : trimmed
    }
    const sourceName = $derived(basename(snapshot.source))
    const destName = $derived(basename(snapshot.destination))

    /** A removal reversal has no destination, so the arrow that gives every other
     *  row its direction never renders and the lone folder name sits against
     *  "Deleting what it created" reading as the thing about to go. The preposition
     *  is what puts it back where it belongs: the reversal clears files INSIDE that
     *  folder. Only this shape needs it; every other row already has its arrow. */
    const sourcePhrase = $derived(
        reversalVariant === 'undoByDeleting' && snapshot.destination === null
            ? tString('queue.row.reversalInFolder', { folder: sourceName })
            : sourceName,
    )

    /** Rollback is one click from unrecoverable (a file it overwrote has no
     *  backup), and it sits beside a Cancel that keeps everything. So the click
     *  raises the question instead of the deletion. `../DETAILS.md` §
     *  "Rollback asks first". */
    let rollbackAsked = $state(false)

    /** What rolling THIS operation back would do to the files. Same picker the
     *  progress dialog uses on the same operation, so the two windows can't ask
     *  the question in different words. `../reversal-wording.ts`. */
    const inFlightVariant = $derived(inFlightRollbackVariant(opKindForWireType(snapshot.operationType)))

</script>

<li class="queue-row" class:selected data-operation-id={snapshot.operationId} data-status={status}>
    <div class="select-cell">
        <!-- The checkbox exists for "Cancel selected", so a settled failure has
             nothing to offer it. -->
        {#if !isFailed}
            <Checkbox checked={selected} onCheckedChange={onToggleSelect} ariaLabel={tString('queue.row.selectAria')} />
        {/if}
    </div>

    <span class="type-cell" aria-hidden="true">
        <Icon name={typeIcon} size={16} />
    </span>

    <div class="summary-row">
        <span class="op-label">{label}</span>
        {#if snapshot.source}
            <span class="path" use:tooltip={{ text: snapshot.source, overflowOnly: true }}>{sourcePhrase}</span>
        {/if}
        {#if snapshot.destination}
            <span class="arrow" aria-hidden="true">&#x2192;</span>
            <span class="path dest" use:tooltip={{ text: snapshot.destination, overflowOnly: true }}>{destName}</span>
        {/if}
    </div>

    <span
        class="status-cell"
        class:running={isRunning && !awaitingAnswer}
        class:paused={isPaused}
        class:queued={isQueued}
        class:failed={isFailed}
        class:awaiting-answer={awaitingAnswer}
        use:tooltip={awaitingAnswer ? tString('queue.row.awaitingAnswerTooltip') : undefined}
    >
        {#if awaitingAnswer}
            <!-- ❌ Not the spinner a running row gets: nothing is turning. -->
            <Icon name="circle-alert" size={14} />
        {:else if isRunning}
            <Spinner size="sm" />
        {:else if isFailed}
            <Icon name="triangle-alert" size={14} />
        {/if}
        <span class="status-text">{statusLabel}</span>
    </span>

    <div class="actions-cell">
        {#if canForeground}
            <!-- ❌ Not a command on the operation: nothing about it changes, only
                 where it's shown. The main window decides whether it can take
                 it and says so there, because that's where the answer lands. -->
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void requestForegroundOperation(snapshot.operationId)}
                aria-label={tString('queue.row.foregroundAria')}
            >
                <span class="btn-inner">
                    <Icon name="app-window" size={13} />
                    {tString('queue.row.foreground')}
                </span>
            </Button>
        {/if}
        {#if isRunning || isPaused}
            <!-- Offered during the scan as much as during the write: the walk
                 parks on the same gate the drivers do. A row that showed no
                 Resume on a scan paused from Pause all left it with no way back
                 except Cancel. -->
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void op?.togglePause()}
                aria-label={pauseResumeAria}
            >
                <span class="btn-inner">
                    <Icon name={isPaused ? 'play' : 'pause'} size={13} />
                    {pauseResumeLabel}
                </span>
            </Button>
        {/if}
        {#if isActionable}
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void op?.cancel()}
                aria-label={tString('queue.row.cancelAria')}
            >
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.cancel')}
                </span>
            </Button>
        {/if}
        {#if isFailed}
            <Button variant="secondary" size="mini" onclick={onDismiss} aria-label={tString('queue.row.dismissAria')}>
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.dismiss')}
                </span>
            </Button>
        {/if}
        {#if canRollback}
            <!-- Danger, like the progress dialog's: the same click stops the same
                 operation, so it can't read as gentler here. What that click DOES
                 to the files is the confirmation's job to say. -->
            <Button
                variant="danger"
                size="mini"
                tooltipContent={tString(inFlightRollbackTooltipKey(inFlightVariant))}
                onclick={() => {
                    rollbackAsked = true
                }}
            >
                <span class="btn-inner">
                    <Icon name="rotate-ccw" size={13} />
                    {tString('fileOperations.transferProgress.conflictRollback')}
                </span>
            </Button>
        {/if}
        <!-- Gated on `canRollback` as well, so an operation that finishes while
             the question is up takes the question with it: there is nothing
             left to undo, and the row beneath already says so. -->
        {#if rollbackAsked && canRollback}
            <RollbackConfirmDialog
                variant={inFlightVariant}
                onConfirm={() => {
                    rollbackAsked = false
                    void op?.rollback()
                }}
                onCancel={() => {
                    rollbackAsked = false
                }}
            />
        {/if}
    </div>

    <!-- Second line for a failure: why it stopped and what to do about it, in
         full. The queue is the surface that promises completeness, so nothing
         is truncated here (the toast is the one that has to abbreviate).
         The pipeline's own title is left out on purpose: it would read
         "Couldn't copy" right beside the status cell's "Couldn't finish". -->
    {#if reason}
        <div class="reason-cell">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `failureReasonFor`: escaped names/paths plus size tiers, no user input. Same boundary as `FallbackErrorContent`. -->
            <p class="reason-message selectable">{@html reason.message}</p>
            <p class="reason-suggestion selectable">{reason.suggestion}</p>
        </div>
    {/if}

    <!-- Second line while the operation is still counting: the same scan-phase
         line the progress dialog shows, so the two surfaces can't disagree
         about what a scanning operation looks like. -->
    {#if showScanLine && progress}
        <div class="readout-cell">
            <ScanPhaseBody
                density="compact"
                sourceFolderPath={snapshot.source ?? ''}
                scanFilesFound={progress.filesDone}
                scanDirsFound={progress.dirsDone ?? 0}
                scanBytesFound={progress.bytesDone}
                scanFilesPerSec={scanRates?.filesPerSecond ?? null}
                scanBytesPerSec={scanRates?.bytesPerSecond ?? null}
                scanCurrentDir={progress.currentDir ?? null}
                currentFile={progress.currentFile}
                paused={isPaused}
            />
        </div>
    {/if}

    <!-- Second line, spanning everything right of the icon gutter: the same
         dual-bar readout the copy dialog shows, in its compact density. -->
    {#if showReadout && progress}
        <div class="readout-cell">
            <TransferProgressReadout
                density="compact"
                bytesDone={progress.bytesDone}
                bytesTotal={progress.bytesTotal}
                filesDone={progress.filesDone}
                filesTotal={progress.filesTotal}
                bytesPerSecond={byteRate}
                filesPerSecond={fileRate}
                {etaSeconds}
                {stall}
                countKind={progressCountKind(opKindForWireType(snapshot.operationType), progress.phase)}
            />
        </div>
    {/if}
</li>

<style>
    .queue-row {
        display: grid;
        /* One row of chrome (select, type, summary, status, actions) with the
           readout on a second line under the summary. The bars need the full
           width far more than they need to sit beside the buttons. */
        grid-template-columns: auto auto minmax(0, 1fr) auto auto;
        align-items: center;
        gap: var(--spacing-xs) var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-md);
        border: 1px solid transparent;
    }

    .queue-row.selected {
        background: var(--color-accent-subtle);
        border-color: var(--color-border-subtle);
    }

    .select-cell {
        display: flex;
        align-items: center;
    }

    .type-cell {
        display: flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .summary-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
        font-size: var(--font-size-sm);
    }

    .readout-cell {
        grid-column: 3 / -1;
        min-width: 0;
    }

    /* The failure's reason takes the readout's line: a settled row has no bars
       to draw, and the prose needs the same full width. An interpolated path
       can be arbitrarily long, so it wraps mid-token rather than pushing the
       row sideways. */
    .reason-cell {
        grid-column: 3 / -1;
        min-width: 0;
        overflow-wrap: anywhere;
    }

    .reason-message {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .reason-suggestion {
        margin: var(--spacing-xxs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    /* The reason is the one thing on a row worth copying out of it. */
    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .op-label {
        font-weight: 500;
        color: var(--color-text-primary);
        flex-shrink: 0;
    }

    .path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
        min-width: 0;
    }

    .path.dest {
        color: var(--color-accent-text);
    }

    .arrow {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .status-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .status-cell.running {
        color: var(--color-accent-text);
    }

    .status-cell.paused {
        color: var(--color-text-secondary);
    }

    /* Queued reads as "waiting", a notch quieter than running/paused. */
    .status-cell.queued {
        color: var(--color-text-tertiary);
    }

    /* The one live row that wants somebody: warmer than running, and not the
       error red a failure owns, because nothing has gone wrong here. */
    .status-cell.awaiting-answer {
        color: var(--color-warning-text);
    }

    /* A failure is the one row state that earns a colour, and severity follows
       the THING, not the surface: this row names the failure and prints its
       reason underneath, so it's error red, like the toast that does the same.
       The corner chip stays amber because it names nothing. */
    .status-cell.failed {
        color: var(--color-error-text);
    }

    .actions-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
