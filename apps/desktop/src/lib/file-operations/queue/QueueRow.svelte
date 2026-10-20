<script lang="ts">
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { isInstantOperation, type OperationRow } from './operations-store.svelte'
    import { operationTypeIcon } from './operation-icon'
    import { failureIncidentFor, failureReasonFor } from './failure-reason'
    import ReportOffer from '#lib/error-reporter/ReportOffer.svelte'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import RollbackConfirmDialog from '../RollbackConfirmDialog.svelte'
    import ScanPhaseBody from '../transfer/ScanPhaseBody.svelte'
    import { stallIncidentFor, stallNoticeFor, waitLineFor } from '../transfer/transfer-stall'
    import { bindOperationSession } from '../operation-session/bind-operation-session.svelte'
    import {
        inFlightRollbackTooltipKey,
        inFlightRollbackVariant,
        reversalWindowClosed,
        rollbackConfirmVariant,
        reversalLabelKey,
    } from '../reversal-wording'
    import { opKindForWireType } from '../op-kind'
    import { archivePhaseLabel, isIndeterminateProgressPhase, progressCountKind } from '../progress-readout'
    import { requestForegroundOperation, retryFailedOperation } from '#lib/tauri-commands/index.js'
    import { offersRetry } from '../transfer/transfer-error-messages'
    import TransferTree from '../transfer-tree/TransferTree.svelte'
    import { rowTreeOperation } from './row-tree'
    import QueueRowSummary from './QueueRowSummary.svelte'
    import { queueKeyAction } from './queue-keys'
    import type { QueueRowOwnMenuFacts } from './queue-row-menu'
    import { QUEUE_ROW_KEYS } from '../decision-keys'
    import { getAppLogger } from '#lib/logging/logger.js'

    const log = getAppLogger('queue')

    interface Props {
        row: OperationRow
        selected: boolean
        /** The window's row cursor is on this row. The page owns the cursor and the keys;
         *  the row only draws it. DETAILS § Keyboard. */
        isCursor?: boolean
        /** This row is the grid's one Tab stop (roving tabindex): the cursor row, or the
         *  first row before there's a cursor. */
        tabbable?: boolean
        /** Two or more rows show a tree, so this one's tree gets a collapse toggle. A
         *  lone tree is always open. The page decides: it's a question about the list. */
        treeCollapsible?: boolean
        /** Focus landed on the row or one of its buttons, so the cursor follows it there. */
        onFocusRow?: () => void
        onToggleSelect: () => void
        /** A right-click on the row: the page opens its menu (`queue-row-menu.ts`). */
        onContextMenu?: () => void
        /** Stop showing a retained failure. Not a command on the operation (it
         *  has none left): the page drops the row the backend retained. The only
         *  way one leaves the list — no timer, no window close, no next
         *  operation. */
        onDismiss: () => void
        /** Rows reserve the grip's column while ANY row can be dragged, so the
         *  checkboxes stay in one line. The page decides: it's a question about the list. */
        gripColumn?: boolean
        /** This row shows a grip: it's waiting, and there's another waiting row to trade with. */
        draggable?: boolean
        /** A press on the grip: the page's drag controller (`queue-drag.svelte.ts`) takes it. */
        onGripPointerDown?: (event: PointerEvent) => void
        /** How far a drag has moved this row from its slot, in px; `null` when it isn't dragged. */
        dragOffset?: number | null
        /** The pointer holds this row (it floats above the list while it does). */
        lifted?: boolean
        /** A dragged row would land in the gap above or below this one. */
        dropCue?: 'above' | 'below' | null
    }

    const {
        row,
        selected,
        isCursor = false,
        tabbable = false,
        treeCollapsible = false,
        onFocusRow,
        onToggleSelect,
        onContextMenu,
        onDismiss,
        gripColumn = false,
        draggable = false,
        onGripPointerDown,
        dragOffset = null,
        lifted = false,
        dropCue = null,
    }: Props = $props()

    /** Prefix for the ids the row's name and description point at. */
    const domId = $props.id()
    let rowEl: HTMLDivElement | undefined = $state()
    /** The row's live tree, which hears F from anywhere in the row (`resumeTreeFollowing`). */
    let treeView: { handleFollowKey(event: KeyboardEvent): boolean } | undefined = $state()

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

    /** Retry, beside Dismiss: the backend kept how this operation started
     *  (`canRetry`), and the failure is one the error dialog would offer Retry
     *  for too (`offersRetry`, the rule both share). It starts a NEW operation,
     *  whose row replaces this one. */
    const canRetry = $derived(isFailed && snapshot.canRetry && snapshot.error !== null && offersRetry(snapshot.error))

    /** A refused retry (the volume is still gone) leaves this row as it was, so
     *  the log is the only place it shows. */
    async function retry(): Promise<void> {
        const operationId = snapshot.operationId
        try {
            await retryFailedOperation(operationId)
        } catch (error) {
            log.warn('Retrying op={operationId} was refused: {error}', { operationId, error })
        }
    }

    /** Why it stopped, in the error dialog's own words. Null on every live row.
     *  `reason.message` is markup from the pipeline (escaped names, size tiers),
     *  so it goes through `{@html}` exactly as the dialog's body does. */
    const reason = $derived(failureReasonFor(snapshot))
    /** What this failure's "Send error report…" reports (the main window opens it). */
    const failureIncident = $derived(failureIncidentFor(snapshot, 'queueFailure'))

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
     *  task is waiting on the `TransferDialog` preview. `rollbackSupport` is a
     *  promise about the OPERATION, so the phase is what decides which controls
     *  make sense right now. */
    const isScanning = $derived(progress?.phase === 'scanning')

    /** Rollback is offered on exactly the operations the backend can reverse
     *  (`rollbackSupport`), and only while there's still something running to
     *  reverse. A scanning operation has written nothing, so it has nothing to put
     *  back; a move removing its originals has already landed everything, so
     *  nothing can come back (`../reversal-wording.ts`). Same verdict the progress
     *  dialog reaches, same wording. */
    const canRollback = $derived(
        snapshot.rollbackSupport === 'supported' &&
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

    /** Asks the main window to show this operation. A rejected emit is logged,
     *  never swallowed: the main window only learns of a press through this
     *  event, so a request that never left reads as the main window ignoring
     *  the button, with nothing in the log to tell the two apart. */
    async function showInMainWindow(): Promise<void> {
        const operationId = snapshot.operationId
        try {
            await requestForegroundOperation(operationId)
        } catch (error) {
            log.warn('Could not ask the main window to show op={operationId}: {error}', {
                operationId,
                error,
            })
        }
    }

    /** The operation has stopped on a clash nobody has answered yet. The
     *  lifecycle status stays `running` throughout (a clash pauses nothing), so
     *  this is the one thing the snapshot can't tell the row; it comes from the
     *  backend's own wait classification, through the session. */
    const awaitingAnswer = $derived(op?.awaitingAnswer ?? false)

    /** A reversal is named by what it will DO to the files ("Putting files
     *  back"), ❌ never by the operation type it runs as: undoing a move is
     *  journaled as a move and undoing a copy as a delete, so the plain action
     *  word would tell a person their undo is deleting things. */
    const archivePhase = $derived(archivePhaseLabel(progress?.phase ?? null, progress?.step))
    const label = $derived(
        reversalVariant === null
            ? archivePhase === null
                ? tString('queue.row.label', { type: snapshot.operationType })
                : tString(archivePhase.key, archivePhase.params)
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
     *  instead of a "Rolling back…" that hides the pause. */
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
    const isIndeterminate = $derived(isIndeterminateProgressPhase(progress?.phase ?? null))
    const showReadout = $derived(
        (isRunning || isPaused) && !isIndeterminate && progress !== null && (progress.bytesTotal > 0 || progress.filesTotal > 0),
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
    /** The same wait line the dialog shows while bytes are on their way but none
     *  has landed. */
    const waitLine = $derived(waitLineFor(progress?.activity))

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

    /** How many rows the inline tree takes at most: room for a folder and its work,
     *  short enough that the waiting rows below stay in view. */
    const TREE_ROWS = 6

    /** The live tree of what this operation is moving, under its readout, or `null`
     *  (`row-tree.ts`: running or paused, past the scan, never a reversal). */
    const treeOperation = $derived(rowTreeOperation(row))
    /** Shut by hand through the toggle, which only exists while 2+ rows show a tree;
     *  with one left, its tree is open again whatever this says. */
    let treeShut = $state(false)
    const treeOpen = $derived(treeOperation !== null && !(treeCollapsible && treeShut))

    const pauseResumeLabel = $derived(
        isPaused ? tString('queue.row.resume') : tString('queue.row.pause'),
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

    // ── The row's half of the window's keyboard (DETAILS § Keyboard) ─────────
    // The page owns the cursor and reads the keys; what a key DOES to one
    // operation stays here, next to the buttons that do the same, so the two
    // can't drift apart.

    /** Puts keyboard focus on the row itself, where the screen reader reads it. */
    export function focusRow(): void {
        rowEl?.focus({ preventScroll: false })
    }

    /** Enter: the Show button's action, where the row offers Show. `false` where it
     *  doesn't (a waiting, instant, or settled operation). */
    export function show(): boolean {
        if (!canForeground) return false
        void showInMainWindow()
        return true
    }

    /** P: the Pause or Resume button's action, where the row offers it. */
    export function pauseOrResume(): boolean {
        if (!isRunning && !isPaused) return false
        void op?.togglePause()
        return true
    }

    /** ⌫: the row's own Cancel or Dismiss, whichever it offers. */
    export function cancelOrDismiss(): boolean {
        if (isFailed) {
            onDismiss()
            return true
        }
        return cancel()
    }

    /** C: the Cancel button's action. Like the button, no confirmation: cancel keeps
     *  every file already written. */
    export function cancel(): boolean {
        if (!isActionable) return false
        void op?.cancel()
        return true
    }

    /** B: the Rollback button's action, which raises the confirmation; the reversal
     *  starts only from its answer. */
    export function rollback(): boolean {
        if (!canRollback) return false
        rollbackAsked = true
        return true
    }

    /** What the right-click menu offers live, from the same state the guards above read,
     *  so a greyed item is exactly a key that would do nothing here. */
    export function menuFacts(): QueueRowOwnMenuFacts {
        return {
            active: isRunning || isPaused,
            paused: isPaused,
            canShow: canForeground,
            canRollback,
            canCancel: isActionable,
            canDismiss: isFailed,
        }
    }

    /** ← / →: steps keyboard focus through the row's buttons, left to right, and
     *  from the first one back to the row. `false` when there's nowhere to go. */
    export function focusControl(step: -1 | 1): boolean {
        if (!rowEl) return false
        const buttons = [...rowEl.querySelectorAll<HTMLButtonElement>('.actions-cell button:not(:disabled)')]
        const at = buttons.indexOf(document.activeElement as HTMLButtonElement)
        const next = at === -1 ? (step === 1 ? 0 : null) : at + step
        if (next === null || next >= buttons.length) return false
        if (next < 0) {
            rowEl.focus()
            return true
        }
        buttons[next].focus()
        return true
    }

    /** Escape inside the tree hands focus back to the row, where the grid's keys work
     *  again; anywhere else it closes the window. Both read the one `queue.close`
     *  command, so the key can't drift between them. */
    function leaveTree(event: KeyboardEvent): void {
        if (queueKeyAction(event)?.kind !== 'close') return
        event.preventDefault()
        focusRow()
    }

    /** A click anywhere on the row puts the cursor there, the way a click picks a
     *  row in any list. Two clicks are left alone: one that ends a text selection in
     *  the failure reason (focusing would collapse it), and one inside a button that
     *  already holds focus. */
    /** F on this row (the grid's cursor row, one of its buttons, or its tree) resumes its
     *  tree's following while that's paused; the grid has no F of its own. A wheel pauses
     *  the tree without focusing it, so F from the row is the one that has to work. */
    function resumeTreeFollowing(event: KeyboardEvent): void {
        treeView?.handleFollowKey(event)
    }

    function handleRowClick(): void {
        if (!rowEl) return
        const textSelection = window.getSelection()
        if (textSelection && !textSelection.isCollapsed) return
        if (document.activeElement && document.activeElement !== rowEl && rowEl.contains(document.activeElement)) return
        rowEl.focus({ preventScroll: true })
    }

</script>

<!-- A grid row: the page's arrow keys move between rows, ← / → reach the
     buttons, and the row's name is its summary plus its status. Every button
     leaves the Tab order (`tabindex={-1}`), so the list is one Tab stop.
     DETAILS § Keyboard has the grid-over-listbox call. -->
<div
    bind:this={rowEl}
    class="queue-row"
    class:selected
    class:is-under-cursor={isCursor}
    class:is-dragged={dragOffset !== null}
    class:is-lifted={lifted}
    class:is-drop-above={dropCue === 'above'}
    class:is-drop-below={dropCue === 'below'}
    style:transform={dragOffset === null ? undefined : `translateY(${String(dragOffset)}px)`}
    role="row"
    tabindex={tabbable ? 0 : -1}
    aria-selected={isFailed ? undefined : selected}
    aria-labelledby="{domId}-summary {domId}-status"
    aria-describedby={reason ? `${domId}-reason` : undefined}
    data-operation-id={snapshot.operationId}
    data-status={status}
    onfocusin={() => onFocusRow?.()}
    onclick={handleRowClick}
    oncontextmenu={() => onContextMenu?.()}
    onkeydown={resumeTreeFollowing}
>
    <div class="select-cell" role="gridcell">
        <!-- The mouse half of ⌥↑ / ⌥↓, which is what keyboard and screen-reader users
             have, so the grip itself is hidden from them. -->
        {#if gripColumn}
            <span class="grip" class:is-draggable={draggable} aria-hidden="true" onpointerdown={draggable ? onGripPointerDown : undefined}>
                {#if draggable}
                    <Icon name="grip-vertical" size={14} />
                {/if}
            </span>
        {/if}
        <!-- The checkbox exists for "Cancel selected", so a settled failure has
             nothing to offer it. Out of the Tab order and the ← / → walk: Space on
             the row checks it, and the row's `aria-selected` says so. -->
        {#if !isFailed}
            <Checkbox
                checked={selected}
                onCheckedChange={onToggleSelect}
                ariaLabel={tString('queue.row.selectAria')}
                tabindex={-1}
            />
        {/if}
    </div>

    <span class="type-cell" aria-hidden="true">
        <Icon name={typeIcon} size={16} />
    </span>

    <QueueRowSummary id="{domId}-summary" {label} {snapshot} {reversalVariant} />

    <span
        id="{domId}-status"
        role="gridcell"
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

    <!-- A keyed button (Show, Pause / Resume, Cancel, Rollback) has no `aria-label`: its name
         is the visible word (WCAG 2.5.3, label in name; axe counts the `aria-hidden` key chip as
         visible text, so no longer sentence could hold it), the key rides `aria-keyshortcuts`,
         and Show's tooltip says where it goes. -->
    <div class="actions-cell" role="gridcell">
        {#if canForeground}
            <!-- ❌ Not a command on the operation: nothing about it changes, only
                 where it's shown. The main window decides whether it can take
                 it and says so there, because that's where the answer lands. -->
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void showInMainWindow()}
                tooltipContent={tString('queue.row.foregroundTooltip')}
                keyHint={QUEUE_ROW_KEYS.show}
                aria-keyshortcuts="{QUEUE_ROW_KEYS.show} Enter"
                tabindex={-1}
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
                keyHint={QUEUE_ROW_KEYS.pauseOrResume}
                aria-keyshortcuts={QUEUE_ROW_KEYS.pauseOrResume}
                tabindex={-1}
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
                keyHint={QUEUE_ROW_KEYS.cancel}
                aria-keyshortcuts="{QUEUE_ROW_KEYS.cancel} Backspace"
                tabindex={-1}
            >
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.cancel')}
                </span>
            </Button>
        {/if}
        {#if canRetry}
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void retry()}
                aria-label={tString('queue.row.retryAria')}
                tabindex={-1}
            >
                <span class="btn-inner">
                    <Icon name="rotate-cw" size={13} />
                    {tString('queue.row.retry')}
                </span>
            </Button>
        {/if}
        {#if failureIncident}
            <ReportOffer incident={failureIncident} size="mini" />
        {/if}
        {#if isFailed}
            <Button
                variant="secondary"
                size="mini"
                onclick={onDismiss}
                aria-label={tString('queue.row.dismissAria')}
                aria-keyshortcuts="Backspace"
                tabindex={-1}
            >
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
                keyHint={QUEUE_ROW_KEYS.rollback}
                aria-keyshortcuts={QUEUE_ROW_KEYS.rollback}
                tabindex={-1}
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
        {#if treeOperation !== null && treeCollapsible}
            <!-- Last in the row's ← / → walk, like any row button. A constant name with
                 `aria-expanded`, so a screen reader hears the state, not a new label. -->
            <Button
                variant="secondary"
                size="mini"
                onclick={() => {
                    treeShut = !treeShut
                }}
                aria-label={tString('queue.row.treeToggleAria')}
                aria-expanded={treeOpen}
                aria-controls="{domId}-tree"
                tooltipContent={tString('queue.row.treeToggleTooltip', { open: treeOpen ? 'yes' : 'no' })}
                tabindex={-1}
            >
                <span class="btn-inner">
                    <Icon name={treeOpen ? 'chevron-down' : 'chevron-right'} size={13} />
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
        <div class="reason-cell" role="gridcell" id="{domId}-reason">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `failureReasonFor`: escaped names/paths plus size tiers, no user input. Same boundary as `FallbackErrorContent`. -->
            <p class="reason-message selectable">{@html reason.message}</p>
            <p class="reason-suggestion selectable">{reason.suggestion}</p>
        </div>
    {/if}

    <!-- Second line while the operation is still counting: the same scan-phase
         line the progress dialog shows, so the two surfaces can't disagree
         about what a scanning operation looks like. -->
    {#if showScanLine && progress}
        <div class="readout-cell" role="gridcell">
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
                {waitLine}
            />
        </div>
    {/if}

    <!-- Second line, spanning everything right of the icon gutter: the same
         dual-bar readout the copy dialog shows, in its compact density. -->
    {#if showReadout && progress}
        <div class="readout-cell" role="gridcell">
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
                {waitLine}
                countKind={progressCountKind(opKindForWireType(snapshot.operationType), progress.phase)}
            />
            {#if stall}
                <ReportOffer incident={stallIncidentFor(stall, snapshot.operationType)} />
            {/if}
        </div>
    {/if}

    <!-- Third line: the live tree, indented under the readout inside this row, so a
         waiting row below (its own checkbox and icon back at the left edge) reads as
         the next item. Tab reaches it from this row only while it's the grid's cursor
         row; inside, the tree owns the keys and Escape returns here. DETAILS § "The
         inline tree". -->
    {#if treeOperation !== null && treeOpen}
        <!-- The cell only hears Escape bubbling up from the focused tree inside it,
             which is the keyboard path; the cell itself is never a focus target. -->
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="tree-cell" role="gridcell" id="{domId}-tree" data-queue-tree onkeydown={leaveTree}>
            <!-- The page keys each row by its operation, so this tree's id never changes. -->
            <TransferTree
                bind:this={treeView}
                operationId={snapshot.operationId}
                tick={op?.progress ?? null}
                operation={treeOperation}
                defaultRows={TREE_ROWS}
                {tabbable}
            />
        </div>
    {/if}
</div>

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
        /* The drop line hangs off the row, into the gap above or below it. */
        position: relative;
    }

    /* ── Dragging to reorder (DETAILS § "Dragging to reorder") ── */

    .grip {
        display: flex;
        align-items: center;
        justify-content: center;
        width: calc(14px * var(--font-scale));
        align-self: stretch;
        color: var(--color-text-tertiary);
    }

    .grip.is-draggable {
        cursor: grab;
    }

    .grip.is-draggable:hover,
    .queue-row.is-lifted .grip {
        color: var(--color-text-secondary);
    }

    /* A dragged row (held, or dropped and on its way into place) draws above the
       rows it passes. */
    .queue-row.is-dragged {
        z-index: 1;
    }

    /* Lifted off the list: an opaque card with a shadow, a touch larger, so it reads
       as picked up and can't be confused with the rows it floats over. */
    .queue-row.is-lifted {
        background: var(--color-bg-primary);
        border-color: var(--color-border-subtle);
        box-shadow: var(--shadow-md);
        scale: 1.01;
    }

    @media (prefers-reduced-motion: no-preference) {
        .queue-row {
            transition:
                scale 0.12s ease-out,
                box-shadow 0.12s ease-out;
        }
    }

    /* The drop line: an accent rule in the gap the dragged row would land in, with a
       ring at its start, like Finder's. */
    .queue-row.is-drop-above::before,
    .queue-row.is-drop-below::before {
        content: '';
        position: absolute;
        left: var(--spacing-md);
        right: var(--spacing-xs);
        height: 2px;
        border-radius: 1px;
        background: var(--color-accent);
        pointer-events: none;
    }

    .queue-row.is-drop-above::after,
    .queue-row.is-drop-below::after {
        content: '';
        position: absolute;
        left: var(--spacing-xs);
        width: 6px;
        height: 6px;
        border: 2px solid var(--color-accent);
        border-radius: 50%;
        background: var(--color-bg-primary);
        pointer-events: none;
    }

    /* Centered on the gap between rows (the list's `--spacing-xxs`), past the 1px border. */
    .queue-row.is-drop-above::before {
        top: calc(-1px - var(--spacing-xxs) / 2 - 1px);
    }

    .queue-row.is-drop-above::after {
        top: calc(-1px - var(--spacing-xxs) / 2 - 5px);
    }

    .queue-row.is-drop-below::before {
        bottom: calc(-1px - var(--spacing-xxs) / 2 - 1px);
    }

    .queue-row.is-drop-below::after {
        bottom: calc(-1px - var(--spacing-xxs) / 2 - 5px);
    }

    .queue-row.selected {
        background: var(--color-accent-subtle);
        border-color: var(--color-border-subtle);
    }

    /* The cursor is the file list's: a quiet fill with an accent hairline, which
       turns accent once the list has focus (`app-file-list.css`). It wins over
       the selection fill, so the cursor stays findable inside a selected block. */
    .queue-row.is-under-cursor {
        background: var(--color-cursor-inactive);
        box-shadow: inset 0 0 0 1px var(--color-cursor-outline);
    }

    /* While focus is inside the row's live tree, the tree's cursor is the active one, and
       this one dims like an unfocused list's: one accent cursor says where the keys go. */
    :global(.queue-grid:focus-within) .queue-row.is-under-cursor:not(:has([data-queue-tree]:focus-within)) {
        background: var(--color-cursor-active);
    }

    /* The cursor IS the focus indicator for a focused row, so the browser's ring
       would draw a second, competing box. A focused BUTTON keeps the global ring. */
    .queue-row:focus-visible {
        outline: none;
    }

    /* Under the cursor's tint the muted tones read at full contrast, as in
       `ColumnList` (secondary and tertiary drop below AA on the lighter accent
       tints). The status colors that carry meaning keep theirs. The summary's
       paths and arrow live in `QueueRowSummary`, hence the `:global`. */
    .queue-row.is-under-cursor :is(.type-cell, .reason-message, .reason-suggestion),
    .queue-row.is-under-cursor :global(:is(.queue-summary .path, .queue-summary .arrow)),
    .queue-row.is-under-cursor .status-cell:not(.running, .failed, .awaiting-answer) {
        color: var(--color-text-primary);
    }

    .select-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        align-self: stretch;
    }

    .type-cell {
        display: flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .readout-cell {
        grid-column: 3 / -1;
        min-width: 0;
    }

    /* The tree starts where the readout does, so the gutter to its left (empty on
       this row, the checkbox and icon on the next) says whose files these are. */
    .tree-cell {
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
