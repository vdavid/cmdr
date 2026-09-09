/**
 * The size rule behind the file context menu's header line.
 *
 * What's worth pinning here is every path to "show no size": the header must show a
 * name alone rather than a number the user would read as a settled fact.
 */

import { afterEach, describe, it, expect } from 'vitest'
import {
  contextMenuCountText,
  contextMenuSizeBytes,
  contextMenuSelectionSizeBytes,
  contextMenuSizeText,
} from './context-menu-target'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { ListingStats } from '../types'

function statsOf(over: Partial<ListingStats> = {}): ListingStats {
  return {
    totalFiles: 10,
    totalDirs: 2,
    totalSize: 10_000,
    totalPhysicalSize: 10_000,
    selectedFiles: 3,
    selectedDirs: 0,
    selectedSize: 3_355_443,
    selectedPhysicalSize: 3_355_443,
    ...over,
  }
}

describe('contextMenuSizeBytes', () => {
  it('totals the rows it is given', () => {
    expect(
      contextMenuSizeBytes([
        { isDirectory: false, size: 100 },
        { isDirectory: false, size: 23 },
      ]),
    ).toBe(123)
  })

  it('answers with the one size for one row', () => {
    expect(contextMenuSizeBytes([{ isDirectory: false, size: 2_100_000 }])).toBe(2_100_000)
  })

  it('has no answer for no rows', () => {
    expect(contextMenuSizeBytes([])).toBeNull()
  })

  it('has no answer once a folder is involved', () => {
    // A folder's size is its subtree's, which the index may still be walking; the pane
    // shows `<dir>` rather than a number, and the header must not disagree.
    expect(contextMenuSizeBytes([{ isDirectory: true, size: 4096 }])).toBeNull()
    expect(
      contextMenuSizeBytes([
        { isDirectory: false, size: 100 },
        { isDirectory: true, size: 4096 },
      ]),
    ).toBeNull()
  })

  it('has no answer when a row has no size yet', () => {
    // ❌ Not a partial sum: it would silently understate the pile.
    expect(
      contextMenuSizeBytes([
        { isDirectory: false, size: 100 },
        { isDirectory: false, size: null },
      ]),
    ).toBeNull()
  })
})

describe('contextMenuSelectionSizeBytes', () => {
  it('takes the selection total off the listing stats', () => {
    expect(contextMenuSelectionSizeBytes(statsOf())).toBe(3_355_443)
  })

  it('has no answer without stats', () => {
    expect(contextMenuSelectionSizeBytes(null)).toBeNull()
  })

  it('has no answer while the backend has sent no selected size', () => {
    expect(contextMenuSelectionSizeBytes(statsOf({ selectedSize: null }))).toBeNull()
  })

  it('has no answer once a folder is selected', () => {
    expect(contextMenuSelectionSizeBytes(statsOf({ selectedDirs: 1 }))).toBeNull()
  })
})

describe('contextMenuSizeText', () => {
  it('renders the size the way the pane column does', () => {
    // Defaults: dynamic unit, binary base.
    expect(contextMenuSizeText(3_355_443)).toBe('3.20 MB')
  })

  it('renders nothing at all when there is no size', () => {
    expect(contextMenuSizeText(null)).toBeUndefined()
  })
})

describe('contextMenuCountText', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('words the count with its noun', () => {
    _setLocaleForTests('en-US')
    expect(contextMenuCountText(3)).toBe('3 items')
  })

  it('groups a large count and picks the noun the way the active locale does', () => {
    // The whole reason the wording crosses IPC rather than being built in Rust: both the
    // separator and the plural form are the locale's, and `crate::intl`'s `menu_t` is a
    // table lookup with neither number formatting nor a plural engine.
    _setLocaleForTests('en-US')
    expect(contextMenuCountText(12_345)).toBe('12,345 items')
    _setLocaleForTests('de-DE')
    expect(contextMenuCountText(12_345)).toBe('12.345 Objekte')
  })

  it('has no wording below two, where the header shows a filename instead', () => {
    _setLocaleForTests('en-US')
    expect(contextMenuCountText(1)).toBeUndefined()
    expect(contextMenuCountText(0)).toBeUndefined()
  })
})
