/**
 * Settings-side helpers for the downloads-notifications mode.
 *
 * The setting key lives in the M7 registry. M5 reads it with a safe default
 * so the feature can ship before the registry entry exists. Once M7 lands
 * the registry entry, the same key path keeps working — no changes here.
 */

import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('downloads')

/** Setting key. Matches the M7 registry entry; defined here so M5 doesn't depend on M7. */
export const DOWNLOADS_NOTIFICATIONS_SETTING_KEY = 'behavior.fileSystemWatching.downloadsNotifications'

export type DownloadsNotificationsMode = 'in-app' | 'macos' | 'both' | 'neither'

const VALID_MODES: ReadonlySet<DownloadsNotificationsMode> = new Set(['in-app', 'macos', 'both', 'neither'])

function isValidMode(value: unknown): value is DownloadsNotificationsMode {
  return typeof value === 'string' && VALID_MODES.has(value as DownloadsNotificationsMode)
}

/**
 * Read the current downloads-notifications mode. Returns `'in-app'` (the
 * registered default) when the setting key isn't in the registry yet (M7
 * adds it) or the stored value is corrupt.
 *
 * Wrapped in try/catch because `getSetting` throws on unknown keys via
 * `getDefaultValue`. Once M7 registers the entry, the catch path becomes
 * unreachable but harmless.
 */
export function getDownloadsNotificationsMode(): DownloadsNotificationsMode {
  try {
    // Cast because M5 ships before the registry knows about this key.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- pre-M7 the key isn't in SettingId
    const value = getSetting(DOWNLOADS_NOTIFICATIONS_SETTING_KEY as any) as unknown
    if (isValidMode(value)) return value
    return 'in-app'
  } catch {
    // Setting not registered yet (M7 territory) — fall back to the documented default.
    return 'in-app'
  }
}

/**
 * Write the current downloads-notifications mode. Wrapped in try/catch for
 * the same reason as the reader: M7 adds the registry entry, but M5's
 * "Stop showing these" button must work right away. Without a registry
 * entry, `setSetting`'s validate step throws — log and continue.
 */
export function setDownloadsNotificationsMode(value: DownloadsNotificationsMode): void {
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- pre-M7 the key isn't in SettingId
    setSetting(DOWNLOADS_NOTIFICATIONS_SETTING_KEY as any, value as any)
  } catch (err) {
    log.warn('Failed to write downloads-notifications mode ({value}): {err}', { value, err: String(err) })
  }
}

/**
 * Deep-link to **Settings > Behavior > File system watching > Downloads
 * notifications**. M5 opens the section ("Drive indexing" still owns the
 * section path); M7 will rename the section to "File system watching" and
 * extend the deep-link helper to focus the specific sub-group.
 */
export async function openSettingsToDownloadsNotifications(): Promise<void> {
  await openSettingsWindow(['Behavior', 'Drive indexing'])
}
