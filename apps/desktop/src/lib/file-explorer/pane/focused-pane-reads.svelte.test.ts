/**
 * Tests for the focused-pane read helpers (`focused-pane-reads.ts`).
 *
 * These read the focused pane's path / volume id / searchable folder from the
 * `explorerState` singleton, so each test seeds the singleton via
 * `_resetForTesting()` + the store's named mutators and asserts the read.
 * `resolveSearchableFolder` runs for real (it's a pure, separately-tested
 * helper) so the searchable-folder case exercises the actual walk-back.
 *
 * A `.svelte.test.ts` so the tab-manager `$state` proxies compile under runes.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'
import { pushPath } from '../navigation/navigation-history'
import { getFocusedPanePath, getFocusedPaneVolumeId, getFocusedPaneSearchableFolder } from './focused-pane-reads'

/** A tab manager whose single active tab sits at `path` on `volumeId`. */
function mgrAt(path: string, volumeId = 'root'): TabManager {
  return createTabManager(createInitialTabState(path, volumeId))
}

describe('focused-pane reads', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  it('getFocusedPanePath reads the focused pane active-tab path', () => {
    explorerState.setTabMgr('left', mgrAt('/left/dir'))
    explorerState.setTabMgr('right', mgrAt('/right/dir'))

    explorerState.setFocusedPane('left')
    expect(getFocusedPanePath()).toBe('/left/dir')

    explorerState.setFocusedPane('right')
    expect(getFocusedPanePath()).toBe('/right/dir')
  })

  it('getFocusedPaneVolumeId reads the focused pane active-tab volume id', () => {
    explorerState.setTabMgr('left', mgrAt('/left/dir', 'vol-left'))
    explorerState.setTabMgr('right', mgrAt('/right/dir', 'vol-right'))

    explorerState.setFocusedPane('right')
    expect(getFocusedPaneVolumeId()).toBe('vol-right')

    explorerState.setFocusedPane('left')
    expect(getFocusedPaneVolumeId()).toBe('vol-left')
  })

  it('getFocusedPaneSearchableFolder returns a real folder as-is', () => {
    explorerState.setTabMgr('left', mgrAt('/projects'))
    explorerState.setFocusedPane('left')

    expect(getFocusedPaneSearchableFolder()).toEqual({
      path: '/projects',
      disabled: false,
      disabledReason: '',
    })
  })

  it('getFocusedPaneSearchableFolder walks history back when the focused pane is a snapshot', () => {
    // Tab visited a real folder, then opened a search-results snapshot. The
    // active path is the snapshot URL; the walk-back finds the real folder.
    const mgr = mgrAt('/real/folder')
    const tab = getActiveTab(mgr)
    tab.history = pushPath(tab.history, 'search-results://sr-1')
    tab.path = 'search-results://sr-1'
    explorerState.setTabMgr('left', mgr)
    explorerState.setFocusedPane('left')

    expect(getFocusedPaneSearchableFolder()).toEqual({
      path: '/real/folder',
      disabled: false,
      disabledReason: '',
    })
  })

  it('getFocusedPaneSearchableFolder is disabled when no real folder is reachable', () => {
    const mgr = mgrAt('search-results://sr-1', 'search-results')
    explorerState.setTabMgr('left', mgr)
    explorerState.setFocusedPane('left')

    const result = getFocusedPaneSearchableFolder()
    expect(result.path).toBeNull()
    expect(result.disabled).toBe(true)
    expect(result.disabledReason).not.toBe('')
  })
})
