/**
 * Downloads notifications event bridge.
 *
 * Subscribes ONCE to the backend `download-detected` Tauri event and
 * dispatches each event to the in-app toast and/or the macOS native
 * notification per the current `behavior.fileSystemWatching.downloadsNotifications`
 * setting (`'in-app' | 'macos' | 'both' | 'neither'`).
 *
 * Mounted from `routes/(main)/+layout.svelte`. The unsubscribe is returned
 * from `startDownloadsEventBridge` so the layout can clean up on destroy.
 *
 * ## Snapshot-at-creation rule
 *
 * Both go-to-latest bindings shown on a toast (the in-app `⌘J` and the global
 * `⌃⌥⌘J`) are captured at event-arrival time and passed as props. A remap
 * between one toast appearing and another arriving DOES update the next toast's
 * hints — that's correct — but never the toast already on screen. When neither
 * binding is teachable (in-app unbound, global off or unbound), `dispatchToast`
 * skips the toast outright.
 *
 * ## FDA defense-in-depth
 *
 * The watcher won't emit `download-detected` when the FDA gate is closed,
 * but we re-check the gate per event before surfacing anything. This
 * guards against any stale event slipping through during a gate flip and
 * mirrors the same defensive shape `goToLatestDownload` uses.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { commands, type DownloadDetectedEvent } from '$lib/ipc/bindings'
import { onDownloadDetected } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getAppLogger } from '$lib/logging/logger'
import { ensureMacosNotificationPermission } from '$lib/notifications/macos-notification-permission'
import { getDownloadsNotificationsMode, type DownloadsNotificationsMode } from './notifications-mode'
import { getGlobalGoToLatestEnabled, getGlobalGoToLatestBinding } from './global-shortcut-setting'
import DownloadToastContent from './DownloadToastContent.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const log = getAppLogger('downloads')

const GO_TO_LATEST_COMMAND_ID = 'downloads.goToLatest'
const TOAST_TIMEOUT_MS = 10_000
const TOAST_GROUP = 'downloads'

/**
 * Mount the listener. Returns an unsubscribe function — call it from the
 * layout's `onDestroy`.
 *
 * The `explorer` reference is captured at mount time; the toast component
 * holds it and uses it to navigate the focused pane when the user clicks
 * Jump. Pass `undefined` for non-main-window contexts (tests, HMR).
 */
export async function startDownloadsEventBridge(explorer: ExplorerAPI | undefined): Promise<UnlistenFn> {
  const unlisten = await onDownloadDetected((payload) => {
    void handleDownloadDetected(payload, explorer)
  })
  log.debug('Downloads event bridge mounted')
  return unlisten
}

async function handleDownloadDetected(
  payload: DownloadDetectedEvent,
  explorer: ExplorerAPI | undefined,
): Promise<void> {
  const mode = getDownloadsNotificationsMode()
  if (mode === 'neither') return

  // Defense in depth: skip every surface if the FDA gate is closed. The
  // watcher shouldn't be emitting in that case; bail anyway so a transient
  // race during a gate flip can't surface a notification before the user's
  // ready for it.
  const status = await commands.downloadsWatcherStatus().catch(() => null)
  if (status?.status === 'ok' && status.data.fdaPending) {
    log.debug('Skipping download-detected dispatch; FDA gate pending')
    return
  }

  log.debug('Dispatching download-detected ({mode}) for {fileName}', {
    mode,
    fileName: payload.fileName,
  })

  if (mode === 'in-app' || mode === 'both') {
    dispatchToast(payload, explorer)
  }
  if (mode === 'macos' || mode === 'both') {
    await dispatchMacosNotification(payload)
  }
}

function dispatchToast(payload: DownloadDetectedEvent, explorer: ExplorerAPI | undefined): void {
  // Snapshot both go-to-latest bindings at toast creation time. The component
  // receives these as props and never re-reads, so a remap between events
  // doesn't mutate an already-visible toast.
  //
  // In-app ⌘J: shown whenever the command is bound; `''` when it's unbound.
  const shortcutHint = getEffectiveShortcuts(GO_TO_LATEST_COMMAND_ID)[0] ?? ''

  // Global ⌃⌥⌘J (jump from any app): only teachable when the hotkey is turned
  // on AND has a binding. A disabled or unbound hotkey contributes no hint, so
  // collapse both cases to `''` for the component.
  const globalBinding = getGlobalGoToLatestEnabled() ? getGlobalGoToLatestBinding() : ''

  // The toast's reason to exist is teaching these shortcuts. With neither one
  // teachable, skip it entirely — even though downloads notifications aren't
  // turned off. (A 'both'-mode macOS notification still fires from the caller;
  // it never carried a shortcut hint anyway.)
  if (shortcutHint === '' && globalBinding === '') {
    log.debug('Skipping downloads toast: neither go-to-latest shortcut is set')
    return
  }

  addToast(DownloadToastContent, {
    level: 'info',
    timeoutMs: TOAST_TIMEOUT_MS,
    toastGroup: TOAST_GROUP,
    props: {
      explorer,
      event: payload,
      shortcutHint,
      globalBinding,
    },
  })
}

async function dispatchMacosNotification(payload: DownloadDetectedEvent): Promise<void> {
  const ok = await ensureMacosNotificationPermission()
  if (!ok) return

  const title = `Downloaded ${payload.fileName}`
  const body = payload.inSubdir ? `in ${relativeSubdir(payload.parentDir)}` : ''
  try {
    sendNotification({ title, body })
  } catch (err) {
    log.warn('Failed to send macOS notification: {err}', { err: String(err) })
  }
}

/**
 * Format a parent-dir path as "Downloads/<subdir>/" for the OS notification
 * body. Mirrors the in-app toast's subdir line so both surfaces feel
 * consistent.
 */
function relativeSubdir(parentDir: string): string {
  const marker = '/Downloads/'
  const i = parentDir.lastIndexOf(marker)
  if (i === -1) return parentDir
  return 'Downloads/' + parentDir.slice(i + marker.length) + '/'
}

/**
 * Re-export the typed setting union so the rest of the app can refer to it
 * via the bridge module rather than reaching into `notifications-mode`.
 */
export type { DownloadsNotificationsMode }
