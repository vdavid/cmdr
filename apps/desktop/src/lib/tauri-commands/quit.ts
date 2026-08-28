// The quit gate's IPC surface. The backend owns the decision and the clock
// (`src-tauri/src/quit/`); the frontend renders the prompt and sends one of two
// answers. Either answer may arrive late, or never — the gate's own deadline
// still fires, which is the whole point of the design.

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { QuitAnswer, QuitRequested } from '$lib/ipc/bindings'

export type { QuitAnswer, QuitRequested }

/** The user pressed Quit. The backend stops every operation and exits.
 *  `noQuitPending` means the answer changed nothing: the countdown had already
 *  run out, or another surface answered first. */
export async function quitConfirm(): Promise<QuitAnswer> {
  return commands.quitConfirm()
}

/** The user pressed "Keep working". The countdown is removed, not deferred. */
export async function quitCancel(): Promise<QuitAnswer> {
  return commands.quitCancel()
}

/** Subscribe to the backend holding a quit: what's still running, and how long
 *  the user has. Returns an `UnlistenFn`; call it on teardown or you leak the
 *  listener. */
export async function onQuitRequested(callback: (event: QuitRequested) => void): Promise<UnlistenFn> {
  return events.quitRequested.listen((event) => {
    callback(event.payload)
  })
}

/** Subscribe to a held quit being called off. The dialog closes itself when the
 *  person clicks "Keep working"; this fires for the answers that come from
 *  somewhere else (an agent over MCP), where nothing else takes the prompt
 *  down. */
export async function onQuitCalledOff(callback: () => void): Promise<UnlistenFn> {
  return events.quitCalledOff.listen(() => {
    callback()
  })
}
