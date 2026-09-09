// macOS Dock (whether Cmdr may be offered a tile, and putting it there)

import { commands } from '$lib/ipc/bindings'
import type { DockPinFailure, DockPinState } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('dock')

/**
 * Whether Cmdr may be offered a place in the Dock, and whether it's already there.
 *
 * One value covers "no tile yet", "already pinned", and every reason to stay quiet, so no caller
 * has to re-derive where Cmdr is installed. See `src-tauri/src/dock/CLAUDE.md`.
 *
 * A backend that can't answer at all reads as `preferencesUnreadable`, which keeps the nudge
 * silent rather than firing it on a guess.
 */
export async function getDockPinState(): Promise<DockPinState> {
  try {
    return await commands.getDockPinState()
  } catch (e) {
    log.warn(`Couldn't ask about the Dock: ${String(e)}`)
    return { kind: 'unavailable', reason: 'preferencesUnreadable' }
  }
}

/**
 * Adds Cmdr to the Dock as the leftmost app tile and restarts the Dock so it appears.
 *
 * Answers `null` on success, or the typed reason it didn't happen. The Dock visibly blinks for
 * about a second while it reloads; that's the restart, and it's expected.
 *
 * An IPC that never comes back at all lands on `timedOut`, which is what it means to the caller:
 * nobody knows whether the tile made it.
 */
export async function addCmdrToDock(): Promise<DockPinFailure | null> {
  try {
    const res = await commands.addCmdrToDock()
    return res.status === 'error' ? res.error : null
  } catch (e) {
    log.warn(`The Dock pin never answered: ${String(e)}`)
    return { kind: 'timedOut' }
  }
}
