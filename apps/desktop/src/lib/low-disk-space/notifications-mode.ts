/**
 * Settings-side helpers for the low-disk-space warning.
 *
 * The two setting keys live in the registry as
 * `behavior.fileSystemWatching.lowDiskSpaceNotifications` and
 * `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent`. Read/write
 * happens via the same `getSetting`/`setSetting` API as any other registry
 * entry; the try/catch around the readers is belt-and-braces against a corrupt
 * stored value (the registry guarantees the default, but a hand-edited
 * `settings.json` could land here).
 */

import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { setLowDiskSpaceConfig } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('low-disk-space')

/** Setting keys. Mirror the registry entries. */
export const LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY = 'behavior.fileSystemWatching.lowDiskSpaceNotifications'
export const LOW_DISK_SPACE_THRESHOLD_SETTING_KEY = 'behavior.fileSystemWatching.lowDiskSpaceThresholdPercent'

/**
 * Anchor id of the **Low disk space** sub-group inside
 * `NotificationsSection.svelte`. Used by `openSettingsToLowDiskSpace` to
 * land the toast's "Disable these notifications" deep-link on the right row.
 */
export const LOW_DISK_SPACE_ANCHOR_ID = 'settings-low-disk-space'

/** Default free-space percent threshold. Mirrors the registry default. */
export const DEFAULT_LOW_DISK_SPACE_THRESHOLD_PERCENT = 5

export type LowDiskSpaceNotificationsMode = 'in-app' | 'macos' | 'off'

const VALID_MODES: ReadonlySet<LowDiskSpaceNotificationsMode> = new Set(['in-app', 'macos', 'off'])

function isValidMode(value: unknown): value is LowDiskSpaceNotificationsMode {
  return typeof value === 'string' && VALID_MODES.has(value as LowDiskSpaceNotificationsMode)
}

/**
 * Read the current low-disk-space notifications mode. Returns `'in-app'` (the
 * registered default) when the stored value is corrupt. The try/catch covers
 * the unlikely "key isn't in the registry" path so callers can be loaded in
 * any order.
 */
export function getLowDiskSpaceNotificationsMode(): LowDiskSpaceNotificationsMode {
  try {
    const value = getSetting(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY) as unknown
    if (isValidMode(value)) return value
    return 'in-app'
  } catch {
    return 'in-app'
  }
}

/**
 * Write the current low-disk-space notifications mode. Wraps `setSetting` in a
 * try/catch as a defensive log so the toast's "Disable these notifications"
 * button never throws even if the registry entry temporarily disappears.
 */
export function setLowDiskSpaceNotificationsMode(value: LowDiskSpaceNotificationsMode): void {
  try {
    setSetting(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY, value)
  } catch (err) {
    log.warn('Failed to write low-disk-space notifications mode ({value}): {err}', { value, err: String(err) })
  }
}

/** Read the current threshold percent, falling back to the registered default. */
export function getLowDiskSpaceThresholdPercent(): number {
  try {
    const value = getSetting(LOW_DISK_SPACE_THRESHOLD_SETTING_KEY) as unknown
    if (typeof value === 'number' && Number.isFinite(value) && value >= 1) return value
    return DEFAULT_LOW_DISK_SPACE_THRESHOLD_PERCENT
  } catch {
    return DEFAULT_LOW_DISK_SPACE_THRESHOLD_PERCENT
  }
}

/**
 * Push the current low-disk-space config to the backend poller. Re-reads both
 * settings fresh at call time (same shape as `ai-config.ts`'s
 * `pushConfigToBackend`), so callers never pass cached values. Wired from
 * `settings-applier.ts` for both setting keys. No startup push needed: the
 * Rust side seeds itself from `settings.json` in `lib.rs`.
 */
export async function pushLowDiskSpaceConfigToBackend(): Promise<void> {
  const mode = getLowDiskSpaceNotificationsMode()
  const threshold = getLowDiskSpaceThresholdPercent()
  try {
    await setLowDiskSpaceConfig(mode !== 'off', threshold)
  } catch (err) {
    log.warn('Failed to push low-disk-space config: {err}', { err: String(err) })
  }
}

/**
 * Deep-link to **Settings > Behavior > Notifications**, scrolled to the
 * Low disk space sub-group. The toast's "Disable these notifications" button
 * calls this after flipping the setting to `'off'`.
 */
export async function openSettingsToLowDiskSpace(): Promise<void> {
  await openSettingsWindow(['Behavior', 'Notifications'], LOW_DISK_SPACE_ANCHOR_ID)
}
