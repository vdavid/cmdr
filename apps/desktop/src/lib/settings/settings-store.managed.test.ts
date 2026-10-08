/**
 * The organization's policy as the settings store applies it. The backend decides which settings
 * are locked (`managed_policy/locked.rs`); the store only overlays that answer on reads, refuses
 * writes it rules out, and never persists the overlay (the policy overlays, it never rewrites).
 *
 * Same fake-disk harness as `settings-store.persistence.test.ts`: `vi.resetModules()` plus a fresh
 * import is a webview reload.
 */
import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest'
import type { LockedSetting, ManagedPolicyView } from '$lib/ipc/bindings'
import { getDefaultValue } from './settings-registry'

const disk = vi.hoisted(() => new Map<string, unknown>())
const policyState = vi.hoisted(() => ({
  initial: undefined as ManagedPolicyView | undefined,
  onChanged: null as ((view: unknown) => void) | null,
}))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(disk.delete(key)),
      has: (key: string) => Promise.resolve(disk.has(key)),
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => Promise.resolve(),
    }),
  ),
}))

vi.mock('./store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

vi.mock('$lib/tauri-commands/settings', () => ({
  recordSettingsDefaults: vi.fn(() => Promise.resolve()),
  getRestrictedWindowSettings: vi.fn(() => Promise.resolve({})),
  persistRestrictedWindowSetting: vi.fn(() => Promise.resolve({ status: 'ok' })),
}))

vi.mock('$lib/tauri-commands', () => ({
  getManagedPolicy: vi.fn(() => Promise.resolve(policyState.initial)),
  onManagedPolicyChanged: vi.fn((handler: (view: unknown) => void) => {
    policyState.onChanged = handler
    return Promise.resolve(() => {})
  }),
}))

beforeAll(async () => {
  await import('./settings-store')
})

function policy(lockedSettings: LockedSetting[]): ManagedPolicyView {
  return {
    managed: lockedSettings.length > 0,
    usageStatsDisabled: false,
    reportsDisabled: false,
    updates: { kind: 'enabled', automaticChecks: true, ceiling: null },
    ai: { mode: 'allowed', allowedCloudHosts: null },
    lockedSettings,
  }
}

const usageStatsOff: LockedSetting = { id: 'analytics.enabled', lock: { kind: 'fixed', value: false } }
const cloudDisallowed: LockedSetting = {
  id: 'ai.provider',
  lock: { kind: 'disallowedValues', values: ['cloud'], fallback: 'off' },
}

/** A fresh window: the store module reloads and initializes under `initial`. */
async function loadStore(initial: ManagedPolicyView) {
  policyState.initial = initial
  policyState.onChanged = null
  const store = await import('./settings-store')
  await store.initializeSettings()
  return store
}

/** Lets the debounced save run, so a test can see what reached the disk. */
async function flushSaves(store: { forceSave: () => Promise<boolean> }) {
  await store.forceSave()
}

beforeEach(() => {
  disk.clear()
  vi.resetModules()
})

describe('settings under a managed policy', () => {
  it('reads a fixed lock over a stored value and writes nothing', async () => {
    disk.set('analytics.enabled', true)
    const store = await loadStore(policy([usageStatsOff]))

    expect(store.getSetting('analytics.enabled')).toBe(false)
    await flushSaves(store)
    expect(disk.get('analytics.enabled')).toBe(true)
  })

  it('reads a disallowed stored value as the fallback, and keeps an allowed one', async () => {
    disk.set('ai.provider', 'cloud')
    const store = await loadStore(policy([cloudDisallowed]))
    expect(store.getSetting('ai.provider')).toBe('off')

    disk.set('ai.provider', 'local')
    vi.resetModules()
    const reloaded = await loadStore(policy([cloudDisallowed]))
    expect(reloaded.getSetting('ai.provider')).toBe('local')
  })

  it('says when the policy, not the person, put a value on screen', async () => {
    disk.set('ai.provider', 'cloud')
    disk.set('analytics.enabled', false)
    const store = await loadStore(policy([cloudDisallowed, usageStatsOff]))

    // A stored `cloud` read as `off`: whatever shows came from the lock.
    expect(store.isOverriddenByPolicy('ai.provider')).toBe(true)
    // Pinned off, and the person had it off too: the read is theirs either way.
    expect(store.isOverriddenByPolicy('analytics.enabled')).toBe(false)
    // No lock at all.
    expect(store.isOverriddenByPolicy('updates.crashReports')).toBe(false)

    store.setSetting('ai.provider', 'local')
    expect(store.isOverriddenByPolicy('ai.provider')).toBe(false)
    // Don't leave the debounced save pending for the next test's fresh store.
    await flushSaves(store)
  })

  it('refuses a write the lock rules out, persisting nothing and notifying nobody', async () => {
    const store = await loadStore(policy([usageStatsOff, cloudDisallowed]))
    const listener = vi.fn()
    store.onSettingChange(listener)

    store.setSetting('analytics.enabled', true)
    store.setSetting('ai.provider', 'cloud')

    expect(listener).not.toHaveBeenCalled()
    expect(store.getSetting('analytics.enabled')).toBe(false)
    await flushSaves(store)
    expect(disk.has('analytics.enabled')).toBe(false)
    expect(disk.has('ai.provider')).toBe(false)
  })

  it('still takes an allowed value of a narrowed setting', async () => {
    const store = await loadStore(policy([cloudDisallowed]))
    store.setSetting('ai.provider', 'local')
    expect(store.getSetting('ai.provider')).toBe('local')
    await flushSaves(store)
    expect(disk.get('ai.provider')).toBe('local')
  })

  it('refuses to reset a fixed setting, so the person’s own choice survives the policy', async () => {
    disk.set('analytics.enabled', true)
    const store = await loadStore(policy([usageStatsOff]))

    store.resetSetting('analytics.enabled')
    await flushSaves(store)
    expect(disk.get('analytics.enabled')).toBe(true)
  })

  it('never reports a locked setting as modified because of the overlay', async () => {
    const store = await loadStore(policy([usageStatsOff]))
    // `analytics.enabled` defaults to on; the overlay reads off, but nobody changed anything.
    expect(getDefaultValue('analytics.enabled')).toBe(true)
    expect(store.getSetting('analytics.enabled')).toBe(false)
    expect(store.isModified('analytics.enabled')).toBe(false)
  })

  it('tells each listener once when a policy change moves an effective value, and not otherwise', async () => {
    disk.set('updates.crashReports', false)
    const store = await loadStore(policy([]))
    const all = vi.fn()
    const analytics = vi.fn()
    const crashReports = vi.fn()
    store.onSettingChange(all)
    store.onSpecificSettingChange('analytics.enabled', analytics)
    store.onSpecificSettingChange('updates.crashReports', crashReports)

    const reportsOff: LockedSetting = { id: 'updates.crashReports', lock: { kind: 'fixed', value: false } }
    policyState.onChanged?.(policy([usageStatsOff, reportsOff]))

    expect(analytics).toHaveBeenCalledTimes(1)
    expect(analytics).toHaveBeenCalledWith(false)
    // Already off on disk, so the lock changes nothing anyone reads.
    expect(crashReports).not.toHaveBeenCalled()
    expect(all).toHaveBeenCalledTimes(1)

    // The profile goes away: the person's own value comes back.
    policyState.onChanged?.(policy([]))
    expect(analytics).toHaveBeenCalledTimes(2)
    expect(analytics).toHaveBeenLastCalledWith(true)

    await flushSaves(store)
    expect(disk.has('analytics.enabled')).toBe(false)
  })
})
