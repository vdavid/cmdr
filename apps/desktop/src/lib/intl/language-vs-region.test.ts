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

import { _setFormatLocaleForTests, _setLocaleForTests, getFormatLocale, getUiLocale, setOsFormatLocale } from './locale'
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

describe('a US-English Mac with a Swedish region', () => {
  // David's own machine, and the case that motivated composing the tag in Rust:
  // `AppleLocale = en_US@rg=sezzzz`. Finder writes `2026-08-19` and
  // `1 234 567,89`; the webview, left to itself, resolves the same machine to
  // plain `en-US` and writes `08/19/2026` and `1,234,567.89`, because WebKit
  // drops the `-u-rg-` override. Rust hands us `en-SE`, which reproduces
  // Foundation exactly. The grouping character below is U+00A0.
  beforeEach(() => {
    setOsFormatLocale('en-SE')
  })
  afterEach(() => {
    setOsFormatLocale(null)
    _setLocaleForTests(null)
  })

  it('formats in the region the user set, not the one their language implies', () => {
    expect(getFormatLocale()).toBe('en-SE')
    expect(formatNumber(1234567)).toBe('1 234 567')
    expect(formatNumber(1234567)).not.toBe('1,234,567')
  })

  it('writes the date Finder writes', () => {
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    expect(formatDateForDisplay(ts, 'system', '').text).toBe('2024-03-15, 14:30')
  })

  it('keeps formatting Swedish however the UI language is set', () => {
    // Decision 2, from the other direction: the OS owns the conventions, so an
    // explicit Hungarian UI must not drag the dates and numbers to `hu`.
    setLocale('hu')

    expect(getUiLocale()).toBe('hu')
    expect(tString('fileExplorer.columns.size')).toBe('Méret')
    expect(getFormatLocale()).toBe('en-SE')
    expect(formatNumber(1234567)).toBe('1 234 567')
  })

  it('falls back to the webview default when the OS has no answer', () => {
    // An unreadable or missing region composes nothing rather than a malformed
    // tag, and off macOS there's no answer at all. Either way the webview's own
    // locale stands, which is a working answer.
    setOsFormatLocale(null)

    expect(getFormatLocale()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
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
