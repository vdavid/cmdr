/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emitTo } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('settings')

const SETTINGS_WIDTH = 800
const SETTINGS_HEIGHT = 600
const SETTINGS_MAX_WIDTH = 852
const SETTINGS_MIN_WIDTH = 600
const SETTINGS_MIN_HEIGHT = 400

/**
 * Opens the settings window, or focuses it if already open. When `section` is provided,
 * the settings window listens for the `navigate-to-section` event and scrolls/highlights
 * the matching section path (e.g., `['Network', 'SMB/Network shares']`).
 */
export async function openSettingsWindow(section?: string[]): Promise<void> {
  const existing = await WebviewWindow.getByLabel('settings')
  if (existing) {
    // Emit to the settings window so it can self-focus. Cross-window setFocus()
    // doesn't reliably bring a window to front on macOS.
    await emitTo('settings', 'focus-self')
    if (section) {
      await emitTo('settings', 'navigate-to-section', { section })
    }
    return
  }

  log.debug('Creating new settings window')

  // JSON-encode the section path because section names can contain `/` (e.g.
  // "SMB/Network shares"). Plain `join('/')` would split incorrectly on the receiving end.
  new WebviewWindow('settings', {
    url: section ? `/settings?section=${encodeURIComponent(JSON.stringify(section))}` : '/settings',
    title: 'Settings',
    width: SETTINGS_WIDTH,
    height: SETTINGS_HEIGHT,
    minWidth: SETTINGS_MIN_WIDTH,
    minHeight: SETTINGS_MIN_HEIGHT,
    maxWidth: SETTINGS_MAX_WIDTH,
    center: true,
    resizable: true,
    decorations: true,
    focus: true,
  })
}
