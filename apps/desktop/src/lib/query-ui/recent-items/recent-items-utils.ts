/**
 * Pure helpers for the recent-searches footer and popover. Kept side-effect-free so the
 * tests can hit them directly.
 */

import type { HistoryEntry, HistoryMode } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'

/** Short badge shown on each chip to signal the search mode. */
export function modeBadge(mode: HistoryMode): string {
  switch (mode) {
    case 'ai':
      return 'AI'
    case 'regex':
      return '.*'
    case 'filename':
      return 'Aa'
  }
}

/** Friendly mode name for tooltips. */
export function modeName(mode: HistoryMode): string {
  switch (mode) {
    case 'ai':
      return tString('queryUi.recent.mode.ai')
    case 'regex':
      return tString('queryUi.recent.mode.regex')
    case 'filename':
      return tString('queryUi.recent.mode.filename')
  }
}

/**
 * Human-readable relative time for chip tooltips. Coarse on purpose — minutes, hours, days,
 * weeks. Always "ago" because history entries are by definition in the past.
 */
export function formatAge(timestampMs: number, nowMs: number = Date.now()): string {
  const deltaSec = Math.max(0, Math.floor((nowMs - timestampMs) / 1000))
  if (deltaSec < 60) return tString('queryUi.age.justNow')
  const minutes = Math.floor(deltaSec / 60)
  if (minutes < 60) return tString('queryUi.age.minutes', { count: String(minutes) })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return tString('queryUi.age.hours', { count: String(hours) })
  const days = Math.floor(hours / 24)
  if (days < 7) return tString('queryUi.age.days', { count: String(days) })
  const weeks = Math.floor(days / 7)
  if (weeks < 5) return tString('queryUi.age.weeks', { count: String(weeks) })
  const months = Math.floor(days / 30)
  if (months < 12) return tString('queryUi.age.months', { count: String(months) })
  const years = Math.floor(days / 365)
  return tString('queryUi.age.years', { count: String(years) })
}

function sizeSummary(filters: HistoryEntry['filters']): string | null {
  if (!filters) return null
  if (filters.sizeMin != null && filters.sizeMax != null) {
    return tString('queryUi.recent.sizeRange', {
      minText: formatBytes(filters.sizeMin),
      maxText: formatBytes(filters.sizeMax),
    })
  }
  if (filters.sizeMin != null) return tString('queryUi.recent.sizeMin', { minText: formatBytes(filters.sizeMin) })
  if (filters.sizeMax != null) return tString('queryUi.recent.sizeMax', { maxText: formatBytes(filters.sizeMax) })
  return null
}

function dateSummary(filters: HistoryEntry['filters']): string | null {
  if (!filters) return null
  if (filters.modifiedAfter != null && filters.modifiedBefore != null) {
    return tString('queryUi.recent.modifiedRange', {
      after: filters.modifiedAfter,
      before: filters.modifiedBefore,
    })
  }
  if (filters.modifiedAfter != null) return tString('queryUi.recent.modifiedAfter', { after: filters.modifiedAfter })
  if (filters.modifiedBefore != null)
    return tString('queryUi.recent.modifiedBefore', { before: filters.modifiedBefore })
  return null
}

/** Short filter summary for the tooltip, e.g. "Size > 1 MB, after 2026-01-01". */
export function filterSummary(entry: HistoryEntry): string {
  const parts: string[] = []
  const size = sizeSummary(entry.filters)
  if (size) parts.push(size)
  const date = dateSummary(entry.filters)
  if (date) parts.push(date)
  if (entry.scope.trim()) parts.push(tString('queryUi.recent.scopeSummary', { scope: entry.scope.trim() }))
  if (entry.caseSensitive) parts.push(tString('queryUi.recent.caseSensitive'))
  if (!entry.excludeSystemDirs) parts.push(tString('queryUi.recent.systemDirsIncluded'))
  return parts.join(', ')
}

function formatBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024) return `${(b / (1024 * 1024 * 1024)).toFixed(1)} GB`
  if (b >= 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`
  if (b >= 1024) return `${(b / 1024).toFixed(1)} KB`
  return `${String(b)} B`
}

/**
 * Build a multi-line plain-text tooltip for a recent-search chip. Plain text (not HTML) so
 * the existing `tooltip` action can render it safely.
 */
export function chipTooltip(entry: HistoryEntry, nowMs: number = Date.now()): string {
  const lines = [
    tString('queryUi.recent.chipTooltipHeader', {
      mode: modeName(entry.mode),
      age: formatAge(entry.timestamp, nowMs),
    }),
  ]
  const summary = filterSummary(entry)
  if (summary) lines.push(summary)
  if (entry.resultCount > 0) {
    lines.push(tString('queryUi.recent.resultCount', { countText: String(entry.resultCount) }))
  }
  return lines.join('\n')
}
