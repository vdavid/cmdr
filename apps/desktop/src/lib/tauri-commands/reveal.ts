// "Reveal in Cmdr": another app's "Show in Finder" landing in our pane.
//
// macOS only, mechanism and all (`src-tauri/src/reveal/`), so each wrapper swallows the
// missing-command rejection other platforms give and does nothing there.
//
// The Settings row's `getRevealHandlerState` / `setRevealHandlerEnabled` wrappers belong
// in this file too when that row gets built.

import { commands } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('reveal')

/**
 * Tell the backend this window is listening, so it can hand over any reveal that arrived
 * before the window existed (a cold launch delivers the OS event long before we mount).
 *
 * Returns as soon as the backend has taken the parked paths; the pane move itself runs on
 * the backend's own runtime, over the same `mcp-nav-to-path` channel this window answers.
 * ⚠️ Call it only once the MCP listeners are up.
 */
export async function drainPendingReveals(): Promise<void> {
  try {
    await commands.drainPendingReveals()
  } catch (error) {
    log.debug('Skipping pending-reveal drain: {error}', { error })
  }
}
