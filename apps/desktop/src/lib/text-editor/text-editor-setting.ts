/**
 * The setting F4 reads, and the way back to the row that holds it.
 *
 * An ordinary registry entry; this module exists so the launch, its toasts, and
 * the tests all name it once. The deep link mirrors "Open terminal here"
 * (`$lib/open-terminal/terminal-app-setting.ts`).
 */

import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
import { getAppLogger } from '$lib/logging/logger'
import { SYSTEM_DEFAULT_EDITOR_CHOICE } from './text-editor-choice'

const log = getAppLogger('fileExplorer')

/** The app F4 opens files in: `system`, a bundle id, or an absolute `.app` path. */
export const TEXT_EDITOR_SETTING_KEY = 'behavior.textEditorApp'

/**
 * The stored choice, reading a missing, empty, or non-string value as the system
 * default. Rust reads an empty choice the same way, so a corrupt value can only
 * cost the user the app they picked, never the press.
 */
export function getTextEditorChoice(): string {
  const value: unknown = getSetting(TEXT_EDITOR_SETTING_KEY)
  return typeof value === 'string' && value.length > 0 ? value : SYSTEM_DEFAULT_EDITOR_CHOICE
}

/** Writes the chosen app. */
export function setTextEditorChoice(appChoice: string): void {
  setSetting(TEXT_EDITOR_SETTING_KEY, appChoice)
}

/** The one-time hint's flag. Hidden: nothing renders a row for it. */
export const TEXT_EDITOR_HINT_SEEN_SETTING_KEY = 'behavior.textEditorHintSeen'

/**
 * Whether the one-time hint has already been shown. Anything but an explicit
 * `false` reads as shown, so a corrupt value can cost the user the hint but never
 * bring back a toast they've already seen.
 */
export function getTextEditorHintSeen(): boolean {
  // `unknown`, not the typed `boolean`: the store can hand back whatever
  // `settings.json` holds, and that's exactly the case this reads defensively.
  const value: unknown = getSetting(TEXT_EDITOR_HINT_SEEN_SETTING_KEY)
  return value !== false
}

/** Spends the one-time hint flag. */
export function markTextEditorHintSeen(): void {
  setSetting(TEXT_EDITOR_HINT_SEEN_SETTING_KEY, true)
}

/**
 * Deep-links to **Settings > Behavior > Navigation & file ops**, scrolled to the
 * text editor row. Every text-editor toast's "Open settings" lands here.
 *
 * Swallows a failure into a log line: a window that won't open is not worth
 * throwing out of a toast button, and the toast has already said its piece.
 */
export async function openSettingsToTextEditor(): Promise<void> {
  try {
    await openSettingsWindow(
      'text-editor-toast',
      ['Behavior', 'Navigation & file ops'],
      settingAnchorId(TEXT_EDITOR_SETTING_KEY),
    )
  } catch (err) {
    log.warn('Failed to open Settings from a text editor toast: {err}', { err: String(err) })
  }
}
