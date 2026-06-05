import { describe, expect, it } from 'vitest'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from './capabilities'

describe('search-results capability strings', () => {
  it('exposes a friendly toast for shortcut-driven blocks', () => {
    expect(SEARCH_RESULTS_NOT_A_FOLDER_TOAST).toBe("Search results aren't a folder. Paste into a real folder instead.")
  })
})
