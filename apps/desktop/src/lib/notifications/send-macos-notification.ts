/**
 * The one way feature code sends a native notification.
 *
 * Checks macOS permission first (`ensureMacosNotificationPermission`), then
 * awaits the backend send. It never throws: a failed send is logged once until
 * a send works again, and the caller has nothing to do about it anyway.
 *
 * ❌ Don't swap in the plugin's JS `sendNotification`: it fires its IPC call
 * from a discarded async function, so a failure skips the caller's try/catch
 * and surfaces as an unhandled rejection, which auto-sends an error report.
 */

import { showNotification } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { LogOnceGate } from '$lib/logging/log-once'
import { ensureMacosNotificationPermission, __resetPermissionStateForTests } from './macos-notification-permission'

const log = getAppLogger('notifications')

const sendFailures = new LogOnceGate()

export async function sendMacosNotification(notification: { title: string; body: string }): Promise<void> {
  if (!(await ensureMacosNotificationPermission())) return

  let failure: string | null
  try {
    const result = await showNotification(notification.title, notification.body)
    failure = result.status === 'error' ? result.error : null
  } catch (err) {
    failure = String(err)
  }

  if (failure === null) {
    sendFailures.clear()
  } else if (sendFailures.shouldLog()) {
    log.warn('Failed to send a macOS notification: {err}', { err: failure })
  }
}

/**
 * Test-only: reset the failure gate and the permission toast state between
 * tests. Production code never touches this.
 */
export function __resetForTests(): void {
  sendFailures.clear()
  __resetPermissionStateForTests()
}
