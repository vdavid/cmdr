// Downloads event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.downloadDetected` / `events.globalShortcutFired` helpers. The watcher
// fires `download-detected` for each eligible final-form download; the global
// go-to-latest hotkey fires `global-shortcut-fired` from any app.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type DownloadDetectedEvent } from '$lib/ipc/bindings'

/** An eligible final-form download landed in the Downloads tree (FDA-gated). */
export function onDownloadDetected(handler: (payload: DownloadDetectedEvent) => void): Promise<UnlistenFn> {
  return events.downloadDetected.listen((event) => {
    handler(event.payload)
  })
}

/** The system-wide go-to-latest hotkey (default ⌃⌥⌘J) fired. Payloadless. */
export function onGlobalShortcutFired(handler: () => void): Promise<UnlistenFn> {
  return events.globalShortcutFired.listen(() => {
    handler()
  })
}
