/**
 * The organization's lock on one setting, as a row primitive renders it: disabled, and described
 * by the managed note `SettingRow` shows under the row. Every primitive reads this itself, so a
 * section never has to remember which of its settings a policy could lock.
 */

import type { SettingId } from '$lib/settings'
import { disabledNoteId } from '$lib/settings/settings-window'
import { isSettingLocked } from '$lib/managed-policy/managed-policy.svelte'

export interface SettingLockState {
  /** The policy pins this setting: the control renders disabled. */
  readonly locked: boolean
  /** The row's managed note while locked, else the caller's own `aria-describedby` target. */
  describedBy: (own?: string) => string | undefined
}

export function useSettingLock(id: SettingId): SettingLockState {
  const locked = $derived(isSettingLocked(id))
  return {
    get locked() {
      return locked
    },
    describedBy: (own) => (locked ? disabledNoteId(id) : own),
  }
}
