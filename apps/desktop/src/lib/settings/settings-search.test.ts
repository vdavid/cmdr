import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import {
  searchSettings,
  getMatchingSections,
  sectionHasMatches,
  highlightMatches,
  clearSearchIndex,
  getMatchIndicesForLabel,
  anyVisible,
  createShouldShow,
  getMatchingSettingIdsInSection,
} from './settings-search'
import { getSettingDefinition } from './settings-registry'

/**
 * On a Mac by default, so `macOSOnly` settings are in the index under test. The
 * same mock drives the searchable-row filter, so both halves of the index answer
 * to one platform.
 */
const isMacOS = vi.hoisted(() => vi.fn(() => true))

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/shortcuts/key-capture')>()),
  isMacOS: () => isMacOS(),
}))

/**
 * Off macOS a `macOSOnly` setting renders nowhere, so a hit would open a page with
 * nothing to show for it. The entry stays registered; only the index drops it.
 */
describe('a macOS-only setting', () => {
  const MAC_ONLY = ['behavior.openTerminalHereApp', 'behavior.textEditorApp', 'behavior.textEditorHintSeen']

  afterEach(() => {
    isMacOS.mockReturnValue(true)
    clearSearchIndex()
  })

  it('stays out of the search index off macOS', () => {
    isMacOS.mockReturnValue(false)
    clearSearchIndex()

    const ids = searchSettings('').map((r) => r.entry.id)
    for (const id of MAC_ONLY) {
      expect(ids).not.toContain(id)
    }
    expect(searchSettings('Sublime Text').map((r) => r.entry.id)).not.toContain('behavior.textEditorApp')
  })

  it('is in the search index on macOS', () => {
    isMacOS.mockReturnValue(true)
    clearSearchIndex()

    expect(searchSettings('').map((r) => r.entry.id)).toEqual(expect.arrayContaining(MAC_ONLY))
    expect(searchSettings('Sublime Text').map((r) => r.entry.id)).toContain('behavior.textEditorApp')
  })
})

describe('searchSettings', () => {
  beforeEach(() => {
    clearSearchIndex()
  })

  it('should return all settings when query is empty', () => {
    const results = searchSettings('')
    expect(results.length).toBeGreaterThan(0)
  })

  it('should return all settings when query is whitespace', () => {
    const results = searchSettings('   ')
    expect(results.length).toBeGreaterThan(0)
  })

  it('should find settings by label', () => {
    const results = searchSettings('density')
    expect(results.length).toBeGreaterThan(0)
    expect(results.some((r) => r.entry.id === 'appearance.uiDensity')).toBe(true)
  })

  it('should find settings by section name', () => {
    const results = searchSettings('appearance')
    expect(results.length).toBeGreaterThan(0)
    // At least one result should be in the Appearance section
    const hasAppearance = results.some((r) => r.entry.section[0] === 'Appearance')
    expect(hasAppearance).toBe(true)
  })

  it('should return empty array when nothing matches', () => {
    const results = searchSettings('xyznonexistent123')
    expect(results).toEqual([])
  })

  it('should include matched indices for highlighting', () => {
    const results = searchSettings('density')
    expect(results.length).toBeGreaterThan(0)
    // Matched indices should be numbers
    for (const result of results) {
      expect(Array.isArray(result.matchedIndices)).toBe(true)
    }
  })
})

describe('advanced settings in the global search index', () => {
  beforeEach(() => {
    clearSearchIndex()
  })

  // Advanced settings (`section: ['Advanced']`) ride the global search pipeline
  // (the Advanced page groups them into cards on the same `shouldShow`
  // predicate), so they MUST be findable in the main settings search.
  it('finds an Advanced-only setting in the global search', () => {
    const ids = searchSettings('prefetch').map((r) => r.entry.id)
    expect(ids).toContain('advanced.prefetchBufferSize')
  })

  it('lights the Advanced sidebar entry for an Advanced-only term', () => {
    const sections = getMatchingSections('prefetch')
    expect(sectionHasMatches(['Advanced'], sections)).toBe(true)
  })

  it('finds dragThreshold by label in the global search', () => {
    const ids = searchSettings('drag threshold').map((r) => r.entry.id)
    expect(ids).toContain('advanced.dragThreshold')
  })

  it('surfaces an Advanced setting by its card title', () => {
    // "Performance" is the shared card title for the three buffer-size rows.
    const ids = searchSettings('performance').map((r) => r.entry.id)
    expect(ids).toContain('advanced.prefetchBufferSize')
    expect(ids).toContain('advanced.virtualizationBufferRows')
  })

  it('highlights label matches for an Advanced row (was always empty pre-un-exclusion)', () => {
    // Pre-fix the Advanced rows were absent from the global index, so
    // `getMatchIndicesForLabel` (which searches that index) returned [] for
    // them and the advanced-row highlight was always empty. Now it works.
    const def = getSettingDefinition('advanced.dragThreshold')
    const label = def?.label ?? ''
    const indices = getMatchIndicesForLabel('drag', 'advanced.dragThreshold')
    expect(indices.length).toBeGreaterThan(0)
    for (const idx of indices) {
      expect(idx).toBeGreaterThanOrEqual(0)
      expect(idx).toBeLessThan(label.length)
    }
  })

  it('does not surface an Advanced-only setting on an unrelated section page', () => {
    // The global index now holds Advanced settings, but each carries its own
    // `section`, so the section-scoped gate keeps them off other pages. An
    // `['Advanced']`-section setting must not leak into, e.g., the Appearance
    // listing page's match set.
    const inListing = getMatchingSettingIdsInSection('prefetch', ['Appearance', 'Listing'])
    expect(inListing.has('advanced.prefetchBufferSize')).toBe(false)
  })

  it('a former mirror now lights only Advanced: "concurrency" → Advanced, not SMB', () => {
    // `network.smbConcurrency` now has `section: ['Advanced']` as its single home
    // (no SMB mirror). Searching its term lights the Advanced sidebar entry and
    // puts the row in Advanced's match set; it must NOT leak back into the SMB
    // page's section-scoped match set.
    const inAdvanced = getMatchingSettingIdsInSection('concurrency', ['Advanced'])
    expect(inAdvanced.has('network.smbConcurrency')).toBe(true)
    const inSmb = getMatchingSettingIdsInSection('concurrency', ['File systems', 'SMB/Network shares'])
    expect(inSmb.has('network.smbConcurrency')).toBe(false)
    const sections = getMatchingSections('concurrency')
    expect(sectionHasMatches(['Advanced'], sections)).toBe(true)
    expect(sectionHasMatches(['File systems', 'SMB/Network shares'], sections)).toBe(false)
  })
})

describe('getMatchingSections', () => {
  it('should return sections containing matching settings', () => {
    const sections = getMatchingSections('density')
    expect(sections.size).toBeGreaterThan(0)
    // Should include the parent section path 'Appearance' or 'Appearance/Zoom and density'
    const hasAppearance = sections.has('Appearance') || sections.has('Appearance/Zoom and density')
    expect(hasAppearance).toBe(true)
  })

  it('should return empty set when nothing matches', () => {
    const sections = getMatchingSections('xyznonexistent123')
    expect(sections.size).toBe(0)
  })
})

describe('sectionHasMatches', () => {
  it('should return true for sections with matching settings', () => {
    const matchingSections = getMatchingSections('density')
    // The function uses path.join('/') to check
    expect(sectionHasMatches(['Appearance'], matchingSections)).toBe(true)
  })

  it('should return false for sections without matches', () => {
    const matchingSections = getMatchingSections('density')
    expect(sectionHasMatches(['NonExistent'], matchingSections)).toBe(false)
  })
})

describe('highlightMatches', () => {
  it('should return single segment when no matches', () => {
    const segments = highlightMatches('hello world', [])
    expect(segments).toEqual([{ text: 'hello world', matched: false }])
  })

  it('should highlight matched characters', () => {
    const segments = highlightMatches('hello', [0, 1])
    expect(segments).toEqual([
      { text: 'he', matched: true },
      { text: 'llo', matched: false },
    ])
  })

  it('should handle non-contiguous matches', () => {
    const segments = highlightMatches('abcde', [0, 2, 4])
    expect(segments.length).toBeGreaterThan(1)
    // Check that matched characters are marked
    expect(segments.some((s) => s.matched && s.text === 'a')).toBe(true)
  })
})

describe('card title indexing', () => {
  beforeEach(() => {
    clearSearchIndex()
  })

  it('appends a setting`s resolved card title to its searchable text', () => {
    const results = searchSettings('')
    const downloads = results.find((r) => r.entry.id === 'behavior.fileSystemWatching.downloadsNotifications')
    // The card title resolves to "Downloads" (settings.fileSystemWatching.cardDownloads).
    expect(downloads?.searchableText).toContain('downloads')
    // It MUST be appended last (after keywords), so label highlight offsets stay correct.
    expect(downloads?.searchableText.endsWith('downloads')).toBe(true)
  })

  it('does not append anything for a setting without a cardKey', () => {
    const results = searchSettings('')
    const density = results.find((r) => r.entry.id === 'appearance.uiDensity')
    // The card title was undefined, so searchableText is just section/label/desc/keywords.
    expect(density?.entry.card).toBeUndefined()
  })

  it('surfaces a setting when searching its card title', () => {
    // "Low disk space" is the card title shared by the two low-disk-space settings.
    const ids = searchSettings('low disk space').map((r) => r.entry.id)
    expect(ids).toContain('behavior.fileSystemWatching.lowDiskSpaceNotifications')
    expect(ids).toContain('behavior.fileSystemWatching.lowDiskSpaceThresholdPercent')
  })

  it('surfaces the image-indexing rows when searching the card title the card renders', () => {
    // `ImageIndexingSection.svelte` titles card 1 with `settings.mediaIndex.cards.enable`
    // ("Enable indexing"), so its rows must carry THAT key. They once carried
    // `settings.mediaIndex.card` ("Image search"), a title no card displays, and searching
    // what the user could actually read returned nothing.
    // All FOUR rows the card renders count, the `parallelism` slider included: it's
    // `hidden` only because it's hand-rendered, and the user reads the same title above it.
    const ids = searchSettings('enable indexing').map((r) => r.entry.id)
    expect(ids).toContain('mediaIndex.enabled')
    expect(ids).toContain('mediaIndex.showFileStatusIcons')
    expect(ids).toContain('mediaIndex.showInSearch')
    expect(ids).toContain('mediaIndex.parallelism')
  })

  it('keeps label highlight offsets correct after appending the card title', () => {
    // Regression guard: card title appended last must not shift label-relative indices.
    // Search the low-disk-space row by its own label and confirm the highlighted span
    // lands inside the label, not past it.
    const def = getSettingDefinition('behavior.fileSystemWatching.lowDiskSpaceNotifications')
    expect(def).toBeDefined()
    const label = def?.label ?? ''
    const indices = getMatchIndicesForLabel('disk', 'behavior.fileSystemWatching.lowDiskSpaceNotifications')
    expect(indices.length).toBeGreaterThan(0)
    for (const idx of indices) {
      expect(idx).toBeGreaterThanOrEqual(0)
      expect(idx).toBeLessThan(label.length)
    }
  })
})

describe('anyVisible', () => {
  it('returns true when at least one id passes the predicate', () => {
    const shouldShow = (id: string) => id === 'b'
    expect(anyVisible(shouldShow, 'a', 'b', 'c')).toBe(true)
  })

  it('returns false when no id passes the predicate', () => {
    const shouldShow = (id: string) => id === 'z'
    expect(anyVisible(shouldShow, 'a', 'b', 'c')).toBe(false)
  })

  it('returns false for an empty id list', () => {
    const shouldShow = () => true
    expect(anyVisible(shouldShow)).toBe(false)
  })

  it('uses the same predicate the rows use (empty query shows all → card visible)', () => {
    // `createShouldShow('')` returns a predicate that is true for every id, so a
    // card built from any member ids is visible on a non-search page.
    const showAll = createShouldShow('')
    expect(anyVisible(showAll, 'indexing.enabled', 'row:indexing.indexSize')).toBe(true)
  })
})

describe('index-size searchable row', () => {
  beforeEach(() => {
    clearSearchIndex()
  })

  it('returns the index-size row when searching "index size"', () => {
    const ids = searchSettings('index size').map((r) => r.entry.id)
    expect(ids).toContain('row:indexing.indexSize')
  })

  it('adds the Drive indexing section to the sidebar match set for "index size"', () => {
    const sections = getMatchingSections('index size')
    expect(sectionHasMatches(['Indexing', 'Drive indexing'], sections)).toBe(true)
  })
})
