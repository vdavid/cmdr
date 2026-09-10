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
import { describe, expect, it, vi } from 'vitest'
import { tString } from '$lib/intl/messages.svelte'
import { buildSectionTree, settingsRegistry, type SettingsSection } from '../settings-registry'
import {
  clearSearchIndex,
  getMatchingSections,
  getMatchingSettingIdsInSection,
  sectionHasMatches,
} from '../settings-search'
import type { SearchableRow, SearchableRowId, SettingId } from '../types'
import { searchableRows } from './searchable-rows'

/** On a Mac by default, so every row, `macOSOnly` ones too, is in the index under test. */
const isMacOS = vi.hoisted(() => vi.fn(() => true))

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/shortcuts/key-capture')>()),
  isMacOS: () => isMacOS(),
}))

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

describe('a row that anchors its section', () => {
  /**
   * ❗ The flag is only for a page NO setting would create. On a page that has
   * controls it would be a lie a reader has to check the registry to catch, and
   * the sibling `after` would silently do nothing.
   */
  it('names a page that carries no setting at all', () => {
    for (const row of searchableRows) {
      if (row.anchorsSection === undefined) continue
      const settingsOnThatPage = settingsRegistry.filter(
        (setting) => setting.section.join('/') === row.section.join('/'),
      )
      expect(settingsOnThatPage).toEqual([])
    }
  })

  it('puts the page in the tree, right after the sibling it names', () => {
    const tree = buildSectionTree()
    for (const row of searchableRows) {
      const anchor = row.anchorsSection
      if (anchor === undefined) continue

      let level = tree
      let node: SettingsSection | undefined
      for (const name of row.section) {
        node = level.find((section) => section.name === name)
        expect(node).toBeDefined()
        level = node?.subsections ?? []
      }

      // Its siblings are the level the page itself sits in.
      let siblings = tree
      for (const name of row.section.slice(0, -1)) {
        siblings = siblings.find((section) => section.name === name)?.subsections ?? []
      }
      const names = siblings.map((section) => section.name)
      expect(names.indexOf(row.section[row.section.length - 1])).toBe(names.indexOf(anchor.after) + 1)
    }
  })
})

/**
 * Off macOS a `macOSOnly` row's markup never renders, so a hit would land on a
 * page with nothing to show for it. The row stays declared; only the index drops it.
 */
describe('a macOS-only row', () => {
  it('stays out of the search index off macOS, while the other rows stay in', () => {
    const macOnly = searchableRows.filter((row) => row.macOSOnly === true)
    const everywhere = searchableRows.filter((row) => row.macOSOnly !== true)
    expect(macOnly.length).toBeGreaterThan(0)

    isMacOS.mockReturnValue(false)
    clearSearchIndex()
    try {
      for (const row of macOnly) {
        expect([...getMatchingSettingIdsInSection(tString(row.labelKey), row.section)]).not.toContain(row.id)
      }
      for (const row of everywhere) {
        expect([...getMatchingSettingIdsInSection(tString(row.labelKey), row.section)]).toContain(row.id)
      }
    } finally {
      isMacOS.mockReturnValue(true)
      clearSearchIndex()
    }
  })
})

describe('the aggregator', () => {
  it('lists every declaration file (an unaggregated `*.rows.ts` is dead weight)', () => {
    const declared = declarations.flatMap((d) => d.rows).map((r) => r.id)
    expect([...searchableRows.map((r) => r.id)].sort()).toEqual([...declared].sort())
  })
})
