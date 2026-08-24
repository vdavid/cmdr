/**
 * Client-safe views over resolved settings adoption. The resolution itself is server-side (it reads
 * the defaults manifest); these only reshape what came back for rendering.
 */

import type { AdoptionValue, SettingAdoption } from './server/settings-defaults.js'

/** The share of eligible installs running the default, or null when nobody's build has the setting. */
export function shareOnDefault(setting: SettingAdoption): number | null {
  if (setting.eligible === 0) return null
  return setting.onDefault / setting.eligible
}

/** The most common value that isn't the default, or null when nobody changed it. */
export function topOverride(setting: SettingAdoption): AdoptionValue | null {
  return setting.values.find((value) => !value.isDefault) ?? null
}

/** A share as a whole-number percent. */
export function formatShare(share: number): string {
  return `${String(Math.round(share * 100))}%`
}

/**
 * Settings people actually touch, most-changed first. A setting nobody has moved says nothing
 * interesting, and there are around a hundred of them, so the table leads with the ones that do.
 */
export function mostChanged(settings: SettingAdoption[]): SettingAdoption[] {
  return settings
    .filter((setting) => setting.eligible > 0 && setting.onDefault < setting.eligible)
    .sort((a, b) => {
      const changedA = 1 - (shareOnDefault(a) ?? 1)
      const changedB = 1 - (shareOnDefault(b) ?? 1)
      return changedB - changedA || a.key.localeCompare(b.key)
    })
}

/** The trailing note about settings nobody has moved, so the count reads as a sentence. */
export function unchangedNote(count: number): string {
  return count === 1
    ? '1 setting sits at its default everywhere'
    : `${String(count)} settings sit at their default everywhere`
}

/** One named setting, for the headline row. Null when the manifest has never carried that key. */
export function settingByKey(settings: SettingAdoption[], key: string): SettingAdoption | null {
  return settings.find((setting) => setting.key === key) ?? null
}

/** How many installs run a named setting at a named value, rendered as a share of the eligible ones. */
export function formatValueShare(setting: SettingAdoption | null, label: string): string {
  if (setting === null || setting.eligible === 0) return '–'
  const match = setting.values.find((value) => value.label === label)
  return formatShare((match?.installs ?? 0) / setting.eligible)
}

/** The share of eligible installs whose effective value is anything BUT `label`. */
export function formatShareUnlike(setting: SettingAdoption | null, label: string): string {
  if (setting === null || setting.eligible === 0) return '–'
  const match = setting.values.find((value) => value.label === label)
  return formatShare(1 - (match?.installs ?? 0) / setting.eligible)
}
