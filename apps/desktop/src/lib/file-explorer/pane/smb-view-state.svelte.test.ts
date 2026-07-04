/**
 * Tests for `smb-view-state.svelte.ts`, the file pane's SMB reconnect + direct-
 * upgrade view state. They pin:
 * - the "Connect directly" upgrade flow's four outcomes (success / credentialsNeeded
 *   / other-failure / thrown) and the no-op when no form is open,
 * - the reconnect cancel + disconnect handlers (manager cancel, OS unmount, and the
 *   walk-up-to-valid-path fallback),
 * - the alt-view decision deriveds mapping the manager's cycle status,
 * - the subscribe `$effect` registering with the manager and kick-starting a cycle
 *   on a landed-broken share, and staying out of the way off an SMB volume.
 *
 * Uses Svelte runes (`$effect.root` + `$state`), so the filename carries the
 * `.svelte.` infix: the factory creates its subscription `$effect` in a reactive
 * root. Async handlers are awaited (or observed via `vi.waitFor`).
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { flushSync } from 'svelte'
import type { VolumeInfo } from '../types'
import type { UpgradeResult } from '$lib/tauri-commands'

const { ipc, manager, resolveValidPathSpy, requestVolumeRefreshSpy, addToastSpy } = vi.hoisted(() => ({
  ipc: {
    disconnectSmbVolume: vi.fn().mockResolvedValue(undefined),
    upgradeToSmbVolumeWithCredentials: vi.fn(),
    getIpcErrorMessage: vi.fn((e: unknown) => String(e)),
  },
  manager: {
    getState: vi.fn(),
    subscribe: vi.fn((_id: string, _cb: () => void) => vi.fn()),
    cancel: vi.fn(),
    startCycle: vi.fn(),
    retryNow: vi.fn(),
  },
  resolveValidPathSpy: vi.fn().mockResolvedValue('/valid'),
  requestVolumeRefreshSpy: vi.fn(),
  addToastSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  disconnectSmbVolume: ipc.disconnectSmbVolume,
  upgradeToSmbVolumeWithCredentials: ipc.upgradeToSmbVolumeWithCredentials,
}))
vi.mock('$lib/tauri-commands/ipc-types', () => ({ getIpcErrorMessage: ipc.getIpcErrorMessage }))
vi.mock('../network/smb-reconnect-manager.svelte', () => ({ smbReconnectManager: manager }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: resolveValidPathSpy }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ requestVolumeRefresh: requestVolumeRefreshSpy }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createSmbViewState, type SmbViewStateDeps } from './smb-view-state.svelte'

const credentialsNeeded: UpgradeResult & { status: 'credentialsNeeded' } = {
  status: 'credentialsNeeded',
  server: 'nas.local',
  share: 'photos',
  port: 445,
  displayName: 'NAS',
  usernameHint: 'admin',
  message: null,
} as unknown as UpgradeResult & { status: 'credentialsNeeded' }

describe('createSmbViewState', () => {
  let dispose: (() => void) | undefined

  function create(opts: { volumeInfo?: VolumeInfo | null; loadDirectory?: Mock; navigateToFallback?: Mock } = {}) {
    const loadDirectory = opts.loadDirectory ?? vi.fn()
    const navigateToFallback = opts.navigateToFallback ?? vi.fn()
    const deps: SmbViewStateDeps = {
      getVolumeId: () => 'smb-vol',
      getCurrentPath: () => '/smb-vol/dir',
      getVolumePath: () => '/smb-vol',
      getCurrentVolumeInfo: () => opts.volumeInfo ?? null,
      loadDirectory,
      navigateToFallback,
    }
    let sub!: ReturnType<typeof createSmbViewState>
    dispose = $effect.root(() => {
      sub = createSmbViewState(deps)
    })
    flushSync()
    return { sub, loadDirectory, navigateToFallback }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    manager.getState.mockReturnValue(null)
    manager.subscribe.mockReturnValue(vi.fn())
    resolveValidPathSpy.mockResolvedValue('/valid')
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('handleSmbUpgradeLogin populates the login form from the credentials-needed result', () => {
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    expect(sub.smbUpgradeLogin).toEqual({
      volumeId: 'smb-vol',
      server: 'nas.local',
      share: 'photos',
      port: 445,
      displayName: 'NAS',
      usernameHint: 'admin',
      errorMessage: undefined,
      isConnecting: false,
    })
  })

  it('upgrade connect success clears the form, refreshes volumes, and toasts success', async () => {
    ipc.upgradeToSmbVolumeWithCredentials.mockResolvedValue({ status: 'success' })
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    await sub.handleSmbUpgradeConnect('admin', 'pw', true)
    expect(sub.smbUpgradeLogin).toBeNull()
    expect(requestVolumeRefreshSpy).toHaveBeenCalled()
    expect(addToastSpy).toHaveBeenCalledWith('fileExplorer.pane.connectedDirectlyToast', { level: 'success' })
  })

  it('upgrade connect credentialsNeeded keeps the form and surfaces the auth error', async () => {
    ipc.upgradeToSmbVolumeWithCredentials.mockResolvedValue({ status: 'credentialsNeeded', message: 'Wrong password' })
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    await sub.handleSmbUpgradeConnect('admin', 'bad', false)
    expect(sub.smbUpgradeLogin).not.toBeNull()
    expect(sub.smbUpgradeLogin?.errorMessage).toBe('Wrong password')
    expect(sub.smbUpgradeLogin?.isConnecting).toBe(false)
    expect(requestVolumeRefreshSpy).not.toHaveBeenCalled()
  })

  it('upgrade connect other-failure clears the form and toasts the error', async () => {
    ipc.upgradeToSmbVolumeWithCredentials.mockResolvedValue({ status: 'failed', message: 'server gone' })
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    await sub.handleSmbUpgradeConnect('admin', 'pw', false)
    expect(sub.smbUpgradeLogin).toBeNull()
    expect(addToastSpy).toHaveBeenCalledWith('fileExplorer.pane.directConnectionFailedToast', { level: 'error' })
  })

  it('upgrade connect clears the form and toasts when the IPC throws', async () => {
    ipc.upgradeToSmbVolumeWithCredentials.mockRejectedValue(new Error('boom'))
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    await sub.handleSmbUpgradeConnect('admin', 'pw', false)
    expect(sub.smbUpgradeLogin).toBeNull()
    expect(addToastSpy).toHaveBeenCalledWith('fileExplorer.pane.directConnectionFailedToast', { level: 'error' })
  })

  it('upgrade connect is a no-op when no form is open', async () => {
    const { sub } = create()
    await sub.handleSmbUpgradeConnect('admin', 'pw', false)
    expect(ipc.upgradeToSmbVolumeWithCredentials).not.toHaveBeenCalled()
  })

  it('handleSmbUpgradeCancel clears the form', () => {
    const { sub } = create()
    sub.handleSmbUpgradeLogin(credentialsNeeded, 'smb-vol')
    sub.handleSmbUpgradeCancel()
    expect(sub.smbUpgradeLogin).toBeNull()
  })

  it('handleSmbReconnectCancel cancels the cycle and walks up to a valid path', async () => {
    const { sub, navigateToFallback } = create()
    sub.handleSmbReconnectCancel()
    expect(manager.cancel).toHaveBeenCalledWith('smb-vol')
    expect(resolveValidPathSpy).toHaveBeenCalledWith('/smb-vol/dir', { volumeRoot: '/smb-vol' })
    await vi.waitFor(() => { expect(navigateToFallback).toHaveBeenCalledWith('/valid'); })
  })

  it('handleSmbReconnectDisconnect cancels, OS-unmounts, and navigates away', async () => {
    const { sub, navigateToFallback } = create()
    sub.handleSmbReconnectDisconnect()
    expect(manager.cancel).toHaveBeenCalledWith('smb-vol')
    expect(ipc.disconnectSmbVolume).toHaveBeenCalledWith('smb-vol')
    await vi.waitFor(() => { expect(navigateToFallback).toHaveBeenCalledWith('/valid'); })
  })

  it('maps the manager cycle status onto the view deriveds', () => {
    manager.getState.mockReturnValue({ status: 'waiting' })
    expect(create().sub.showSmbReconnecting).toBe(true)
    dispose?.()

    manager.getState.mockReturnValue({ status: 'attempting' })
    expect(create().sub.showSmbReconnecting).toBe(true)
    dispose?.()

    manager.getState.mockReturnValue({ status: 'gave-up' })
    expect(create().sub.showSmbGaveUp).toBe(true)
    dispose?.()

    manager.getState.mockReturnValue({ status: 'needs-auth' })
    expect(create().sub.showSmbNeedsAuth).toBe(true)
    dispose?.()

    manager.getState.mockReturnValue(null)
    const { sub } = create()
    expect(sub.showSmbReconnecting).toBe(false)
    expect(sub.showSmbGaveUp).toBe(false)
    expect(sub.showSmbNeedsAuth).toBe(false)
  })

  it('subscribes to the manager and kick-starts a cycle on a landed-broken SMB share', () => {
    create({ volumeInfo: { smbConnectionState: 'disconnected' } as unknown as VolumeInfo })
    expect(manager.subscribe).toHaveBeenCalledWith('smb-vol', expect.any(Function))
    expect(manager.startCycle).toHaveBeenCalledWith('smb-vol')
  })

  it('does not subscribe off an SMB volume', () => {
    create({ volumeInfo: { smbConnectionState: null } as unknown as VolumeInfo })
    expect(manager.subscribe).not.toHaveBeenCalled()
  })

  it('reloads the current directory when the reconnect success callback fires', () => {
    let capturedOnSuccess: (() => void) | undefined
    manager.subscribe.mockImplementation((_id: string, cb: () => void) => {
      capturedOnSuccess = cb
      return vi.fn()
    })
    const { loadDirectory } = create({
      volumeInfo: { smbConnectionState: 'connected' } as unknown as VolumeInfo,
    })
    capturedOnSuccess?.()
    expect(loadDirectory).toHaveBeenCalledWith('/smb-vol/dir')
  })
})
