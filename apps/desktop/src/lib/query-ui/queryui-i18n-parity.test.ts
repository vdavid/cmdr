/**
 * Base-locale (en) parity net for the query-ui i18n migration.
 *
 * Every user-facing string in `lib/query-ui/` (the shared search/selection dialog
 * primitives, the filter chips, and the recent-items helpers) moved from hardcoded
 * English into the `queryUi.*` catalog, resolved through `t()` / `tString()`. This
 * is a behavior-preserving MOVE: every rendered en string must be byte-identical to
 * the pre-migration copy. These goldens are the literals that lived in the
 * components and pure helpers before the move; a future copy edit lands in the
 * catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'
import { deriveSizeChip, deriveDateChip, deriveScopeChip } from './filter-chips/filter-chip-state'
import { byteUnitLabel, buildDatePresets } from './filter-chips/filter-popover-helpers'
import { patternRowLabel } from './ai-summary'
import { chipTooltip, modeName, formatAge } from './recent-items/recent-items-utils'
import type { HistoryEntry } from '$lib/tauri-commands'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('query-ui static-string parity (en)', () => {
  it('resolves bar, mode-chip, and run-button copy', () => {
    expect(tString('queryUi.bar.placeholder.ai')).toBe("Describe what you're looking for")
    expect(tString('queryUi.bar.placeholder.filename')).toBe('Filename pattern (use * and ? as wildcards)')
    expect(tString('queryUi.bar.runLabel')).toBe('Search')
    expect(tString('queryUi.mode.ai.label')).toBe('Ask anything')
    expect(tString('queryUi.mode.content.tooltip')).toBe('Coming soon: full-text search inside files')
  })

  it('resolves results-table headers and states', () => {
    expect(tString('queryUi.results.col.name')).toBe('Name')
    expect(tString('queryUi.results.col.actions')).toBe('Actions')
    expect(tString('queryUi.results.searching')).toBe('Searching...')
    expect(tString('queryUi.results.noMatchHeading')).toBe('No files match these criteria:')
    expect(tString('queryUi.results.indexNotReady')).toBe(
      'Drive index not ready. Search is available after the initial scan completes.',
    )
  })

  it('resolves scope-popover toggle and footer copy', () => {
    expect(tString('queryUi.scope.toggle.hideBoring')).toBe('Hide boring folders')
    expect(tString('queryUi.scope.toggle.caseSensitive')).toBe('Case-sensitive')
    expect(tString('queryUi.scope.useCurrentFolder')).toBe('Use current folder')
    expect(tString('queryUi.scope.allFolders')).toBe('All folders')
    expect(tString('queryUi.scope.placeholder')).toBe('All folders')
  })

  it('resolves the AI transparency strip copy', () => {
    expect(tString('queryUi.ai.lead')).toBe("Here's what the agent did:")
    expect(tString('queryUi.ai.empty')).toBe('Nothing to filter on yet. Try rephrasing?')
    expect(tString('queryUi.ai.refine')).toBe('Refine…')
  })
})

describe('query-ui interpolated-string parity (en)', () => {
  it('resolves the index-ready empty-state line with plural agreement', () => {
    expect(tString('queryUi.empty.indexReady', { countText: '1', count: 1 })).toBe('Index ready · 1 entry')
    expect(tString('queryUi.empty.indexReady', { countText: '1,234', count: 1234 })).toBe('Index ready · 1,234 entries')
  })

  it('resolves the results status-bar count line', () => {
    expect(tString('queryUi.results.resultCount', { shownText: '30', totalText: '1,234' })).toBe('30 of 1,234 results')
    expect(tString('queryUi.results.indexReadyStatus', { countText: '12.3K' })).toBe('Index ready (12.3K entries)')
    expect(tString('queryUi.results.scanningWithCount', { countText: '999' })).toBe(
      'Scanning in progress (999 entries)...',
    )
  })
})

describe('query-ui filter-chip summary parity (en)', () => {
  it('renders single-bound and range size summaries', () => {
    expect(deriveSizeChip('gte', '100', 'MB', '', 'B', 'binary').summary).toBe('> 100 MB')
    expect(deriveSizeChip('lte', '5', 'GB', '', 'B', 'binary').summary).toBe('< 5 GB')
    expect(deriveSizeChip('eq', '0', 'B', '', 'B', 'binary').summary).toBe('= 0 B')
    expect(deriveSizeChip('between', '1', 'MB', '200', 'MB', 'binary').summary).toBe('1 MB – 200 MB')
  })

  it('renders date summaries', () => {
    expect(deriveDateChip('after', '2026-01-01', '').summary).toBe('after 2026-01-01')
    expect(deriveDateChip('before', '2026-06-30', '').summary).toBe('before 2026-06-30')
    expect(deriveDateChip('between', '2026-01-01', '2026-06-30').summary).toBe('2026-01-01 – 2026-06-30')
  })

  it('renders the includes-system-folders scope summary', () => {
    expect(deriveScopeChip('', false).summary).toBe('includes system folders')
  })

  it('renders byte/bytes unit labels', () => {
    expect(byteUnitLabel('1')).toBe('byte')
    expect(byteUnitLabel('5')).toBe('bytes')
  })

  it('renders dynamic date-preset labels at a fixed anchor', () => {
    // Anchor: 2026-05-20 (a Wednesday), US locale → week starts Sunday.
    const presets = buildDatePresets(new Date(2026, 4, 20), 'en-US')
    const byKey = (k: string) => presets.find((p) => p.key === k)?.label
    expect(byKey('today')).toBe('today 0:00')
    expect(byKey('yesterday')).toBe('yesterday 0:00')
    expect(byKey('thisWeek')).toBe('this Sunday 0:00')
    expect(byKey('thisMonth')).toBe('1st of May 0:00')
    expect(byKey('lastMonth')).toBe('1st of April, 2026, 0:00')
    expect(byKey('yearStart')).toBe('1st of January, 2026, 0:00')
  })
})

describe('query-ui recent-items parity (en)', () => {
  it('renders mode names and the pattern-row label', () => {
    expect(modeName('ai')).toBe('AI')
    expect(modeName('regex')).toBe('Regex')
    expect(modeName('filename')).toBe('Filename')
    expect(patternRowLabel('glob')).toBe('Glob')
    expect(patternRowLabel('regex')).toBe('Regex')
    expect(patternRowLabel(null)).toBe('Pattern')
  })

  it('renders coarse relative ages', () => {
    const now = 1_000_000_000_000
    expect(formatAge(now, now)).toBe('just now')
    expect(formatAge(now - 5 * 60_000, now)).toBe('5m ago')
    expect(formatAge(now - 3 * 3_600_000, now)).toBe('3h ago')
    expect(formatAge(now - 2 * 86_400_000, now)).toBe('2d ago')
  })

  it('renders a chip tooltip with header, filters, and result count', () => {
    const now = 1_000_000_000_000
    const entry: HistoryEntry = {
      id: 'x',
      timestamp: now - 86_400_000,
      mode: 'filename',
      query: '*.pdf',
      filters: { sizeMin: 1024 * 1024, sizeMax: null, modifiedAfter: '2026-01-01', modifiedBefore: null },
      scope: '/Users/me/Docs',
      caseSensitive: true,
      excludeSystemDirs: false,
      resultCount: 42,
    }
    expect(chipTooltip(entry, now)).toBe(
      'Filename · 1d ago\n' +
        'size > 1.0 MB, after 2026-01-01, scope: /Users/me/Docs, case-sensitive, system dirs included\n' +
        '42 results last time',
    )
  })
})
