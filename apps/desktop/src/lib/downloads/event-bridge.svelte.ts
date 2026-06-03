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
 * The shortcut hint shown on each in-app toast is captured at event-arrival
 * time and passed as a prop. A remap of the in-app go-to-latest binding between
 * one toast appearing and another arriving DOES update the next toast's
 * hint — that's correct — but never the toast already on screen.
 *
 * ## FDA defense-in-depth
 *
 * The watcher won't emit `download-detected` when the FDA gate is closed,
 * but we re-check the gate per event before surfacing anything. This
 * guards against any stale event slipping through during a gate flip and
 * mirrors the same defensive shape `goToLatestDownload` uses.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import { commands } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getAppLogger } from '$lib/logging/logger'
import { getDownloadsNotificationsMode, type DownloadsNotificationsMode } from './notifications-mode'
import DownloadToastContent from './DownloadToastContent.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const log = getAppLogger('downloads')

const DOWNLOAD_DETECTED_EVENT = 'download-detected'
const GO_TO_LATEST_COMMAND_ID = 'downloads.goToLatest'
const TOAST_TIMEOUT_MS = 10_000
const TOAST_GROUP = 'downloads'
const PERMISSION_DENIED_TOAST_ID = 'downloads:macos-permission-denied'

interface DownloadDetectedPayload {
  path: string
  parentDir: string
  fileName: string
  observedAtMs: number
  inSubdir: boolean
  sizeBytes: number | null
}

/**
 * Whether we've already asked the OS for notification permission in this
 * session. macOS only shows the system dialog once per process lifetime
 * regardless, but we cache the answer so we don't await an `isPermissionGranted`
 * round-trip per event when the user already granted it.
 *
 * `null` means "not yet asked"; subsequent values reflect the latest answer.
 */
let cachedPermission: 'granted' | 'denied' | null = null

/**
 * Mount the listener. Returns an unsubscribe function — call it from the
 * layout's `onDestroy`.
 *
 * The `explorer` reference is captured at mount time; the toast component
 * holds it and uses it to navigate the focused pane when the user clicks
 * Jump. Pass `undefined` for non-main-window contexts (tests, HMR).
 */
export async function startDownloadsEventBridge(explorer: ExplorerAPI | undefined): Promise<UnlistenFn> {
  const unlisten = await listen<DownloadDetectedPayload>(DOWNLOAD_DETECTED_EVENT, (event) => {
    void handleDownloadDetected(event.payload, explorer)
  })
  log.debug('Downloads event bridge mounted')
  return unlisten
}

async function handleDownloadDetected(
  payload: DownloadDetectedPayload,
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

function dispatchToast(payload: DownloadDetectedPayload, explorer: ExplorerAPI | undefined): void {
  // Snapshot the current binding at toast creation time. The component
  // receives this as a prop and never re-reads, so a remap between events
  // doesn't mutate an already-visible toast.
  const shortcuts = getEffectiveShortcuts(GO_TO_LATEST_COMMAND_ID)
  const shortcutHint = shortcuts[0] ?? ''

  addToast(DownloadToastContent, {
    level: 'info',
    timeoutMs: TOAST_TIMEOUT_MS,
    toastGroup: TOAST_GROUP,
    props: {
      explorer,
      event: payload,
      shortcutHint,
    },
  })
}

async function dispatchMacosNotification(payload: DownloadDetectedPayload): Promise<void> {
  const ok = await ensurePermissionGranted()
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
 * Ask the OS for notification permission if we haven't already, cache the
 * answer for the rest of the session, and surface a single INFO toast on
 * denial so the user knows what happened.
 *
 * The macOS dialog wording is fixed by Apple; we can't customize it. The
 * dialog only appears the first time per process lifetime — `requestPermission`
 * on subsequent calls just returns the cached OS answer.
 */
async function ensurePermissionGranted(): Promise<boolean> {
  if (cachedPermission === 'granted') return true
  if (cachedPermission === 'denied') return false

  let granted: boolean
  try {
    granted = await isPermissionGranted()
  } catch (err) {
    log.warn('Failed to query macOS notification permission: {err}', { err: String(err) })
    return false
  }

  if (!granted) {
    let response: 'granted' | 'denied' | 'default'
    try {
      response = await requestPermission()
    } catch (err) {
      log.warn('Failed to request macOS notification permission: {err}', { err: String(err) })
      return false
    }
    if (response !== 'granted') {
      cachedPermission = 'denied'
      surfaceDeniedToast()
      return false
    }
  }

  cachedPermission = 'granted'
  return true
}

function surfaceDeniedToast(): void {
  // INFO level with a dedup id so repeated denials only stack one toast.
  // No retries — the user can flip the setting back when they're ready.
  addToast('macOS notifications are off. Open System Settings to allow them.', {
    id: PERMISSION_DENIED_TOAST_ID,
    level: 'info',
  })
}

/**
 * Test-only: reset the in-module permission cache between tests. Production
 * code never touches this.
 */
export function __resetPermissionCacheForTests(): void {
  cachedPermission = null
}

/**
 * Re-export the typed setting union so the rest of the app can refer to it
 * via the bridge module rather than reaching into `notifications-mode`.
 */
export type { DownloadsNotificationsMode }
