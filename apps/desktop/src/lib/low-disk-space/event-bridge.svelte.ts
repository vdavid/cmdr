/**
 * Low-disk-space event bridge.
 *
 * Subscribes ONCE to the backend `low-disk-space` Tauri event (emitted by
 * `space_poller.rs` when the boot volume's free space crosses below the
 * configured percent threshold) and dispatches it to the in-app toast OR the
 * macOS native notification per the current
 * `behavior.fileSystemWatching.lowDiskSpaceNotifications` setting
 * (`'in-app' | 'macos' | 'off'`).
 *
 * Mounted from `routes/(main)/+page.svelte` next to the downloads bridge. The
 * unsubscribe is returned so the caller can clean up on destroy.
 *
 * The in-app toast is persistent (no auto-dismiss — low disk space stays true
 * until the user acts) with a per-volume dedup id, so a re-fire after the
 * hysteresis re-arms replaces the visible toast instead of stacking.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { ensureMacosNotificationPermission } from '$lib/notifications/macos-notification-permission'
import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
import { onLowDiskSpace } from '$lib/tauri-commands'
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

  log.debug('Dispatching low-disk-space ({mode}): {freePercent}% free on {volumeId}', {
    mode,
    freePercent: payload.freePercent,
    volumeId: payload.volumeId,
  })

  if (mode === 'in-app') {
    dispatchToast(payload)
  } else {
    await dispatchMacosNotification(payload)
  }
}

function dispatchToast(payload: LowDiskSpacePayload): void {
  addToast(LowDiskSpaceToastContent, {
    level: 'warn',
    dismissal: 'persistent',
    // Per-volume dedup: a re-fire replaces the visible toast in place.
    id: `low-disk-space:${payload.volumeId}`,
    closeTooltip: 'Dismiss',
    props: {
      availableBytes: payload.availableBytes,
      freePercent: payload.freePercent,
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
      title: 'Low disk space',
      body: `${freeText} free (${percentText}%) on your startup disk.`,
    })
  } catch (err) {
    log.warn('Failed to send macOS notification: {err}', { err: String(err) })
  }
}
