/**
 * Settings search is reactive to the UI language: a `$derived` (or markup) over a search re-runs on
 * a language switch, so the sidebar's lit sections and the shown pages follow a switch made
 * mid-search instead of keeping the old language's match set until the next keystroke.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { flushSync } from 'svelte'
import { setLocale, _clearCompiledCacheForTests } from '#lib/intl/messages.svelte.js'
import { getMatchingSections, getMatchingSettingIdsInSection } from './settings-search'

let dispose: (() => void) | undefined

afterEach(() => {
  dispose?.()
  dispose = undefined
  setLocale(null)
  _clearCompiledCacheForTests()
})

describe('settings search follows a language switch', () => {
  it('re-derives the lit sections without a new query', () => {
    setLocale('en')
    // Warm the cache outside any reactive context, as the first keystroke would: the derived
    // below then hits cached results and must still learn that the language matters.
    getMatchingSections('témamód')
    let sections = new Set<string>()
    dispose = $effect.root(() => {
      // "Témamód" is the Hungarian theme label: nothing in English matches it.
      const matching = $derived(getMatchingSections('témamód'))
      $effect(() => {
        sections = matching
      })
    })
    flushSync()
    expect(sections.has('Appearance')).toBe(false)

    setLocale('hu')
    flushSync()
    expect(sections.has('Appearance')).toBe(true)
  })

  it('re-derives a section’s match set without a new query', () => {
    setLocale('en')
    getMatchingSettingIdsInSection('témamód', ['Appearance'])
    let size = -1
    dispose = $effect.root(() => {
      const ids = $derived(getMatchingSettingIdsInSection('témamód', ['Appearance']))
      $effect(() => {
        size = ids.size
      })
    })
    flushSync()
    expect(size).toBe(0)

    setLocale('hu')
    flushSync()
    expect(size).toBeGreaterThan(0)
  })
})
