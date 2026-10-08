/**
 * How one setting lock from the backend's `locked_settings` applies to a value: the frontend twin
 * of `managed_policy::overlay` and `refuses_write`. Pure. The backend decides WHICH settings are
 * locked and what they read as; these only apply that answer.
 */

import type { SettingLock } from '$lib/ipc/bindings'

/** What a setting reads as under `lock`, given the value the store holds (or its default). */
export function lockedValue(lock: SettingLock | undefined, stored: unknown): unknown {
  if (lock === undefined) return stored
  if (lock.kind === 'fixed') return lock.value
  return lock.values.some((value) => value === stored) ? lock.fallback : stored
}

/** Whether `lock` lets the person (or an MCP client) write `value`. A fixed lock allows nothing. */
export function lockAllowsWrite(lock: SettingLock | undefined, value: unknown): boolean {
  if (lock === undefined) return true
  if (lock.kind === 'fixed') return false
  return !lock.values.some((disallowed) => disallowed === value)
}
