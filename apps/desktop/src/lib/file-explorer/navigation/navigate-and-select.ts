/**
 * Shared navigation primitives for the "jump somewhere in the focused pane"
 * features: "Go to latest download" (⌘J) and "Go to path" (⌘G). Both want to
 * point a pane at a directory and (for files) land the cursor on a specific
 * entry, with the same careful handling of `navigateToPath`'s
 * `string | Promise<void>` return.
 *
 * `navigateToPath` returns a sync error string when navigation can't even
 * start (a snapshot pane on a missing volume, etc.); otherwise it returns a
 * Promise that settles when the listing completes. We await the Promise but
 * report-and-bail on the sync-error string — without the listing settled,
 * `moveCursor` would race against an empty cache.
 */

import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'

const log = getAppLogger('navigation')

type Pane = 'left' | 'right'

/**
 * Navigate `pane` to a directory. No cursor move — the directory's own normal
 * navigation lands the cursor on the 0th row (`..`).
 */
export async function navigateToDirInPane(explorer: ExplorerAPI, pane: Pane, dir: string): Promise<void> {
  const navResult = explorer.navigateToPath(pane, dir)
  if (typeof navResult === 'string') {
    log.warn('navigateToDirInPane: navigateToPath refused {pane} {dir}: {reason}', {
      pane,
      dir,
      reason: navResult,
    })
    return
  }
  await navResult
}

/**
 * Navigate `pane` to `parentDir`, then move the cursor onto `fileName` so the
 * file is revealed/selected (we do NOT open it).
 */
export async function navigateToFileInPane(
  explorer: ExplorerAPI,
  pane: Pane,
  parentDir: string,
  fileName: string,
): Promise<void> {
  const navResult = explorer.navigateToPath(pane, parentDir)
  if (typeof navResult === 'string') {
    log.warn('navigateToFileInPane: navigateToPath refused {pane} {parentDir}: {reason}', {
      pane,
      parentDir,
      reason: navResult,
    })
    return
  }
  await navResult
  await explorer.moveCursor(pane, fileName)
}
