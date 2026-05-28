/**
 * Settings-side helpers for the downloads-notifications mode.
 *
 * The setting key lives in the registry as
 * `behavior.fileSystemWatching.downloadsNotifications`. Read/write happens via
 * the same `getSetting`/`setSetting` API as any other registry entry; the
 * try/catch around the reader is belt-and-braces against a corrupt value (the
 * registry guarantees the default, but defending the parse keeps the bridge
 * resilient if `settings.json` is hand-edited).
 */

import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('downloads')

/** Setting key. Mirrors the registry entry. */
export const DOWNLOADS_NOTIFICATIONS_SETTING_KEY = 'behavior.fileSystemWatching.downloadsNotifications'

/**
 * Anchor id of the **Downloads notifications** sub-group inside
 * `FileSystemWatchingSection.svelte`. Used by `openSettingsToDownloadsNotifications`
 * to land the M5 "Stop showing these" deep-link on the right row.
 */
export const DOWNLOADS_NOTIFICATIONS_ANCHOR_ID = 'settings-downloads-notifications'

export type DownloadsNotificationsMode = 'in-app' | 'macos' | 'both' | 'neither'

const VALID_MODES: ReadonlySet<DownloadsNotificationsMode> = new Set(['in-app', 'macos', 'both', 'neither'])

function isValidMode(value: unknown): value is DownloadsNotificationsMode {
  return typeof value === 'string' && VALID_MODES.has(value as DownloadsNotificationsMode)
}

/**
 * Read the current downloads-notifications mode. Returns `'in-app'` (the
 * registered default) when the stored value is corrupt. The try/catch covers
 * the unlikely "key isn't in the registry" path so callers can be loaded in
 * any order.
 */
export function getDownloadsNotificationsMode(): DownloadsNotificationsMode {
  try {
    const value = getSetting(DOWNLOADS_NOTIFICATIONS_SETTING_KEY) as unknown
    if (isValidMode(value)) return value
    return 'in-app'
  } catch {
    return 'in-app'
  }
}

/**
 * Write the current downloads-notifications mode. Wraps `setSetting` in a
 * try/catch as a defensive log so the M5 "Stop showing these" button never
 * throws even if the registry entry temporarily disappears (mid-rename, etc.).
 */
export function setDownloadsNotificationsMode(value: DownloadsNotificationsMode): void {
  try {
    setSetting(DOWNLOADS_NOTIFICATIONS_SETTING_KEY, value)
  } catch (err) {
    log.warn('Failed to write downloads-notifications mode ({value}): {err}', { value, err: String(err) })
  }
}

/**
 * Deep-link to **Settings > Behavior > File system watching**, scrolled to the
 * Downloads notifications sub-group. M5's "Stop showing these" button calls
 * this after flipping the setting to `'neither'`.
 */
export async function openSettingsToDownloadsNotifications(): Promise<void> {
  await openSettingsWindow(['Behavior', 'File system watching'], DOWNLOADS_NOTIFICATIONS_ANCHOR_ID)
}
