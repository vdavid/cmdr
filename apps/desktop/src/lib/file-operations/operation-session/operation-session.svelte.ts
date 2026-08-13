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
import type { ConflictResolutionOutcome } from '$lib/tauri-commands'

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
   *  bar-is-moving truth, never `is_running`. */
  readonly status: OperationSnapshot['status'] | null
  /** The latest `write-progress` tick, or `null` before the first one. */
  readonly progress: WriteProgressEvent | null
  /** What the operation is doing: `scanning` while it counts, then the write
   *  phases. `null` before the first tick. */
  readonly phase: WriteOperationPhase | null
  /** The latest tick's numbers, branded. `null` before the first tick. */
  readonly readout: TransferReadout | null
  /** The speed to DISPLAY: the backend's byte rate, or `null` when there's
   *  nothing honest to show. Every view renders this, never
   *  `progress.bytesPerSecond`. */
  readonly bytesPerSecondDisplay: BytesPerSecond | null
  /** The file rate to DISPLAY, on the same terms as
   *  {@link bytesPerSecondDisplay}. */
  readonly filesPerSecondDisplay: number | null
  /** The ETA to DISPLAY: the backend's, through this session's smoother. Every
   *  view of this operation renders this, never `progress.etaSeconds`. */
  readonly etaSecondsDisplay: Seconds | null
  readonly scan: ScanReadout
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

  /** Whether the operation is actually moving right now. A paused one emits no
   *  further ticks, so the last rate and ETA it measured stay put, describing a
   *  transfer that has stopped: a speed and a "58s left" that nobody can stand
   *  behind. The three display getters below answer `null` instead, so no
   *  surface can render them, and the lifecycle STATUS decides it — a parked
   *  operation still answers `is_running: true`. */
  function moving(): boolean {
    return snapshot?.status !== 'paused'
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
    }
  }

  // The lifecycle status is what the toggle steers by. ❌ Never `is_running`: a
  // parked operation stays in the write-op state map and answers `true` there,
  // so a toggle reading it would try to pause what is already paused.
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
      return moving() ? (readout?.bytesPerSecond ?? null) : null
    },
    get filesPerSecondDisplay(): number | null {
      return moving() ? (progress?.filesPerSecond ?? null) : null
    },
    get etaSecondsDisplay(): Seconds | null {
      return moving() ? etaSecondsDisplay : null
    },
    get scan(): ScanReadout {
      return {
        filesFound: scanFilesFound,
        dirsFound: scanDirsFound,
        bytesFound: scanBytesFound,
        currentDir: scanCurrentDir,
        filesPerSecond: scanFilesPerSecond,
        bytesPerSecond: scanBytesPerSecond,
      }
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

    pause: commands.pause,
    resume: commands.resume,
    togglePause: commands.togglePause,
    cancel: commands.cancel,
    rollback: commands.rollback,
    /** Answers the clash and lets go of it. Any verdict settles the question —
     *  the backend arbitrates between whoever answered — so only a call that
     *  never landed leaves the prompt up for another try. */
    async resolveConflict(
      resolution: ConflictResolution,
      applyToAll: boolean,
    ): Promise<ConflictResolutionOutcome | null> {
      const outcome = await commands.resolveConflict(resolution, applyToAll)
      if (outcome !== null) conflict = null
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
