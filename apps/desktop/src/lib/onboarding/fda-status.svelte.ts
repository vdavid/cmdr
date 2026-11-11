/**
 * Whether this Mac has given Cmdr Full Disk Access, as one reactive fact the whole UI reads.
 *
 * Two surfaces need it and they must never disagree: the status-corner badge that makes a missing
 * grant visible at all, and the extra paragraph an error message adds when a refusal is the
 * kind FDA explains. Both ask here.
 *
 * ❗ Probes with `checkFullDiskAccessQuiet`, never the loud `checkFullDiskAccess`, which fires
 * a macOS TCC popup. This runs on mount and on every window focus, so the loud one would
 * stack popups on someone who simply alt-tabbed.
 */
import { getAppLogger } from '#lib/logging/logger.js'
import { isMacOS } from '#lib/platform/index.js'
import { checkFullDiskAccessQuiet } from '#lib/tauri-commands/index.js'

const log = getAppLogger('fda-status')

/**
 * `null` until the first probe answers. Distinct from `false` on purpose: "we haven't looked
 * yet" must not render as "you have no access", or the badge flashes on every launch for
 * people who granted it long ago.
 */
let granted = $state<boolean | null>(null)

/** True only once we've actually asked AND the answer was no. Always false off macOS. */
export function fdaIsMissing(): boolean {
  return isMacOS() && granted === false
}

/**
 * Re-probe and store the answer. Safe to call often (it's a cheap syscall-level check) and
 * safe to call off macOS, where it records "granted" so nothing downstream offers a fix for
 * a permission that doesn't exist on the platform.
 */
export async function refreshFdaStatus(): Promise<void> {
  if (!isMacOS()) {
    granted = true
    return
  }
  try {
    granted = await checkFullDiskAccessQuiet()
  } catch (e) {
    // A broken probe must not invent a missing grant: that would put a scary badge in the
    // title bar because an IPC call failed. Leave the last known answer alone.
    log.debug('Full Disk Access probe failed, keeping the last answer: {error}', { error: e })
  }
}

/** Test-only: clears the cached answer so each test starts from "not probed yet". */
export function _resetFdaStatusForTests(): void {
  granted = null
}
