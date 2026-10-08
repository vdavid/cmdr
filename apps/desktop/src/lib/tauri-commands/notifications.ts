// Native system notifications (sending one, and whether macOS will show it)

import { commands } from '$lib/ipc/bindings'
import type { NotificationPermission } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('notifications')

/**
 * Whether macOS will show Cmdr's notifications right now. Cheap (one XPC
 * round-trip), so callers ask per send. A backend that can't answer reads as
 * `unknown`, which callers treat as "send and let macOS decide".
 */
export async function getNotificationPermission(): Promise<NotificationPermission> {
  try {
    return await commands.getNotificationPermission()
  } catch (e) {
    log.debug(`Couldn't ask about notification permission: ${String(e)}`)
    return 'unknown'
  }
}

/**
 * Sends a native notification. Passes the typed `Result` through; the `Err`
 * string is the plugin's message, for the log only. Feature code goes through
 * `sendMacosNotification` in `$lib/notifications`, which handles both.
 */
export function showNotification(title: string, body: string) {
  return commands.showNotification(title, body)
}
