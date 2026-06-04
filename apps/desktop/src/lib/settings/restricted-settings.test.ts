/**
 * Tests for the restricted-window settings path: windows without
 * `tauri-plugin-store` capability (the viewer) initialize from the
 * `get_restricted_window_settings` snapshot and persist through the
 * `persist_restricted_window_setting` command instead of the store plugin.
 *
 * mockIPC can't simulate Tauri's permission gate, so what these tests pin is
 * the *avoidance* contract: in restricted mode the store plugin must never be
 * invoked at all (in production those calls are ACL-denied and used to fire an
 * error-level log — and therefore an auto error report — on every viewer open).
 * The full capability round trip is covered by the
 * `viewer-wordwrap-persistence` Playwright spec.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearIpcMocks, installIpcMock, type IpcRecorder } from '$lib/ipc/test-helpers'
import type { RestrictedWindowSettings } from '$lib/ipc/bindings'

let ipc: IpcRecorder

const snapshot: RestrictedWindowSettings = {
  viewerWordWrap: true,
  fileViewerSuppressBinaryWarning: null,
  appearanceTextSize: 125,
  appearanceAppColor: 'cmdr-gold',
}

beforeEach(() => {
  // settings-store keeps module-level state (initialized flag, cache, mode);
  // a fresh module per test keeps them independent.
  vi.resetModules()
  ipc = installIpcMock()
  ipc.mock('plugin:event|listen', () => 1)
  ipc.mock('plugin:event|unlisten', () => null)
  ipc.mock('plugin:event|emit', () => null)
})

afterEach(() => {
  clearIpcMocks()
})

async function importStore() {
  return await import('./settings-store')
}

describe('initializeSettings({ restrictedWindow: true })', () => {
  it('seeds the cache from the snapshot and never touches the store plugin', async () => {
    ipc.mock('get_restricted_window_settings', () => snapshot)
    const store = await importStore()

    await store.initializeSettings({ restrictedWindow: true })

    expect(store.getSetting('viewer.wordWrap')).toBe(true)
    expect(store.getSetting('appearance.textSize')).toBe(125)
    expect(store.getSetting('appearance.appColor')).toBe('cmdr-gold')
    // null snapshot fields fall back to the registry default
    expect(store.getSetting('fileViewer.suppressBinaryWarning')).toBe(false)
    // The whole point: no store-plugin call ever leaves this window.
    // (The event API doesn't route through `invoke` under mockIPC, so the
    // cross-window listener registration isn't observable here; the Playwright
    // spec covers live cross-window updates.)
    expect(ipc.calls.some((c) => c.command.startsWith('plugin:store|'))).toBe(false)
  })

  it('does not throw when the snapshot command fails; defaults apply', async () => {
    ipc.mock('get_restricted_window_settings', () => {
      throw new Error('IPC unavailable')
    })
    const store = await importStore()

    await expect(store.initializeSettings({ restrictedWindow: true })).resolves.toBeUndefined()
    expect(store.getSetting('viewer.wordWrap')).toBe(false)
  })

  it('drops snapshot values that fail registry validation', async () => {
    ipc.mock('get_restricted_window_settings', () => ({
      ...snapshot,
      appearanceAppColor: 'not-a-real-color',
    }))
    const store = await importStore()

    await store.initializeSettings({ restrictedWindow: true })

    expect(store.getSetting('appearance.appColor')).toBe('system')
    expect(store.getSetting('viewer.wordWrap')).toBe(true)
  })
})

describe('setSetting in restricted mode', () => {
  it('persists allowlisted settings via the typed command, not the store', async () => {
    ipc.mock('get_restricted_window_settings', () => snapshot)
    ipc.mock('persist_restricted_window_setting', () => null)
    const store = await importStore()
    await store.initializeSettings({ restrictedWindow: true })

    store.setSetting('viewer.wordWrap', false)
    // The persist command is fire-and-forget; flush the microtask queue.
    await Promise.resolve()

    const call = ipc.lastCall('persist_restricted_window_setting')
    expect(call?.payload).toMatchObject({ setting: 'viewerWordWrap', value: false })
    expect(ipc.calls.some((c) => c.command.startsWith('plugin:store|'))).toBe(false)
  })

  it('treats non-allowlisted settings as session-only (no persist call)', async () => {
    ipc.mock('get_restricted_window_settings', () => snapshot)
    const store = await importStore()
    await store.initializeSettings({ restrictedWindow: true })

    store.setSetting('appearance.textSize', 110)
    await Promise.resolve()

    expect(store.getSetting('appearance.textSize')).toBe(110)
    expect(ipc.callCount('persist_restricted_window_setting')).toBe(0)
    expect(ipc.calls.some((c) => c.command.startsWith('plugin:store|'))).toBe(false)
  })
})

describe('restricted-settings bridge (main-window side)', () => {
  it('persists allowlisted forwarded settings', async () => {
    vi.doMock('./settings-store', () => ({
      persistSettingFromRestrictedWindow: vi.fn(),
    }))
    const storeMock = await import('./settings-store')
    const bridge = await import('./restricted-settings-bridge')

    bridge.handlePersistRestrictedSetting({ id: 'viewer.wordWrap', value: true })

    expect(storeMock.persistSettingFromRestrictedWindow).toHaveBeenCalledWith('viewer.wordWrap', true)
    vi.doUnmock('./settings-store')
  })

  it('refuses settings outside the allowlist (any webview can emit events)', async () => {
    vi.doMock('./settings-store', () => ({
      persistSettingFromRestrictedWindow: vi.fn(),
    }))
    const storeMock = await import('./settings-store')
    const bridge = await import('./restricted-settings-bridge')

    bridge.handlePersistRestrictedSetting({ id: 'updates.errorReports', value: true })
    bridge.handlePersistRestrictedSetting({ id: 'developer.mcpEnabled', value: true })

    expect(storeMock.persistSettingFromRestrictedWindow).not.toHaveBeenCalled()
    vi.doUnmock('./settings-store')
  })

  it('refuses allowlisted ids with invalid values', async () => {
    vi.doMock('./settings-store', () => ({
      persistSettingFromRestrictedWindow: vi.fn(),
    }))
    const storeMock = await import('./settings-store')
    const bridge = await import('./restricted-settings-bridge')

    bridge.handlePersistRestrictedSetting({ id: 'viewer.wordWrap', value: 'yes-please' })

    expect(storeMock.persistSettingFromRestrictedWindow).not.toHaveBeenCalled()
    vi.doUnmock('./settings-store')
  })

  it('setup is idempotent and cleanup releases the listener', async () => {
    const unlistenSpy = vi.fn()
    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn(() => Promise.resolve(unlistenSpy)),
    }))
    vi.doMock('./settings-store', () => ({
      persistSettingFromRestrictedWindow: vi.fn(),
    }))
    const { listen } = await import('@tauri-apps/api/event')
    const bridge = await import('./restricted-settings-bridge')

    await bridge.setupRestrictedSettingsBridge()
    await bridge.setupRestrictedSettingsBridge() // second call must not re-register
    expect(listen).toHaveBeenCalledTimes(1)
    expect(vi.mocked(listen).mock.calls[0][0]).toBe('persist-restricted-setting')

    bridge.cleanupRestrictedSettingsBridge()
    expect(unlistenSpy).toHaveBeenCalledTimes(1)

    // After cleanup, setup registers again.
    await bridge.setupRestrictedSettingsBridge()
    expect(listen).toHaveBeenCalledTimes(2)

    vi.doUnmock('@tauri-apps/api/event')
    vi.doUnmock('./settings-store')
  })

  it('routes a forwarded event payload through the handler', async () => {
    type Handler = (event: { payload: unknown }) => void
    let captured: Handler | undefined
    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn((_event: string, handler: Handler) => {
        captured = handler
        return Promise.resolve(vi.fn())
      }),
    }))
    vi.doMock('./settings-store', () => ({
      persistSettingFromRestrictedWindow: vi.fn(),
    }))
    const storeMock = await import('./settings-store')
    const bridge = await import('./restricted-settings-bridge')

    await bridge.setupRestrictedSettingsBridge()
    if (!captured) throw new Error('listener was not registered')
    captured({ payload: { id: 'fileViewer.suppressBinaryWarning', value: true } })

    expect(storeMock.persistSettingFromRestrictedWindow).toHaveBeenCalledWith('fileViewer.suppressBinaryWarning', true)

    bridge.cleanupRestrictedSettingsBridge()
    vi.doUnmock('@tauri-apps/api/event')
    vi.doUnmock('./settings-store')
  })
})
