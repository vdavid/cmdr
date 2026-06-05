/**
 * Shared navigation primitives for the "jump somewhere in the focused pane"
 * features: "Go to latest download" (⌘J) and "Go to path" (⌘G). Both want to
 * point a pane at a directory and (for files) land the cursor on a specific
 * entry, with the same careful handling of `navigate()`'s `NavigateResult`.
 *
 * `navigate()` returns `{ status: 'refused', reason }` when navigation can't
 * even start (a snapshot pane on a missing volume, the network/MTP refusals);
 * otherwise `{ status: 'started', settled }` where `settled` resolves when the
 * navigation settles. We await `settled` but report-and-bail on a refusal —
 * without the listing settled, `moveCursor` would race against an empty cache.
 *
 * NOTE (L2-adjacent): for the cross-volume snapshot arm, `settled` resolves
 * BEFORE the new listing loads (it resolves when the volume-switch commit is
 * done). `navigateToFileInPane` relies on `moveCursor`'s own internal
 * `whenLoadSettles` to bridge that gap — `settled` is the navigation-started
 * gate, `whenLoadSettles` is the real listing gate. Don't try to collapse them.
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
  const result = explorer.navigate({ pane, to: { path: dir }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToDirInPane: navigate refused {pane} {dir}: {reason}', {
      pane,
      dir,
      reason: result.reason.message,
    })
    return
  }
  await result.settled
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
  const result = explorer.navigate({ pane, to: { path: parentDir }, source: 'user' })
  if (result.status === 'refused') {
    log.warn('navigateToFileInPane: navigate refused {pane} {parentDir}: {reason}', {
      pane,
      parentDir,
      reason: result.reason.message,
    })
    return
  }
  await result.settled
  await explorer.moveCursor(pane, fileName)
}
