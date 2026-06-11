/**
 * Behavior tests for `SearchDialog.svelte`.
 *
 * Pins:
 *   - `⌘N` inside the dialog clears state (and the input is refocused).
 *   - Close + reopen preserves state (the dialog doesn't wipe state on unmount).
 *   - `⌘1` / `⌘2` / `⌘3` switch modes; numbering shifts when AI is off.
 *   - `⌘Enter` triggers an AI search regardless of active mode (when AI is enabled).
 *   - Switching mode preserves the typed query.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
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
  getLastAiPattern,
  getLastAiPatternKind,
  getLastAiLabel,
  getSizeFilter,
} from './search-state.svelte'

let aiProvider: 'off' | 'local' | 'cloud' = 'off'
let autoApplySetting = true
const autoApplyListeners = new Set<(id: string, value: boolean) => void>()

// vi.mock is hoisted above all top-level `const`s; use vi.hoisted for shared mock instances.
const { translateSearchQueryMock, searchFilesMock, addRecentSearchMock } = vi.hoisted(() => ({
  translateSearchQueryMock: vi.fn(() => Promise.resolve({ display: {}, query: {} } as TranslateResult)),
  searchFilesMock: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
  addRecentSearchMock: vi.fn(() => Promise.resolve()),
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
  getRecentSearches: vi.fn(() => Promise.resolve([])),
  addRecentSearch: addRecentSearchMock,
  removeRecentSearch: vi.fn(() => Promise.resolve()),
  clearRecentSearches: vi.fn(() => Promise.resolve()),
  applyRecentSearchesMaxCount: vi.fn(() => Promise.resolve()),
  showFileContextMenu: vi.fn(() => Promise.resolve()),
  showInFinder: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(() => Promise.resolve()),
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

function dispatchKey(target: Element, key: string, meta = false, shift = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: meta,
    shiftKey: shift,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

interface MountDialogOptions {
  onClose?: () => void
  onShowAllInMainWindow?: (snapshotId: string) => void
}

/**
 * Tracks every mounted dialog so a per-test `afterEach` can tear down anything
 * the test forgot (or never reached) to clean up. Without this, a failing
 * assertion before `cleanup()` leaves the dialog in `document.body`, and the
 * NEXT test's input events route to the stale dialog (which then quietly
 * fires `scheduleSearch` / `executeQuery` with its old `autoApplyEnabled`,
 * triggering hard-to-diagnose cascade failures).
 */
const liveDialogs: { component: ReturnType<typeof mount>; target: HTMLDivElement }[] = []

afterEach(() => {
  while (liveDialogs.length > 0) {
    const entry = liveDialogs.pop()
    if (!entry) break
    try {
      void unmount(entry.component)
    } catch {
      /* component may already be gone if the test called cleanup() */
    }
    entry.target.remove()
  }
})

async function mountDialog(opts: MountDialogOptions = {}): Promise<{ overlay: Element; cleanup: () => void }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SearchDialog, {
    target,
    props: {
      onNavigate: () => {},
      onClose: opts.onClose ?? (() => {}),
      searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
      onShowAllInMainWindow: opts.onShowAllInMainWindow,
    },
  })
  const entry = { component, target }
  liveDialogs.push(entry)
  await tick()
  // Let prepareSearchIndex resolve so isIndexReady flips and aiEnabled stabilizes.
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return {
    overlay,
    cleanup: () => {
      const idx = liveDialogs.indexOf(entry)
      if (idx >= 0) liveDialogs.splice(idx, 1)
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

  it("switching mode swaps the bar to the target mode's hand-typed buffer (carrying into an empty target)", async () => {
    // Each mode owns its own input buffer. Switching from AI to filename restores filename's
    // last hand-typed value; when that buffer is empty, the outgoing term carries across so
    // the user's words don't vanish (M5 carry-over). The AI prompt stays available via
    // `getLastAiPrompt()` for the transparency strip regardless.
    const { overlay, cleanup } = await mountDialog()
    setMode('ai')
    setQuery('big files')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('filename')
    // Filename's buffer was empty, so the outgoing 'big files' carries into the bar.
    expect(getQuery()).toBe('big files')
    cleanup()
  })

  // R4: ⌘⏎ and ⇧⏎ are no-ops in the search dialog. Bare Enter is the only path
  // that runs a search or opens the cursor row. The earlier "⌘Enter triggers AI"
  // shortcut is gone per David's request.
  it('R4: ⌘Enter is a no-op (does not run AI even when AI is enabled)', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('large screenshots')
    dispatchKey(overlay, 'Enter', true)
    await tick()
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    expect(searchFilesMock).not.toHaveBeenCalled()
    cleanup()
  })

  it('R4: ⇧Enter is a no-op (does not run a search)', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('foo')
    dispatchKey(overlay, 'Enter', false, true)
    await tick()
    expect(searchFilesMock).not.toHaveBeenCalled()
    cleanup()
  })

  it('R4: bare Enter still runs the search', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('foo')
    dispatchKey(overlay, 'Enter')
    await tick()
    expect(searchFilesMock).toHaveBeenCalled()
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

  // Regression: the previous `translateAi` was a stub that fired the IPC and threw the
  // result away. Tests only asserted the IPC was CALLED, so the stub passed. This asserts
  // the translated pattern, label, and size filter actually land in Search state.
  it('applies the AI-translated pattern, label, and size filter (not just calls the IPC)', async () => {
    translateSearchQueryMock.mockResolvedValueOnce({
      display: {
        namePattern: '*.png',
        patternType: 'glob',
        minSize: 10 * 1024 * 1024,
        maxSize: null,
      },
      query: { caseSensitive: null, excludeSystemDirs: null },
      caveat: null,
      label: 'Big screenshots',
    } as unknown as TranslateResult)
    const { overlay, cleanup } = await mountDialog()
    setQuery('big screenshots')
    setMode('ai')
    dispatchKey(overlay, 'Enter')
    await flushAi()

    expect(getLastAiPattern()).toBe('*.png')
    expect(getLastAiPatternKind()).toBe('glob')
    expect(getLastAiLabel()).toBe('Big screenshots')
    expect(getSizeFilter()).toBe('gte')

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

describe('SearchDialog auto-apply', () => {
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

    // Second arg is the current type filter as context (both → null at the start). (M6)
    expect(translateSearchQueryMock).toHaveBeenCalledWith('large screenshots', null)
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

describe('SearchDialog path-pill navigation shortcuts', () => {
  beforeEach(() => {
    clearSearchState()
    aiProvider = 'off'
    autoApplySetting = true
    autoApplyListeners.clear()
    searchFilesMock.mockReset()
  })

  function dispatchAltKey(target: Element, key: string): KeyboardEvent {
    const event = new KeyboardEvent('keydown', {
      key,
      altKey: true,
      bubbles: true,
      cancelable: true,
    })
    target.dispatchEvent(event)
    return event
  }

  async function seedResultsAndMount(): Promise<{ overlay: Element; navigated: string[]; cleanup: () => void }> {
    // searchFilesMock's inferred resolved type is `{ entries: never[]; totalCount: number }`
    // (since the default mock returns an empty array literal). Cast to the broader shape
    // expected at runtime so the seeded row's fields type-check.
    searchFilesMock.mockResolvedValueOnce({
      entries: [
        {
          name: 'photo.jpg',
          path: '/Users/test/pictures/photo.jpg',
          parentPath: '/Users/test/pictures',
          isDirectory: false,
          size: 1000,
          modifiedAt: 1_700_000_000,
          iconId: 'ext:jpg',
        },
      ],
      totalCount: 1,
    } as Awaited<ReturnType<typeof searchFilesMock>>)

    const navigated: string[] = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(SearchDialog, {
      target,
      props: {
        onNavigate: (path: string) => {
          navigated.push(path)
        },
        onClose: () => {},
        searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
      },
    })
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()

    // Drive a search to populate results + set cursor to row 0.
    setQuery('photo*')
    setMode('filename')
    const overlay = target.querySelector('.search-overlay') as Element
    dispatchKey(overlay, 'Enter')
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    setCursorIndex(0)

    return {
      overlay,
      navigated,
      cleanup: () => {
        void unmount(component)
        target.remove()
      },
    }
  }

  it("⌥← navigates to the cursor row file's parent folder", async () => {
    const { overlay, navigated, cleanup } = await seedResultsAndMount()
    dispatchAltKey(overlay, 'ArrowLeft')
    await tick()
    expect(navigated).toEqual(['/Users/test/pictures'])
    cleanup()
  })

  it("⌥→ navigates to the cursor row's own path (descend back)", async () => {
    const { overlay, navigated, cleanup } = await seedResultsAndMount()
    dispatchAltKey(overlay, 'ArrowRight')
    await tick()
    expect(navigated).toEqual(['/Users/test/pictures/photo.jpg'])
    cleanup()
  })

  it('⌥← and ⌥→ are no-ops when there are no results', async () => {
    const navigated: string[] = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(SearchDialog, {
      target,
      props: {
        onNavigate: (p: string) => {
          navigated.push(p)
        },
        onClose: () => {},
        searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
      },
    })
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    const overlay = target.querySelector('.search-overlay') as Element
    dispatchAltKey(overlay, 'ArrowLeft')
    dispatchAltKey(overlay, 'ArrowRight')
    await tick()
    expect(navigated).toEqual([])
    void unmount(component)
    target.remove()
  })
})

describe('SearchDialog "Open in pane" (M8b)', () => {
  beforeEach(async () => {
    clearSearchState()
    aiProvider = 'off'
    autoApplySetting = true
    autoApplyListeners.clear()
    addRecentSearchMock.mockClear()
    // Reset the snapshot store so each test sees a fresh `sr-1` id.
    const { _resetForTesting } = await import('./snapshot-store.svelte')
    _resetForTesting()
  })

  async function seedResults(): Promise<void> {
    const { setResults, setTotalCount } = await import('./search-state.svelte')
    setResults([
      {
        name: 'doc.pdf',
        path: '/Users/test/docs/doc.pdf',
        parentPath: '/Users/test/docs',
        isDirectory: false,
        size: 1024,
        modifiedAt: 1_700_000_000,
        iconId: 'ext:pdf',
      },
    ])
    setTotalCount(1)
  }

  it('calls onOpenInPane with the new snapshot id, persists to recent searches, and closes the dialog', async () => {
    let openedId: string | null = null
    let closed = false
    const { cleanup } = await mountDialog({
      onClose: () => {
        closed = true
      },
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    setQuery('*.pdf')
    setMode('filename')
    await seedResults()
    await tick()

    // Find and click the "Open in pane" footer button.
    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    expect(btn).not.toBeNull()
    btn.click()
    // Let the (sync) handler run and any micro-tasks resolve.
    await tick()
    await Promise.resolve()

    expect(openedId).toMatch(/^sr-\d+$/)
    expect(closed).toBe(true)
    expect(addRecentSearchMock).toHaveBeenCalledTimes(1)
    const firstCall = addRecentSearchMock.mock.calls[0] as unknown[] | undefined
    expect(firstCall).toBeDefined()
    const entry = firstCall?.[0] as { mode: string; query: string; resultCount: number }
    expect(entry.mode).toBe('filename')
    expect(entry.query).toBe('*.pdf')
    expect(entry.resultCount).toBe(1)

    cleanup()
  })

  it('stores the snapshot in the snapshot store under the returned id', async () => {
    let openedId: string | null = null
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    setQuery('foo')
    setMode('filename')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    btn.click()
    await tick()

    const { getSnapshot, getLastAttemptId } = await import('./snapshot-store.svelte')
    expect(openedId).not.toBeNull()
    // `openedId` is mutated through the onShowAllInMainWindow callback above; TS
    // narrowing doesn't follow that, so we assert non-null after the expect.
    const snap = getSnapshot(openedId as unknown as string)
    expect(snap).toBeDefined()
    expect(snap?.mode).toBe('filename')
    expect(snap?.entries.length).toBe(1)
    // The "last attempt" slot is pinned to the new id (refcount-wise).
    expect(getLastAttemptId()).toBe(openedId as unknown as string)

    cleanup()
  })

  it('uses the original AI prompt for the snapshot label when in AI mode', async () => {
    aiProvider = 'cloud'
    let openedId: string | null = null
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    const { setLastAiPrompt } = await import('./search-state.svelte')
    setMode('ai')
    setQuery('*.pdf') // AI translation overwrote the natural-language query
    setLastAiPrompt('find my pdf invoices')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    btn.click()
    await tick()

    const { getSnapshot } = await import('./snapshot-store.svelte')
    expect(openedId).not.toBeNull()
    const snap = getSnapshot(openedId as unknown as string)
    expect(snap?.label).toBe('find my pdf invoices')

    cleanup()
  })

  it('renders the Show all button disabled and does nothing when there are no results', async () => {
    let opened = false
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: () => {
        opened = true
      },
    })
    // No results seeded.
    await tick()
    // The button stays VISIBLE when resultCount === 0, just rendered disabled. Yanking
    // it would jump the layout while the user is mid-thought.
    const btn = document.body.querySelector<HTMLButtonElement>('button[aria-label="Show all in main window"]')
    expect(btn).not.toBeNull()
    expect(btn?.disabled).toBe(true)
    btn?.click()
    expect(opened).toBe(false)
    cleanup()
  })
})
