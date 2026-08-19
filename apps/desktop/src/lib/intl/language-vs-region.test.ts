/**
 * Language and region are two settings, and this is the test that says so.
 *
 * macOS keeps them apart (System Settings > General > Language & Region), and
 * so does the app: a Hungarian speaker living in Sweden reads Hungarian copy
 * over Swedish dates and Swedish number grouping. Picking a UI language is not
 * permission to overwrite the conventions they chose in System Settings.
 *
 * The scenario below is exactly that person: UI language `hu`, OS formatting
 * locale `sv-SE`. The Size and Modified column HEADERS come out Hungarian
 * ("Méret", "Módosítva") while the cells under them come out Swedish.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { _setFormatLocaleForTests, _setLocaleForTests, getFormatLocale, getUiLocale } from './locale'
import { setLocale, tString } from './messages.svelte'
import { formatDateForDisplay } from '$lib/settings/format-utils'
import { formatFileSizeWithFormat } from '$lib/units'
import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'

describe('a Hungarian speaker on a Swedish Mac', () => {
  beforeEach(() => {
    _setLocaleForTests('hu')
    _setFormatLocaleForTests('sv-SE')
  })
  afterEach(() => {
    _setLocaleForTests(null)
    _setFormatLocaleForTests(null)
  })

  it('reads the two locales apart', () => {
    expect(getUiLocale()).toBe('hu')
    expect(getFormatLocale()).toBe('sv-SE')
  })

  it('shows Hungarian copy', () => {
    expect(tString('fileExplorer.columns.size')).toBe('Méret')
    expect(tString('fileExplorer.columns.modified')).toBe('Módosítva')
  })

  it('shows Swedish sizes, not Hungarian ones', () => {
    // Swedish and Hungarian agree on the decimal comma, so the counts below are
    // what tells them apart: sv-SE groups with a no-break space, hu with a
    // period.
    expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1,50 KB')
    expect(formatNumber(1234567)).toBe('1 234 567')
    expect(formatNumber(1234567)).not.toBe('1.234.567')
  })

  it("shows a Swedish 'system' date, not a Hungarian one", () => {
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    const swedish = new Intl.DateTimeFormat('sv-SE', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
      .formatToParts(new Date(ts * 1000))
      .map((p) => p.value)
      .join('')
    expect(formatDateForDisplay(ts, 'system', '').text).toBe(swedish)
  })
})

describe('the Language picker', () => {
  afterEach(() => {
    setLocale(null)
    _setFormatLocaleForTests(null)
  })

  it('switches the copy and leaves the formatting conventions alone', () => {
    _setFormatLocaleForTests('sv-SE')
    const swedishCount = formatNumber(1234567)

    setLocale('hu')

    expect(getUiLocale()).toBe('hu')
    expect(tString('fileExplorer.columns.size')).toBe('Méret')
    expect(getFormatLocale()).toBe('sv-SE')
    expect(formatNumber(1234567)).toBe(swedishCount)
  })
})
