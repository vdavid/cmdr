// The quit gate's IPC surface. The backend owns the decision and the clock
// (`src-tauri/src/quit/`); the frontend renders the prompt and sends one of two
// answers. Either answer may arrive late, or never — the gate's own deadline
// still fires, which is the whole point of the design.

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { QuitRequested } from '$lib/ipc/bindings'

export type { QuitRequested }

/** The user pressed Quit. The backend stops every operation and exits. */
export async function quitConfirm(): Promise<void> {
  await commands.quitConfirm()
}

/** The user pressed "Keep working". The countdown is removed, not deferred. */
export async function quitCancel(): Promise<void> {
  await commands.quitCancel()
}

/** Subscribe to the backend holding a quit: what's still running, and how long
 *  the user has. Returns an `UnlistenFn`; call it on teardown or you leak the
 *  listener. */
export async function onQuitRequested(callback: (event: QuitRequested) => void): Promise<UnlistenFn> {
  return events.quitRequested.listen((event) => {
    callback(event.payload)
  })
}
