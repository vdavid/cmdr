// Downloads event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.downloadDetected` / `events.globalShortcutFired` helpers. The watcher
// fires `download-detected` for each eligible final-form download; the global
// go-to-latest hotkey fires `global-shortcut-fired` from any app.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type DownloadDetectedEvent } from '$lib/ipc/bindings'

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

// ============================================================================
// Downloads-watcher commands
// ============================================================================
// Callers branch on the typed `Result`/error discriminant, so these are
// passthroughs — they don't unwrap the generated `Result` shape.

/** Read-only snapshot of the downloads watcher's state. */
export function downloadsWatcherStatus() {
  return commands.downloadsWatcherStatus()
}

/** Goes to the most recently observed eligible download. */
export function goToLatestDownload() {
  return commands.goToLatestDownload()
}

/** Applies a Settings change (toggle + binding) to the live global-shortcut registration. */
export function setGlobalGoToLatestShortcut(enabled: boolean, binding: string) {
  return commands.setGlobalGoToLatestShortcut(enabled, binding)
}

/** Re-evaluates the downloads-watcher FDA gate and starts/stops the watcher accordingly. */
export function recheckDownloadsWatcherGate() {
  return commands.recheckDownloadsWatcherGate()
}
