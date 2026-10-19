import { afterEach, describe, expect, it } from 'vitest'
import { setLocale } from '#lib/intl/messages.svelte.js'
import { searchResultsNotAFolderToast } from './capabilities'

afterEach(() => {
  setLocale(null)
})

describe('search-results capability strings', () => {
  it('exposes a friendly toast for shortcut-driven blocks', () => {
    expect(searchResultsNotAFolderToast()).toBe('Search results aren’t a folder. Paste into a real folder instead.')
  })

  it('speaks the language picked after the module loaded', () => {
    setLocale('hu')
    expect(searchResultsNotAFolderToast()).toBe('A találatok nem egy mappa. Illeszd be inkább egy valódi mappába.')
  })
})
