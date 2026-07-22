/**
 * Low-disk-space event bridge.
 *
 * Subscribes ONCE to the backend `low-disk-space` Tauri event (emitted by
 * `space_poller.rs` on each hysteresis edge) and dispatches per the current
 * `behavior.fileSystemWatching.lowDiskSpaceNotifications` setting
 * (`'in-app' | 'macos' | 'off'`).
 *
 * The event carries both edges via `is_low`:
 * - `true` (free space fell below the threshold): show the in-app toast, or
 *   send the macOS native notification.
 * - `false` (free space recovered above the re-arm margin): dismiss the in-app
 *   toast. A delivered macOS notification can't be recalled, so that mode
 *   no-ops on recovery.
 *
 * While the in-app toast is up it live-follows the boot volume's space on its
 * own (via `volume-space-changed`); this bridge only owns the show/dismiss
 * edges. The toast uses a per-volume dedup id, so a re-fire after recovery
 * replaces any lingering toast instead of stacking.
 *
 * Mounted from `routes/(main)/+page.svelte` next to the downloads bridge. The
 * unsubscribe is returned so the caller can clean up on destroy.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { addToast, dismissToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { ensureMacosNotificationPermission } from '$lib/notifications/macos-notification-permission'
import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
import { onLowDiskSpace } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import type { LowDiskSpacePayload } from '$lib/ipc/bindings'
import { getLowDiskSpaceNotificationsMode } from './notifications-mode'
import LowDiskSpaceToastContent from './LowDiskSpaceToastContent.svelte'

const log = getAppLogger('low-disk-space')

/**
 * Mount the listener. Returns an unsubscribe function — call it from the
 * caller's `onDestroy`.
 */
export async function startLowDiskSpaceEventBridge(): Promise<UnlistenFn> {
  const unlisten = await onLowDiskSpace((payload) => {
    void handleLowDiskSpace(payload)
  })
  log.debug('Low-disk-space event bridge mounted')
  return unlisten
}

async function handleLowDiskSpace(payload: LowDiskSpacePayload): Promise<void> {
  // Defense in depth: the backend removes its boot-volume watcher when the
  // warning is off, so no event should arrive. Bail anyway so a transient
  // race during a settings flip can't surface a stale warning.
  const mode = getLowDiskSpaceNotificationsMode()
  if (mode === 'off') return

  log.debug('Dispatching low-disk-space ({mode}, low={isLow}): {freePercent}% free on {volumeId}', {
    mode,
    isLow: payload.isLow,
    freePercent: payload.freePercent,
    volumeId: payload.volumeId,
  })

  if (mode === 'in-app') {
    if (payload.isLow) dispatchToast(payload)
    else dismissToast(toastId(payload.volumeId))
    return
  }

  // macOS native: notify on the low edge only. A delivered notification can't
  // be recalled, so recovery has nothing to do here.
  if (payload.isLow) await dispatchMacosNotification(payload)
}

/** Per-volume dedup id, shared by show and dismiss. */
function toastId(volumeId: string): string {
  return `low-disk-space:${volumeId}`
}

function dispatchToast(payload: LowDiskSpacePayload): void {
  addToast(LowDiskSpaceToastContent, {
    level: 'warn',
    dismissal: 'persistent',
    // Per-volume dedup: a re-fire replaces the visible toast in place.
    id: toastId(payload.volumeId),
    closeTooltip: tString('lowDiskSpace.toast.closeTooltip'),
    props: {
      volumeId: payload.volumeId,
      availableBytes: payload.availableBytes,
      totalBytes: payload.totalBytes,
    },
  })
}

async function dispatchMacosNotification(payload: LowDiskSpacePayload): Promise<void> {
  const ok = await ensureMacosNotificationPermission()
  if (!ok) return

  const freeText = formatFileSizeWithFormat(payload.availableBytes, getFileSizeFormat())
  const percentText = payload.freePercent.toFixed(1)
  try {
    sendNotification({
      title: tString('lowDiskSpace.notification.title'),
      body: tString('lowDiskSpace.notification.body', { freeText, percentText }),
    })
  } catch (err) {
    log.warn('Failed to send macOS notification: {err}', { err: String(err) })
  }
}
