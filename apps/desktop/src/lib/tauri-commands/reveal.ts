// "Reveal in Cmdr": another app's "Show in Finder" landing in our pane.
//
// macOS only, mechanism and all (`src-tauri/src/reveal/`), so each wrapper swallows the
// missing-command rejection other platforms give and does nothing there.

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { RevealHandlerState } from '$lib/ipc/bindings'
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

/**
 * Who currently owns the `NSFileViewer` key: us, nobody, or another app.
 *
 * Reads through to the OS on every call. ❌ Never cache it in a setting: the key is
 * machine state anyone can change from outside Cmdr, so a stored flag would show a
 * switch that disagrees with the Mac it sits on.
 *
 * `unavailable` covers every build that must not write the key (a debug, worktree, or
 * E2E instance) as well as every non-macOS platform, where the command doesn't exist.
 */
export async function getRevealHandlerState(): Promise<RevealHandlerState> {
  try {
    return await commands.getRevealHandlerState()
  } catch (error) {
    log.debug('Reveal handler state is unavailable: {error}', { error })
    return { kind: 'unavailable' }
  }
}

/**
 * Take the `NSFileViewer` key, or give it up.
 *
 * Returns the state the OS was left in, not the state that was asked for: another app
 * can hold the key by the time the click lands, and the row has to render the truth.
 */
export async function setRevealHandlerEnabled(enabled: boolean): Promise<RevealHandlerState> {
  try {
    return await commands.setRevealHandlerEnabled(enabled)
  } catch (error) {
    log.debug('Could not set the reveal handler: {error}', { error })
    return { kind: 'unavailable' }
  }
}

/**
 * Fires each time a reveal from another app actually moves a pane.
 *
 * ❗ "It landed", ❌ never "one arrived": the backend emits only after the pane move
 * succeeds, so a reveal onto a dead mount doesn't announce anything. Payloadless — the
 * paths reach this window over `mcp-nav-to-path` and a second copy could disagree.
 */
export function onRevealDelivered(handler: () => void): Promise<UnlistenFn> {
  return events.revealDelivered.listen(() => {
    handler()
  })
}
