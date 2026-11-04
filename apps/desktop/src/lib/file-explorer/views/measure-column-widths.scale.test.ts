/**
 * The Ext column's floor scales with text size like every measured track, so at
 * 150 % a short extension's column isn't pinned at the 100 % floor of 28 px.
 * Its own file because it mocks the text scale for the whole module.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { _setMeasureForTests, computeFullListColumnWidths } from './measure-column-widths'

vi.mock('#lib/text-size.svelte.js', () => ({
  getEffectiveScale: () => 1.5,
  onDebouncedScaleChange: () => () => {},
}))

const args = {
  entries: [],
  parentDirStats: null,
  formattedDate: () => ({ text: '', segments: [] }),
  sizeDisplayMode: 'smart' as const,
  isSizeUpdating: () => false,
  showSizeMismatchWarning: false,
  sortBy: 'name' as const,
  dateColumn: 'modified' as const,
  sizeFormatOpts: { unit: 'dynamic' as const, format: 'binary' as const },
  isRestricted: () => false,
}

afterEach(() => {
  _setMeasureForTests(null)
})

describe('computeFullListColumnWidths at 150 % text size', () => {
  it('scales the Ext floor', () => {
    // "Ext" measures 15 px here, well under the floor.
    _setMeasureForTests((text) => text.length * 5)
    expect(computeFullListColumnWidths(args).ext).toBe(42)
  })
})
