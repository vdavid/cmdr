/**
 * The organization's managed policy (MDM) as this window shows it: one reactive copy of the
 * backend's `ManagedPolicyView`, fetched once and followed through `managed-policy-changed`.
 *
 * The backend decides and enforces everything (`src-tauri/src/managed_policy/`); this only holds
 * its answer for the UI. The settings store starts it (`initializeSettings`) and overlays the
 * per-setting locks on reads, so a component asks `isSettingLocked(id)` here for the visual state
 * and reads values through `getSetting` as always.
 */

import type { ManagedPolicyView, SettingLock } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('managed-policy')

/** What a window shows before (or without) an answer: nothing managed. The backend enforces regardless. */
export const UNMANAGED: ManagedPolicyView = {
  managed: false,
  usageStatsDisabled: false,
  reportsDisabled: false,
  updates: { kind: 'enabled', automaticChecks: true, ceiling: null },
  ai: { mode: 'allowed', allowedCloudHosts: null },
  lockedSettings: [],
}

let policy = $state<ManagedPolicyView>(UNMANAGED)
let started = false

/** A policy change, with the view it replaced (the store needs both to tell what moved). */
export interface ManagedPolicyChange {
  previous: ManagedPolicyView
  next: ManagedPolicyView
}

/** The policy this window shows. Reactive. */
export function getManagedPolicyView(): ManagedPolicyView {
  return policy
}

/** The lock the policy puts on setting `id`, if any. Reactive. */
export function settingLockIn(view: ManagedPolicyView, id: string): SettingLock | undefined {
  return view.lockedSettings.find((locked) => locked.id === id)?.lock
}

/** The lock the current policy puts on setting `id`, if any. Reactive. */
export function getSettingLock(id: string): SettingLock | undefined {
  return settingLockIn(policy, id)
}

/**
 * Whether the policy pins setting `id` to one value, so its row renders disabled with the managed
 * note. A setting with only some values ruled out (`ai.provider` under `DisableCloudAI`) stays
 * usable: its own control disables just those options. Reactive.
 */
export function isSettingLocked(id: string): boolean {
  return getSettingLock(id)?.kind === 'fixed'
}

/** Whether the policy constrains setting `id` at all. Reactive. */
export function isSettingManaged(id: string): boolean {
  return getSettingLock(id) !== undefined
}

/**
 * Fetch the policy and follow its changes. Once per window; later calls do nothing. `onChange`
 * runs after the new view is in place, with the one it replaced. A failed fetch leaves the window
 * on `UNMANAGED` with a warning: the UI then shows no lock, and the backend still refuses.
 */
export async function initManagedPolicy(onChange: (change: ManagedPolicyChange) => void): Promise<void> {
  if (started) return
  started = true
  // Subscribe first so no change slips past; a change that lands while the fetch is out is newer
  // than (or as new as) its answer, so the answer then loses.
  // An object, not a `let`: TypeScript can't see the listener flip it across the `await`.
  const firstRead = { superseded: false }
  try {
    // Loaded on first use: the settings store imports this file, and a static import of the
    // barrel would drag the whole IPC surface into every settings consumer (`settings-store.ts`).
    const { getManagedPolicy: fetchManagedPolicy, onManagedPolicyChanged } = await import('$lib/tauri-commands')
    await onManagedPolicyChanged((next) => {
      firstRead.superseded = true
      const previous = policy
      policy = next
      log.info('The managed policy changed (managed: {managed})', { managed: next.managed })
      onChange({ previous, next })
    })
    const fetched = await fetchManagedPolicy()
    if (!firstRead.superseded) policy = fetched
  } catch (error) {
    log.warn('Could not read the managed policy, showing nothing as managed: {error}', { error: String(error) })
  }
}
