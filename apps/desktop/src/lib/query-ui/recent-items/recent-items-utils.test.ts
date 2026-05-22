import { describe, it, expect } from 'vitest'
import type { HistoryEntry } from '$lib/tauri-commands'
import { chipTooltip, filterSummary, formatAge, modeBadge, modeName } from './recent-items-utils'

function makeEntry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id: 'test-id',
    timestamp: Date.now(),
    mode: 'filename',
    query: '*.pdf',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
    ...overrides,
  }
}

describe('modeBadge', () => {
  it('maps each mode to a short two-char badge', () => {
    expect(modeBadge('ai')).toBe('AI')
    expect(modeBadge('filename')).toBe('Aa')
    expect(modeBadge('regex')).toBe('.*')
  })
})

describe('modeName', () => {
  it('returns friendly capitalized names', () => {
    expect(modeName('ai')).toBe('AI')
    expect(modeName('filename')).toBe('Filename')
    expect(modeName('regex')).toBe('Regex')
  })
})

describe('formatAge', () => {
  const NOW = Date.UTC(2026, 4, 20, 12, 0, 0)

  it('returns "just now" for very recent timestamps', () => {
    expect(formatAge(NOW - 30 * 1000, NOW)).toBe('just now')
  })

  it('returns minutes for sub-hour ages', () => {
    expect(formatAge(NOW - 5 * 60 * 1000, NOW)).toBe('5m ago')
  })

  it('returns hours for sub-day ages', () => {
    expect(formatAge(NOW - 3 * 60 * 60 * 1000, NOW)).toBe('3h ago')
  })

  it('returns days within the first week', () => {
    expect(formatAge(NOW - 2 * 24 * 60 * 60 * 1000, NOW)).toBe('2d ago')
  })

  it('returns weeks beyond seven days', () => {
    expect(formatAge(NOW - 14 * 24 * 60 * 60 * 1000, NOW)).toBe('2w ago')
  })

  it('clamps negative deltas to "just now"', () => {
    // Future timestamps shouldn't blow up the formatter.
    expect(formatAge(NOW + 60_000, NOW)).toBe('just now')
  })
})

describe('filterSummary', () => {
  it('returns empty string when no filters are set', () => {
    expect(filterSummary(makeEntry())).toBe('')
  })

  it('shows "size >" when only sizeMin is set', () => {
    const entry = makeEntry({ filters: { sizeMin: 1024 * 1024 } })
    expect(filterSummary(entry)).toContain('size > 1.0 MB')
  })

  it('shows a range when both size bounds are set', () => {
    const entry = makeEntry({
      filters: { sizeMin: 1024, sizeMax: 1024 * 1024 },
    })
    expect(filterSummary(entry)).toContain('size 1.0 KB–1.0 MB')
  })

  it('includes scope, case-sensitive, and system-dirs notes when set', () => {
    const entry = makeEntry({
      scope: '/Users/test',
      caseSensitive: true,
      excludeSystemDirs: false,
    })
    const out = filterSummary(entry)
    expect(out).toContain('scope: /Users/test')
    expect(out).toContain('case-sensitive')
    expect(out).toContain('system dirs included')
  })
})

describe('chipTooltip', () => {
  const NOW = Date.UTC(2026, 4, 20, 12, 0, 0)

  it('combines mode, age, filter summary, and result count', () => {
    const entry = makeEntry({
      mode: 'ai',
      timestamp: NOW - 60 * 60 * 1000,
      filters: { sizeMin: 1024 * 1024 },
      resultCount: 42,
    })
    const out = chipTooltip(entry, NOW)
    expect(out).toContain('AI · 1h ago')
    expect(out).toContain('size > 1.0 MB')
    expect(out).toContain('42 results last time')
  })

  it('omits the filter line when no filters are set', () => {
    const entry = makeEntry({ mode: 'regex', timestamp: NOW, resultCount: 0 })
    const out = chipTooltip(entry, NOW)
    expect(out).toBe('Regex · just now')
  })
})
