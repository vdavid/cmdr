/**
 * Shared macOS notification permission flow for every feature that sends
 * native notifications (downloads, low disk space, future ones).
 *
 * Asks the OS for permission if we haven't already, caches the answer for the
 * rest of the session, and surfaces a single INFO toast on denial so the user
 * knows what happened. We DON'T flip the user's setting and we DON'T retry:
 * the user can re-enable notifications in System Settings whenever; their
 * preference stays put.
 *
 * The macOS dialog wording is fixed by Apple; we can't customize it. The
 * dialog only appears the first time per process lifetime — `requestPermission`
 * on subsequent calls just returns the cached OS answer.
 */

import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('notifications')

const PERMISSION_DENIED_TOAST_ID = 'notifications:macos-permission-denied'

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
 * Returns `true` when macOS notifications may be sent. On first denial,
 * surfaces one INFO toast (stable dedup id, so repeated denials across
 * features only stack one toast).
 */
export async function ensureMacosNotificationPermission(): Promise<boolean> {
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
