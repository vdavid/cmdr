/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 *
 * Dimensions scale with the user's effective text size: at 100% the values
 * below are the literal pixel dimensions; at 200% everything is doubled.
 * This keeps all settings rows visible and proportional. The settings page
 * itself updates `setMinSize`/`setMaxSize` live when the user moves the
 * slider — see `routes/settings/+page.svelte`.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emitTo } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { getEffectiveScale } from '$lib/text-size.svelte'

const log = getAppLogger('settings')

/** Base dimensions at scale = 1 (the historical hard-coded values). */
export const SETTINGS_BASE_WIDTH = 800
export const SETTINGS_BASE_HEIGHT = 600
export const SETTINGS_BASE_MAX_WIDTH = 852
export const SETTINGS_BASE_MIN_WIDTH = 600
export const SETTINGS_BASE_MIN_HEIGHT = 400

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
  const scale = getEffectiveScale()

  new WebviewWindow('settings', {
    url: section ? `/settings?section=${encodeURIComponent(JSON.stringify(section))}` : '/settings',
    title: 'Settings',
    width: SETTINGS_BASE_WIDTH * scale,
    height: SETTINGS_BASE_HEIGHT * scale,
    minWidth: SETTINGS_BASE_MIN_WIDTH * scale,
    minHeight: SETTINGS_BASE_MIN_HEIGHT * scale,
    maxWidth: SETTINGS_BASE_MAX_WIDTH * scale,
    center: true,
    resizable: true,
    decorations: true,
    focus: true,
  })
}
