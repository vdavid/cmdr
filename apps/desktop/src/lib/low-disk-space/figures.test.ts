import { afterEach, describe, it, expect, vi } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

import { lowSpaceFigures } from './figures'

describe('lowSpaceFigures', () => {
  afterEach(() => {
    _setLocaleForTests('en-US')
  })

  it('scales the free figure to the drive, like the status bar does', () => {
    _setLocaleForTests('en-US')
    // 42 GiB free of a 1 TiB drive: whole gigabytes, since a tenth is smaller than a pixel of it.
    expect(lowSpaceFigures(42_000_000_000, 1_000_000_000_000)).toEqual({ freeText: '39 GiB', percentText: '4.2' })
  })

  it('writes the percent the way the locale does', () => {
    _setLocaleForTests('de-DE')
    expect(lowSpaceFigures(42_000_000_000, 1_000_000_000_000).percentText).toBe('4,2')
  })

  it('reads an unknown total as nothing to warn about', () => {
    _setLocaleForTests('en-US')
    expect(lowSpaceFigures(0, 0).percentText).toBe('100.0')
  })
})
