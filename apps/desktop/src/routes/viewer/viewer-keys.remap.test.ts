/**
 * The viewer's single keys come from the shortcuts registry (`Viewer window`, #441), so a
 * rebinding in Settings moves them and an emptied list turns them off (WCAG 2.1.4). The store
 * is stubbed with a per-test override map over the real registry defaults.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'

const overrides = vi.hoisted(() => new Map<string, string[]>())

vi.mock('#lib/shortcuts/shortcuts-store.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/shortcuts/shortcuts-store.js')>()
  // Overrides are in the platform's own spelling, as the real store hands them out (this suite
  // runs on the fallback platform, Linux).
  return {
    ...actual,
    getEffectiveShortcuts: (id: string) => overrides.get(id) ?? actual.getDefaultShortcuts(id),
  }
})

import { createViewerKeyboard, handleTailToggleKey, handleToggleKey } from './viewer-keyboard'
import { modeForKey } from './viewer-view-mode'
import MediaImageView from './MediaImageView.svelte'

const key = (k: string, mods: KeyboardEventInit = {}) =>
  new KeyboardEvent('keydown', { key: k, cancelable: true, ...mods })

afterEach(() => {
  overrides.clear()
})

describe('viewer keys follow the shortcuts registry', () => {
  it('word wrap moves to its new key and leaves W alone', () => {
    overrides.set('viewer.toggleWordWrap', ['Ctrl+W'])
    const toggle = vi.fn()

    expect(handleToggleKey(key('w'), toggle)).toBe(false)
    expect(handleToggleKey(key('w', { ctrlKey: true, code: 'KeyW' }), toggle)).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('a rebinding that carries a modifier still reaches word wrap through the full router', () => {
    overrides.set('viewer.toggleWordWrap', ['Ctrl+W'])
    const toggleWordWrap = vi.fn()
    const keyboard = createViewerKeyboard(makeDeps({ toggleWordWrap }))

    keyboard.handleKeyDown(key('w', { ctrlKey: true, code: 'KeyW' }))

    expect(toggleWordWrap).toHaveBeenCalledOnce()
  })

  it('an emptied list turns tail mode off', () => {
    overrides.set('viewer.toggleTail', [])
    const toggle = vi.fn()

    expect(handleTailToggleKey(key('f'), toggle)).toBe(false)
    expect(toggle).not.toHaveBeenCalled()
  })

  it('the view-mode keys follow their rebindings', () => {
    overrides.set('viewer.viewAsHex', ['H'])

    expect(modeForKey(key('h'), false)).toBe('hex')
    expect(modeForKey(key('3'), false)).toBeNull()
  })

  it("the image's zoom keys follow their rebindings", () => {
    overrides.set('viewer.zoomIn', ['I'])
    overrides.set('viewer.zoomOut', [])
    const target = document.createElement('div')
    document.body.appendChild(target)
    const view = mount(MediaImageView, { target, props: { src: 'asset://x.png', fileName: 'x.png' } }) as {
      handleKey: (e: KeyboardEvent) => boolean
    }

    expect(view.handleKey(key('+'))).toBe(false)
    expect(view.handleKey(key('-'))).toBe(false)
    expect(view.handleKey(key('i'))).toBe(true)
    flushSync()
    expect(target.querySelector('img')?.style.transform).toContain('scale(1.25)')

    void unmount(view as never)
    target.remove()
  })
})

type KeyboardDeps = Parameters<typeof createViewerKeyboard>[0]

function makeDeps(overridesForDeps: Partial<KeyboardDeps>): KeyboardDeps {
  const noop = vi.fn()
  return {
    getTotalRows: () => 10,
    getTotalBytes: () => 100,
    getRowText: () => 'line',
    getLastRenderedRow: () => 9,
    selection: { selection: null, selectAll: noop, selectToEof: noop, setFocus: noop },
    scroll: {
      scrollByRows: noop,
      scrollByPages: noop,
      scrollToStart: noop,
      scrollToEnd: noop,
      scrollByColumns: noop,
      ensureRowVisible: noop,
      ensureColumnVisible: noop,
    },
    search: {
      searchVisible: false,
      searchStatus: 'idle',
      searchInputRef: null,
      openSearch: noop,
      closeSearch: noop,
      stopSearch: noop,
      findNext: noop,
      findPrev: noop,
      toggleUseRegex: noop,
      toggleCaseSensitive: noop,
    },
    copy: { busy: false, cancelInFlight: () => Promise.resolve() },
    isCopyConfirmOpen: () => false,
    isCopyRefuseOpen: () => false,
    cancelCopyConfirm: noop,
    dismissCopyRefuse: noop,
    logEscape: noop,
    runCopy: noop,
    toggleTailMode: noop,
    toggleWordWrap: noop,
    closeWindow: noop,
    ...overridesForDeps,
  }
}
