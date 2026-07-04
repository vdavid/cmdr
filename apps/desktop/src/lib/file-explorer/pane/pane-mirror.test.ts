import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { NavigateIntent, NavigateResult } from './navigate'
import type { FilePaneAPI } from './types'

const { getCurrentEntrySpy, pushHistoryEntrySpy } = vi.hoisted(() => ({
  getCurrentEntrySpy: vi.fn(),
  pushHistoryEntrySpy: vi.fn(() => ({ pushed: true }) as unknown as NavigationHistory),
}))

vi.mock('../navigation/navigation-history', () => ({ getCurrentEntry: getCurrentEntrySpy }))
vi.mock('../tabs/tab-state-manager.svelte', () => ({ pushHistoryEntry: pushHistoryEntrySpy }))

import { createPaneMirror, type PaneMirrorDeps } from './pane-mirror'

function makePaneRef(overrides: Record<string, unknown> = {}) {
  return {
    getCursorEntry: vi.fn(() => null),
    getNetworkCursorEntry: vi.fn(() => null),
    setNetworkHost: vi.fn(),
    setNetworkAutoMount: vi.fn(),
    ...overrides,
  }
}

function setup(opts: {
  refs?: Record<'left' | 'right', ReturnType<typeof makePaneRef> | undefined>
  volumeIdByPane?: Record<'left' | 'right', string>
  pathByPane?: Record<'left' | 'right', string>
  focused?: 'left' | 'right'
}) {
  const navigate = vi.fn<(i: NavigateIntent) => NavigateResult>(
    () => ({ status: 'started' }) as unknown as NavigateResult,
  )
  const setFocusedPane = vi.fn()
  const setPaneHistory = vi.fn()
  let focused = opts.focused ?? 'left'
  const deps: PaneMirrorDeps = {
    navigate,
    getPaneRef: (p) => opts.refs?.[p] as unknown as FilePaneAPI | undefined,
    getPaneVolumeId: (p) => opts.volumeIdByPane?.[p] ?? 'root',
    getPanePath: (p) => opts.pathByPane?.[p] ?? '/',
    getPaneHistory: () => ({}) as NavigationHistory,
    setPaneHistory,
    getFocusedPane: () => focused,
    setFocusedPane: (p) => {
      focused = p
      setFocusedPane(p)
    },
  }
  return { mirror: createPaneMirror(deps), navigate, setFocusedPane, setPaneHistory }
}

describe('createPaneMirror', () => {
  beforeEach(() => vi.clearAllMocks())

  it('is a no-op when source and target are the same pane', () => {
    const { mirror, navigate } = setup({ refs: { left: makePaneRef(), right: makePaneRef() } })
    mirror.copyPathBetweenPanes('left', 'left')
    expect(navigate).not.toHaveBeenCalled()
  })

  it('mirrors a local listing to the target pane via a goTo/mirror navigation', () => {
    getCurrentEntrySpy.mockReturnValue({ networkHost: null })
    const left = makePaneRef()
    const right = makePaneRef()
    const { mirror, navigate } = setup({
      refs: { left, right },
      volumeIdByPane: { left: 'root', right: 'usb' },
      pathByPane: { left: '/a/b', right: '/x' },
      focused: 'right', // source (left) not focused → no cursor refine
    })

    mirror.copyPathBetweenPanes('left', 'right')

    expect(navigate).toHaveBeenCalledWith({
      pane: 'right',
      to: { goTo: { volumeId: 'root', path: '/a/b' } },
      source: 'mirror',
    })
  })

  it('refines to the folder under the cursor when the source pane is focused', () => {
    getCurrentEntrySpy.mockReturnValue({ networkHost: null })
    const left = makePaneRef({
      getCursorEntry: vi.fn(() => ({ isDirectory: true, name: 'sub', path: '/a/b/sub' }) as never),
    })
    const right = makePaneRef()
    const { mirror, navigate } = setup({
      refs: { left, right },
      volumeIdByPane: { left: 'root', right: 'usb' },
      pathByPane: { left: '/a/b', right: '/x' },
      focused: 'left',
    })

    mirror.copyPathBetweenPanes('left', 'right')

    expect(navigate).toHaveBeenCalledWith({
      pane: 'right',
      to: { goTo: { volumeId: 'root', path: '/a/b/sub' } },
      source: 'mirror',
    })
  })

  it('skips the redundant navigation when the target already shows the same volume + path', () => {
    getCurrentEntrySpy.mockReturnValue({ networkHost: null })
    const { mirror, navigate, setFocusedPane } = setup({
      refs: { left: makePaneRef(), right: makePaneRef() },
      volumeIdByPane: { left: 'root', right: 'root' },
      pathByPane: { left: '/same', right: '/same' },
      focused: 'right',
    })

    mirror.copyPathBetweenPanes('left', 'right')

    expect(navigate).not.toHaveBeenCalled()
    // Focus was already on 'right' (the original), so no restore write is needed.
    expect(setFocusedPane).not.toHaveBeenCalled()
  })

  it('mirrors a network host under the cursor to the target pane', () => {
    getCurrentEntrySpy.mockReturnValue({ networkHost: { name: 'srcHost' } })
    const left = makePaneRef({
      getNetworkCursorEntry: vi.fn(() => ({ kind: 'host', host: { name: 'pickedHost' } }) as never),
    })
    const right = makePaneRef()
    const { mirror, setPaneHistory } = setup({
      refs: { left, right },
      volumeIdByPane: { left: 'network', right: 'root' },
      focused: 'left',
    })

    mirror.copyPathBetweenPanes('left', 'right')

    expect(right.setNetworkHost).toHaveBeenCalledWith({ name: 'pickedHost' })
    expect(setPaneHistory).toHaveBeenCalled()
  })
})
