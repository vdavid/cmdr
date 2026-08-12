/**
 * This window's session registry, and the two lifecycle calls a page makes.
 *
 * Sessions are per-window and that is fine: the operation queue is a separate
 * `WebviewWindow`, so two webviews can't share reactive state and don't need to.
 * Each builds its own projection of the same broadcast events; the backend
 * registry stays the single source of truth.
 *
 * `init` runs at window init, next to `initMainWindowOperations()`, and before
 * any view can ask for a session: the fan-out's `listen()` is async, so
 * subscribing lazily on the first session would drop everything that arrives
 * while the promise is in flight, which is exactly the dispatch → dialog →
 * session sequence on a cold main window.
 */

import { createOperationSessionRegistry, type OperationSessionRegistry } from './operation-session-registry'

/** `$state.raw`: the registry is a getter-bearing object, so it must NOT be
 *  deeply proxied; only the swap between instance and `null` is reactive, so a
 *  view that renders before `init()` resolves re-renders when it appears. */
let registry = $state.raw<OperationSessionRegistry | null>(null)

/**
 * Creates the registry and subscribes its fan-out. Idempotent: a second call
 * while one is live is a no-op, so a double mount can't leak a second listener
 * set. Never throws (the fan-out logs and degrades to silent sessions).
 */
export async function initOperationSessions(): Promise<void> {
  if (registry) return
  // A FRESH instance per init, never a revived one: `dispose()` drops the
  // fan-out's listeners, so re-initing the same object would stay deaf.
  const instance = createOperationSessionRegistry()
  registry = instance
  await instance.init()
}

/** Drops every session and every listener, and forgets the instance. Safe
 *  without an init, safe twice, and safe while `init()` is still in flight. */
export function destroyOperationSessions(): void {
  registry?.dispose()
  registry = null
}

/** This window's registry, or `null` before `initOperationSessions()` resolves. */
export function getOperationSessions(): OperationSessionRegistry | null {
  return registry
}
