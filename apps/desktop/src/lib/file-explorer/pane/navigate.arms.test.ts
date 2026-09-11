/**
 * `navigate(intent, deps)` headless seam tests: WHICH arm handles an intent.
 *
 * The pinned-tab fork (L7 — unified across both the in-place and switch arms,
 * plus the `MAX_TABS_PER_PANE` fall-through), the `{ snapshot }` arm, the
 * edge-flow `'fallback'` source (terminal commit + the history-push
 * asymmetry), the `{ history }` arm (back/parent, including a cross-volume
 * back that restores a network host), the `{ location }` arm's self-routing
 * by volume, and the `{ volumeId, path }` volume-(re)select arm (guards the
 * C1 regression: it always switches, even when `volumeId` already matches).
 *
 * Exercises `navigate()` and `commitPathFromListing()` DIRECTLY against
 * injected fakes (no mount needed); harness in `navigate.test-fixtures.ts`.
 * Assertions are on OBSERVABLE OUTCOMES (committed tab state, history depth,
 * tab-manager splits), never internal function identities.
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { navigate, commitPathFromListing } from './navigate'
import { makeHarness, type Harness } from './navigate.test-fixtures'
import { getActiveTab } from '../tabs/tab-state-manager.svelte'

let h: Harness
beforeEach(() => {
  h = makeHarness()
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
    expect(h.addToast).toHaveBeenCalledWith('left', 'Tab limit reached', { level: 'warn' })
  })
})

// The former "cross-volume snapshot branch (L5)" suite is subsumed: a snapshot
// pane opening a real entry resolves the entry's `Location` at the edge (FilePane
// → `onGoToLocation`), then a `{ location }` to a different volume takes the
// switch arm — covered by the "{ location } arm" suite below. The no-volume case
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
  // the byte-for-byte contract in navigate.refusals.test.ts.
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

  it('re-selecting Network from inside a host clears the host, so the pane lands on the host list', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })

    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'network', path: 'smb://' } }, source: 'user' }, h.deps)

    // Pre-fix the pane kept its open host, so `select_volume Network` was a silent
    // no-op: the share list (or a mount-error pane) stayed up and the MCP tool
    // timed out waiting for the volume name to drop back to plain "Network".
    expect(h.paneState.left.paneRef?.setNetworkHost).toHaveBeenCalledWith(null)
  })

  it('switching to a non-network volume leaves the network host alone (the pane clears it itself)', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })

    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    expect(h.paneState.left.paneRef?.setNetworkHost).not.toHaveBeenCalled()
  })
})
