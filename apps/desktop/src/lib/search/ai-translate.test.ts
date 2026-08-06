/**
 * Where an AI translation LANDS.
 *
 * The regression that made these worth writing: the callback used to fire the IPC and
 * throw the answer away, and every "was translate called?" assertion passed over it. So
 * each test here asserts the STATE after a translation, and the returned changed-field
 * list, never the call alone.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TranslateResult } from '$lib/ipc/bindings'

const { translateSearchQueryMock } = vi.hoisted(() => ({
  translateSearchQueryMock: vi.fn(() => Promise.resolve({ display: {}, query: {} } as TranslateResult)),
}))

vi.mock('$lib/tauri-commands', () => ({ translateSearchQuery: translateSearchQueryMock }))

import { applyAiTranslationToState, applyScopeFromAi, patternKindFromDisplay, translateAi } from './ai-translate'
import {
  clearSearchState,
  getCaseSensitive,
  getExcludeSystemDirs,
  getLastAiLabel,
  getLastAiPattern,
  getLastAiPatternKind,
  getScope,
  getSizeFilter,
  getDateFilter,
  searchQueryState,
} from './search-state.svelte'

/** A translate answer with only the fields a test cares about. */
function answer(overrides: { display?: Record<string, unknown>; query?: Record<string, unknown>; label?: string }) {
  return {
    display: overrides.display ?? {},
    query: overrides.query ?? {},
    caveat: null,
    ...(overrides.label === undefined ? {} : { label: overrides.label }),
  } as unknown as TranslateResult
}

beforeEach(() => {
  clearSearchState()
  translateSearchQueryMock.mockReset()
})

describe('patternKindFromDisplay', () => {
  it('recovers the two structured kinds and treats everything else as absent', () => {
    expect(patternKindFromDisplay('glob')).toBe('glob')
    expect(patternKindFromDisplay('regex')).toBe('regex')
    expect(patternKindFromDisplay(null)).toBeNull()
    expect(patternKindFromDisplay(undefined)).toBeNull()
    expect(patternKindFromDisplay('literal')).toBeNull()
  })
})

describe('applyScopeFromAi', () => {
  it('folds includes and excludes into one comma-separated expression', () => {
    expect(applyScopeFromAi(['~/Documents', '~/Desktop'], ['node_modules'])).toBe(true)
    expect(getScope()).toBe('~/Documents, ~/Desktop, !node_modules')
  })

  it('leaves the scope alone when the AI named neither', () => {
    expect(applyScopeFromAi(null, [])).toBe(false)
    expect(getScope()).toBe('')
  })
})

describe('applyAiTranslationToState', () => {
  it('writes the pattern, its kind, and the label into their own slots', () => {
    const changed = applyAiTranslationToState(
      answer({ display: { namePattern: '*.png', patternType: 'glob' }, label: 'Screenshots' }),
    )
    expect(getLastAiPattern()).toBe('*.png')
    expect(getLastAiPatternKind()).toBe('glob')
    expect(getLastAiLabel()).toBe('Screenshots')
    expect(changed).toContain('pattern')
  })

  it('writes case sensitivity and only ever turns the system-dir exclusion OFF', () => {
    const changed = applyAiTranslationToState(
      answer({ query: { caseSensitive: true, excludeSystemDirs: false } }),
    )
    expect(getCaseSensitive()).toBe(true)
    expect(getExcludeSystemDirs()).toBe(false)
    expect(changed).toEqual(expect.arrayContaining(['caseSensitive', 'excludeSystemDirs']))
  })

  it('resets size and date before applying, so a previous run cannot leak through', () => {
    applyAiTranslationToState(answer({ display: { minSize: 5 * 1024 * 1024 } }))
    expect(getSizeFilter()).toBe('gte')

    // The second answer names no size at all: the chip must go back to `any` rather than
    // keep the first run's bound (`applySizeFromAi` no-ops on a null bound).
    const changed = applyAiTranslationToState(answer({ display: { namePattern: '*.txt' } }))
    expect(getSizeFilter()).toBe('any')
    expect(getDateFilter()).toBe('any')
    expect(changed).not.toContain('size')
  })

  it('leaves the type filter alone when the AI stays silent about it', () => {
    applyAiTranslationToState(answer({ display: { isDirectory: true } }))
    expect(searchQueryState.getTypeFilter()).toBe('folder')

    // Deliberately NOT reset-first like size/date: an omitted type keeps the user's choice.
    applyAiTranslationToState(answer({ display: { namePattern: '*.txt' } }))
    expect(searchQueryState.getTypeFilter()).toBe('folder')
  })
})

describe('translateAi', () => {
  it('hands the current type in as context and returns the caveat plus the changed chips', async () => {
    searchQueryState.setTypeFilter('file')
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.pdf', patternType: 'glob' },
      query: {},
      caveat: 'I looked at PDFs only.',
    } as unknown as TranslateResult)

    const result = await translateAi('my pdfs')

    expect(translateSearchQueryMock).toHaveBeenCalledWith('my pdfs', false)
    expect(result?.caveat).toBe('I looked at PDFs only.')
    expect(result?.highlightedFields).toContain('pattern')
    expect(getLastAiPattern()).toBe('*.pdf')
  })

  it('lets the IPC error through, so QueryDialog can say which one it was', async () => {
    translateSearchQueryMock.mockRejectedValueOnce(new Error('quota'))
    await expect(translateAi('anything')).rejects.toThrow('quota')
  })
})
