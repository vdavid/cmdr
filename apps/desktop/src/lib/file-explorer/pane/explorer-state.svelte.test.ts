/**
 * Tests for the explorer store (`explorer-state.svelte.ts`).
 *
 * The store un-traps `DualPaneExplorer`'s navigation + UI-chrome state into one
 * module: `focusedPane`, `showHiddenFiles`, `leftPaneWidthPercent`, and the two
 * tab-manager holders. State is module-private; only getters and named mutators
 * cross the boundary (A1/A2).
 *
 * Coverage:
 * - factory isolation (two instances never share state),
 * - getter/mutator round-trips for every field,
 * - `_resetForTesting` clears the default instance to defaults,
 * - `getTabMgr` returns the LIVE `$state` holder reference, so a `$derived`
 *   reading through it keeps tracking after `setTabMgr` swaps the holder (the
 *   reactivity-transparency contract the Phase-0 `PaneAccess` getters rely on).
 *
 * This is a `.svelte.test.ts` so the test body itself gets rune compilation —
 * the live-reference assertion needs a real `$derived` + `$effect.root`.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { flushSync } from 'svelte'
import { createExplorerState, explorerState, _resetForTesting } from './explorer-state.svelte'
import { createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'

/** A throwaway tab manager rooted at `path` on the default volume. */
function mgrAt(path: string): TabManager {
  return createTabManager(createInitialTabState(path, 'root'))
}

describe('createExplorerState: defaults', () => {
  it('starts left-focused, hidden files shown, panes split 50/50', () => {
    const s = createExplorerState()
    expect(s.getFocusedPane()).toBe('left')
    expect(s.getShowHiddenFiles()).toBe(true)
    expect(s.getLeftPaneWidthPercent()).toBe(50)
  })

  it('starts with a tab manager per pane, each at the home folder', () => {
    const s = createExplorerState()
    expect(getActiveTab(s.getTabMgr('left')).path).toBe('~')
    expect(getActiveTab(s.getTabMgr('right')).path).toBe('~')
    expect(s.getTabMgr('left')).not.toBe(s.getTabMgr('right'))
  })
})

describe('createExplorerState: getter/mutator round-trips', () => {
  it('setFocusedPane stores the focused pane', () => {
    const s = createExplorerState()
    s.setFocusedPane('right')
    expect(s.getFocusedPane()).toBe('right')
    s.setFocusedPane('left')
    expect(s.getFocusedPane()).toBe('left')
  })

  it('setShowHiddenFiles stores the flag', () => {
    const s = createExplorerState()
    s.setShowHiddenFiles(false)
    expect(s.getShowHiddenFiles()).toBe(false)
    s.setShowHiddenFiles(true)
    expect(s.getShowHiddenFiles()).toBe(true)
  })

  it('toggleHiddenFiles flips the flag', () => {
    const s = createExplorerState()
    expect(s.getShowHiddenFiles()).toBe(true)
    s.toggleHiddenFiles()
    expect(s.getShowHiddenFiles()).toBe(false)
    s.toggleHiddenFiles()
    expect(s.getShowHiddenFiles()).toBe(true)
  })

  it('setLeftPaneWidthPercent stores the layout split', () => {
    const s = createExplorerState()
    s.setLeftPaneWidthPercent(33)
    expect(s.getLeftPaneWidthPercent()).toBe(33)
  })

  it('setTabMgr swaps the holder for the given pane only', () => {
    // `$state<TabManager>` wraps the held object in a reactive proxy, so the
    // contract is behavioral (the active tab read through the holder), not
    // proxy identity — `getTabMgr` returns a live reference, which a `===`
    // check against the pre-proxy object would wrongly reject.
    const s = createExplorerState()
    s.setTabMgr('left', mgrAt('/left'))
    s.setTabMgr('right', mgrAt('/right'))
    expect(getActiveTab(s.getTabMgr('left')).path).toBe('/left')
    expect(getActiveTab(s.getTabMgr('right')).path).toBe('/right')
  })
})

describe('createExplorerState: factory isolation', () => {
  it('two instances do not share scalar state', () => {
    const a = createExplorerState()
    const b = createExplorerState()
    a.setFocusedPane('right')
    a.setShowHiddenFiles(false)
    a.setLeftPaneWidthPercent(20)
    expect(b.getFocusedPane()).toBe('left')
    expect(b.getShowHiddenFiles()).toBe(true)
    expect(b.getLeftPaneWidthPercent()).toBe(50)
  })

  it('two instances do not share tab-manager holders', () => {
    const a = createExplorerState()
    const b = createExplorerState()
    a.setTabMgr('left', mgrAt('/a-left'))
    expect(getActiveTab(a.getTabMgr('left')).path).toBe('/a-left')
    expect(getActiveTab(b.getTabMgr('left')).path).toBe('~')
  })
})

describe('explorerState default instance: _resetForTesting', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  it('clears every field back to defaults', () => {
    explorerState.setFocusedPane('right')
    explorerState.setShowHiddenFiles(false)
    explorerState.setLeftPaneWidthPercent(70)
    explorerState.setTabMgr('left', mgrAt('/scratch'))

    _resetForTesting()

    expect(explorerState.getFocusedPane()).toBe('left')
    expect(explorerState.getShowHiddenFiles()).toBe(true)
    expect(explorerState.getLeftPaneWidthPercent()).toBe(50)
    expect(getActiveTab(explorerState.getTabMgr('left')).path).toBe('~')
    expect(getActiveTab(explorerState.getTabMgr('right')).path).toBe('~')
  })
})

describe('getTabMgr live-reference reactivity', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  it('a $derived reading through getTabMgr re-runs after setTabMgr swaps the holder', () => {
    const s = createExplorerState()

    let observed = ''
    const dispose = $effect.root(() => {
      // The derived reads the holder through the getter, exactly like the
      // 12 per-pane component deriveds in `DualPaneExplorer`. If the getter
      // returned a copy/snapshot, this would stop tracking once the holder is
      // swapped.
      const activePath = $derived(getActiveTab(s.getTabMgr('left')).path)
      $effect(() => {
        observed = activePath
      })
    })
    flushSync()
    expect(observed).toBe('~')

    s.setTabMgr('left', mgrAt('/swapped'))
    flushSync()
    expect(observed).toBe('/swapped')

    dispose()
  })

  it('a $derived re-runs when the held tab manager mutates in place', () => {
    const s = createExplorerState()

    let observed: number | undefined
    const dispose = $effect.root(() => {
      const tabCount = $derived(s.getTabMgr('left').tabs.length)
      $effect(() => {
        observed = tabCount
      })
    })
    flushSync()
    expect(observed).toBe(1)

    // Mutating the live holder's reactive `tabs` array must reach the derived.
    s.getTabMgr('left').tabs = [...s.getTabMgr('left').tabs, createInitialTabState('/extra', 'root')]
    flushSync()
    expect(observed).toBe(2)

    dispose()
  })
})
