/**
 * The single reactive source the queue window renders from.
 *
 * Two streams merge here (per the plan's "subscribe, don't poll"):
 *  - `operations-changed`: the THIN registry snapshot (membership + lifecycle
 *    status). This decides which rows exist and each row's status. It does NOT
 *    carry 200 ms progress, so it stays cheap.
 *  - `write-progress`: the existing per-file progress stream, keyed by
 *    `operationId`. This drives the live per-row bars / ETA. Progress for an op
 *    no longer in the snapshot is dropped.
 *
 * A row is `OperationSnapshot` (the membership/status fact) plus the latest
 * `WriteProgressEvent` for that op (or `null` before the first tick). The window
 * reads `getOperations()`; M4's auto-queue surfacing reads the same store.
 *
 * IMPORTANT: a paused op still reports `is_running: true` from the backend
 * status query (it stays in the write-operation-state map). The bar-is-moving
 * truth is the SNAPSHOT `status`, never `is_running`. Read `row.status`.
 */

import {
  listOperations,
  onOperationsChanged,
  type OperationSnapshot,
  type UnlistenFn,
  type WriteProgressEvent,
} from '$lib/tauri-commands'
import { onWriteProgress } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('queue')

/** One operation as the window renders it: its membership/status snapshot plus
 *  the latest live progress (null until the first `write-progress` tick). */
export interface OperationRow {
  snapshot: OperationSnapshot
  progress: WriteProgressEvent | null
}

/** Lifecycle statuses that mean the op is finished and stops being actionable.
 *  Kept as a typed set (not a string-substring test) per `no-string-matching`. */
const TERMINAL_STATUSES = new Set<OperationSnapshot['status']>(['done', 'cancelled', 'failed'])

export function isTerminalStatus(status: OperationSnapshot['status']): boolean {
  return TERMINAL_STATUSES.has(status)
}

/**
 * Creates an operations store instance. One per queue window. Call `init()`
 * after mount (it seeds from `list_operations` and subscribes to both streams)
 * and `dispose()` on teardown (it drops both listeners).
 */
export function createOperationsStore() {
  // The snapshot rows, in arrival order. `operations-changed` replaces the whole
  // set each tick, so we re-key the progress map against it.
  let snapshots = $state<OperationSnapshot[]>([])
  // Latest progress per operationId. A plain object keyed by id; pruned to the
  // current snapshot membership on every snapshot tick so a finished op's
  // progress can't leak.
  let progressById = $state<Record<string, WriteProgressEvent>>({})

  let unlistenSnapshots: UnlistenFn | null = null
  let unlistenProgress: UnlistenFn | null = null
  let disposed = false

  /** The merged rows the window renders. Reactive. */
  const operations = $derived<OperationRow[]>(
    snapshots.map((snapshot) => ({
      snapshot,
      progress: progressById[snapshot.operationId] ?? null,
    })),
  )

  function applySnapshot(next: OperationSnapshot[]) {
    snapshots = next
    // Prune progress for ops that left the snapshot so the map can't grow without
    // bound and a stale bar can't outlive its row. A transient local lookup, not
    // reactive state, so a plain Set is right here (SvelteSet is for $state).
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local, not reactive state
    const liveIds = new Set(next.map((op) => op.operationId))
    const pruned: Record<string, WriteProgressEvent> = {}
    for (const [id, event] of Object.entries(progressById)) {
      if (liveIds.has(id)) pruned[id] = event
    }
    progressById = pruned
  }

  function applyProgress(event: WriteProgressEvent) {
    // Ignore progress for ops we don't (yet) know about: the snapshot is the
    // membership source of truth. The op will get its bar once its snapshot
    // arrives and a later progress tick lands.
    if (!snapshots.some((op) => op.operationId === event.operationId)) return
    progressById = { ...progressById, [event.operationId]: event }
  }

  async function init(): Promise<void> {
    try {
      // Subscribe BEFORE seeding so a tick between seed and subscribe isn't
      // missed (mirrors the dialog's subscribe-then-start ordering).
      unlistenSnapshots = await onOperationsChanged((event) => {
        applySnapshot(event.operations)
      })
      unlistenProgress = await onWriteProgress((event) => {
        applyProgress(event)
      })
      // If we were disposed while awaiting, undo the subscriptions.
      if (disposed) {
        unlistenSnapshots()
        unlistenProgress()
        unlistenSnapshots = null
        unlistenProgress = null
        return
      }
      const initial = await listOperations()
      // A snapshot tick may have already populated `snapshots` while we awaited;
      // only seed if it hasn't, so we don't clobber fresher data with the seed.
      if (snapshots.length === 0) applySnapshot(initial)
    } catch (error) {
      // Perms / IPC failures must surface as a log line, not a dead window.
      log.warn('Failed to initialize operations store: {error}', { error: String(error) })
    }
  }

  function dispose(): void {
    disposed = true
    unlistenSnapshots?.()
    unlistenProgress?.()
    unlistenSnapshots = null
    unlistenProgress = null
  }

  return {
    /** The merged, reactive rows the window renders. */
    get operations(): OperationRow[] {
      return operations
    },
    /** Whether any operation is currently running (not paused, not terminal). */
    get hasRunning(): boolean {
      return snapshots.some((op) => op.status === 'running')
    },
    /** Whether any operation is currently paused. */
    get hasPaused(): boolean {
      return snapshots.some((op) => op.status === 'paused')
    },
    init,
    dispose,
    // Test seam: drive the reducers directly without a live backend.
    _testApplySnapshot: applySnapshot,
    _testApplyProgress: applyProgress,
  }
}
