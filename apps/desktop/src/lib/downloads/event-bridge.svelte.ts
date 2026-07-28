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
 *
 * ## Two surfaces, two ways of staying at one
 *
 * The in-app toast dispatches immediately and lets the toast store's group cap
 * evict the previous one. The macOS notification can't do that (nothing we send
 * reaches the OS as a replaceable identifier), so it coalesces instead: a burst
 * is held for `MACOS_COALESCE_MS` and becomes ONE banner. See `DETAILS.md`
 * § macOS notifications can't be deduped.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { sendNotification } from '@tauri-apps/plugin-notification'
import type { DownloadDetectedEvent } from '$lib/ipc/bindings'
import { downloadsWatcherStatus, onDownloadDetected } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getAppLogger } from '$lib/logging/logger'
import { ensureMacosNotificationPermission } from '$lib/notifications/macos-notification-permission'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import { getDownloadsNotificationsMode, type DownloadsNotificationsMode } from './notifications-mode'
import { getGlobalGoToLatestEnabled, getGlobalGoToLatestBinding } from './global-shortcut-setting'
import { getDownloadsToastCollapsed } from './downloads-toast-collapsed'
import DownloadToastContent from './DownloadToastContent.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const log = getAppLogger('downloads')

const GO_TO_LATEST_COMMAND_ID = 'downloads.goToLatest'
const TOAST_GROUP = 'downloads'
/**
 * One downloads toast on screen at a time. A burst (a browser saving several
 * files at once) would otherwise stack up to five near-identical teaching
 * toasts, each ~430px wide, burying the pane. The store's group cap evicts the
 * previous downloads toast when a new one arrives, so what's visible is always
 * the newest file, with a fresh 10s timer.
 *
 * A toast jumps to the file IT advertised (`goToDownload`, not
 * `goToLatestDownload`), so eviction does drop that one-click target. During a
 * burst the user hasn't acted on it yet, and `⌘J` still reaches the newest
 * file. Costs and alternatives: `DETAILS.md` § One toast at a time.
 */
const MAX_DOWNLOAD_TOASTS = 1
/**
 * The downloads toast is wider than the default (360) to give the keyboard
 * animation room to read. Capped by the toast container's own max-width.
 */
const TOAST_WIDTH_PX = 432
/**
 * Auto-hide window for the downloads toast. It teaches a shortcut but doesn't
 * demand a decision, so it dismisses on a timer like other transient toasts
 * (and pauses while hovered, per the toast store's hover behavior). The user
 * can also collapse it to a compact form that the next toast remembers.
 */
const TOAST_TIMEOUT_MS = 10_000
/**
 * How long the macOS notification path holds a burst before sending one banner
 * for it. Long enough to catch a browser saving several files in one go, short
 * enough that a lone download doesn't feel delayed.
 *
 * It's a FIXED window opened by the first event of a burst, not a debounce that
 * restarts on each one: a sustained stream (a torrent client unpacking) would
 * otherwise keep pushing the deadline out and never notify at all. A stream
 * instead gets one banner per window, which is a bound rather than silence.
 */
const MACOS_COALESCE_MS = 400

/**
 * The burst being accumulated for the macOS surface, or `null` between bursts.
 * `latest` is what the banner will name; `count` is how many detections it
 * stands for.
 */
let macosBurst: { latest: DownloadDetectedEvent; count: number } | null = null
let macosBurstTimer: ReturnType<typeof setTimeout> | null = null

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
  return () => {
    // Drop a burst still waiting out its window, so a teardown can't fire a
    // notification for a window nobody is listening to any more.
    cancelPendingMacosBurst()
    unlisten()
  }
}

function cancelPendingMacosBurst(): void {
  if (macosBurstTimer !== null) {
    clearTimeout(macosBurstTimer)
    macosBurstTimer = null
  }
  macosBurst = null
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
  const status = await downloadsWatcherStatus().catch(() => null)
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
    queueMacosNotification(payload)
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
    // Transient: the toast teaches a shortcut but doesn't need a decision, so it
    // auto-hides after 10s (pausing while hovered). Only one is visible at a
    // time (`maxInGroup`). Wider than the default so the keyboard animation
    // reads (`widthPx`).
    timeoutMs: TOAST_TIMEOUT_MS,
    toastGroup: TOAST_GROUP,
    maxInGroup: MAX_DOWNLOAD_TOASTS,
    widthPx: TOAST_WIDTH_PX,
    props: {
      explorer,
      event: payload,
      shortcutHint,
      globalBinding,
      // New toasts open in the last-used collapsed/expanded state.
      initialCollapsed: getDownloadsToastCollapsed(),
    },
  })
}

/**
 * Fold a detection into the current burst, opening a window if there isn't one.
 * Returns immediately: the banner goes out when the window closes.
 */
function queueMacosNotification(payload: DownloadDetectedEvent): void {
  if (macosBurst !== null) {
    macosBurst.latest = payload
    macosBurst.count += 1
    return
  }
  macosBurst = { latest: payload, count: 1 }
  macosBurstTimer = setTimeout(() => {
    void flushMacosBurst()
  }, MACOS_COALESCE_MS)
}

async function flushMacosBurst(): Promise<void> {
  const burst = macosBurst
  macosBurstTimer = null
  macosBurst = null
  if (burst === null) return

  // Ask for permission only once the window has closed: a burst that the user
  // never gets a banner for shouldn't cost them a permission prompt either.
  const ok = await ensureMacosNotificationPermission()
  if (!ok) return

  const { title, body } = describeMacosBurst(burst.latest, burst.count)
  try {
    sendNotification({ title, body })
  } catch (err) {
    log.warn('Failed to send macOS notification: {err}', { err: String(err) })
  }
}

/**
 * The banner's wording. A lone download keeps the single-file phrasing it has
 * always had (name in the title, folder in the body); a coalesced burst leads
 * with how many landed and names the newest in the body. The subdir line is
 * dropped for a burst on purpose: its files can come from different folders,
 * so one folder name would be a claim about the others that isn't true.
 */
function describeMacosBurst(latest: DownloadDetectedEvent, count: number): { title: string; body: string } {
  if (count === 1) {
    return {
      title: tString('downloads.notification.title', { fileName: latest.fileName }),
      body: latest.inSubdir ? tString('downloads.toast.inSubdir', { subdir: relativeSubdir(latest.parentDir) }) : '',
    }
  }
  return {
    title: tString('downloads.notification.titleMultiple', { count, countText: formatInteger(count) }),
    body: tString('downloads.notification.mostRecent', { fileName: latest.fileName }),
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
