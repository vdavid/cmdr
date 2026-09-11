/**
 * `navigate()`'s return point: what a pane showed before commits ran ahead of its
 * listing, and the `{ returnTo }` arm a cancelled load takes to get back there.
 * Shares the fake per-pane harness from `navigate.test-fixtures.ts`.
 */
import { describe, it, expect } from 'vitest'
import { navigate, commitPathFromListing, returnPointFor } from './navigate'
import { makeHarness, flush, type Harness } from './navigate.test-fixtures'

function selectVolume(h: Harness, volumeId: string, path: string): void {
  navigate({ pane: 'left', to: { selectVolume: { volumeId, path } }, source: 'user' }, h.deps)
}

function returnToShown(h: Harness, selectName?: string): void {
  const point = returnPointFor(h.deps, 'left')
  if (!point) throw new Error('expected a return point')
  navigate({ pane: 'left', to: { returnTo: point }, source: 'cancel', selectName }, h.deps)
}

describe('the return point', () => {
  it('remembers what the tab held before a volume switch', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    expect(returnPointFor(h.deps, 'left')).toMatchObject({
      shown: { volumeId: 'root', path: '/Users/me' },
      historyIndex: 0,
    })
  })

  it('keeps the first point across a second commit that runs before anything lands', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    selectVolume(h, 'ext', '/Volumes/Ext/b')
    expect(returnPointFor(h.deps, 'left')?.shown).toEqual({ volumeId: 'root', path: '/Users/me' })
  })

  it('is forgotten once a listing lands', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    commitPathFromListing(h.deps, 'left', '/Volumes/Ext/a')
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })

  it('is forgotten when the destination shows without a listing', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'network', 'smb://')
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })

  it('is not honored once the tab no longer sits where the commits left it', () => {
    // A pane swap or a root-follow rewrite moves the tab without navigating.
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    h.tab('left').path = '/Volumes/Ext/elsewhere'
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })

  it('is not honored in another tab', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    const mgr = h.mgr('left')
    mgr.tabs.push({ ...h.tab('left'), id: 'other-tab' })
    mgr.activeTabId = 'other-tab'
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })

  it('remembers the entry a history walk left', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    commitPathFromListing(h.deps, 'left', '/Users/me/a') // history [me, me/a] at 1
    navigate({ pane: 'left', to: { history: 'back' }, source: 'user' }, h.deps)
    expect(returnPointFor(h.deps, 'left')).toMatchObject({
      shown: { volumeId: 'root', path: '/Users/me/a' },
      historyIndex: 1,
    })
  })

  it('remembers what a landed switch showed before its background correction moves it', async () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    h.determineNavigationPath.mockResolvedValueOnce('/Volumes/Ext/photos')
    selectVolume(h, 'ext', '/Volumes/Ext')
    commitPathFromListing(h.deps, 'left', '/Volumes/Ext') // the switch lands before the correction
    await flush()
    expect(h.tab('left').path).toBe('/Volumes/Ext/photos')
    expect(returnPointFor(h.deps, 'left')?.shown).toEqual({ volumeId: 'ext', path: '/Volumes/Ext' })
  })

  it("isn't remembered by a cancel's own commits", () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me/gone' } })
    navigate(
      {
        pane: 'left',
        to: { selectVolume: { volumeId: 'root', path: '/Users/me' } },
        source: 'cancel',
        pushHistory: false,
      },
      h.deps,
    )
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })
})

describe('the { returnTo } arm', () => {
  it('restores the volume, path, and history index, and pushes nothing', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    selectVolume(h, 'ext', '/Volumes/Ext/a')
    selectVolume(h, 'ext', '/Volumes/Ext/b')
    const depth = h.tab('left').history.stack.length

    returnToShown(h)

    expect(h.tab('left')).toMatchObject({ volumeId: 'root', path: '/Users/me' })
    expect(h.tab('left').history.currentIndex).toBe(0)
    expect(h.tab('left').history.stack.length).toBe(depth)
    // Cross-volume: the pane's props re-list it, like any volume switch.
    expect(h.paneState.left.paneRef?.navigateToPath).not.toHaveBeenCalled()
    expect(returnPointFor(h.deps, 'left')).toBeNull()
  })

  it('re-lists a same-volume return through the pane, with the entry to select', () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    commitPathFromListing(h.deps, 'left', '/Users/me/a')
    navigate({ pane: 'left', to: { history: 'back' }, source: 'user' }, h.deps)
    commitPathFromListing(h.deps, 'left', '/Users/me') // Back landed, at index 0
    navigate({ pane: 'left', to: { history: 'forward' }, source: 'user' }, h.deps)

    returnToShown(h, 'a')

    expect(h.tab('left').history.currentIndex).toBe(0)
    expect(h.tab('left').path).toBe('/Users/me')
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me', 'a')
  })

  it('drops a background correction still resolving for the cancelled switch', async () => {
    const h = makeHarness({ left: { volumeId: 'root', path: '/Users/me' } })
    let resolveCorrection: (path: string) => void = () => {}
    const correction = new Promise<string>((resolve) => {
      resolveCorrection = resolve
    })
    h.determineNavigationPath.mockReturnValueOnce(correction)
    selectVolume(h, 'ext', '/Volumes/Ext')

    returnToShown(h)
    resolveCorrection('/Volumes/Ext/photos')
    await flush()

    expect(h.tab('left')).toMatchObject({ volumeId: 'root', path: '/Users/me' })
  })

  it('reopens the Servers hub host the pane showed', () => {
    const h = makeHarness({ left: { volumeId: 'network', path: 'smb://' } })
    const host = { name: 'nas', hostname: 'nas.local' } as never
    h.tab('left').history = { stack: [{ volumeId: 'network', path: 'smb://', networkHost: host }], currentIndex: 0 }
    selectVolume(h, 'ext', '/Volumes/Ext')

    returnToShown(h)

    expect(h.tab('left').volumeId).toBe('network')
    expect(h.paneState.left.paneRef?.setNetworkHost).toHaveBeenLastCalledWith(host)
  })
})
