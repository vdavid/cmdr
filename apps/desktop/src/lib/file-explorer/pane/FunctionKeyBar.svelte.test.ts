/**
 * Store-driven F-bar tests: the button `disabled` flags read the focused PANE's
 * `VolumeCapabilities` (`capabilitiesForPane`, volume id + path), not a
 * `volumeId === 'search-results'` string compare and not the volume row alone.
 * These seed the `explorerState` singleton to a given kind and assert the resulting
 * disablement, pinning the capability wiring at the component level.
 *
 * `capabilitiesForPane` short-circuits on the routed kinds (archive by suffix, git
 * portal by path shape) and `capabilitiesFor` on the two virtual ids, all before any
 * volume-store lookup, so none of these panes needs a store stub. A `.svelte.test.ts`
 * so the tab-manager `$state` proxies compile under runes.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'
import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { createTabManager, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'

/** A tab manager whose single active tab sits on `volumeId` at `path`. */
function mgrOn(volumeId: string, path: string): TabManager {
  return createTabManager(createInitialTabState(path, volumeId))
}

/**
 * Mounts the bar with the focused (left) pane on `volumeId` at `path` and returns its
 * buttons. The path matters: the two ROUTED kinds (archive, git portal) are
 * kind-from-path on top of a parent drive whose own id is writable.
 */
function mountOn(volumeId: string, path = '/dir'): HTMLButtonElement[] {
  explorerState.setFocusedPane('left')
  explorerState.setTabMgr('left', mgrOn(volumeId, path))
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
    // caps: canWrite false ⇒ F2, F7 disabled;
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

  /**
   * The two ROUTED kinds. Their pane sits on the parent drive's `volumeId`, which is
   * perfectly writable, so a bar reading capabilities from the id alone offered New
   * folder and Rename inside a git snapshot and inside a read-only tar, then refused on
   * press. `docs/design-principles.md` prefers a disabled key to a refusal dialog, so
   * the bar reads the PANE's row (`capabilitiesForPane`), which resolves the kind from
   * the path.
   */
  it('a virtual `.git` snapshot disables F2 / F7, keeping the source ops that read real content', () => {
    const [f2Rename, f3View, f4Edit, f5Copy, f6Move, f7NewFolder, f8Delete] = mountOn(
      'root',
      '/Users/david/code/.git/branches/main',
    )

    expect(f7NewFolder.disabled).toBe(true)
    expect(f2Rename.disabled).toBe(true)
    // A snapshot is git history, not a directory, but its rows are real content the
    // transfer reads through the portal.
    expect(f5Copy.disabled).toBe(false)
    expect(f6Move.disabled).toBe(false)
    expect(f8Delete.disabled).toBe(false)
    expect(f3View.disabled).toBe(false)
    expect(f4Edit.disabled).toBe(false)
  })

  it('a read-only archive (tar) disables F2 / F7, keeping copy-out', () => {
    const [f2Rename, , , f5Copy, , f7NewFolder] = mountOn('root', '/Users/david/backup.tar/etc')

    expect(f7NewFolder.disabled).toBe(true)
    expect(f2Rename.disabled).toBe(true)
    // Copying files OUT is the headline read feature; it stays.
    expect(f5Copy.disabled).toBe(false)
  })

  it('a zip keeps F2 / F7 enabled — it is the one writable archive format', () => {
    const [f2Rename, , , , , f7NewFolder] = mountOn('root', '/Users/david/bundle.zip/inner')

    expect(f7NewFolder.disabled).toBe(false)
    expect(f2Rename.disabled).toBe(false)
  })

  it('a real file under a repo dot-git folder is not a portal path and keeps the drive row', () => {
    // `.git/config` and friends are ordinary files on the parent volume: still
    // editable, renamable, deletable, which the backend defends too.
    const [f2Rename, , , , , f7NewFolder] = mountOn('root', '/Users/david/code/.git')

    expect(f7NewFolder.disabled).toBe(false)
    expect(f2Rename.disabled).toBe(false)
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
