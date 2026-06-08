// AI lifecycle event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.ai*` helpers. The install flow emits these in sequence
// (`ai-extracting` → repeated `ai-download-progress` → `ai-verifying` →
// `ai-installing` → `ai-install-complete`); `ai-starting` / `ai-server-ready`
// bracket a server boot on a returning launch.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type DownloadProgress } from '$lib/ipc/bindings'

/** Model-download progress (bytes, total, speed, ETA), throttled to ~200 ms. */
export function onAiDownloadProgress(handler: (payload: DownloadProgress) => void): Promise<UnlistenFn> {
  return events.aiDownloadProgress.listen((event) => {
    handler(event.payload)
  })
}

/** The local server is starting up (boot on a returning launch). */
export function onAiStarting(handler: () => void): Promise<UnlistenFn> {
  return events.aiStarting.listen(() => {
    handler()
  })
}

/** The local server became healthy and ready. */
export function onAiServerReady(handler: () => void): Promise<UnlistenFn> {
  return events.aiServerReady.listen(() => {
    handler()
  })
}

/** Post-download file-size verification started. */
export function onAiVerifying(handler: () => void): Promise<UnlistenFn> {
  return events.aiVerifying.listen(() => {
    handler()
  })
}

/** Server startup (health-check polling) began. */
export function onAiInstalling(handler: () => void): Promise<UnlistenFn> {
  return events.aiInstalling.listen(() => {
    handler()
  })
}

/** The server is healthy and the install completed. */
export function onAiInstallComplete(handler: () => void): Promise<UnlistenFn> {
  return events.aiInstallComplete.listen(() => {
    handler()
  })
}

/** Binary extraction from the bundled archive started (usually instant). */
export function onAiExtracting(handler: () => void): Promise<UnlistenFn> {
  return events.aiExtracting.listen(() => {
    handler()
  })
}
