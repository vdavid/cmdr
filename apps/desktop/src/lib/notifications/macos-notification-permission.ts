/**
 * Shared macOS notification permission flow for every feature that sends
 * native notifications (downloads, low disk space, future ones).
 *
 * Asks macOS per send whether Cmdr's notifications are on (the backend reads
 * the System Settings switch; see `src-tauri/src/notifications.rs`), so a user
 * who turns them back on gets banners again without restarting. When they're
 * off, one INFO toast per off-stretch says so. We DON'T flip anything for the
 * user and we DON'T nag: the toast comes back only after notifications were on
 * again in between.
 *
 * Cmdr never asks macOS for permission itself. The first post makes macOS show
 * its own prompt (wording fixed by Apple), and until the user answers it, the
 * switch reads as off.
 */

import { getNotificationPermission } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'

const PERMISSION_DENIED_TOAST_ID = 'notifications:macos-permission-denied'

/** Whether the "notifications are off" toast already showed since they were last on. */
let deniedToastShown = false

/**
 * Returns `true` unless macOS says Cmdr's notifications are off. "Not asked
 * yet" and "can't tell" both return `true`: posting is what makes macOS ask,
 * and a dev build can't tell at all.
 */
export async function ensureMacosNotificationPermission(): Promise<boolean> {
  const permission = await getNotificationPermission()
  if (permission !== 'denied') {
    deniedToastShown = false
    return true
  }
  if (!deniedToastShown) {
    deniedToastShown = true
    // INFO level with a dedup id, so features asking at the same moment stack one toast.
    addToast(tString('notifications.permissionDenied'), {
      id: PERMISSION_DENIED_TOAST_ID,
      level: 'info',
    })
  }
  return false
}

/**
 * Test-only: reset the in-module toast state between tests. Production code
 * never touches this.
 */
export function __resetPermissionStateForTests(): void {
  deniedToastShown = false
}
