/**
 * The main window's Escape key, and the full-screen exit it may trigger.
 *
 * WKWebView hands every keydown the page leaves unprevented back to AppKit, which
 * turns Escape into `cancelOperation:`, and a full-screen window answers that by
 * leaving full screen. So an Escape that closed a dialog, a menu, or a dropdown
 * would ALSO restore the window unless something prevented its default. Two
 * pieces keep AppKit from ever seeing an Escape from this window:
 *
 * - `installEscapeStopClaims` makes stopping an Escape claim it: a handler that
 *   stops the key has used it, so it must not reach AppKit either. That covers
 *   every local handler (dropdowns, menus, popovers, the rename editor) without
 *   each one having to remember `preventDefault`.
 * - The document dispatcher (`global-keydown.ts`) prevents every Escape that
 *   reaches it, and hands one nothing used to `exitFullScreenOnEscape`.
 *
 * Leaving full screen is then OUR decision, gated on the
 * `advanced.exitFullScreenOnEscape` setting, instead of an accident of which
 * handler forgot `preventDefault`.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import { hasCapability } from '#lib/platform/index.js'
import { getSetting, setSetting } from '#lib/settings/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import EscapeFullScreenToastContent from './EscapeFullScreenToastContent.svelte'
import { ESCAPE_FULL_SCREEN_TOAST_ID } from './escape-full-screen-toast-id'

const log = getAppLogger('escapeKey')

/** Makes `stopPropagation` / `stopImmediatePropagation` on an Escape also prevent its default. */
function claimOnStop(event: KeyboardEvent): void {
  if (event.key !== 'Escape' || event.isComposing) return
  const stop = event.stopPropagation.bind(event)
  const stopImmediate = event.stopImmediatePropagation.bind(event)
  event.stopPropagation = () => {
    event.preventDefault()
    stop()
  }
  event.stopImmediatePropagation = () => {
    event.preventDefault()
    stopImmediate()
  }
}

/**
 * Installs the capture-phase listener that makes a stopped Escape a claimed one.
 * Capture on `window` runs before any handler in the page can stop the event.
 * Returns the uninstaller.
 */
export function installEscapeStopClaims(): () => void {
  window.addEventListener('keydown', claimOnStop, true)
  return () => {
    window.removeEventListener('keydown', claimOnStop, true)
  }
}

/**
 * Runs for an Escape nothing in the window used. Leaves full screen when the
 * setting allows it and the window is in full screen, and the first time that
 * happens, tells the user where the switch is. Only where Cmdr offers full
 * screen (`fullScreen`): elsewhere the setting is hidden, so it decides nothing.
 */
export async function exitFullScreenOnEscape(): Promise<void> {
  if (!hasCapability('fullScreen')) return
  if (!getSetting('advanced.exitFullScreenOnEscape')) return
  try {
    const appWindow = getCurrentWindow()
    if (!(await appWindow.isFullscreen())) return
    await appWindow.setFullscreen(false)
  } catch (error) {
    log.warn("Couldn't leave full screen on Escape: {error}", { error })
    return
  }
  if (getSetting('advanced.exitFullScreenOnEscapeHintShown')) return
  setSetting('advanced.exitFullScreenOnEscapeHintShown', true)
  addToast(EscapeFullScreenToastContent, {
    id: ESCAPE_FULL_SCREEN_TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
  })
}
