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

import { SvelteSet } from 'svelte/reactivity'
import { getRestrictedPaths, onRestrictedPathsChanged } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'

const logger = getAppLogger('restricted-paths-store')

const paths = new SvelteSet<string>()
let initialized = false

/** Reactive check: is this exact path currently TCC-restricted? */
export function isRestricted(path: string): boolean {
  return paths.has(path)
}

/**
 * Initialize the store: hydrate from the backend snapshot and subscribe to
 * `restricted-paths-changed` events. Idempotent.
 */
export async function initRestrictedPathsStore(): Promise<void> {
  if (initialized) return

  await onRestrictedPathsChanged((payload) => {
    paths.clear()
    for (const p of payload.paths) paths.add(p)
    logger.debug('restricted-paths-changed: {count} {pathsNoun}', {
      count: payload.paths.length,
      pathsNoun: pluralize(payload.paths.length, 'path'),
    })
  })

  const bootstrap = await getRestrictedPaths()
  // Only apply bootstrap if no event has updated the set yet.
  if (paths.size === 0) {
    for (const p of bootstrap) paths.add(p)
  }
  initialized = true
  logger.debug('Restricted-paths store initialized ({count} {pathsNoun})', {
    count: paths.size,
    pathsNoun: pluralize(paths.size, 'path'),
  })
}
