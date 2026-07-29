/**
 * What happens to the current result set on ⏎, ⌥⏎, a row click, and the footer buttons.
 *
 * The subtle part is the Selection-style fallback: a consumer with no `secondaryAction`
 * sends every "open the cursor row" gesture to the primary action over the WHOLE set
 * instead. And ⌥⏎ carries a `length > 0` guard the footer's primary button doesn't, so a
 * shared helper would quietly change one of them.
 */

import { describe, it, expect } from 'vitest'
import type { SearchResultEntry } from '$lib/tauri-commands'
import {
  activatePrimary,
  activatePrimaryOnResults,
  activateResultAt,
  activateSecondaryAtCursor,
  dispatchEnterAction,
} from './result-actions'
import { makeQueryDialogConfig, sampleEntries } from './test-helpers'

function makeConfig(opts: { results?: SearchResultEntry[]; withSecondary?: boolean; withPrimary?: boolean } = {}) {
  const primary: SearchResultEntry[][] = []
  const secondary: SearchResultEntry[] = []
  const config = makeQueryDialogConfig({
    primaryAction:
      opts.withPrimary === false
        ? undefined
        : {
            label: 'Primary',
            shortcutHint: '⌥⏎',
            handler: (entries) => {
              primary.push(entries)
            },
          },
    secondaryAction: opts.withSecondary
      ? {
          label: 'Secondary',
          shortcutHint: '⏎',
          handler: (entry) => {
            secondary.push(entry)
          },
        }
      : undefined,
  })
  config.state.setResults(opts.results ?? [])
  return { config, primary, secondary }
}

describe('activatePrimary (footer primary button)', () => {
  it('hands the current result set to the primary action', () => {
    const { config, primary } = makeConfig({ results: sampleEntries(2) })
    activatePrimary(config)
    expect(primary).toHaveLength(1)
    expect(primary[0]).toHaveLength(2)
  })

  it('fires even with an empty set (the button owns its own disabled state)', () => {
    const { config, primary } = makeConfig({ results: [] })
    activatePrimary(config)
    expect(primary).toEqual([[]])
  })

  it('does nothing when the consumer wired no primary action', () => {
    const { config, primary } = makeConfig({ results: sampleEntries(2), withPrimary: false })
    activatePrimary(config)
    expect(primary).toEqual([])
  })
})

describe('activatePrimaryOnResults (⌥⏎)', () => {
  it('fires the primary action when there are results', () => {
    const { config, primary } = makeConfig({ results: sampleEntries(3) })
    activatePrimaryOnResults(config)
    expect(primary).toHaveLength(1)
  })

  it('stays quiet on an empty result set', () => {
    const { config, primary } = makeConfig({ results: [] })
    activatePrimaryOnResults(config)
    expect(primary).toEqual([])
  })
})

describe('activateSecondaryAtCursor (footer secondary button)', () => {
  it('acts on the cursor row', () => {
    const { config, secondary } = makeConfig({ results: sampleEntries(3), withSecondary: true })
    config.state.setCursorIndex(2)
    activateSecondaryAtCursor(config)
    expect(secondary.map((e) => e.name)).toEqual(['file-2.jpg'])
  })

  it('does nothing when the cursor points past the end', () => {
    const { config, secondary } = makeConfig({ results: sampleEntries(1), withSecondary: true })
    config.state.setCursorIndex(5)
    activateSecondaryAtCursor(config)
    expect(secondary).toEqual([])
  })

  it('does nothing for a consumer with no secondary action', () => {
    const { config, primary, secondary } = makeConfig({ results: sampleEntries(2) })
    activateSecondaryAtCursor(config)
    expect(secondary).toEqual([])
    expect(primary).toEqual([])
  })
})

describe('activateResultAt (row click)', () => {
  it('opens the clicked row through the secondary action', () => {
    const { config, secondary } = makeConfig({ results: sampleEntries(3), withSecondary: true })
    activateResultAt(config, 1)
    expect(secondary.map((e) => e.name)).toEqual(['file-1.jpg'])
  })

  it('falls back to the primary action over the whole set when there is no secondary', () => {
    const { config, primary } = makeConfig({ results: sampleEntries(3) })
    activateResultAt(config, 1)
    expect(primary).toHaveLength(1)
    expect(primary[0]).toHaveLength(3)
  })

  it('ignores an out-of-range index', () => {
    const { config, primary, secondary } = makeConfig({ results: sampleEntries(2), withSecondary: true })
    activateResultAt(config, 9)
    expect(secondary).toEqual([])
    expect(primary).toEqual([])
  })
})

describe('dispatchEnterAction (bare ⏎)', () => {
  it('runs the query when ⏎ owns "run-search"', () => {
    const { config, primary, secondary } = makeConfig({ results: sampleEntries(2), withSecondary: true })
    let ran = 0
    dispatchEnterAction(config, 'run-search', () => {
      ran += 1
    })
    expect(ran).toBe(1)
    expect(primary).toEqual([])
    expect(secondary).toEqual([])
  })

  it('opens the cursor row when ⏎ owns "go-to-file"', () => {
    const { config, secondary } = makeConfig({ results: sampleEntries(3), withSecondary: true })
    config.state.setCursorIndex(1)
    let ran = 0
    dispatchEnterAction(config, 'go-to-file', () => {
      ran += 1
    })
    expect(ran).toBe(0)
    expect(secondary.map((e) => e.name)).toEqual(['file-1.jpg'])
  })

  it('falls through to the primary action for a consumer with no secondary', () => {
    const { config, primary } = makeConfig({ results: sampleEntries(3) })
    dispatchEnterAction(config, 'go-to-file', () => {})
    expect(primary).toHaveLength(1)
    expect(primary[0]).toHaveLength(3)
  })

  it('does nothing on "go-to-file" with a cursor past the end', () => {
    const { config, secondary } = makeConfig({ results: sampleEntries(2), withSecondary: true })
    config.state.setCursorIndex(7)
    dispatchEnterAction(config, 'go-to-file', () => {})
    expect(secondary).toEqual([])
  })
})
