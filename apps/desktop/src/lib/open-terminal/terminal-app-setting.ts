/**
 * The two settings "Open terminal here" reads, and the way back to the row that
 * holds the first one.
 *
 * Both are ordinary registry entries; this module exists so the action, its two
 * toasts, and the tests all name them once. The deep link mirrors the downloads
 * and low-disk-space toasts (`$lib/downloads/notifications-mode.ts`).
 */

import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
import { TERMINAL_APP_BUNDLE_ID } from '$lib/settings/sections/terminal-app-options'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

/** The app the action launches: a bundle id, or an absolute `.app` path. */
export const TERMINAL_APP_SETTING_KEY = 'behavior.openTerminalHereApp'

/** The one-time hint toast's flag. Hidden: nothing renders a row for it. */
export const TERMINAL_HINT_SEEN_SETTING_KEY = 'behavior.openTerminalHereToastSeen'

/**
 * The stored choice, falling back to Terminal.app when the value is missing or
 * isn't a string. Rust falls back the same way, so a corrupt value can only cost
 * the user the app they picked, never the action.
 */
export function getTerminalAppChoice(): string {
  const value: unknown = getSetting(TERMINAL_APP_SETTING_KEY)
  return typeof value === 'string' && value.length > 0 ? value : TERMINAL_APP_BUNDLE_ID
}

/** Writes the chosen app. The settings row picks the change up on its next render. */
export function setTerminalAppChoice(appChoice: string): void {
  setSetting(TERMINAL_APP_SETTING_KEY, appChoice)
}

/** Whether the one-time hint has already been shown. */
export function getTerminalHintSeen(): boolean {
  return getSetting(TERMINAL_HINT_SEEN_SETTING_KEY)
}

/** Spends the one-time hint flag. */
export function markTerminalHintSeen(): void {
  setSetting(TERMINAL_HINT_SEEN_SETTING_KEY, true)
}

/**
 * Deep-links to **Settings > Behavior > Navigation & file ops**, scrolled to the
 * "Open terminal here uses" row. Both toasts' buttons land here.
 *
 * Swallows a failure into a log line: a window that won't open is not worth
 * throwing out of a toast button, and the toast has already said its piece.
 */
export async function openSettingsToTerminalApp(): Promise<void> {
  try {
    await openSettingsWindow(
      'open-terminal-toast',
      ['Behavior', 'Navigation & file ops'],
      settingAnchorId(TERMINAL_APP_SETTING_KEY),
    )
  } catch (err) {
    log.warn('Failed to open Settings from an open-terminal-here toast: {err}', { err: String(err) })
  }
}
