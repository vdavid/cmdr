/**
 * Store-driven F-bar tests: the button `disabled` flags now read the focused
 * pane's `VolumeCapabilities` (invariant A6), not a `volumeId === 'search-results'`
 * string compare. These seed the `explorerState` singleton to a given kind and
 * assert the resulting disablement, pinning the M2 capability wiring at the
 * component level.
 *
 * `capabilitiesFor` short-circuits on the two virtual ids before any volume-store
 * lookup, so the search-results / network panes need no store stub. A
 * `.svelte.test.ts` so the tab-manager `$state` proxies compile under runes.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'
import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { createTabManager, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'

/** A tab manager whose single active tab sits on `volumeId`. */
function mgrOn(volumeId: string): TabManager {
  return createTabManager(createInitialTabState('/dir', volumeId))
}

/** Mounts the bar with the focused (left) pane on `volumeId` and returns its buttons. */
function mountOn(volumeId: string): HTMLButtonElement[] {
  explorerState.setFocusedPane('left')
  explorerState.setTabMgr('left', mgrOn(volumeId))
  const target = document.createElement('div')
  mount(FunctionKeyBar, { target, props: { visible: true } })
  flushSync()
  return Array.from(target.querySelectorAll('button'))
}

describe('FunctionKeyBar capability disablement', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  it('a real (local) pane enables every default-state button', () => {
    // Buttons: F2 Rename, F3 View, F4 Edit, F5 Copy, F6 Move, F7 New folder, F8 Delete.
    const buttons = mountOn('root')
    for (const button of buttons) {
      expect(button.disabled).toBe(false)
    }
  })

  it('a search-results pane disables F2 / F7 (destination ops), keeps F5 / F6 / F8 (source ops)', () => {
    // caps: canRenameInPlace / canCreateChild false ⇒ F2, F7 disabled;
    // canBeSource true ⇒ F5, F6, F8 enabled (snapshot rows are real files).
    const [f2Rename, f3View, f4Edit, f5Copy, f6Move, f7NewFolder, f8Delete] = mountOn('search-results')

    expect(f2Rename.disabled).toBe(true)
    expect(f7NewFolder.disabled).toBe(true)
    expect(f5Copy.disabled).toBe(false)
    expect(f6Move.disabled).toBe(false)
    expect(f8Delete.disabled).toBe(false)
    // View / Edit are never gated by destination caps.
    expect(f3View.disabled).toBe(false)
    expect(f4Edit.disabled).toBe(false)
  })

  it('a network pane disables both destination AND source buttons (canBeSource: false)', () => {
    // The network host/share list isn't files, so it can neither source nor host
    // an op. The F-bar honestly reflects that now (canBeSource: false) — the bar
    // is inert on a focused network pane either way (the ops no-op deep down).
    const [f2Rename, , , f5Copy, f6Move, f7NewFolder, f8Delete] = mountOn('network')

    expect(f2Rename.disabled).toBe(true)
    expect(f7NewFolder.disabled).toBe(true)
    expect(f5Copy.disabled).toBe(true)
    expect(f6Move.disabled).toBe(true)
    expect(f8Delete.disabled).toBe(true)
  })
})
