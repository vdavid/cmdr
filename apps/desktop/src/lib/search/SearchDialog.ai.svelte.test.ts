/**
 * The AI transparency strip and the filter writes behind it.
 *
 * Pins:
 *   - The strip appears after an AI run and shows the prompt + caveat, and hides on `⌘N`
 *     or on the next successful non-AI run.
 *   - A translation actually LANDS in Search state (pattern, label, size), rather than the
 *     IPC merely being called: the earlier stub fired the call and threw the answer away,
 *     and every "was it called" assertion passed over it.
 *   - Size / date reset before each AI run; type is leave-alone-if-null.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import type { TranslateResult } from '$lib/ipc/bindings'
import {
  setQuery,
  setMode,
  getLastAiPrompt,
  getLastAiCaveat,
  getLastAiPattern,
  getLastAiPatternKind,
  getLastAiLabel,
  getSizeFilter,
  searchQueryState,
} from './search-state.svelte'
import {
  dispatchKey,
  flushAi,
  mountDialog,
  resetSearchDialogTest,
  translateSearchQueryMock,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog AI transparency strip', () => {
  beforeEach(async () => {
    await resetSearchDialogTest({ aiProvider: 'cloud' })
    translateSearchQueryMock.mockReset()
  })

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

  it('a second AI run does not let a leftover size filter survive, but keeps a type the first run set', async () => {
    // Regression: `applyAiSharedFilters` must reset size + date to `any` before applying the
    // AI's bounds, the way Selection does. `applySizeFromAi` / `applyDateFromAi` no-op when the
    // AI returns no bound, so without the reset a first run's size filter (≥ 5 MB) would silently
    // survive a second run that returns no size. Type is the deliberate asymmetry: when the AI
    // omits type, the user's current choice (set here by run #1) must NOT be reset.
    // First run: ≥ 5 MB size + folders-only type.
    translateSearchQueryMock.mockResolvedValueOnce({
      display: {
        namePattern: '*.pdf',
        patternType: 'glob',
        minSize: 5 * 1024 * 1024,
        maxSize: null,
        isDirectory: true,
      },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)
    // Second run: a different pattern, NO size, NO type (the AI stayed silent on both).
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.txt', patternType: 'glob', minSize: null, maxSize: null, isDirectory: null },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)

    const { overlay, cleanup } = await mountDialog()
    setMode('ai')

    // First AI run.
    setQuery('big pdf folders')
    dispatchKey(overlay, 'Enter')
    await flushAi()
    expect(getSizeFilter()).toBe('gte')
    expect(searchQueryState.getTypeFilter()).toBe('folder')

    // Second AI run: omits size and type.
    setQuery('text files')
    dispatchKey(overlay, 'Enter')
    await flushAi()

    // Size must be back to `any` (the first run's ≥ 5 MB must NOT leak through).
    expect(getSizeFilter()).toBe('any')
    // Type must be untouched: the AI omitting type keeps the user's (run #1's) folder choice.
    expect(searchQueryState.getTypeFilter()).toBe('folder')

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
