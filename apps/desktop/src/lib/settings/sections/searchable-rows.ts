/**
 * Every searchable non-setting row, gathered for the search index.
 *
 * Each section that has such rows declares them in a `<Component>.rows.ts`
 * beside its markup; this file is the one place that lists those siblings, so
 * `settings-search.ts` has a single import and adding a row still touches only
 * the file next to the component. A new `*.rows.ts` that isn't listed here is a
 * test failure (`searchable-rows.test.ts`).
 *
 * These declarations are for SEARCH only. They never decide what renders: the
 * sections stay free-form Svelte and gate their own markup on `shouldShow(id)`.
 * Type and rationale: `../types.ts` § Searchable rows.
 */

import { tString } from '$lib/intl/messages.svelte'
import type { SearchableEntry, SearchableRow } from '../types'
import { adbRows } from './AdbSection.rows'
import { advancedRows } from './AdvancedSection.rows'
import { askCmdrRows } from './AskCmdrSection.rows'
import { driveIndexingRows } from './DriveIndexingSection.rows'
import { keyboardShortcutsRows } from './KeyboardShortcutsSection.rows'
import { licenseRows } from './LicenseSection.rows'
import { serversRows } from './ServersSection.rows'
import { updatesRows } from './UpdatesSection.rows'

/** Every declared row, in section order (search doesn't rank by it; readers do). */
export const searchableRows: SearchableRow[] = [
  ...driveIndexingRows,
  ...askCmdrRows,
  ...serversRows,
  ...adbRows,
  ...updatesRows,
  ...licenseRows,
  ...keyboardShortcutsRows,
  ...advancedRows,
]

/**
 * Turns an authored row into the index's entry shape, with `label` / `card`
 * getter-backed so they resolve through the current catalog at read time (the
 * same trick `resolveDefinition` plays for settings).
 */
function resolveRow(row: SearchableRow): SearchableEntry {
  const { labelKey, cardKey } = row
  const entry: SearchableEntry = {
    id: row.id,
    section: row.section,
    description: '',
    keywords: row.keywords ?? [],
    get label() {
      return tString(labelKey)
    },
  }
  if (cardKey !== undefined) {
    Object.defineProperty(entry, 'card', { enumerable: true, get: () => tString(cardKey) })
  }
  return entry
}

/** The rows as search-index entries, merged with the registry by `buildSearchIndex`. */
export const searchableRowEntries: SearchableEntry[] = searchableRows.map(resolveRow)
