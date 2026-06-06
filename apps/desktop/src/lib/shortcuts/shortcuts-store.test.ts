/**
 * Round-trip persistence tests for the shortcuts store.
 *
 * These guard the two disk-state invariants documented in `CLAUDE.md`
 * § "Empty array vs missing key":
 *
 * - A persisted empty array (`"shortcut:<id>": []`) means "user removed all
 *   shortcuts, don't use defaults" and must survive a reload.
 * - A `shortcut:<id>` key with no in-memory entry is stale and must be deleted
 *   on save, so a removed/reset customization can't resurrect at next load.
 *
 * "Reload" is simulated with `vi.resetModules()`: the store's in-memory map is
 * module-scoped, so re-importing the module after a reset mimics a fresh webview
 * re-reading `shortcuts.json` from disk. The mock `load()` is backed by a single
 * shared `Map` (`disk`) that persists across resets, standing in for the file on
 * disk.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
// Static import so the file genuinely exercises source (not just its mocks).
// `getDefaultShortcuts` reads the registry, not module-scoped store state, so
// it stays valid across the `vi.resetModules()` reloads below.
import { getDefaultShortcuts } from './shortcuts-store'

// Shared backing store for the fake plugin-store, persisting across
// `vi.resetModules()` to stand in for the on-disk `shortcuts.json`. A `Map`
// avoids dynamic property delete. Declared via `vi.hoisted` so the hoisted
// `vi.mock` factory can capture it.
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
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => Promise.resolve(),
    })
  }),
}))

vi.mock('$lib/settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    updateMenuAccelerator: () => Promise.resolve({ status: 'ok' as const, data: null }),
  },
}))

// Import fresh after a module reset so the store's module-scoped map starts empty,
// then re-reads `disk`. Returns the relevant store functions.
async function loadStore() {
  return await import('./shortcuts-store')
}

// The mutating store APIs (`setShortcut`, `addShortcut`, `removeShortcut`,
// `resetShortcut`) are synchronous and fire `void saveToStore()`. Flush a few
// microtask turns so the async write to `disk` lands before we assert on it.
async function flushSave() {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

beforeEach(() => {
  // Fresh disk per test; resetModules so each test starts with an uninitialized store.
  disk.clear()
  vi.resetModules()
})

describe('shortcuts-store persistence round-trips', () => {
  it('keeps a removed-only-default shortcut removed across a reload (RC2)', async () => {
    // `app.hide` defaults to ['⌘H']. Remove the only shortcut, leaving [].
    let store = await loadStore()
    await store.initializeShortcuts()

    store.removeShortcut('app.hide', 0)
    await flushSave()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual([])
    // Disk must hold the empty array, not the absence of the key.
    expect(disk.get('shortcut:app.hide')).toEqual([])

    // Reload (fresh webview re-reads disk).
    vi.resetModules()
    store = await loadStore()
    await store.initializeShortcuts()

    expect(store.getEffectiveShortcuts('app.hide')).toEqual([])
  })

  it('does not resurrect a removed shortcut on a default-[] command (RC3)', async () => {
    // `app.showAll` defaults to []. Add a custom, then remove it.
    let store = await loadStore()
    await store.initializeShortcuts()

    store.addShortcut('app.showAll', 'F7')
    await flushSave()
    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(['F7'])
    expect(disk.get('shortcut:app.showAll')).toEqual(['F7'])

    store.removeShortcut('app.showAll', 0)
    await flushSave()
    // Now matches the [] default, so the map entry is cleaned up and the stale
    // disk key must be deleted.
    expect(disk.has('shortcut:app.showAll')).toBe(false)

    vi.resetModules()
    store = await loadStore()
    await store.initializeShortcuts()

    expect(store.getEffectiveShortcuts('app.showAll')).toEqual([])
  })

  it('reset-to-default survives a reload (RC3)', async () => {
    // Customize `app.hide` away from its default, then reset it.
    let store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.hide', 0, '⌃X')
    await flushSave()
    expect(disk.get('shortcut:app.hide')).toEqual(['⌃X'])

    store.resetShortcut('app.hide')
    await flushSave()
    // After reset the stale disk key must be gone.
    expect(disk.has('shortcut:app.hide')).toBe(false)

    vi.resetModules()
    store = await loadStore()
    await store.initializeShortcuts()

    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
  })

  it('persists and reloads a normal customization (regression)', async () => {
    let store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()
    expect(disk.get('shortcut:app.showAll')).toEqual(['F9'])

    vi.resetModules()
    store = await loadStore()
    await store.initializeShortcuts()

    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(['F9'])
  })

  it('resetAllShortcuts clears every customization across a reload', async () => {
    let store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()
    store.setShortcut('app.hide', 0, '⌃X')
    await flushSave()
    expect(disk.get('shortcut:app.showAll')).toEqual(['F9'])
    expect(disk.get('shortcut:app.hide')).toEqual(['⌃X'])

    await store.resetAllShortcuts()
    expect(disk.has('shortcut:app.showAll')).toBe(false)
    expect(disk.has('shortcut:app.hide')).toBe(false)

    vi.resetModules()
    store = await loadStore()
    await store.initializeShortcuts()

    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(getDefaultShortcuts('app.showAll'))
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
  })

  it('ignores non-array (garbage) values at load', async () => {
    // Simulate a corrupted entry on disk.
    disk.set('shortcut:app.showAll', 'not-an-array')

    const store = await loadStore()
    await store.initializeShortcuts()

    // Garbage is skipped, so the command falls back to its registry default ([]).
    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(getDefaultShortcuts('app.showAll'))
    expect(store.isShortcutModified('app.showAll')).toBe(false)
  })
})
