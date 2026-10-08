import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { LockedSetting, ManagedPolicyView } from '$lib/ipc/bindings'
import { lockAllowsWrite, lockedValue } from './overlay'

const ipc = vi.hoisted(() => ({
  fetch: vi.fn<() => Promise<unknown>>(),
  onChanged: null as ((view: unknown) => void) | null,
}))

vi.mock('$lib/tauri-commands', () => ({
  getManagedPolicy: () => ipc.fetch(),
  onManagedPolicyChanged: (handler: (view: unknown) => void) => {
    ipc.onChanged = handler
    return Promise.resolve(() => {})
  },
}))

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

async function load() {
  return await import('./managed-policy.svelte')
}

beforeEach(() => {
  vi.resetModules()
  ipc.fetch.mockReset()
  ipc.onChanged = null
})

describe('a lock applied to one value', () => {
  it('pins a fixed setting and refuses every write to it', () => {
    expect(lockedValue(usageStatsOff.lock, true)).toBe(false)
    expect(lockAllowsWrite(usageStatsOff.lock, false)).toBe(false)
  })

  it('maps only a ruled-out value to the fallback, and refuses only that value', () => {
    expect(lockedValue(cloudDisallowed.lock, 'cloud')).toBe('off')
    expect(lockedValue(cloudDisallowed.lock, 'local')).toBe('local')
    expect(lockAllowsWrite(cloudDisallowed.lock, 'cloud')).toBe(false)
    expect(lockAllowsWrite(cloudDisallowed.lock, 'local')).toBe(true)
  })

  it('leaves an unlocked setting alone', () => {
    expect(lockedValue(undefined, 'cloud')).toBe('cloud')
    expect(lockAllowsWrite(undefined, 'cloud')).toBe(true)
  })
})

describe('managed policy store', () => {
  it('tells a pinned setting from a narrowed one', async () => {
    ipc.fetch.mockResolvedValue(policy([usageStatsOff, cloudDisallowed]))
    const store = await load()
    await store.initManagedPolicy(() => {})

    expect(store.isSettingLocked('analytics.enabled')).toBe(true)
    expect(store.isSettingLocked('ai.provider')).toBe(false)
    expect(store.isSettingManaged('ai.provider')).toBe(true)
    expect(store.isSettingManaged('updates.autoCheck')).toBe(false)
  })

  it('shows nothing as managed when the fetch fails', async () => {
    ipc.fetch.mockRejectedValue(new Error('no backend'))
    const store = await load()
    await store.initManagedPolicy(() => {})
    expect(store.getManagedPolicyView()).toEqual(store.UNMANAGED)
  })

  it('keeps a change that lands while the first fetch is out, over the fetch answer', async () => {
    let answer: (view: ManagedPolicyView) => void = () => {}
    ipc.fetch.mockReturnValue(new Promise((resolve) => (answer = resolve)))
    const store = await load()
    const onChange = vi.fn()
    const started = store.initManagedPolicy(onChange)
    await vi.waitFor(() => {
      expect(ipc.onChanged).not.toBeNull()
    })

    ipc.onChanged?.(policy([usageStatsOff]))
    answer(policy([]))
    await started

    expect(store.isSettingLocked('analytics.enabled')).toBe(true)
    expect(onChange).toHaveBeenCalledTimes(1)
  })
})
