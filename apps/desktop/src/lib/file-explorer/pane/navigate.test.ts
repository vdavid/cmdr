/**
 * `navigate(intent, deps)` headless seam tests.
 *
 * Exercise `navigate()` DIRECTLY against injected fakes (no mount needed): the
 * cross-volume snapshot branch, the pinned-tab fork (both arms +
 * `MAX_TABS_PER_PANE`), the token drop of a stale background correction, the
 * edge-flow `'fallback'` source (terminal commit + the history-push asymmetry),
 * every refusal kind → its exact `message` (L12), the per-arm optimistic-commit
 * ordering (P4), and the same-token self-re-entry rule for the parent-nav /
 * walk-up completion.
 *
 * The fakes back the store reads/writes with a REAL `TabManager` per pane (so the
 * pinned-fork tab splice + the commit are observable on real tab state), a real
 * volume map, and spies for the FilePane handle, the resolver, persistence, and
 * focus. Assertions are on OBSERVABLE OUTCOMES (committed tab state, history
 * depth, persisted events, returned refusal `message`s), never internal function
 * identities — the same discipline the mounted suites use.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  navigate,
  commitPathFromListing,
  type NavigateDeps,
  type PersistEvent,
  type LastUsedPathRecord,
} from './navigate'
import { createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'
import type { FilePaneAPI } from './types'

/** A live volume map the fake deps resolve paths/names against (a Map so misses are `undefined`). */
const VOLUMES = new Map<string, { id: string; name: string; path: string }>([
  ['root', { id: 'root', name: 'Macintosh HD', path: '/' }],
  ['ext', { id: 'ext', name: 'Ext', path: '/Volumes/Ext' }],
  ['network', { id: 'network', name: 'Network', path: 'smb://' }],
])

/** A FilePane stub: every method a no-op, `navigateToPath` returns a resolvable promise we can track. */
function makePaneRefStub(): {
  ref: FilePaneAPI
  navigateToPath: ReturnType<typeof vi.fn>
  setNetworkHost: ReturnType<typeof vi.fn>
  navigateToParent: ReturnType<typeof vi.fn>
} {
  const navigateToPath = vi.fn().mockResolvedValue(undefined)
  const navigateToParent = vi.fn().mockResolvedValue(true)
  const setNetworkHost = vi.fn()
  const ref = new Proxy(
    { navigateToPath, navigateToParent, setNetworkHost },
    {
      get(target, prop) {
        if (prop in target) return (target as Record<string | symbol, unknown>)[prop]
        return () => undefined
      },
    },
  ) as unknown as FilePaneAPI
  return { ref, navigateToPath, navigateToParent, setNetworkHost }
}

interface Harness {
  deps: NavigateDeps
  mgr: (pane: 'left' | 'right') => TabManager
  tab: (pane: 'left' | 'right') => ReturnType<typeof getActiveTab>
  persistEvents: PersistEvent[]
  lastUsedRecords: LastUsedPathRecord[]
  paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }>
  determineNavigationPath: ReturnType<typeof vi.fn>
  setFocusedPane: ReturnType<typeof vi.fn>
  addToast: ReturnType<typeof vi.fn>
}

/** The store-backed read/write deps over the real per-pane tab managers (live references, no snapshots). */
function makeStoreDeps(
  managers: Record<'left' | 'right', TabManager>,
  paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }>,
  tab: (pane: 'left' | 'right') => ReturnType<typeof getActiveTab>,
): Pick<
  NavigateDeps,
  | 'getTabMgr'
  | 'getPaneVolumeId'
  | 'getPanePath'
  | 'getPaneHistory'
  | 'getPaneVolumePath'
  | 'getPaneVolumeName'
  | 'otherPane'
  | 'setPaneVolumeId'
  | 'setPanePath'
  | 'setPaneHistory'
  | 'getPaneRef'
> {
  return {
    getTabMgr: (pane) => managers[pane],
    getPaneVolumeId: (pane) => tab(pane).volumeId,
    getPanePath: (pane) => tab(pane).path,
    getPaneHistory: (pane) => tab(pane).history,
    getPaneVolumePath: (pane) => volumePathFor(tab(pane).volumeId),
    getPaneVolumeName: (pane) => volumeNameFor(tab(pane).volumeId),
    otherPane: (pane) => (pane === 'left' ? 'right' : 'left'),
    setPaneVolumeId: (pane, volumeId) => {
      tab(pane).volumeId = volumeId
    },
    setPanePath: (pane, path) => {
      tab(pane).path = path
    },
    setPaneHistory: (pane, history) => {
      tab(pane).history = history
    },
    getPaneRef: (pane) => paneState[pane].paneRef?.ref,
  }
}

interface HarnessOpts {
  left?: { path: string; volumeId: string }
  right?: { path: string; volumeId: string }
  suppressRef?: ('left' | 'right')[]
}

/** Builds one pane's tab manager + (optionally suppressed) FilePane stub from the opts. */
function makePaneFixture(spec: { path: string; volumeId: string } | undefined, suppressed: boolean) {
  const mgr = createTabManager(createInitialTabState(spec?.path ?? '/Users/me', spec?.volumeId ?? 'root'))
  const state: { paneRef?: ReturnType<typeof makePaneRefStub> } = suppressed ? {} : { paneRef: makePaneRefStub() }
  return { mgr, state }
}

/** Builds a fresh harness: real per-pane tab managers + spied side effects. */
function makeHarness(opts?: HarnessOpts): Harness {
  const suppress = new Set(opts?.suppressRef ?? [])
  const left = makePaneFixture(opts?.left, suppress.has('left'))
  const right = makePaneFixture(opts?.right, suppress.has('right'))
  const managers: Record<'left' | 'right', TabManager> = { left: left.mgr, right: right.mgr }
  const paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }> = {
    left: left.state,
    right: right.state,
  }

  const persistEvents: PersistEvent[] = []
  const lastUsedRecords: LastUsedPathRecord[] = []

  const determineNavigationPath = vi
    .fn()
    .mockImplementation((_v, _vp, targetPath: string) => Promise.resolve(targetPath))
  const setFocusedPane = vi.fn()
  const addToast = vi.fn()

  const tab = (pane: 'left' | 'right') => getActiveTab(managers[pane])

  const deps: NavigateDeps = {
    ...makeStoreDeps(managers, paneState, tab),
    setFocusedPane,
    getVolumePathById: (volumeId) => VOLUMES.get(volumeId)?.path,
    determineNavigationPath,
    persist: (event) => {
      persistEvents.push(event)
      if (event.kind === 'last-used-path') lastUsedRecords.push(event.record)
    },
    addToast,
    tokens: new Map(),
    correctionGen: { value: 0 },
  }

  return {
    deps,
    mgr: (pane) => managers[pane],
    tab,
    persistEvents,
    lastUsedRecords,
    paneState,
    determineNavigationPath,
    setFocusedPane,
    addToast,
  }
}

/** Volume mount path for a pane's current volume (`smb://` for network, `/` for unknown). */
function volumePathFor(volumeId: string): string {
  if (volumeId === 'network') return 'smb://'
  return VOLUMES.get(volumeId)?.path ?? '/'
}

/** Display name for a pane's current volume (`Network` for the virtual volume, else the live name). */
function volumeNameFor(volumeId: string): string | undefined {
  if (volumeId === 'network') return 'Network'
  return VOLUMES.get(volumeId)?.name
}

/** Flush queued microtasks (the cross-volume async IIFE + correction `.then`). */
async function flush(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

let h: Harness
beforeEach(() => {
  h = makeHarness()
})

describe('in-place path nav (P4 — NOT optimistic, commits at listing-complete)', () => {
  it('drives the FilePane primitive and returns its promise as `settled`; does NOT commit on call', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } }, source: 'user' },
      h.deps,
    )

    expect(result.status).toBe('started')
    // The FilePane primitive was driven; the path has NOT advanced yet (in-place
    // commit lands later via commitPathFromListing).
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/sub', undefined)
    expect(h.tab('left').path).toBe('/Users/me')
  })

  it('forwards `selectName` to the FilePane primitive', () => {
    navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } },
        source: 'user',
        selectName: 'file.txt',
      },
      h.deps,
    )
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/sub', 'file.txt')
  })

  it('commitPathFromListing commits the path + pushes one history entry + records last-used', () => {
    // Drive the in-place nav, then land its completion.
    navigate({ pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } }, source: 'user' }, h.deps)
    const depthBefore = h.tab('left').history.stack.length

    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/sub')

    expect(committed).toBe(true)
    expect(h.tab('left').path).toBe('/Users/me/sub')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)?.path).toBe('/Users/me/sub')
    expect(h.lastUsedRecords).toContainEqual({ volumeId: 'root', path: '/Users/me/sub' })
  })
})

describe('volume switch (P4 — truly optimistic, synchronous commit)', () => {
  it('commits volumeId + path + history SYNCHRONOUSLY, before any listing', () => {
    const depthBefore = h.tab('left').history.stack.length
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    // Committed immediately (no await, no listing event).
    expect(h.tab('left').volumeId).toBe('ext')
    expect(h.tab('left').path).toBe('/Volumes/Ext')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)).toMatchObject({ volumeId: 'ext', path: '/Volumes/Ext' })
  })

  it("records the OLD path as the old volume's last-used before the swap", () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    // The pre-save of the OLD path under the OLD volume (DPE:615).
    expect(h.lastUsedRecords[0]).toEqual({ volumeId: 'root', path: '/Users/me' })
  })

  it("shifts focus to the navigated pane for a 'user' source", () => {
    navigate({ pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    expect(h.setFocusedPane).toHaveBeenCalledWith('right')
  })

  it("does NOT shift focus for a 'mirror' source (L1 restoreFocus semantics)", () => {
    navigate(
      { pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'mirror' },
      h.deps,
    )
    expect(h.setFocusedPane).not.toHaveBeenCalled()
  })

  it('uses the volume mount path for the background correction lookup', () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    expect(h.determineNavigationPath).toHaveBeenCalledWith('ext', '/Volumes/Ext', '/Volumes/Ext', expect.anything())
  })
})

describe('background correction (global correctionGen, the old volumeChangeGeneration)', () => {
  it('applies a better path when the correction is still the latest', async () => {
    h.determineNavigationPath.mockResolvedValue('/Volumes/Ext/photos')
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    await flush()

    expect(h.tab('left').path).toBe('/Volumes/Ext/photos')
    expect(h.tab('left').history.stack.at(-1)?.path).toBe('/Volumes/Ext/photos')
  })

  it('DROPS a stale correction superseded by a newer volume change on the SAME pane', async () => {
    // First switch: its correction resolves to a "better" path but slowly.
    let resolveFirst: (p: string) => void = () => {}
    const slowCorrection = new Promise<string>((r) => {
      resolveFirst = r
    })
    h.determineNavigationPath.mockReturnValueOnce(slowCorrection)
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    // A newer navigate() bumps the global correctionGen before the first resolves.
    h.determineNavigationPath.mockResolvedValueOnce('/')
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'user' }, h.deps)
    await flush()
    expect(h.tab('left').volumeId).toBe('root')
    expect(h.tab('left').path).toBe('/')

    // Now the STALE first correction resolves — it must be dropped (gen superseded).
    resolveFirst('/Volumes/Ext/should-be-dropped')
    await flush()
    expect(h.tab('left').path).toBe('/') // unchanged — stale correction dropped
    expect(h.tab('left').volumeId).toBe('root')
  })

  it('DROPS a left-pane correction superseded by a volume change on the RIGHT pane (GLOBAL gen)', async () => {
    // The correctionGen is GLOBAL (the old `volumeChangeGeneration` was a single
    // counter shared by both panes), not per-pane: a volume change on the RIGHT
    // pane must drop a still-pending correction on the LEFT pane. Without this, a
    // simultaneous two-pane reset (E2E `ensureAppReady`'s double `mcp-volume-select`)
    // runs both corrections and re-enters the listing cycle on both panes — a freeze.
    let resolveLeft: (p: string) => void = () => {}
    const slowLeft = new Promise<string>((r) => {
      resolveLeft = r
    })
    h.determineNavigationPath.mockReturnValueOnce(slowLeft)
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    const leftPathAfterSwitch = h.tab('left').path

    // The RIGHT pane switches volumes — this bumps the GLOBAL correctionGen.
    h.determineNavigationPath.mockResolvedValueOnce('/Volumes/Ext')
    navigate({ pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    await flush()

    // The left correction resolves late — dropped because the right switch bumped
    // the shared gen past it.
    resolveLeft('/Volumes/Ext/should-be-dropped')
    await flush()
    expect(h.tab('left').path).toBe(leftPathAfterSwitch) // unchanged — left correction dropped
  })
})

describe('pinned-tab fork (L7 — unified, both arms)', () => {
  // The PATH fork lives at the listing-completion landing (`commitPathFromListing`),
  // not at the `navigate()` call: both coordinator-initiated in-place navs (which
  // drive the FilePane, then re-enter via onPathChange) and FilePane-internal navs
  // (Enter on a folder — bypass navigate() entirely) must fork identically, so the
  // single fork point is the onPathChange landing. The in-place `navigate()` arm
  // just drives the FilePane primitive.
  it('path-change landing on a pinned tab opens a NEW unpinned tab; the pinned tab is unchanged', () => {
    getActiveTab(h.mgr('left')).pinned = true
    const pinnedId = getActiveTab(h.mgr('left')).id
    const countBefore = h.mgr('left').tabs.length

    // The in-place arm drives the FilePane; the fork happens when the listing lands.
    navigate({ pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/docs' } }, source: 'user' }, h.deps)
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/docs', undefined)
    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/docs')

    expect(committed).toBe(false) // a fork is not an in-place commit
    expect(h.mgr('left').tabs.length).toBe(countBefore + 1)
    const stillPinned = h.mgr('left').tabs.find((t) => t.id === pinnedId)
    expect(stillPinned?.path).toBe('/Users/me')
    expect(stillPinned?.pinned).toBe(true)
    const active = getActiveTab(h.mgr('left'))
    expect(active.id).not.toBe(pinnedId)
    expect(active.pinned).toBe(false)
    expect(active.path).toBe('/Users/me/docs')
  })

  it('volume-change on a pinned tab opens a NEW unpinned tab with the target volume', () => {
    getActiveTab(h.mgr('left')).pinned = true
    const pinnedId = getActiveTab(h.mgr('left')).id
    const countBefore = h.mgr('left').tabs.length

    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    expect(h.mgr('left').tabs.length).toBe(countBefore + 1)
    const active = getActiveTab(h.mgr('left'))
    expect(active.id).not.toBe(pinnedId)
    expect(active.pinned).toBe(false)
    expect(active.volumeId).toBe('ext')
    expect(active.path).toBe('/Volumes/Ext')
  })

  it('at MAX_TABS_PER_PANE a pinned path landing commits in-place and toasts "Tab limit reached"', () => {
    const mgr = h.mgr('left')
    getActiveTab(mgr).pinned = true
    const activeId = getActiveTab(mgr).id
    while (mgr.tabs.length < 10) {
      mgr.tabs.push({
        id: `filler-${String(mgr.tabs.length)}`,
        path: '/Users/me',
        volumeId: 'root',
        history: { stack: [{ volumeId: 'root', path: '/Users/me' }], currentIndex: 0 },
        sortBy: 'name',
        sortOrder: 'ascending',
        viewMode: 'brief',
        pinned: false,
        cursorFilename: null,
        unreachable: null,
      })
    }

    // At cap, the landing falls through to an in-place commit on the pinned tab.
    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/docs')

    expect(committed).toBe(true) // in-place fall-through commits
    expect(mgr.tabs.length).toBe(10) // no new tab
    expect(getActiveTab(mgr).id).toBe(activeId) // pinned tab stayed active
    expect(getActiveTab(mgr).path).toBe('/Users/me/docs')
    expect(h.addToast).toHaveBeenCalledWith('Tab limit reached', { level: 'warn' })
  })
})

// The former "cross-volume snapshot branch (L5)" suite is subsumed: a snapshot
// pane opening a real entry resolves the entry's `Location` at the edge (FilePane
// → `onGoToLocation`), then a `{ location }` to a different volume takes the
// switch arm — covered by the "{ location } arm" suite above. The no-volume case
// is now the edge resolver's friendly toast, covered in navigate-and-select.test.

describe('snapshot open ({ snapshot } arm)', () => {
  it('builds the search-results:// URL and commits via the volume-change machinery', () => {
    const depthBefore = h.tab('left').history.stack.length
    navigate({ pane: 'left', to: { snapshot: 'sr-9' }, source: 'user' }, h.deps)

    expect(h.tab('left').volumeId).toBe('search-results')
    expect(h.tab('left').path).toBe('search-results://sr-9')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.setFocusedPane).toHaveBeenCalledWith('left')
  })
})

describe("edge-flow fallback (source: 'fallback') — terminal commit + history-push asymmetry", () => {
  beforeEach(() => {
    h = makeHarness({ left: { path: '/Volumes/Ext/photos', volumeId: 'ext' } })
  })

  it('MTP-fatal / retry / open-home style: commits the recovery target AND pushes a history entry', () => {
    const depthBefore = h.tab('left').history.stack.length
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'fallback' }, h.deps)

    expect(h.tab('left').volumeId).toBe('root')
    expect(h.tab('left').path).toBe('/')
    // The three pushing fallbacks DO grow a Back target.
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)).toMatchObject({ volumeId: 'root', path: '/' })
  })

  it('unmount style (pushHistory: false): commits the redirect WITHOUT growing a Back target', () => {
    const depthBefore = h.tab('left').history.stack.length
    navigate(
      { pane: 'left', to: { selectVolume: { volumeId: 'root', path: '~' } }, source: 'fallback', pushHistory: false },
      h.deps,
    )

    expect(h.tab('left').volumeId).toBe('root')
    expect(h.tab('left').path).toBe('~')
    // The asymmetry: an unmount redirect must NOT push history.
    expect(h.tab('left').history.stack.length).toBe(depthBefore)
  })

  it('is terminal: no OLD-path pre-save and no background correction', () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'fallback' }, h.deps)

    // No last-used-path pre-save of the (broken/gone) old volume.
    expect(h.lastUsedRecords).toEqual([])
    // No "best path" correction scheduled (the recovery target IS the answer).
    expect(h.determineNavigationPath).not.toHaveBeenCalled()
  })

  it('does NOT shift the focused pane (L1: fallbacks re-anchor DOM focus, not the focused pane)', () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'fallback' }, h.deps)
    expect(h.setFocusedPane).not.toHaveBeenCalled()
  })

  it('commits in-place on a PINNED active tab (terminal skips the pinned-tab fork)', () => {
    getActiveTab(h.mgr('left')).pinned = true
    const pinnedId = getActiveTab(h.mgr('left')).id
    const countBefore = h.mgr('left').tabs.length

    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'fallback' }, h.deps)

    // No new tab — the recovery commits on the active (pinned) tab itself.
    expect(h.mgr('left').tabs.length).toBe(countBefore)
    expect(getActiveTab(h.mgr('left')).id).toBe(pinnedId)
    expect(getActiveTab(h.mgr('left')).volumeId).toBe('root')
    expect(getActiveTab(h.mgr('left')).path).toBe('/')
  })
})

describe('history walk ({ history } arm)', () => {
  it('back moves to the previous entry and commits path + history', () => {
    // Build a two-entry history: /Users/me -> /Users/me/deep (current).
    const mgr = h.mgr('left')
    getActiveTab(mgr).history = {
      stack: [
        { volumeId: 'root', path: '/Users/me' },
        { volumeId: 'root', path: '/Users/me/deep' },
      ],
      currentIndex: 1,
    }
    getActiveTab(mgr).path = '/Users/me/deep'

    navigate({ pane: 'left', to: { history: 'back' }, source: 'user' }, h.deps)

    expect(h.tab('left').path).toBe('/Users/me')
    expect(h.tab('left').history.currentIndex).toBe(0)
  })

  it('back at the oldest entry is a no-op', () => {
    const indexBefore = h.tab('left').history.currentIndex
    const pathBefore = h.tab('left').path
    navigate({ pane: 'left', to: { history: 'back' }, source: 'user' }, h.deps)
    expect(h.tab('left').history.currentIndex).toBe(indexBefore)
    expect(h.tab('left').path).toBe(pathBefore)
  })

  it('parent delegates to the FilePane navigateToParent primitive', async () => {
    const result = navigate({ pane: 'left', to: { history: 'parent' }, source: 'user' }, h.deps)
    expect(h.paneState.left.paneRef?.navigateToParent).toHaveBeenCalled()
    expect(result.status).toBe('started')
    if (result.status === 'started') await result.settled
  })

  it('back across volumes switches the pane volume and restores a network host', () => {
    const mgr = h.mgr('left')
    const host = { id: 'srv', name: 'srv', hostname: 'srv.local', port: 445 }
    getActiveTab(mgr).history = {
      stack: [
        { volumeId: 'network', path: 'smb://', networkHost: host },
        { volumeId: 'root', path: '/Users/me/deep' },
      ],
      currentIndex: 1,
    }
    getActiveTab(mgr).volumeId = 'root'
    getActiveTab(mgr).path = '/Users/me/deep'

    navigate({ pane: 'left', to: { history: 'back' }, source: 'user' }, h.deps)

    expect(h.tab('left').volumeId).toBe('network')
    expect(h.tab('left').path).toBe('smb://')
    expect(h.paneState.left.paneRef?.setNetworkHost).toHaveBeenCalledWith(host)
  })
})

describe('commitPathFromListing — stale-listing drop policy (L6, token + foreign-path)', () => {
  it('real-volume branch: drops a listing whose path is not on the pane volume', () => {
    h = makeHarness({ left: { path: '/Volumes/Ext', volumeId: 'ext' } })
    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/deep')
    expect(committed).toBe(false)
    expect(h.tab('left').path).toBe('/Volumes/Ext')
    expect(h.lastUsedRecords).toEqual([])
  })

  it('network branch: drops a non-smb path', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })
    expect(commitPathFromListing(h.deps, 'left', '/Users/me/deep')).toBe(false)
    expect(h.tab('left').path).toBe('smb://')
  })

  it('search-results branch: drops a non-search-results path', () => {
    h = makeHarness({ left: { path: 'search-results://sr-1', volumeId: 'search-results' } })
    expect(commitPathFromListing(h.deps, 'left', '/Library/x')).toBe(false)
    expect(h.tab('left').path).toBe('search-results://sr-1')
  })

  it('commits a non-stale path that IS on the current volume', () => {
    const depthBefore = h.tab('left').history.stack.length
    expect(commitPathFromListing(h.deps, 'left', '/Users/me/deep')).toBe(true)
    expect(h.tab('left').path).toBe('/Users/me/deep')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
  })
})

describe('same-token self-re-entry (parent-nav / walk-up completion)', () => {
  it('a parent-nav completion re-entering via commitPathFromListing is NOT dropped', () => {
    // Start a parent-nav (mints a token, drives the primitive).
    navigate({ pane: 'left', to: { history: 'parent' }, source: 'user' }, h.deps)
    const tokenAfterParent = h.deps.tokens.get('left')

    // The primitive's onPathChange fires for the resolved parent — same logical
    // navigation, no new navigate() minted a token, so it commits (not dropped).
    const committed = commitPathFromListing(h.deps, 'left', '/Users')
    expect(committed).toBe(true)
    expect(h.tab('left').path).toBe('/Users')
    // The token was NOT bumped by the self-re-entry.
    expect(h.deps.tokens.get('left')).toBe(tokenAfterParent)
  })
})

describe('{ location } arm — self-routing by volume', () => {
  it('(the bug) volumeId ≠ current switches the volume and lands on the resolved volume', () => {
    // Repro: pane sits on an SMB-like fake volume; navigating to a `root`
    // location must SWITCH to root, not load the local path over the NAS.
    h = makeHarness({ left: { path: '/naspi/share', volumeId: 'naspi' } })
    const depthBefore = h.tab('left').history.stack.length

    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Library/x' } }, source: 'mcp' },
      h.deps,
    )

    expect(result.status).toBe('started')
    // Switch arm: optimistic synchronous commit, no listing needed.
    expect(h.tab('left').volumeId).toBe('root')
    expect(h.tab('left').path).toBe('/Library/x')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)).toMatchObject({ volumeId: 'root', path: '/Library/x' })
    // It drove a volume switch, NOT the in-place FilePane primitive.
    expect(h.paneState.left.paneRef?.navigateToPath).not.toHaveBeenCalled()
  })

  it('volumeId === current takes the in-place arm (NOT optimistic; commit lands via commitPathFromListing as push-path)', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } }, source: 'user' },
      h.deps,
    )

    expect(result.status).toBe('started')
    // In-place: drives the FilePane primitive; path has NOT advanced yet.
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/sub', undefined)
    expect(h.tab('left').path).toBe('/Users/me')

    const depthBefore = h.tab('left').history.stack.length
    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/sub')
    expect(committed).toBe(true)
    expect(h.tab('left').path).toBe('/Users/me/sub')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.lastUsedRecords).toContainEqual({ volumeId: 'root', path: '/Users/me/sub' })
  })

  it('a pinned tab forks a new unpinned tab on the { location } cross-volume switch', () => {
    h = makeHarness({ left: { path: '/naspi/share', volumeId: 'naspi' } })
    getActiveTab(h.mgr('left')).pinned = true
    const pinnedId = getActiveTab(h.mgr('left')).id
    const countBefore = h.mgr('left').tabs.length

    navigate({ pane: 'left', to: { goTo: { volumeId: 'root', path: '/' } }, source: 'user' }, h.deps)

    expect(h.mgr('left').tabs.length).toBe(countBefore + 1)
    const active = getActiveTab(h.mgr('left'))
    expect(active.id).not.toBe(pinnedId)
    expect(active.pinned).toBe(false)
    expect(active.volumeId).toBe('root')
    expect(active.path).toBe('/')
  })

  // The in-place arm's network/MTP refusals (for a same-volume `{ location }`) are
  // the byte-for-byte contract in the "refusal strings (L12)" describe below.
})

describe('{ volumeId, path } volume-(re)select — ALWAYS the switch arm (guards the C1 regression)', () => {
  it('volumeId === current STILL switches (optimistic commit + history push), never the in-place/refusal path', () => {
    h = makeHarness({ left: { path: '/Volumes/Ext', volumeId: 'ext' } })
    const depthBefore = h.tab('left').history.stack.length

    // A volume-(re)select passing the CURRENT volume id (network-restore-on-cancel,
    // selectVolumeByIndex re-select, mirror, etc.) must take the switch arm.
    const result = navigate(
      { pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext/photos' } }, source: 'user' },
      h.deps,
    )

    expect(result.status).toBe('started')
    expect(h.tab('left').volumeId).toBe('ext')
    expect(h.tab('left').path).toBe('/Volumes/Ext/photos')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    // Switch arm, not in-place: the FilePane primitive was not driven.
    expect(h.paneState.left.paneRef?.navigateToPath).not.toHaveBeenCalled()
  })
})

describe('refusal strings (L12) — byte-for-byte contract', () => {
  // Each fires from the in-place arm: a `{ location }` whose volumeId equals the
  // pane's current one (same-volume) routes in-place, where the refusals live.
  it('network-volume pane returns the exact select_volume refusal string', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'network', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'on-network-volume',
        message: 'Pane is on the Network volume. Use select_volume to switch to a local volume first.',
      },
    })
  })

  it('MTP path mismatch returns the exact "not on this MTP volume" string (note the em dash)', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: 'mtp://otherdev/2/DCIM' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: { kind: 'mtp-unconnected', message: 'Pane is not on this MTP volume — call select_volume first.' },
    })
  })

  it('on-MTP-volume pane returns the exact "on the … MTP volume" string (volumeName falls back to id)', () => {
    h = makeHarness({ left: { path: 'mtp://dev/1/DCIM', volumeId: 'mtp-dev:1' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'mtp-dev:1', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'mtp-unconnected',
        message: 'Pane is on the mtp-dev:1 MTP volume. Use select_volume to switch to a local volume first.',
      },
    })
  })

  it('pane-unavailable returns the exact "Pane not available" string', () => {
    h = makeHarness({ suppressRef: ['left'] })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({ status: 'refused', reason: { kind: 'pane-unavailable', message: 'Pane not available' } })
  })
})
