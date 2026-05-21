/**
 * Behavior tests for `SearchDialog.svelte`.
 *
 * Covers M1's two new contracts:
 *   1. `⌘N` inside the dialog clears state (and the active input is refocused).
 *   2. Close + reopen preserves state (the dialog no longer wipes state on unmount).
 *
 * State preservation is the load-bearing change behind the new "search-state stays alive
 * across dialog close/reopen" UX. The module-level `$state` in `search-state.svelte.ts`
 * already outlives the component; we just verify nothing in the dialog secretly wipes it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SearchDialog from './SearchDialog.svelte'
import {
  clearSearchState,
  getNamePattern,
  setNamePattern,
  getScope,
  setScope,
  getAiPrompt,
  setAiPrompt,
  getCursorIndex,
  setCursorIndex,
} from './search-state.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
  searchFiles: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
  releaseSearchIndex: vi.fn(() => Promise.resolve()),
  translateSearchQuery: vi.fn(() => Promise.resolve({ display: {}, query: {} })),
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve([])),
  onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 'off'),
}))

vi.mock('$lib/indexing', () => ({
  isScanning: vi.fn(() => false),
  getEntriesScanned: vi.fn(() => 0),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

function dispatchKey(target: Element, key: string, meta = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: meta,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

describe('SearchDialog state preservation and ⌘N', () => {
  beforeEach(() => {
    clearSearchState()
  })

  it('preserves state across close and reopen', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)

    const component = mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    await tick()

    setNamePattern('*.pdf')
    setScope('~/Documents')
    setCursorIndex(3)

    void unmount(component)
    target.remove()
    await tick()

    expect(getNamePattern()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)

    const target2 = document.createElement('div')
    document.body.appendChild(target2)
    mount(SearchDialog, {
      target: target2,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    await tick()

    expect(getNamePattern()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)
  })

  it('⌘N clears state inside the dialog', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)

    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    await tick()

    setNamePattern('*.pdf')
    setScope('~/Documents')
    setAiPrompt('large screenshots')
    setCursorIndex(5)

    const overlay = target.querySelector('.search-overlay')
    expect(overlay).toBeTruthy()
    dispatchKey(overlay as Element, 'n', true)
    await tick()

    expect(getNamePattern()).toBe('')
    expect(getScope()).toBe('')
    expect(getAiPrompt()).toBe('')
    expect(getCursorIndex()).toBe(0)
  })
})
