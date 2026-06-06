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

// Fake `@tauri-apps/api/event` bus. `listen` records each (name, handler) pair
// in a shared registry so a test can drive a "remote window" by invoking the
// captured handler; `emit` records each (name, payload) so a test can assert
// what a mutation broadcast. Declared via `vi.hoisted` so the registries survive
// the `vi.resetModules()` reloads (they stand in for the cross-window OS bus,
// which a webview reload doesn't reset).
const eventBus = vi.hoisted(() => ({
  listeners: new Map<string, Array<(event: { payload: unknown }) => void>>(),
  emits: [] as Array<{ name: string; payload: unknown }>,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const arr = eventBus.listeners.get(name) ?? []
    arr.push(handler)
    eventBus.listeners.set(name, arr)
    return Promise.resolve(() => {
      const cur = eventBus.listeners.get(name) ?? []
      eventBus.listeners.set(
        name,
        cur.filter((h) => h !== handler),
      )
    })
  }),
  emit: vi.fn((name: string, payload: unknown) => {
    eventBus.emits.push({ name, payload })
    return Promise.resolve()
  }),
}))

// Deliver an event to every handler registered in THIS window for `name`. Stands
// in for the OS delivering a cross-window broadcast. The originating window also
// receives its own emits on the real bus, so tests use this to prove the
// loop-guard drops self-originated events.
function deliver(name: string, payload: unknown): void {
  for (const handler of eventBus.listeners.get(name) ?? []) {
    handler({ payload })
  }
}

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
  eventBus.listeners.clear()
  eventBus.emits.length = 0
  vi.resetModules()
})

// All `shortcuts:changed` emit payloads recorded so far.
function changedEmits(): unknown[] {
  return eventBus.emits.filter((e) => e.name === 'shortcuts:changed').map((e) => e.payload)
}

describe('shortcuts-store persistence round-trips', () => {
  it('keeps a removed-only-default shortcut removed across a reload', async () => {
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

  it('does not resurrect a removed shortcut on a default-[] command', async () => {
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

  it('reset-to-default survives a reload', async () => {
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

describe('shortcuts-store cross-window propagation', () => {
  it('setShortcut emits a shortcuts:changed event with the command id and new shortcuts', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()

    const emits = changedEmits() as Array<{ commandId?: string; shortcuts?: unknown }>
    const forCmd = emits.find((p) => p.commandId === 'app.showAll')
    expect(forCmd).toBeDefined()
    expect(forCmd?.shortcuts).toEqual(['F9'])
  })

  it('applying a received remote change updates effective shortcuts AND fires listeners, without saving or re-emitting', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    const seen: string[] = []
    store.onShortcutChange((id) => seen.push(id))

    // A second window rebound app.showAll to F9 and broadcast it. The emit
    // carries that window's senderId, so it differs from ours.
    deliver('shortcuts:changed', { senderId: 'other-window', commandId: 'app.showAll', shortcuts: ['F9'] })
    await flushSave()

    // Local effective state reflects the remote change.
    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(['F9'])
    // Local reactive consumers were notified.
    expect(seen).toContain('app.showAll')
    // The writer already persisted; we must NOT write disk again here.
    expect(disk.has('shortcut:app.showAll')).toBe(false)
    // And we must NOT re-broadcast (that would loop).
    expect(changedEmits()).toHaveLength(0)
  })

  it('propagates the "removed all shortcuts" empty-array state as [], not as a reset', async () => {
    // `app.hide` defaults to ['⌘H']. Removing its only shortcut leaves [], which
    // means "user removed all bindings, don't fall back to defaults" — distinct
    // from a reset (which would send null and revert to ['⌘H']).
    const store = await loadStore()
    await store.initializeShortcuts()

    store.removeShortcut('app.hide', 0)
    await flushSave()

    const emits = changedEmits() as Array<{ commandId?: string; shortcuts?: unknown }>
    const forCmd = emits.find((p) => p.commandId === 'app.hide')
    expect(forCmd?.shortcuts).toEqual([])

    // A receiving window applies [] (removed-all), not the ⌘H default.
    deliver('shortcuts:changed', { senderId: 'other-window', commandId: 'app.hide', shortcuts: [] })
    expect(store.getEffectiveShortcuts('app.hide')).toEqual([])
  })

  it('a received reset (null shortcuts) clears the local custom entry and notifies', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    // Local window has a customization first.
    store.setShortcut('app.hide', 0, '⌃X')
    await flushSave()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(['⌃X'])

    const seen: string[] = []
    store.onShortcutChange((id) => seen.push(id))

    // Another window reset app.hide to its default.
    deliver('shortcuts:changed', { senderId: 'other-window', commandId: 'app.hide', shortcuts: null })

    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
    expect(store.isShortcutModified('app.hide')).toBe(false)
    expect(seen).toContain('app.hide')
  })

  it('reset-all round-trips: a received reset-all clears every local customization and notifies each', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()
    store.setShortcut('app.hide', 0, '⌃X')
    await flushSave()

    const seen: string[] = []
    store.onShortcutChange((id) => seen.push(id))

    deliver('shortcuts:changed', { senderId: 'other-window', resetAll: true })

    expect(store.getEffectiveShortcuts('app.showAll')).toEqual(getDefaultShortcuts('app.showAll'))
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
    expect(seen).toContain('app.showAll')
    expect(seen).toContain('app.hide')
  })

  it('resetAllShortcuts emits a reset-all marker', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()
    eventBus.emits.length = 0 // ignore the setShortcut emit

    await store.resetAllShortcuts()

    const emits = changedEmits() as Array<{ resetAll?: boolean }>
    expect(emits.some((p) => p.resetAll === true)).toBe(true)
  })

  it('loop guard: the originating window ignores its own broadcast (no double-apply, no notify)', async () => {
    const store = await loadStore()
    await store.initializeShortcuts()

    store.setShortcut('app.showAll', 0, 'F9')
    await flushSave()

    // Grab the payload this window actually emitted, including its own senderId.
    const ownEmit = changedEmits()[0] as { senderId: string; commandId: string; shortcuts: unknown }
    expect(ownEmit.senderId).toBeTruthy()

    const seen: string[] = []
    store.onShortcutChange((id) => seen.push(id))

    // The OS echoes our own emit back to us. The loop guard must drop it.
    deliver('shortcuts:changed', ownEmit)

    expect(seen).not.toContain('app.showAll')
    expect(changedEmits()).toHaveLength(1) // still just the original, no re-emit
  })
})

describe('shortcuts-store loading heals leaked empty-string entries', () => {
  // The old "+ add" flow materialized a real `''` entry in the store the instant
  // the user clicked +, and several exit paths (click away, click +/pill on
  // another row) leaked it to disk. The add flow no longer writes `''`, but some
  // users (and the dev instance) already have leaked garbage persisted. Loading
  // heals it per this matrix, distinguishing real "removed all" `[]` from
  // empty-string junk. See CLAUDE.md § "Empty array vs missing key".

  it('keeps a genuine removed-all [] (not treated as junk)', async () => {
    // `app.hide` defaults to ['⌘H']. A stored [] means the user removed it.
    disk.set('shortcut:app.hide', [])
    const store = await loadStore()
    await store.initializeShortcuts()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual([])
    expect(store.isShortcutModified('app.hide')).toBe(true)
  })

  it("drops a [''] junk entry entirely, falling back to the default", async () => {
    disk.set('shortcut:app.hide', [''])
    const store = await loadStore()
    await store.initializeShortcuts()
    // Healed away: no custom entry, so the registry default shows (platform-converted).
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
    expect(store.isShortcutModified('app.hide')).toBe(false)
  })

  it("drops a ['', ''] (multi-leak) junk entry entirely", async () => {
    disk.set('shortcut:app.hide', ['', ''])
    const store = await loadStore()
    await store.initializeShortcuts()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(getDefaultShortcuts('app.hide'))
    expect(store.isShortcutModified('app.hide')).toBe(false)
  })

  it("drops [''] junk on a default-[] command (no resurrection, no spurious modified)", async () => {
    // `app.showAll` defaults to []. A leaked [''] must not register as modified.
    disk.set('shortcut:app.showAll', [''])
    const store = await loadStore()
    await store.initializeShortcuts()
    expect(store.getEffectiveShortcuts('app.showAll')).toEqual([])
    expect(store.isShortcutModified('app.showAll')).toBe(false)
  })

  it("filters trailing '' from a mixed entry, keeping the real shortcut", async () => {
    disk.set('shortcut:app.hide', ['⌘X', ''])
    const store = await loadStore()
    await store.initializeShortcuts()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(['⌘X'])
    expect(store.isShortcutModified('app.hide')).toBe(true)
  })

  it('leaves a normal custom entry untouched', async () => {
    disk.set('shortcut:app.hide', ['⌘X', '⌘Y'])
    const store = await loadStore()
    await store.initializeShortcuts()
    expect(store.getEffectiveShortcuts('app.hide')).toEqual(['⌘X', '⌘Y'])
  })
})
