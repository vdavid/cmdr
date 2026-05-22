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
let autoApplySetting = true
const autoApplyListeners = new Set<(id: string, value: boolean) => void>()

// vi.mock is hoisted above all top-level `const`s; use vi.hoisted for shared mock instances.
const { translateSearchQueryMock, searchFilesMock } = vi.hoisted(() => ({
  translateSearchQueryMock: vi.fn(() => Promise.resolve({ display: {}, query: {} } as TranslateResult)),
  searchFilesMock: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
  searchFiles: searchFilesMock,
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
    if (key === 'search.autoApply') return autoApplySetting
    return undefined
  }),
  onSpecificSettingChange: vi.fn((id: string, listener: (id: string, value: boolean) => void) => {
    if (id !== 'search.autoApply') return () => {}
    autoApplyListeners.add(listener)
    return () => autoApplyListeners.delete(listener)
  }),
}))

/** Test helper: simulate a settings.json change for `search.autoApply` and notify subscribers. */
function setAutoApplyForTest(value: boolean): void {
  autoApplySetting = value
  for (const listener of autoApplyListeners) listener('search.autoApply', value)
}

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
    autoApplySetting = true
    autoApplyListeners.clear()
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
    autoApplySetting = true
    autoApplyListeners.clear()
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
    autoApplySetting = true
    autoApplyListeners.clear()
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
    autoApplySetting = true
    autoApplyListeners.clear()
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

describe('SearchDialog auto-apply (M6)', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'off'
    autoApplySetting = true
    autoApplyListeners.clear()
    searchFilesMock.mockClear()
    translateSearchQueryMock.mockClear()
  })

  it('fires exactly one search after the 1 s debounce when typing in filename mode', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()

      // The dialog's `handleQueryInput` calls `setQuery` + `scheduleSearch`. We simulate a few
      // keystrokes back to back, each resetting the debounce timer.
      const input = document.body.querySelector('input.query-input') as HTMLInputElement
      input.value = 'p'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'ph'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'pho'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      // 200 ms (the old debounce) is not enough; 1000 ms is.
      vi.advanceTimersByTime(200)
      expect(searchFilesMock).not.toHaveBeenCalled()
      vi.advanceTimersByTime(900)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not auto-apply when search.autoApply is off', async () => {
    autoApplySetting = false
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('input.query-input') as HTMLInputElement
      input.value = '*.pdf'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      // Even far past the debounce window, nothing fires automatically.
      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(searchFilesMock).not.toHaveBeenCalled()
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('live-applies a setting toggle from on to off and back', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('input.query-input') as HTMLInputElement

      // 1) Auto-apply on: type, advance 1 s, search fires.
      input.value = '*.pdf'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(1_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)

      // 2) Toggle the setting off. Subsequent typing must not auto-apply.
      setAutoApplyForTest(false)
      input.value = '*.txt'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)

      // 3) Toggle the setting back on. The next keystroke does auto-apply.
      setAutoApplyForTest(true)
      input.value = '*.txt!'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(1_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(2)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not auto-apply in AI mode regardless of the setting', async () => {
    aiProvider = 'cloud'
    autoApplySetting = true
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      translateSearchQueryMock.mockClear()
      setMode('ai')

      const input = document.body.querySelector('input.query-input') as HTMLInputElement
      input.value = 'big screenshots'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(translateSearchQueryMock).not.toHaveBeenCalled()
      expect(searchFilesMock).not.toHaveBeenCalled()
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('suppresses auto-apply during IME composition and fires exactly once on compositionend', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('input.query-input') as HTMLInputElement

      // Start a composition. Each `input` during composition is one keystroke; we mustn't fire.
      input.dispatchEvent(new CompositionEvent('compositionstart'))
      input.value = 'ｐ'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'ｐｈ'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      expect(searchFilesMock).not.toHaveBeenCalled()

      // End composition: the parent resets the debounce and we should get exactly one fire after
      // SEARCH_AUTO_APPLY_DEBOUNCE_MS.
      input.dispatchEvent(new CompositionEvent('compositionend'))
      vi.advanceTimersByTime(999)
      expect(searchFilesMock).not.toHaveBeenCalled()
      vi.advanceTimersByTime(1)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('clicking the ⏎ run button triggers a search in filename mode', async () => {
    autoApplySetting = false
    const { cleanup } = await mountDialog()
    searchFilesMock.mockClear()
    setQuery('*.pdf')
    await tick()

    const runButton = document.body.querySelector('button.run-button') as HTMLButtonElement
    expect(runButton).not.toBeNull()
    runButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(searchFilesMock).toHaveBeenCalledTimes(1)
    cleanup()
  })

  it('clicking the ⏎ run button triggers an AI search in AI mode', async () => {
    aiProvider = 'cloud'
    autoApplySetting = true
    translateSearchQueryMock.mockResolvedValueOnce({
      display: {},
      query: {},
    } as TranslateResult)
    const { cleanup } = await mountDialog()
    translateSearchQueryMock.mockClear()
    setMode('ai')
    setQuery('large screenshots')
    await tick()

    const runButton = document.body.querySelector('button.run-button') as HTMLButtonElement
    runButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(translateSearchQueryMock).toHaveBeenCalledWith('large screenshots')
    cleanup()
  })

  it('shows the "Press Enter to search" hint when auto-apply is off and the query changed', async () => {
    autoApplySetting = false
    const { cleanup } = await mountDialog()
    setQuery('photos')
    await tick()

    const hint = document.body.querySelector('.run-hint')
    expect(hint).not.toBeNull()
    expect(hint?.textContent).toMatch(/Press Enter to search/i)
    cleanup()
  })

  it('shows the hint in AI mode (even with auto-apply on) when the query is unsent', async () => {
    aiProvider = 'cloud'
    autoApplySetting = true
    const { cleanup } = await mountDialog()
    setMode('ai')
    setQuery('big files this week')
    await tick()

    const hint = document.body.querySelector('.run-hint')
    expect(hint).not.toBeNull()
    cleanup()
  })

  it('hides the hint when auto-apply is on and mode is filename/regex', async () => {
    autoApplySetting = true
    const { cleanup } = await mountDialog()
    setMode('filename')
    setQuery('*.pdf')
    await tick()

    expect(document.body.querySelector('.run-hint')).toBeNull()
    cleanup()
  })
})
