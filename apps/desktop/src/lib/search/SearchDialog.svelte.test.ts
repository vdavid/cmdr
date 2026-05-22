/**
 * Behavior tests for `SearchDialog.svelte`.
 *
 * Pins the M1 state-preservation contract and the M2 unified-bar contract:
 *   - `⌘N` inside the dialog clears state (and the input is refocused).
 *   - Close + reopen preserves state (the dialog no longer wipes state on unmount).
 *   - `⌘1` / `⌘2` / `⌘3` switch modes; numbering shifts when AI is off.
 *   - `⌘Enter` triggers an AI search regardless of active mode (when AI is enabled).
 *   - Switching mode preserves the typed query.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SearchDialog from './SearchDialog.svelte'
import type { TranslateResult } from '$lib/ipc/bindings'
import {
  clearSearchState,
  getQuery,
  setQuery,
  getMode,
  setMode,
  getScope,
  setScope,
  getCursorIndex,
  setCursorIndex,
  getLastAiPrompt,
  getLastAiCaveat,
} from './search-state.svelte'

let aiProvider: 'off' | 'local' | 'cloud' = 'off'

// vi.mock is hoisted above all top-level `const`s; use vi.hoisted for shared mock instances.
const { translateSearchQueryMock } = vi.hoisted(() => ({
  translateSearchQueryMock: vi.fn(() => Promise.resolve({ display: {}, query: {} } as TranslateResult)),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
  searchFiles: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
  releaseSearchIndex: vi.fn(() => Promise.resolve()),
  translateSearchQuery: translateSearchQueryMock,
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve([])),
  onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return aiProvider
    return undefined
  }),
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

async function mountDialog(): Promise<{ overlay: Element; cleanup: () => void }> {
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
  // Let prepareSearchIndex resolve so isIndexReady flips and aiEnabled stabilizes.
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return {
    overlay,
    cleanup: () => {
      void unmount(component)
      target.remove()
    },
  }
}

describe('SearchDialog state preservation and ⌘N', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'off'
  })

  it('preserves state across close and reopen', async () => {
    const { cleanup } = await mountDialog()

    setQuery('*.pdf')
    setScope('~/Documents')
    setCursorIndex(3)

    cleanup()
    await tick()

    expect(getQuery()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)

    const { cleanup: cleanup2 } = await mountDialog()

    expect(getQuery()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)

    cleanup2()
  })

  it('⌘N clears state inside the dialog', async () => {
    const { overlay, cleanup } = await mountDialog()

    setQuery('*.pdf')
    setScope('~/Documents')
    setCursorIndex(5)

    dispatchKey(overlay, 'n', true)
    await tick()

    expect(getQuery()).toBe('')
    expect(getScope()).toBe('')
    expect(getCursorIndex()).toBe(0)

    cleanup()
  })
})

describe('SearchDialog mode shortcuts (AI on)', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'cloud'
    translateSearchQueryMock.mockClear()
  })

  it('⌘1 switches to AI mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '1', true)
    await tick()
    expect(getMode()).toBe('ai')
    cleanup()
  })

  it('⌘2 switches to filename mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('ai')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘3 switches to regex mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '3', true)
    await tick()
    expect(getMode()).toBe('regex')
    cleanup()
  })

  it('switching mode preserves the typed query', async () => {
    const { overlay, cleanup } = await mountDialog()
    setQuery('big files')
    setMode('ai')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('filename')
    expect(getQuery()).toBe('big files')
    cleanup()
  })

  it('⌘Enter triggers AI search regardless of active mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('large screenshots')
    dispatchKey(overlay, 'Enter', true)
    await tick()
    expect(translateSearchQueryMock).toHaveBeenCalledWith('large screenshots')
    cleanup()
  })
})

describe('SearchDialog mode shortcuts (AI off)', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'off'
    translateSearchQueryMock.mockClear()
  })

  it('⌘1 switches to filename when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('regex')
    dispatchKey(overlay, '1', true)
    await tick()
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘2 switches to regex when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('regex')
    cleanup()
  })

  it('⌘3 is a no-op when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '3', true)
    await tick()
    // mode stayed put
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘Enter does not call AI when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setQuery('whatever')
    dispatchKey(overlay, 'Enter', true)
    await tick()
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    cleanup()
  })
})

describe('SearchDialog AI transparency strip', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'cloud'
    translateSearchQueryMock.mockReset()
  })

  async function flushAi(): Promise<void> {
    // The AI flow chains a few microtasks: translateSearchQuery -> applyAiFilters -> executeSearch.
    // Resolve all of them so the strip stabilizes before we assert.
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
  }

  it('appears after an AI run and shows the prompt + caveat', async () => {
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.png', patternType: 'glob' },
      query: {},
      caveat: "I treated 'big' as larger than 10 MB.",
    } as unknown as TranslateResult)
    const { overlay, cleanup } = await mountDialog()
    setQuery('big screenshots')
    setMode('ai')
    dispatchKey(overlay, 'Enter')
    await flushAi()

    expect(getLastAiPrompt()).toBe('big screenshots')
    expect(getLastAiCaveat()).toBe("I treated 'big' as larger than 10 MB.")

    const strip = document.body.querySelector('.ai-transparency-strip')
    expect(strip).not.toBeNull()
    expect(strip?.querySelector('.ai-prompt')?.textContent).toBe('big screenshots')
    expect(strip?.querySelector('.ai-caveat')?.textContent).toBe("I treated 'big' as larger than 10 MB.")

    cleanup()
  })

  it('hides on ⌘N (clear search state)', async () => {
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.pdf', patternType: 'glob' },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)
    const { overlay, cleanup } = await mountDialog()
    setQuery('pdfs from this week')
    setMode('ai')
    dispatchKey(overlay, 'Enter')
    await flushAi()
    expect(getLastAiPrompt()).toBe('pdfs from this week')

    dispatchKey(overlay, 'n', true)
    await tick()
    expect(getLastAiPrompt()).toBeNull()
    expect(document.body.querySelector('.ai-transparency-strip')).toBeNull()

    cleanup()
  })

  it('hides when a non-AI search runs successfully', async () => {
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.pdf', patternType: 'glob' },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)
    const { overlay, cleanup } = await mountDialog()
    setQuery('pdfs from this week')
    setMode('ai')
    dispatchKey(overlay, 'Enter')
    await flushAi()
    expect(getLastAiPrompt()).toBe('pdfs from this week')

    // Switch to filename mode and run a manual search.
    setMode('filename')
    setQuery('*.txt')
    dispatchKey(overlay, 'Enter')
    await flushAi()

    expect(getLastAiPrompt()).toBeNull()
    expect(getLastAiCaveat()).toBeNull()
    expect(document.body.querySelector('.ai-transparency-strip')).toBeNull()

    cleanup()
  })
})
