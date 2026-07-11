/**
 * Round-trip persistence tests for the settings store's sparse model.
 *
 * `settings.json` holds ONLY keys an actor explicitly set (the Settings UI, the
 * MCP `set_setting` tool, a migration). Everything else resolves to the registry
 * default at read time. "Explicit" is a STRUCTURAL property of the write path
 * (which mutator ran), never a value comparison against the default: a deliberate
 * choice that happens to equal the default must persist so it survives a future
 * default change. These tests pin that contract:
 *
 *  (a) a fresh store with no file resolves to registry defaults and SAVES NOTHING,
 *  (b) an explicit set persists exactly that key,
 *  (c) a set to a value equal to the default STILL persists (structural),
 *  (d) reset DELETES the key (resolves back to default),
 *  (e) a legacy full `settings.json` keeps every present key (incl. equal-to-default),
 *  (f) pre-init reads return defaults and write nothing.
 *
 * Mirrors `shortcuts-store.test.ts`: a shared `disk` Map backs a fake
 * `@tauri-apps/plugin-store`, and `vi.resetModules()` + a fresh dynamic import
 * simulates a webview reload re-reading the file. The store's module-scoped
 * cache/explicit-set start empty on each import.
 */
import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest'
import { getDefaultValue } from './settings-registry'

// Shared backing store for the fake plugin-store, persisting across
// `vi.resetModules()` to stand in for the on-disk `settings.json`.
const disk = vi.hoisted(() => new Map<string, unknown>())

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn((_path: string, opts?: { defaults?: Record<string, unknown> }) => {
    // Apply defaults only for keys not already present (matches plugin-store).
    for (const [k, v] of Object.entries(opts?.defaults ?? {})) {
      if (!disk.has(k)) disk.set(k, v)
    }
    return Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(disk.delete(key)),
      has: (key: string) => Promise.resolve(disk.has(key)),
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => Promise.resolve(),
    })
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}))

vi.mock('./store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

// The backend-facing settings commands are irrelevant to on-disk persistence;
// stub them so init's fire-and-forget `record_settings_defaults` push and the
// restricted-window snapshot don't hit a real IPC layer.
vi.mock('$lib/tauri-commands/settings', () => ({
  recordSettingsDefaults: vi.fn(() => Promise.resolve()),
  getRestrictedWindowSettings: vi.fn(() => Promise.resolve({})),
  persistRestrictedWindowSetting: vi.fn(() => Promise.resolve({ status: 'ok' })),
}))

// Warm Vite's transform cache: `settings-store` transitively pulls the whole
// intl catalog, a cold transform runs several seconds and would race the
// per-test timeout. A `beforeAll` gets the longer hook budget.
beforeAll(async () => {
  await import('./settings-store')
})

async function loadStore() {
  return await import('./settings-store')
}

// Keys the store writes that aren't settings themselves.
const META_KEYS = new Set(['_schemaVersion'])

/** Setting ids currently persisted on disk (excludes bookkeeping keys). */
function persistedSettingKeys(): string[] {
  return [...disk.keys()].filter((k) => !META_KEYS.has(k))
}

beforeEach(() => {
  disk.clear()
  vi.resetModules()
})

describe('sparse settings persistence', () => {
  it('(a) a fresh store with no file saves nothing', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    // Init must not materialize anything: no settings, no bookkeeping key.
    expect([...disk.keys()]).toEqual([])
    // Reads still resolve to the registry default.
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
  })

  it('(b) an explicit set persists exactly that key', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    await store.forceSave()

    expect(disk.get('developer.mcpEnabled')).toBe(true)
    expect(persistedSettingKeys()).toEqual(['developer.mcpEnabled'])
  })

  it('(c) a set to the default value still persists (structural, not value-compare)', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    const def = getDefaultValue('developer.mcpEnabled')
    expect(def).toBe(false) // guard: the whole point is persisting a default-equal choice
    store.setSetting('developer.mcpEnabled', def)
    await store.forceSave()

    // Present on disk even though it equals the registry default: a deliberate
    // choice must survive a future default flip.
    expect(disk.has('developer.mcpEnabled')).toBe(true)
    expect(disk.get('developer.mcpEnabled')).toBe(false)
  })

  it('(d) reset deletes the key so it resolves back to default', async () => {
    const store = await loadStore()
    await store.initializeSettings()

    store.setSetting('developer.mcpEnabled', true)
    await store.forceSave()
    expect(disk.has('developer.mcpEnabled')).toBe(true)

    store.resetSetting('developer.mcpEnabled')
    await store.forceSave()

    expect(disk.has('developer.mcpEnabled')).toBe(false)
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
  })

  it('(e) a legacy full settings.json keeps every present key, incl. equal-to-default', async () => {
    // Seed a legacy file: a non-default choice plus a key explicitly stored at
    // its default value. Both are "explicit" because they're present on load.
    disk.set('developer.mcpEnabled', true)
    disk.set('developer.verboseLogging', getDefaultValue('developer.verboseLogging')) // equals default
    disk.set('_schemaVersion', 2)

    const store = await loadStore()
    await store.initializeSettings()

    expect(store.getSetting('developer.mcpEnabled')).toBe(true)
    expect(store.getSetting('developer.verboseLogging')).toBe(getDefaultValue('developer.verboseLogging'))

    // A save must not drop the equal-to-default key that was present on load.
    await store.forceSave()
    expect(disk.has('developer.mcpEnabled')).toBe(true)
    expect(disk.has('developer.verboseLogging')).toBe(true)
  })

  it('(f) pre-init reads return defaults and write nothing', async () => {
    const store = await loadStore()

    // Read several settings before initializeSettings() resolves.
    expect(store.getSetting('developer.mcpEnabled')).toBe(getDefaultValue('developer.mcpEnabled'))
    expect(store.getSetting('developer.verboseLogging')).toBe(getDefaultValue('developer.verboseLogging'))

    await store.initializeSettings()

    // Those reads must not have leaked anything to disk.
    expect([...disk.keys()]).toEqual([])
  })
})
