/**
 * Reactive store for paths macOS TCC currently blocks Cmdr from reading.
 *
 * The backend records every `PermissionDenied` on a path that matches its
 * hard-coded "possibly TCC-restricted on macOS" list (`~/Downloads`,
 * `~/Documents`, `~/Desktop`, `~/Pictures`, `~/Movies`, `~/Music`,
 * `~/Library/Safari`, `~/Library/Mail`, `~/Library/Messages`, iCloud Drive,
 * `~/Library/CloudStorage`, third-party app Containers, network volumes).
 * It also re-probes the set when Cmdr regains focus (after the user toggled
 * permissions in System Settings), so the UI feels live without polling.
 *
 * The store hydrates once via `getRestrictedPaths()` and patches itself
 * from the `restricted-paths-changed` event afterwards.
 *
 * Call `initRestrictedPathsStore()` once at startup. `isRestricted(path)`
 * is the helper components import.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { SvelteSet } from 'svelte/reactivity'
import { getRestrictedPaths } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const logger = getAppLogger('restricted-paths-store')

interface RestrictedPathsChangedPayload {
  paths: string[]
}

const paths = new SvelteSet<string>()
let initialized = false
let unlisten: UnlistenFn | undefined

/** Reactive check: is this exact path currently TCC-restricted? */
export function isRestricted(path: string): boolean {
  return paths.has(path)
}

/** Reactive: number of restricted paths in the set. */
export function getRestrictedCount(): number {
  return paths.size
}

/**
 * Initialize the store: hydrate from the backend snapshot and subscribe to
 * `restricted-paths-changed` events. Idempotent.
 */
export async function initRestrictedPathsStore(): Promise<void> {
  if (initialized) return

  unlisten = await listen<RestrictedPathsChangedPayload>('restricted-paths-changed', (event) => {
    paths.clear()
    for (const p of event.payload.paths) paths.add(p)
    logger.debug('restricted-paths-changed: {count} paths', { count: event.payload.paths.length })
  })

  const bootstrap = await getRestrictedPaths()
  // Only apply bootstrap if no event has updated the set yet.
  if (paths.size === 0) {
    for (const p of bootstrap) paths.add(p)
  }
  initialized = true
  logger.debug('Restricted-paths store initialized ({count} paths)', { count: paths.size })
}

/** Tear down the listener. Call on app shutdown / hot-reload. */
export function cleanupRestrictedPathsStore(): void {
  unlisten?.()
  unlisten = undefined
  paths.clear()
  initialized = false
}
