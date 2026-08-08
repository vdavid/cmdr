/**
 * The MAIN window's instance of the operations store.
 *
 * The queue window has always had one; the main window needs its own so corner
 * status (and, later, failure notices) can read live operation state without a
 * new backend event, a new IPC command, or polling. Both instances come from the
 * same `createOperationsStore()` factory and subscribe to the same two app-wide
 * streams (`operations-changed` + `write-progress`) — two webviews can't share
 * reactive state, and they don't need to: the backend is the single source.
 *
 * The instance lives here rather than in `routes/(main)/+page.svelte` so its
 * lifecycle is testable on its own and so consumers (the status corner) read it
 * through a named seam instead of a prop threaded down the component tree. The
 * page owns only the two lifecycle calls, next to `initIndexState()` /
 * `destroyIndexState()`.
 *
 * Cost: with an empty queue this is two idle listeners; during a transfer it's
 * one small object per 200 ms progress event, exactly what the queue window
 * already carries. Don't memoise ahead of a measurement.
 */

import { createOperationsStore, type OperationRow } from './operations-store.svelte'

/** The store's public shape, so consumers can name the type. */
export type OperationsStore = ReturnType<typeof createOperationsStore>

/** `$state.raw`: the store is a getter-bearing object, so it must NOT be deeply
 *  proxied; only the swap between instance and `null` needs to be reactive (a
 *  consumer that renders before `init()` resolves has to re-render when the
 *  instance appears). The rows inside are reactive on their own. */
let store = $state.raw<OperationsStore | null>(null)

/**
 * Creates the instance and subscribes it. Idempotent: a second call while one is
 * live is a no-op, so a double mount can't leak a second listener pair. Never
 * throws — the store logs and degrades to an empty list if the IPC fails.
 */
export async function initMainWindowOperations(): Promise<void> {
  if (store) return
  // A FRESH instance per init, never a revived one: `dispose()` latches the
  // store's disposed flag, so re-initing the same object would unsubscribe
  // itself immediately and render nothing (remount, HMR).
  const instance = createOperationsStore()
  store = instance
  await instance.init()
}

/** Drops both listeners and forgets the instance. Safe without an init, safe
 *  twice, and safe while `init()` is still in flight (the store's own guard
 *  unsubscribes whatever lands late). */
export function destroyMainWindowOperations(): void {
  store?.dispose()
  store = null
}

/** The live store, or `null` before `initMainWindowOperations()` resolves. */
export function getMainWindowOperations(): OperationsStore | null {
  return store
}

/** The merged rows, empty before init. The reading-most-often seam: consumers
 *  that only render operations shouldn't have to handle the null instance. */
export function getMainWindowOperationRows(): OperationRow[] {
  return store?.operations ?? []
}
