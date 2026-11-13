/**
 * The empty state both file lists show: "Empty folder" for a folder with nothing in it,
 * or, when it only LOOKS empty because hidden files are off, how many hidden items it
 * holds and how to reveal them, with the live `view.showHidden` binding (or just the menu
 * path once the person unbinds it).
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import EmptyFolderState from './EmptyFolderState.svelte'
import { removeShortcut, resetShortcut, setShortcut } from '#lib/shortcuts/shortcuts-store.js'

describe('EmptyFolderState', () => {
  const cleanups: (() => void)[] = []

  afterEach(() => {
    for (const c of cleanups) c()
    cleanups.length = 0
    resetShortcut('view.showHidden')
  })

  function paragraphs(hiddenCount: number): () => string[] {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(EmptyFolderState, { target, props: { hiddenCount } })
    flushSync()
    cleanups.push(() => {
      void unmount(component)
      target.remove()
    })
    return () => Array.from(target.querySelectorAll('p'), (p) => p.textContent)
  }

  it('says "Empty folder" when nothing is hidden', () => {
    expect(paragraphs(0)()).toEqual(['Empty folder'])
  })

  it('names one hidden item in the singular', () => {
    setShortcut('view.showHidden', 0, '⌘⇧.')
    expect(paragraphs(1)()).toEqual([
      'There’s one hidden item in this folder.',
      'To see hidden items, press ⇧⌘. or choose “View > Show hidden files”.',
    ])
  })

  it('shows the shortcut as a key cap, so its period never reads as the sentence’s', () => {
    setShortcut('view.showHidden', 0, '⌘⇧.')
    paragraphs(2)
    const keyCaps = Array.from(document.querySelectorAll('.empty-folder-state kbd'), (k) => k.textContent)
    expect(keyCaps).toEqual(['⇧⌘.'])
  })

  it('counts several hidden items, with thousands separators', () => {
    setShortcut('view.showHidden', 0, '⌘⇧.')
    expect(paragraphs(1234)()[0]).toBe('There are 1,234 hidden items in this folder.')
  })

  it('follows a rebind live, and names only the menu once the shortcut is gone', () => {
    setShortcut('view.showHidden', 0, '⌘H')
    const read = paragraphs(3)
    expect(read()[1]).toBe('To see hidden items, press ⌘H or choose “View > Show hidden files”.')

    removeShortcut('view.showHidden', 0)
    flushSync()
    expect(read()[1]).toBe('To see hidden items, choose “View > Show hidden files”.')
  })
})
