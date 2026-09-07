/**
 * Tests for `smb-view-state.svelte.ts`, the file pane's SMB reconnect view state.
 * They pin:
 * - the reconnect cancel + disconnect handlers (manager cancel, OS unmount, and the
 *   walk-up-to-valid-path fallback),
 * - the one `RemoteConnectState` the manager's cycle status maps onto (and the
 *   gave-up status that deliberately maps to the banner instead),
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
import type { SignInShape } from '$lib/ipc/bindings'

const { ipc, manager, resolveValidPathSpy, addToastSpy } = vi.hoisted(() => ({
  ipc: {
    disconnectSmbVolume: vi.fn().mockResolvedValue(undefined),
  },
  manager: {
    getState: vi.fn(),
    getSignInShape: vi.fn<(volumeId: string) => SignInShape | null>(() => null),
    subscribe: vi.fn((_id: string, _cb: () => void) => vi.fn()),
    cancel: vi.fn(),
    startCycle: vi.fn(),
    retryNow: vi.fn(),
  },
  resolveValidPathSpy: vi.fn().mockResolvedValue('/valid'),
  addToastSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  disconnectSmbVolume: ipc.disconnectSmbVolume,
  disconnectPlace: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: resolveValidPathSpy }))
vi.mock('$lib/servers/open-sign-in', () => ({ openSignInForPlace: vi.fn().mockResolvedValue({ signedIn: false }) }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('../network/smb-reconnect-manager.svelte', () => ({
  smbReconnectManager: manager,
  reconnectCycleLines: () => ['keeps trying'],
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createSmbViewState, type SmbViewStateDeps } from './smb-view-state.svelte'

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
    manager.getSignInShape.mockReturnValue(null)
    manager.subscribe.mockReturnValue(vi.fn())
    resolveValidPathSpy.mockResolvedValue('/valid')
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('handleSmbReconnectCancel cancels the cycle and walks up to a valid path', async () => {
    const { sub, navigateToFallback } = create()
    sub.handleSmbReconnectCancel()
    expect(manager.cancel).toHaveBeenCalledWith('smb-vol')
    expect(resolveValidPathSpy).toHaveBeenCalledWith('/smb-vol/dir', { volumeRoot: '/smb-vol' })
    await vi.waitFor(() => {
      expect(navigateToFallback).toHaveBeenCalledWith('/valid')
    })
  })

  it('handleSmbReconnectDisconnect cancels, OS-unmounts, and navigates away', async () => {
    const { sub, navigateToFallback } = create()
    sub.handleSmbReconnectDisconnect()
    expect(manager.cancel).toHaveBeenCalledWith('smb-vol')
    expect(ipc.disconnectSmbVolume).toHaveBeenCalledWith('smb-vol')
    await vi.waitFor(() => {
      expect(navigateToFallback).toHaveBeenCalledWith('/valid')
    })
  })

  it('renders a waiting cycle as a countdown the user can skip or stop', () => {
    manager.getState.mockReturnValue({ status: 'waiting', attemptIndex: 2, currentDelayMs: 8000, waitStartedAt: 1234 })
    const state = create().sub.remoteConnectState
    expect(state?.kind).toBe('connecting')
    if (state?.kind !== 'connecting') throw new Error('unreachable')
    // ❗ The countdown is what makes the wait honest: without it a person can't
    // tell a slow handshake from a wedged one.
    expect(state.cycle?.waiting).toEqual({ startedAt: 1234, durationMs: 8000 })
    expect(state.cycle?.lines.length).toBeGreaterThan(0)

    state.cycle?.retryNow()
    expect(manager.retryNow).toHaveBeenCalledWith('smb-vol')
  })

  it('drops the countdown while an attempt is in flight, so nothing pretends to be waiting', () => {
    manager.getState.mockReturnValue({
      status: 'attempting',
      attemptIndex: 1,
      currentDelayMs: 4000,
      waitStartedAt: 1234,
    })
    const state = create().sub.remoteConnectState
    expect(state?.kind).toBe('connecting')
    if (state?.kind !== 'connecting') throw new Error('unreachable')
    expect(state.cycle?.waiting).toBeNull()
  })

  it('sends a gave-up cycle to the banner rather than a pane state of its own', () => {
    manager.getState.mockReturnValue({ status: 'gave-up', attemptIndex: 4, currentDelayMs: 0, waitStartedAt: 0 })
    const { sub } = create()
    expect(sub.showGaveUp).toBe(true)
    // ❗ Two renderers for one state is what this avoids.
    expect(sub.remoteConnectState).toBeNull()
  })

  it('offers Sign in when a credential is what is missing', () => {
    manager.getState.mockReturnValue({ status: 'needs-auth', attemptIndex: 0, currentDelayMs: 0, waitStartedAt: 0 })
    manager.getSignInShape.mockReturnValue({ kind: 'username_password', guestAllowed: false })
    const state = create().sub.remoteConnectState
    expect(state?.kind).toBe('signed_out')
    if (state?.kind !== 'signed_out') throw new Error('unreachable')
    expect(state.signIn).toBeTypeOf('function')
  })

  it('offers no Sign in button when the backend says there is nothing to ask for', () => {
    // ❌ No inert affordance: a `nothing` shape means no secret a person could
    // type would bring the session back.
    manager.getState.mockReturnValue({ status: 'needs-auth', attemptIndex: 0, currentDelayMs: 0, waitStartedAt: 0 })
    manager.getSignInShape.mockReturnValue({ kind: 'nothing' })
    const state = create().sub.remoteConnectState
    expect(state).toEqual({ kind: 'signed_out', signIn: null })
  })

  it('shows the changed-key banner, whose only way out is Disconnect', () => {
    manager.getState.mockReturnValue({
      status: 'needs-host-key',
      attemptIndex: 0,
      currentDelayMs: 0,
      waitStartedAt: 0,
    })
    const state = create().sub.remoteConnectState
    expect(state?.kind).toBe('host_key_changed')
    if (state?.kind !== 'host_key_changed') throw new Error('unreachable')
    expect(state.disconnect).toBeTypeOf('function')
  })

  it('renders nothing when no cycle is running', () => {
    manager.getState.mockReturnValue(null)
    const { sub } = create()
    expect(sub.remoteConnectState).toBeNull()
    expect(sub.showGaveUp).toBe(false)
  })

  it('subscribes to the manager and kick-starts a cycle on a landed-broken SMB share', () => {
    create({ volumeInfo: { connectionState: 'disconnected' } as unknown as VolumeInfo })
    expect(manager.subscribe).toHaveBeenCalledWith('smb-vol', expect.any(Function))
    expect(manager.startCycle).toHaveBeenCalledWith('smb-vol')
  })

  it('does not subscribe off a volume with a session', () => {
    create({ volumeInfo: { connectionState: null } as unknown as VolumeInfo })
    expect(manager.subscribe).not.toHaveBeenCalled()
  })

  it('does not subscribe a saved server that was never connected', () => {
    // The greyed row: nothing is in flight, so there is no cycle to join, and
    // enrolling it would start a backoff loop against a server nobody dialed.
    create({ volumeInfo: { connectionState: 'saved' } as unknown as VolumeInfo })
    expect(manager.subscribe).not.toHaveBeenCalled()
    expect(manager.startCycle).not.toHaveBeenCalled()
  })

  it('reloads the current directory when the reconnect success callback fires', () => {
    let capturedOnSuccess: (() => void) | undefined
    manager.subscribe.mockImplementation((_id: string, cb: () => void) => {
      capturedOnSuccess = cb
      return vi.fn()
    })
    const { loadDirectory } = create({
      volumeInfo: { connectionState: 'direct' } as unknown as VolumeInfo,
    })
    capturedOnSuccess?.()
    expect(loadDirectory).toHaveBeenCalledWith('/smb-vol/dir')
  })
})
