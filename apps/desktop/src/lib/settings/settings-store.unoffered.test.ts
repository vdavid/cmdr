/**
 * A stored enum value whose option this OS doesn't offer (`ai.provider` = `local` on a build with
 * no local AI: a `settings.json` carried over from a Mac). It reads as the setting's default, so
 * every reader sees a state this OS can be in, and the file keeps the value for the Mac.
 *
 * Same fake-disk harness as `settings-store.managed.test.ts`.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, onTestFinished } from 'vitest'
import type { LockedSetting, ManagedPolicyView } from '#lib/ipc/bindings.js'
import { withCapabilities } from '#lib/platform/test-platform.js'

const disk = vi.hoisted(() => new Map<string, unknown>())
const policyState = vi.hoisted(() => ({ initial: undefined as ManagedPolicyView | undefined }))

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

vi.mock('#lib/tauri-commands/settings.js', () => ({
  recordSettingsDefaults: vi.fn(() => Promise.resolve()),
  getRestrictedWindowSettings: vi.fn(() => Promise.resolve({})),
  persistRestrictedWindowSetting: vi.fn(() => Promise.resolve({ status: 'ok' })),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  getManagedPolicy: vi.fn(() => Promise.resolve(policyState.initial)),
  onManagedPolicyChanged: vi.fn(() => Promise.resolve(() => {})),
}))

// Stateless and heavy, so passed through to load once per file: `docs/testing.md` § "Reloading a
// module per test".
vi.mock('./settings-registry', async (importOriginal) => await importOriginal())
vi.mock('./settings-migrations', async (importOriginal) => await importOriginal())

beforeAll(async () => {
  await import('./settings-store')
})

function policy(lockedSettings: LockedSetting[]): ManagedPolicyView {
  return {
    managed: lockedSettings.length > 0,
    usageStatsDisabled: false,
    reportsDisabled: false,
    securityLogRequired: false,
    smbEncryptionRequired: false,
    updates: { kind: 'enabled', automaticChecks: true, ceiling: null },
    ai: { mode: 'allowed', allowedCloudHosts: null },
    lockedSettings,
  }
}

async function loadStore(initial: ManagedPolicyView = policy([])) {
  policyState.initial = initial
  const store = await import('./settings-store')
  await store.initializeSettings()
  return store
}

beforeEach(() => {
  disk.clear()
  vi.resetModules()
})

describe('a stored value this OS has no option for', () => {
  it('reads as the default where the option is not offered, and the file keeps it', async () => {
    onTestFinished(withCapabilities())
    disk.set('ai.provider', 'local')
    const store = await loadStore()

    expect(store.getSetting('ai.provider')).toBe('off')
    expect(store.unofferedSettingValue('ai.provider')).toBe('local')
    await store.forceSave()
    expect(disk.get('ai.provider')).toBe('local')
  })

  it('reads as stored where the option is offered', async () => {
    onTestFinished(withCapabilities('localAi'))
    disk.set('ai.provider', 'local')
    const store = await loadStore()

    expect(store.getSetting('ai.provider')).toBe('local')
    expect(store.unofferedSettingValue('ai.provider')).toBeUndefined()
  })

  it('leaves every other value alone', async () => {
    onTestFinished(withCapabilities())
    disk.set('ai.provider', 'cloud')
    const store = await loadStore()

    expect(store.getSetting('ai.provider')).toBe('cloud')
    expect(store.unofferedSettingValue('ai.provider')).toBeUndefined()
    expect(store.unofferedSettingValue('appearance.language')).toBeUndefined()
  })

  it('resolves a value the organization pins the same way', async () => {
    onTestFinished(withCapabilities())
    const pinnedLocal: LockedSetting = { id: 'ai.provider', lock: { kind: 'fixed', value: 'local' } }
    const store = await loadStore(policy([pinnedLocal]))

    expect(store.getSetting('ai.provider')).toBe('off')
    expect(store.unofferedSettingValue('ai.provider')).toBe('local')
  })

  it('refuses to write an option this OS does not offer', async () => {
    onTestFinished(withCapabilities())
    const store = await loadStore()

    expect(() => {
      store.setSetting('ai.provider', 'local')
    }).toThrow()
    expect(store.getSetting('ai.provider')).toBe('off')
    await store.forceSave()
    expect(disk.has('ai.provider')).toBe(false)
  })

  it('writes the option where it is offered', async () => {
    onTestFinished(withCapabilities('localAi'))
    const store = await loadStore()

    store.setSetting('ai.provider', 'local')
    expect(store.getSetting('ai.provider')).toBe('local')
  })
})
