/**
 * One operation, watched. A session binds to an `operationId`, reads the window's
 * event fan-out, and exposes what that operation IS right now: its lifecycle
 * status, its phase, its counts, its rates, its smoothed ETA, and how it ended.
 *
 * It does not know whether anything is rendering it. Zero views is an ordinary
 * state (that is what "backgrounded" means), and two views of one operation share
 * one session, which is what keeps "one operation, one truth" structural rather
 * than remembered: the ETA smoother and the scan-rate estimator are both
 * STATEFUL, so a second one started at a different moment disagrees with the
 * first for as long as it takes to converge. See `DETAILS.md`.
 *
 * It also holds the commands: pause, resume, cancel, rollback, and answering a
 * clash all go through the session, so the operation never has to care which
 * surface pressed the button and every other surface watching it sees that the
 * button was pressed. Their guards and their IPC live in
 * `operation-session-commands.svelte.ts`.
 *
 * ## No `$derived` in here
 *
 * Every field is `$state`, read through a getter. A session outlives the view
 * that first asked for it (that is the whole point), and a `$derived` created
 * while a component is initialising belongs to that component's reactive scope,
 * so it would be torn down when the view unmounts while the session lived on.
 * Composed values are built in the getter instead; consumers wrap them in their
 * own `$derived`, in their own scope, where the ownership is correct.
 */

import { listOperations, type OperationSnapshot, type WriteProgressEvent } from '$lib/tauri-commands'
import type { WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent } from '$lib/tauri-commands'
import type { WriteOperationPhase } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import { createEtaSmoother, transferReadout, type TransferReadout } from '../progress-readout'
import { ScanThroughput } from '../scan-throughput'
import type { BytesPerSecond, Seconds } from '$lib/units'
import type { ConflictResolution } from '$lib/file-explorer/types'
import type { OperationDelivery, OperationEventFanout } from './operation-event-fanout'
import { createOperationSessionCommands, type OperationSessionCommands } from './operation-session-commands.svelte'
import type { ConflictId, ConflictResolutionOutcome } from '$lib/tauri-commands'

const log = getAppLogger('operationSession')

/** How an operation ended, as the session learned it.
 *
 *  `gone` is the seeding miss case: the window asked `list_operations()` for an
 *  operation nobody had told it anything about, and the registry didn't have it.
 *  A terminal operation leaves the snapshot entirely, so "already over" is the
 *  only honest reading, and it beats sitting empty forever. */
export type OperationOutcome =
  | { kind: 'complete'; event: WriteCompleteEvent }
  | { kind: 'error'; event: WriteErrorEvent }
  | { kind: 'cancelled'; event: WriteCancelledEvent }
  | { kind: 'gone' }

/** What the scanning phase says about itself. Zeroes outside a scan. */
export interface ScanReadout {
  filesFound: number
  dirsFound: number
  bytesFound: number
  currentDir: string | null
  /** Frontend-computed, because the backend emits no rate while scanning. */
  filesPerSecond: number | null
  bytesPerSecond: number | null
}

export interface OperationSession extends OperationSessionCommands {
  readonly operationId: string
  /** The operation's row in the registry snapshot, or `null` until one arrives.
   *  Holds its LAST known row after the operation leaves the snapshot. */
  readonly snapshot: OperationSnapshot | null
  /** The lifecycle status: `queued` / `running` / `paused` / … . The
   *  bar-is-moving truth, and the only one every surface reads. */
  readonly status: OperationSnapshot['status'] | null
  /** The latest `write-progress` tick, or `null` before the first one. */
  readonly progress: WriteProgressEvent | null
  /** What the operation is doing: `scanning` while it counts, then the write
   *  phases. `null` before the first tick. */
  readonly phase: WriteOperationPhase | null
  /** The latest tick's numbers, branded. `null` before the first tick. */
  readonly readout: TransferReadout | null
  /** The speed to DISPLAY: the backend's byte rate, or `null` while a person is
   *  deciding something (a pause, an unanswered clash), where the last measured
   *  rate describes a transfer that has stopped. Every view renders this, never
   *  `progress.bytesPerSecond`. */
  readonly bytesPerSecondDisplay: BytesPerSecond | null
  /** The file rate to DISPLAY, on the same terms as
   *  {@link bytesPerSecondDisplay}. */
  readonly filesPerSecondDisplay: number | null
  /** The ETA to DISPLAY: the backend's, through this session's smoother. Every
   *  view of this operation renders this, never `progress.etaSeconds`. Unlike
   *  the two rates it SURVIVES a pause and a clash — how much longer this will
   *  take is exactly what someone deciding wants to know, and the backend keeps
   *  it honest by leaving their thinking time out of the rate window. */
  readonly etaSecondsDisplay: Seconds | null
  readonly scan: ScanReadout
  /** The operation has stopped on a clash nobody has answered yet — a thing to
   *  DO, unlike the pause the lifecycle status already names. For a surface
   *  saying "this one needs you"; the numbers above hide themselves off the
   *  same fact, so no two views can word one wait differently. */
  readonly awaitingAnswer: boolean
  /** The conflict the operation is parked on, if any. Cleared once the backend
   *  has ruled on it, whichever surface asked. */
  readonly conflict: WriteConflictEvent | null
  /** How it ended, or `null` while it's live. Write-once. */
  readonly outcome: OperationOutcome | null
  /** Whether the operation is over. Comes from the terminal EVENTS (or the
   *  seeding miss case), never from leaving the snapshot: "removed" is what a
   *  completed, a cancelled, and a never-existed operation all look like. */
  readonly settled: boolean
  /** Whether `write-settled` has landed: the backend task is fully torn down.
   *  Separate from {@link settled}, which is about the outcome. */
  readonly settleEventReceived: boolean
  /** The registry published a snapshot without this operation, having carried it
   *  in an earlier one. Membership, not an outcome: it says the backend has let
   *  the operation go, never HOW it ended, so a surface that reports an ending
   *  reads {@link settled} and a surface that only offers CONTROLS reads this.
   *
   *  It exists for the operation-log reversal, which emits progress but no
   *  terminal event of its own: leaving the registry is the only word its end
   *  ever gets, and without this its Pause and Cancel stay live and do nothing.
   *  ❌ Never fold it into `settled`: the two arrive on different Tauri channels,
   *  so a removal can beat the `write-complete` it followed, and an outcome is
   *  write-once. */
  readonly leftRegistry: boolean
  /** Detach from the fan-out and stop updating. The registry calls this when
   *  the last view releases the operation. */
  dispose: () => void
}

export function createOperationSession(operationId: string, fanout: OperationEventFanout): OperationSession {
  let snapshot = $state.raw<OperationSnapshot | null>(null)
  let progress = $state.raw<WriteProgressEvent | null>(null)
  let readout = $state.raw<TransferReadout | null>(null)
  let etaSecondsDisplay = $state.raw<Seconds | null>(null)
  let conflict = $state.raw<WriteConflictEvent | null>(null)
  let outcome = $state.raw<OperationOutcome | null>(null)
  let settleEventReceived = $state(false)
  let leftRegistry = $state(false)

  let scanFilesFound = $state(0)
  let scanDirsFound = $state(0)
  let scanBytesFound = $state(0)
  let scanCurrentDir = $state.raw<string | null>(null)
  let scanFilesPerSecond = $state.raw<number | null>(null)
  let scanBytesPerSecond = $state.raw<number | null>(null)

  // Both are stateful estimators, and both are why two views must share one
  // session: fed the same samples from different starting points, they disagree.
  const etaSmoother = createEtaSmoother()
  const scanThroughput = new ScanThroughput()

  /** Anything the fan-out has handed us: a snapshot row, a buffered event, a
   *  live event. Gates the seed, which is older by the time it resolves. */
  let receivedDelivery = false
  let disposed = false

  /** Read through a call rather than the flag, because the attach below sets it
   *  from inside a callback and straight-line type narrowing can't see that. */
  function heardAnything(): boolean {
    return receivedDelivery
  }

  function applyProgress(event: WriteProgressEvent): void {
    // A phase change resets the backend's own estimator, so the displayed
    // number re-warms with it instead of dragging the last phase's value along.
    if (progress !== null && event.phase !== progress.phase) {
      etaSmoother.reset()
      etaSecondsDisplay = null
      if (progress.phase === 'scanning') {
        scanThroughput.reset()
        scanFilesPerSecond = null
        scanBytesPerSecond = null
      }
    }

    progress = event
    const branded = transferReadout(event)
    readout = branded
    etaSecondsDisplay = etaSmoother.push(branded.etaSeconds)

    if (event.phase === 'scanning') {
      scanFilesFound = event.filesDone
      scanDirsFound = event.dirsDone ?? 0
      scanBytesFound = event.bytesDone
      scanCurrentDir = event.currentDir ?? null
      const rates = scanThroughput.push({
        timestampMs: Date.now(),
        files: event.filesDone,
        bytes: event.bytesDone,
      })
      scanFilesPerSecond = rates.filesPerSecond
      scanBytesPerSecond = rates.bytesPerSecond
    }
  }

  /** Whether the operation is standing still because a PERSON is deciding
   *  something: they paused it, or they haven't answered its clash yet.
   *
   *  A parked operation emits no further ticks, so the last speed it measured
   *  sits there describing a transfer that has stopped, and "4.1 MB/s" over a
   *  frozen copy is a number nobody can stand behind. The ETA is a different
   *  claim and stays: the backend excludes human-wait time from the rate window
   *  (`write_operations/human_wait.rs`), so "58s left" is still what remains
   *  once the person is done, which is exactly what they want to know while
   *  deciding.
   *
   *  Two signals, because two shapes of operation report differently: the
   *  lifecycle STATUS carries the pause, and the backend's own wait
   *  classification carries the clash (an operation parked on one is still
   *  `running`). Both are known-facts tests, so a session
   *  that hasn't heard anything yet says "not waiting" and its first frames
   *  render normally rather than blanking a running transfer. */
  function awaitingHuman(): boolean {
    return snapshot?.status === 'paused' || awaitingAnswer()
  }

  /** The narrower half of `awaitingHuman`: the operation is parked on a clash
   *  nobody has answered yet, which is a thing to DO rather than a thing that
   *  was done. A pause is the other half, and the lifecycle status already
   *  names that one, so a view wanting to say "this one needs you" wants this. */
  function awaitingAnswer(): boolean {
    return progress?.activity?.waitingOn === 'conflict'
  }

  /** First outcome wins: a cancel that races a completion must not flip the
   *  answer under a view that already rendered it. */
  function settle(next: OperationOutcome): void {
    if (outcome === null) outcome = next
  }

  function apply(delivery: OperationDelivery): void {
    if (disposed) return
    receivedDelivery = true
    switch (delivery.kind) {
      case 'snapshot':
        snapshot = delivery.snapshot
        leftRegistry = false
        break
      case 'absent':
        // Only a session that has HELD a row can read an absence as "it left".
        // Before that, absence is the ordinary state of an operation this window
        // has not been told about yet.
        if (snapshot !== null) leftRegistry = true
        break
      case 'progress':
        applyProgress(delivery.event)
        break
      case 'complete':
        settle({ kind: 'complete', event: delivery.event })
        break
      case 'error':
        settle({ kind: 'error', event: delivery.event })
        break
      case 'cancelled':
        settle({ kind: 'cancelled', event: delivery.event })
        break
      case 'settled':
        settleEventReceived = true
        break
      case 'conflict':
        conflict = delivery.event
        break
      case 'conflictResolved':
        // Somebody answered it: this window, another window, or an agent over
        // MCP. Only the clash it NAMES goes — the operation raises its next one
        // the moment it takes an answer, and dropping "whatever we're holding"
        // would throw away a live question and park the transfer with nothing
        // on screen.
        if (conflict?.conflictId === delivery.event.conflictId) conflict = null
        break
    }
  }

  // The toggle steers by the lifecycle status this session already holds. ❌ Never
  // a round trip: the answer is on screen, and one asked for would arrive
  // describing a state the user may have changed while it was in flight.
  const commands = createOperationSessionCommands(operationId, () => snapshot?.status === 'paused')

  // Claim, flush, and go live are ONE synchronous block. Nothing may `await`
  // between the attach and the return: an event that arrived while a promise
  // settled would be delivered out of order, and the ETA smoother is stateful.
  const attachment = fanout.attach(operationId, apply)

  /**
   * Recover an operation this window knows nothing about yet: a reload lands
   * mid-transfer, or a view adopts an operation started elsewhere.
   *
   * Deliberately fire-and-forget, and deliberately guarded. `list_operations()`
   * is an `await` away, so anything the fan-out delivers in the meantime is
   * FRESHER than the seed, and applying the seed on top would overwrite a
   * terminal event with a stale "still running". Same shape (and same guard) as
   * `createOperationsStore.init`.
   */
  async function seed(): Promise<void> {
    try {
      const operations = await listOperations()
      if (disposed || receivedDelivery) return
      const row = operations.find((op) => op.operationId === operationId)
      if (row) {
        snapshot = row
        return
      }
      // Nothing knows this operation: it ended before this window looked.
      log.debug('Seeding found no record for op={operationId}; resolving it as already over', { operationId })
      settle({ kind: 'gone' })
    } catch (error) {
      // A failed seed leaves the session live and event-driven, which is the
      // safe direction: a running operation still reports itself.
      log.warn('Failed to seed session for op={operationId}: {error}', { operationId, error: String(error) })
    }
  }

  // Only for an operation the attach told us nothing about. A live window's
  // fan-out already holds the latest snapshot, so every row that appears while
  // it is up is claimed with its row in hand, and asking the backend again would
  // be one IPC round trip per view for an answer we have.
  if (!heardAnything()) void seed()

  return {
    operationId,
    get snapshot(): OperationSnapshot | null {
      return snapshot
    },
    get status(): OperationSnapshot['status'] | null {
      return snapshot?.status ?? null
    },
    get progress(): WriteProgressEvent | null {
      return progress
    },
    get phase(): WriteOperationPhase | null {
      return progress?.phase ?? null
    },
    get readout(): TransferReadout | null {
      return readout
    },
    get bytesPerSecondDisplay(): BytesPerSecond | null {
      return awaitingHuman() ? null : (readout?.bytesPerSecond ?? null)
    },
    get filesPerSecondDisplay(): number | null {
      return awaitingHuman() ? null : (progress?.filesPerSecond ?? null)
    },
    get etaSecondsDisplay(): Seconds | null {
      return etaSecondsDisplay
    },
    /** The scan's tallies, with its RATES dropped while the person is being
     *  waited on, for the reason `bytesPerSecondDisplay` drops the write's: a
     *  parked walk emits no ticks, so the last rate would sit frozen on screen
     *  describing a scan that is standing still. The tallies stay — they are
     *  what the walk has found, and that is still true while it waits. */
    get scan(): ScanReadout {
      const waiting = awaitingHuman()
      return {
        filesFound: scanFilesFound,
        dirsFound: scanDirsFound,
        bytesFound: scanBytesFound,
        currentDir: scanCurrentDir,
        filesPerSecond: waiting ? null : scanFilesPerSecond,
        bytesPerSecond: waiting ? null : scanBytesPerSecond,
      }
    },
    get awaitingAnswer(): boolean {
      return awaitingAnswer()
    },
    get conflict(): WriteConflictEvent | null {
      return conflict
    },
    get outcome(): OperationOutcome | null {
      return outcome
    },
    get settled(): boolean {
      return outcome !== null
    },
    get settleEventReceived(): boolean {
      return settleEventReceived
    },
    get leftRegistry(): boolean {
      return leftRegistry
    },

    pause: commands.pause,
    resume: commands.resume,
    togglePause: commands.togglePause,
    cancel: commands.cancel,
    rollback: commands.rollback,
    /** Answers the clash named by `conflictId` and lets go of THAT clash. Any
     *  verdict settles the question — the backend arbitrates between whoever
     *  answered — so only a call that never landed leaves the prompt up for
     *  another try.
     *
     *  Which clash is let go of is the whole point of the id. The backend raises
     *  the next one the moment it takes this answer, so a newer clash routinely
     *  lands in the slot while this call is still in the air; clearing whatever
     *  sits there on return would throw that one away, and the transfer would
     *  park forever with nothing on screen. */
    async resolveConflict(
      conflictId: ConflictId,
      resolution: ConflictResolution,
      applyToAll: boolean,
    ): Promise<ConflictResolutionOutcome | null> {
      const outcome = await commands.resolveConflict(conflictId, resolution, applyToAll)
      if (outcome !== null && conflict?.conflictId === conflictId) conflict = null
      return outcome
    },
    get pauseInFlight(): boolean {
      return commands.pauseInFlight
    },
    get cancelling(): boolean {
      return commands.cancelling
    },
    get rollingBack(): boolean {
      return commands.rollingBack
    },
    get resolvingConflict(): boolean {
      return commands.resolvingConflict
    },

    dispose(): void {
      disposed = true
      attachment.detach()
    },
  }
}
