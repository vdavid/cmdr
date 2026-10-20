/**
 * The shortcuts store in a restricted window (the viewer, #441): no `tauri-plugin-store`
 * access, so it seeds from the typed `get_restricted_window_shortcuts` snapshot and follows
 * live rebinds through `shortcuts:changed`, the way a store-backed window does.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { loadSpy, snapshot, listeners, warnSpy } = vi.hoisted(() => ({
  loadSpy: vi.fn(),
  snapshot: { value: {} },
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  warnSpy: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-store', () => ({ load: loadSpy }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return Promise.resolve(() => listeners.delete(name))
  }),
  emit: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    updateMenuAccelerator: () => Promise.resolve({ status: 'ok' as const, data: null }),
    getRestrictedWindowShortcuts: () =>
      snapshot.value instanceof Error ? Promise.reject(snapshot.value) : Promise.resolve(snapshot.value),
  },
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: warnSpy, info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))
// Stateless and heavy (they pull in the i18n runtime and the IPC layer), so passed
// through to load once per file: see `docs/testing.md` § "Reloading a module per test".
vi.mock('#lib/commands/command-registry.js', async (importOriginal) => await importOriginal())
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => await importOriginal())

// Static, so the module graph is transformed once up front instead of inside the first
// test's timeout; each test still gets a fresh store through `vi.resetModules()`.
import { getDefaultShortcuts } from './shortcuts-store'

async function initRestricted() {
  const store = await import('./shortcuts-store')
  await store.initializeShortcuts({ restrictedWindow: true })
  return store
}

beforeEach(() => {
  snapshot.value = {}
  listeners.clear()
  vi.clearAllMocks()
  vi.resetModules()
})

describe('shortcuts-store in a restricted window', () => {
  it('seeds from the typed snapshot and never touches the store plugin', async () => {
    snapshot.value = { 'viewer.toggleWordWrap': ['⌥W'], 'viewer.toggleTail': [] }

    const store = await initRestricted()

    expect(store.getEffectiveShortcuts('viewer.toggleWordWrap')).toEqual(['⌥W'])
    // `[]` is "turned off", not "use the default".
    expect(store.getEffectiveShortcuts('viewer.toggleTail')).toEqual([])
    expect(store.getEffectiveShortcuts('viewer.viewAsText')).toEqual(getDefaultShortcuts('viewer.viewAsText'))
    expect(loadSpy).not.toHaveBeenCalled()
  })

  it('heals the snapshot the way a store load does', async () => {
    snapshot.value = {
      'viewer.toggleWordWrap': [''],
      'file.copy': ['⌘⌫', ''],
      'nav.up': ['K'],
    }

    const store = await initRestricted()

    expect(store.getEffectiveShortcuts('viewer.toggleWordWrap')).toEqual(['W'])
    expect(store.getEffectiveShortcuts('file.copy')).toEqual(['⌘Backspace'])
    // A fixed-key command's persisted entry is a no-op illusion everywhere.
    expect(store.isShortcutModified('nav.up')).toBe(false)
  })

  it('follows a rebind made in Settings while it is open', async () => {
    const store = await initRestricted()

    listeners.get('shortcuts:changed')?.({
      payload: { senderId: 'settings-window', commandId: 'viewer.toggleWordWrap', shortcuts: ['⌥W'] },
    })

    expect(store.getEffectiveShortcuts('viewer.toggleWordWrap')).toEqual(['⌥W'])
  })

  it('falls back to the defaults, with a warning, when the snapshot read fails', async () => {
    snapshot.value = new Error('IPC down')

    const store = await initRestricted()

    expect(store.getEffectiveShortcuts('viewer.toggleWordWrap')).toEqual(['W'])
    expect(warnSpy).toHaveBeenCalled()
  })
})
