/**
 * The state a boolean setting control needs, shared by `SettingCheckbox` and
 * `SettingSwitch`: the registry label, the current value kept in sync with
 * external resets, and the write-through `set`. Call it during component init;
 * the `onMount` inside binds to the calling component.
 */

import { onMount } from 'svelte'
import {
  getSetting,
  setSetting,
  getSettingDefinition,
  onSpecificSettingChange,
  type SettingId,
  type SettingsValues,
} from '$lib/settings'

export interface BooleanSetting {
  /** The registry label, falling back to the id for an unregistered setting. */
  readonly label: string
  readonly checked: boolean
  set: (next: boolean) => void
}

export function useBooleanSetting(id: SettingId): BooleanSetting {
  const label = getSettingDefinition(id)?.label ?? id

  let checked = $state(getSetting(id) as boolean)

  // Subscribe to setting changes (for external resets)
  onMount(() => {
    return onSpecificSettingChange(id, (newValue) => {
      checked = newValue as boolean
    })
  })

  return {
    label,
    get checked() {
      return checked
    },
    set: (next: boolean) => {
      checked = next
      setSetting(id, next as SettingsValues[typeof id])
    },
  }
}
