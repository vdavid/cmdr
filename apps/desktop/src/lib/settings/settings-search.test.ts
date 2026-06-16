import { describe, it, expect, beforeEach } from 'vitest'
import {
  searchSettings,
  searchAdvancedSettings,
  getMatchingSections,
  sectionHasMatches,
  highlightMatches,
  clearSearchIndex,
  getMatchIndicesForLabel,
} from './settings-search'
import { getSettingDefinition } from './settings-registry'

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
    expect(results.some((r) => r.setting.id === 'appearance.uiDensity')).toBe(true)
  })

  it('should find settings by section name', () => {
    const results = searchSettings('appearance')
    expect(results.length).toBeGreaterThan(0)
    // At least one result should be in the Appearance section
    const hasAppearance = results.some((r) => r.setting.section[0] === 'Appearance')
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

describe('searchAdvancedSettings', () => {
  it('should return all advanced settings when query is empty', () => {
    const results = searchAdvancedSettings('')
    expect(results.length).toBeGreaterThan(0)
    for (const result of results) {
      expect(result.setting.showInAdvanced).toBe(true)
    }
  })

  it('should find advanced settings by label', () => {
    const results = searchAdvancedSettings('drag')
    // Should find dragThreshold
    const hasDragThreshold = results.some((r) => r.setting.id.includes('dragThreshold'))
    expect(hasDragThreshold).toBe(true)
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
    const downloads = results.find((r) => r.setting.id === 'behavior.fileSystemWatching.downloadsNotifications')
    // The card title resolves to "Downloads notifications" (settings.fileSystemWatching.cardDownloads).
    expect(downloads?.searchableText).toContain('downloads notifications')
    // It MUST be appended last (after keywords), so label highlight offsets stay correct.
    expect(downloads?.searchableText.endsWith('downloads notifications')).toBe(true)
  })

  it('does not append anything for a setting without a cardKey', () => {
    const results = searchSettings('')
    const density = results.find((r) => r.setting.id === 'appearance.uiDensity')
    // The card title was undefined, so searchableText is just section/label/desc/keywords.
    expect(density?.setting.card).toBeUndefined()
  })

  it('surfaces a setting when searching its card title', () => {
    // "Low disk space" is the card title shared by the two low-disk-space settings.
    const ids = searchSettings('low disk space').map((r) => r.setting.id)
    expect(ids).toContain('behavior.fileSystemWatching.lowDiskSpaceNotifications')
    expect(ids).toContain('behavior.fileSystemWatching.lowDiskSpaceThresholdPercent')
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

describe('index-size hidden anchor search', () => {
  beforeEach(() => {
    clearSearchIndex()
  })

  it('returns the indexing.indexSize anchor when searching "index size"', () => {
    const ids = searchSettings('index size').map((r) => r.setting.id)
    expect(ids).toContain('indexing.indexSize')
  })

  it('adds the File system watching section to the sidebar match set for "index size"', () => {
    const sections = getMatchingSections('index size')
    expect(sectionHasMatches(['Behavior', 'File system watching'], sections)).toBe(true)
  })
})
