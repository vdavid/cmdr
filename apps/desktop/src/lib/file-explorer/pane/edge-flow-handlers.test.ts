import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TabManager } from '../tabs/tab-state-manager.svelte'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'
import type { ReturnPoint } from './return-point'

const { getDefaultVolumeIdSpy, resolvePathVolumeSpy, pathExistsSpy, requestVolumeRefreshSpy, resolveValidPathSpy } =
  vi.hoisted(() => ({
    getDefaultVolumeIdSpy: vi.fn<() => Promise<string>>(),
    resolvePathVolumeSpy: vi.fn<() => Promise<{ volume: { id: string } | null }>>(),
    pathExistsSpy: vi.fn<() => Promise<boolean>>(),
    requestVolumeRefreshSpy: vi.fn(),
    resolveValidPathSpy: vi.fn<() => Promise<string | null>>(),
  }))

vi.mock('$lib/tauri-commands', () => ({
  getDefaultVolumeId: getDefaultVolumeIdSpy,
  resolvePathVolume: resolvePathVolumeSpy,
  pathExists: pathExistsSpy,
}))
vi.mock('$lib/stores/volume-store.svelte', () => ({ requestVolumeRefresh: requestVolumeRefreshSpy }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: resolveValidPathSpy }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createEdgeFlowHandlers, type EdgeFlowHandlersDeps } from './edge-flow-handlers'

function makeTabMgr(unreachable: unknown): TabManager {
  return { tabs: [{ id: 't', unreachable }], activeTabId: 't' } as unknown as TabManager
}

function setup(opts: {
  returnPoint?: ReturnPoint
  volumes?: VolumeInfo[]
  volumeIdByPane?: Record<'left' | 'right', string>
  tabMgr?: TabManager
}) {
  const navigate = vi.fn<(i: NavigateIntent) => NavigateResult>(() => ({
    status: 'started',
    settled: Promise.resolve(),
  }))
  const focusContainer = vi.fn()
  const deps: EdgeFlowHandlersDeps = {
    navigate,
    getReturnPoint: () => opts.returnPoint ?? null,
    getPaneVolumeId: (p) => opts.volumeIdByPane?.[p] ?? 'root',
    getTabMgr: () => opts.tabMgr ?? makeTabMgr(null),
    getVolumes: () => opts.volumes ?? [],
    focusContainer,
  }
  return { handlers: createEdgeFlowHandlers(deps), navigate, focusContainer }
}

describe('createEdgeFlowHandlers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getDefaultVolumeIdSpy.mockResolvedValue('root')
    pathExistsSpy.mockResolvedValue(true)
  })

  describe('handleCancelLoading', () => {
    const shownA = { volumeId: 'root', path: '/a' }

    it('with a return point, returns there, selecting the folder the load was opening when it sits right inside', () => {
      const point = {
        tabId: 't',
        shown: shownA,
        historyIndex: 1,
        entry: shownA,
        ahead: { volumeId: 'root', path: '/a/b' },
      }
      const { handlers, navigate, focusContainer } = setup({ returnPoint: point })

      handlers.handleCancelLoading('left', {
        cancelled: { volumeId: 'root', path: '/a/b' },
        lastShown: { volumeId: 'root', path: '/elsewhere' },
      })

      expect(navigate).toHaveBeenCalledWith({
        pane: 'left',
        to: { returnTo: point },
        source: 'cancel',
        selectName: 'b',
      })
      expect(focusContainer).toHaveBeenCalled()
    })

    it('without one, re-lists the location the pane last showed, with the child under the cursor', () => {
      const { handlers, navigate } = setup({})

      handlers.handleCancelLoading('right', { cancelled: { volumeId: 'root', path: '/a/b' }, lastShown: shownA })

      expect(navigate).toHaveBeenCalledWith({ pane: 'right', to: { goTo: shownA }, source: 'cancel', selectName: 'b' })
    })

    it('selects nothing when the cancelled folder is not directly inside the shown one', () => {
      const { handlers, navigate } = setup({})

      handlers.handleCancelLoading('left', { cancelled: { volumeId: 'root', path: '/c/d' }, lastShown: shownA })

      expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { goTo: shownA }, source: 'cancel' })
    })

    it('a cancelled re-list of the shown folder keeps the entry that load was bringing under the cursor', () => {
      const { handlers, navigate } = setup({})

      handlers.handleCancelLoading('left', {
        cancelled: { volumeId: 'root', path: '/a', selectName: 'child' },
        lastShown: shownA,
      })

      expect(navigate).toHaveBeenCalledWith({
        pane: 'left',
        to: { goTo: shownA },
        source: 'cancel',
        selectName: 'child',
      })
    })

    it('stops a load the pane already switched away from, and does nothing more', () => {
      const { handlers, navigate, focusContainer } = setup({ volumeIdByPane: { left: 'network', right: 'root' } })

      handlers.handleCancelLoading('left', { cancelled: { volumeId: 'root', path: '/a/b' }, lastShown: shownA })

      expect(navigate).not.toHaveBeenCalled()
      expect(focusContainer).toHaveBeenCalled()
    })

    it('before anything was shown, walks up to the nearest valid parent with no history push', async () => {
      resolveValidPathSpy.mockResolvedValue('/a')
      const { handlers, navigate } = setup({})

      handlers.handleCancelLoading('left', { cancelled: { volumeId: 'root', path: '/a/b' }, lastShown: null })
      await vi.waitFor(() => {
        expect(navigate).toHaveBeenCalled()
      })

      expect(navigate).toHaveBeenCalledWith({
        pane: 'left',
        to: { selectVolume: { volumeId: 'root', path: '/a' } },
        source: 'cancel',
        pushHistory: false,
      })
    })

    it('walks up on the volume the load ran on, asking that volume', async () => {
      // Pre-fix the walk asked the boot disk about a phone's folders, heard "gone"
      // at every level, and landed the pane on the phone's root.
      resolveValidPathSpy.mockResolvedValue('adb://R58M/sdcard')
      const { handlers, navigate } = setup({
        volumes: [{ id: 'adb-phone', path: 'adb://R58M' } as VolumeInfo],
        volumeIdByPane: { left: 'adb-phone', right: 'root' },
      })

      handlers.handleCancelLoading('left', {
        cancelled: { volumeId: 'adb-phone', path: 'adb://R58M/sdcard/a' },
        lastShown: null,
      })
      await vi.waitFor(() => {
        expect(navigate).toHaveBeenCalled()
      })

      expect(resolveValidPathSpy).toHaveBeenCalledWith('adb://R58M/sdcard', {
        volumeRoot: 'adb://R58M',
        volumeId: 'adb-phone',
      })
    })
  })

  it('handleMtpFatalError falls back to the default volume at its path (history push)', async () => {
    getDefaultVolumeIdSpy.mockResolvedValue('root')
    const { handlers, navigate } = setup({
      volumes: [{ id: 'root', name: 'Root', path: '/' } as unknown as VolumeInfo],
    })

    await handlers.handleMtpFatalError('left', 'device gone')

    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'root', path: '/' } },
      source: 'fallback',
    })
  })

  it('handleRetryUnreachable clears unreachable, navigates, and refreshes volumes', async () => {
    resolvePathVolumeSpy.mockResolvedValue({ volume: { id: 'usb' } })
    const tabMgr = makeTabMgr({ originalPath: '/Volumes/USB/x' })
    const { handlers, navigate } = setup({ tabMgr })

    await handlers.handleRetryUnreachable('left')

    const tab = tabMgr.tabs[0] as unknown as { unreachable: unknown }
    expect(tab.unreachable).toBeNull()
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'usb', path: '/Volumes/USB/x' } },
      source: 'fallback',
    })
    expect(requestVolumeRefreshSpy).toHaveBeenCalled()
  })

  it('handleRetryUnreachable is a no-op when the tab is not unreachable', async () => {
    const { handlers, navigate } = setup({ tabMgr: makeTabMgr(null) })
    await handlers.handleRetryUnreachable('left')
    expect(navigate).not.toHaveBeenCalled()
  })

  it('handleOpenHome clears unreachable and navigates home on the default volume', async () => {
    getDefaultVolumeIdSpy.mockResolvedValue('root')
    const tabMgr = makeTabMgr({ originalPath: '/gone' })
    const { handlers, navigate } = setup({ tabMgr })

    await handlers.handleOpenHome('right')

    const tab = tabMgr.tabs[0] as unknown as { unreachable: unknown }
    expect(tab.unreachable).toBeNull()
    expect(navigate).toHaveBeenCalledWith({
      pane: 'right',
      to: { selectVolume: { volumeId: 'root', path: '~' } },
      source: 'fallback',
    })
  })

  it('handleVolumeUnmount redirects only affected panes, with no history push, falling back to / when home is gone', async () => {
    getDefaultVolumeIdSpy.mockResolvedValue('root')
    pathExistsSpy.mockResolvedValue(false)
    const { handlers, navigate } = setup({ volumeIdByPane: { left: 'usb', right: 'root' } })

    await handlers.handleVolumeUnmount('usb')

    expect(navigate).toHaveBeenCalledTimes(1)
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'root', path: '/' } },
      source: 'fallback',
      pushHistory: false,
    })
  })
})
