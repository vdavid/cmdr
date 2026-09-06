/**
 * The two invariants that keep searchable rows honest.
 *
 * A `SearchableRow` is a claim about markup that lives in another file: "this
 * section renders a row with this label, and gates it on this id". Nothing in the
 * type system checks that claim, and both halves failing are silent — a row for
 * markup that no longer exists sends a searcher to a page that doesn't have it,
 * and an id no gate reads leaves the card it belongs to filtered away (the blank
 * pane this whole mechanism exists to prevent).
 *
 * So: every declared row must name copy its sibling component really renders, and
 * every id must be free of `SettingId`'s space.
 */

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { tString } from '$lib/intl/messages.svelte'
import { settingsRegistry } from '../settings-registry'
import {
  clearSearchIndex,
  getMatchingSections,
  getMatchingSettingIdsInSection,
  sectionHasMatches,
} from '../settings-search'
import type { SearchableRow, SearchableRowId, SettingId } from '../types'
import { searchableRows } from './searchable-rows'

/**
 * The compile-time half of the collision guard: `never` the moment a
 * `SettingsValues` key takes the `row:` prefix, which makes the assignment below
 * a `svelte-check` error. The runtime test covers the other direction (a row id
 * spelled like an existing setting).
 */
type NoRowPrefixedSettingId = [Extract<SettingId, SearchableRowId>] extends [never] ? true : never
const settingIdsAreRowFree: NoRowPrefixedSettingId = true

/** Vitest's root is `apps/desktop` (see `vitest.config.ts`). */
const SECTIONS_DIR = resolve(process.cwd(), 'src/lib/settings/sections')

/** Every `*.rows.ts` beside a section component, keyed by its file name. */
const rowModules = import.meta.glob<Record<string, unknown>>('./*.rows.ts', { eager: true })

/** `{ file: 'DriveIndexingSection', rows: [...] }` for each declaration file. */
const declarations = Object.entries(rowModules).map(([path, module]) => {
  const file = path.replace('./', '').replace('.rows.ts', '')
  const rows = Object.values(module).flatMap((exported) =>
    Array.isArray(exported) ? (exported as SearchableRow[]) : [],
  )
  return { file, rows }
})

function componentSource(file: string): string {
  return readFileSync(resolve(SECTIONS_DIR, `${file}.svelte`), 'utf8')
}

describe('searchable rows match the markup they describe', () => {
  it('finds at least one declaration file (the glob resolved)', () => {
    expect(declarations.length).toBeGreaterThan(0)
    expect(declarations.every((d) => d.rows.length > 0)).toBe(true)
  })

  for (const { file, rows } of declarations) {
    describe(file, () => {
      const source = componentSource(file)
      // A section that filters its own rows must gate these too. One that doesn't
      // (`LicenseSection`, `KeyboardShortcutsSection`) renders its rows whatever the
      // query is, so there's no gate to be missing from and no card to leave empty.
      const filtersRows = source.includes('shouldShow(')

      for (const row of rows) {
        // allowed-pluralize-noun: `renders` is the verb of a test name, not a count and a noun
        it(`${row.id} renders its label key`, () => {
          expect(source).toContain(row.labelKey)
        })

        if (filtersRows) {
          it(`${row.id} is gated on its own id`, () => {
            expect(source).toContain(`'${row.id}'`)
          })
        }
      }
    })
  }
})

describe('searchable row ids', () => {
  it('never collide with a setting id', () => {
    expect(settingIdsAreRowFree).toBe(true)
    const settingIds = new Set<string>(settingsRegistry.map((s) => s.id))
    for (const row of searchableRows) {
      expect(settingIds.has(row.id)).toBe(false)
    }
  })

  it('all carry the `row:` prefix and are unique', () => {
    const ids = searchableRows.map((r) => r.id)
    for (const id of ids) {
      expect(id.startsWith('row:')).toBe(true)
    }
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('name a hosting section', () => {
    for (const row of searchableRows) {
      expect(row.section.length).toBeGreaterThan(0)
    }
  })
})

describe('every row is reachable by the copy it renders', () => {
  for (const row of searchableRows) {
    it(`${row.id} matches its own label, inside its own section`, () => {
      clearSearchIndex()
      const label = tString(row.labelKey)
      // Section-scoped, because that's the set `SettingsContent` gates the page on
      // and each section gates its rows on: a hit anywhere else is a blank pane.
      const ids = getMatchingSettingIdsInSection(label, row.section)
      expect([...ids]).toContain(row.id)
      expect(sectionHasMatches(row.section, getMatchingSections(label))).toBe(true)
    })
  }
})

describe('the aggregator', () => {
  it('lists every declaration file (an unaggregated `*.rows.ts` is dead weight)', () => {
    const declared = declarations.flatMap((d) => d.rows).map((r) => r.id)
    expect([...searchableRows.map((r) => r.id)].sort()).toEqual([...declared].sort())
  })
})
