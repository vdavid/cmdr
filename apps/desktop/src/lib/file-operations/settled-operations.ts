/**
 * Which write operations have finished tearing down, and how to wait for one.
 *
 * `write-settled` fires exactly once per operation, AFTER its terminal event and
 * after the journal's finalize barrier. That last part is the reason this module
 * exists: the journal batches item rows in memory and flushes the tail inside
 * `finalize` (`operation_log/capture.rs`, `RECORD_BATCH` is 512), and
 * `finalize_operation` blocks on the writer thread's reply. So an operation
 * smaller than the batch has NOTHING readable in the journal at `write-complete`
 * time, and everything readable by `write-settled`. Anyone reading an
 * operation's rows has to wait for the settle.
 *
 * **The settle routinely lands before anyone asks.** It follows its terminal
 * event by microseconds, while the frontend's own completion handling is held
 * for up to `MIN_DISPLAY_MS`. A waiter armed at that point would be waiting for
 * something that already happened, so this remembers the recent ids and answers
 * those immediately. That memory is the whole difference between a wait that
 * works and one that always times out.
 *
 * One subscription per window, started from the main page's init, the same shape
 * as `$lib/search/snapshot-purge.ts`.
 */

import { onWriteSettled } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import type { UnlistenFn } from '@tauri-apps/api/event'

const log = getAppLogger('fileOperations')

/**
 * How many settled ids to remember. A waiter asks within a second of the settle,
 * so this only has to outlive that; it's sized for a burst of small operations
 * rather than for history.
 */
const REMEMBERED_SETTLED_IDS = 64

/**
 * How long {@link whenOperationSettled} waits before giving up.
 *
 * A settle follows its terminal event by microseconds, so reaching this means
 * the event isn't coming at all (a webview that missed it, an operation torn
 * down some other way). The caller treats that as "don't do the follow-up",
 * never as an error.
 */
const SETTLE_WAIT_TIMEOUT_MS = 5000

/** Recently settled ids, newest last. The `Set` answers, the array bounds it. */
const settledIds = new Set<string>()
const settledOrder: string[] = []

/** Waiters keyed by operation id, resolved by the event or by the timeout. */
const waiters = new Map<string, Set<(settled: boolean) => void>>()

let unlisten: UnlistenFn | null = null
/** In flight, so a double init can't leak a second listener. */
let subscribing: Promise<void> | null = null

/** Records a settle and releases whoever was waiting on it. */
function noteSettled(operationId: string): void {
  if (!settledIds.has(operationId)) {
    settledIds.add(operationId)
    settledOrder.push(operationId)
    while (settledOrder.length > REMEMBERED_SETTLED_IDS) {
      const evicted = settledOrder.shift()
      if (evicted !== undefined) settledIds.delete(evicted)
    }
  }

  const pending = waiters.get(operationId)
  if (!pending) return
  waiters.delete(operationId)
  for (const resolve of pending) resolve(true)
}

/**
 * Subscribes this window to the settle stream. Idempotent, safe to await
 * concurrently, and never throws: a failed subscription costs the follow-ups
 * that wait on a settle, not the operations themselves.
 */
export async function initSettledOperationsWatch(): Promise<void> {
  if (unlisten) return
  subscribing ??= onWriteSettled((event) => {
    noteSettled(event.operationId)
  })
    .then((fn) => {
      unlisten = fn
    })
    .catch((error: unknown) => {
      log.warn('Could not watch settled operations: {error}', { error })
    })
    .finally(() => {
      subscribing = null
    })
  await subscribing
}

/** Drops this window's subscription and forgets what it recorded. */
export function destroySettledOperationsWatch(): void {
  unlisten?.()
  unlisten = null
  settledIds.clear()
  settledOrder.length = 0
  for (const pending of waiters.values()) {
    for (const resolve of pending) resolve(false)
  }
  waiters.clear()
}

/**
 * Resolves `true` once `operationId` has settled, or `false` if it hasn't within
 * {@link SETTLE_WAIT_TIMEOUT_MS}.
 *
 * An id that settled BEFORE the call resolves immediately, which is the common
 * case rather than the exception. Never rejects.
 */
export function whenOperationSettled(operationId: string): Promise<boolean> {
  if (settledIds.has(operationId)) return Promise.resolve(true)

  return new Promise<boolean>((resolve) => {
    let done = false
    const settle = (value: boolean): void => {
      if (done) return
      done = true
      clearTimeout(timer)
      waiters.get(operationId)?.delete(settle)
      resolve(value)
    }

    const timer = setTimeout(() => {
      settle(false)
    }, SETTLE_WAIT_TIMEOUT_MS)

    const pending = waiters.get(operationId) ?? new Set<(settled: boolean) => void>()
    pending.add(settle)
    waiters.set(operationId, pending)
  })
}
