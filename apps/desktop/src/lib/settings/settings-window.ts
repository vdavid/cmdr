/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 *
 * **Sizing model.** Width has two parts:
 *
 *   window_width = chrome (fixed) + content_area (scales with text size)
 *
 * The chrome covers the fixed-width sidebar (220 px) plus the content
 * wrapper's horizontal padding (16 px each side = 32 px). Whatever the text
 * scale, those values stay constant: only the readable content area scales,
 * so a row reads with the same proportions at every size. Height scales
 * fully (no fixed-height chrome inside).
 *
 * The settings page (`routes/settings/+page.svelte`) updates `setMinSize` /
 * `setMaxSize` live when the user moves the slider so the constraints track
 * the new scale. See that file for the live-update logic.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emitTo } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { getEffectiveScale } from '$lib/text-size.svelte'
import { decorateChildWindowTitle } from '$lib/app-mode'

const log = getAppLogger('settings')

/**
 * Fixed-width chrome that does NOT scale with text size:
 *   - Sidebar: 220 px (`.settings-sidebar { width: 220px }`)
 *   - Content wrapper padding: `var(--spacing-lg)` × 2 = 32 px
 *
 * Keep in sync with `routes/settings/+page.svelte`'s `.settings-sidebar` and
 * `.settings-content-wrapper` rules.
 */
export const SETTINGS_CHROME_WIDTH = 252

/** Content-area width at scale = 1. Window total = chrome + content × scale. */
export const SETTINGS_CONTENT_BASE_MIN_WIDTH = 348
export const SETTINGS_CONTENT_BASE_MAX_WIDTH = 600

/** Height scales fully, with no fixed-height chrome inside the settings layout. */
export const SETTINGS_BASE_HEIGHT = 600
export const SETTINGS_BASE_MIN_HEIGHT = 400

export const settingsMinWidth = (scale: number): number =>
  SETTINGS_CHROME_WIDTH + SETTINGS_CONTENT_BASE_MIN_WIDTH * scale
export const settingsMaxWidth = (scale: number): number =>
  SETTINGS_CHROME_WIDTH + SETTINGS_CONTENT_BASE_MAX_WIDTH * scale

/**
 * Opens the settings window, or focuses it if already open. When `section` is provided,
 * the settings window listens for the `navigate-to-section` event and scrolls/highlights
 * the matching section path (e.g., `['File systems', 'SMB/Network shares']`).
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
    title: decorateChildWindowTitle('Settings'),
    // Open at max width so the content-area starts at its scaled cap; user can
    // shrink down to `settingsMinWidth(scale)`.
    width: settingsMaxWidth(scale),
    height: SETTINGS_BASE_HEIGHT * scale,
    minWidth: settingsMinWidth(scale),
    minHeight: SETTINGS_BASE_MIN_HEIGHT * scale,
    maxWidth: settingsMaxWidth(scale),
    center: true,
    resizable: true,
    decorations: true,
    focus: true,
  })
}
