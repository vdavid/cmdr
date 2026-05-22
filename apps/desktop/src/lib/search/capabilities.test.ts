import { describe, expect, it } from 'vitest'
import {
  searchResultsVolumeCapabilities,
  SEARCH_RESULTS_NOT_A_FOLDER_TOAST,
  type SearchResultsCapabilities,
} from './capabilities'

describe('searchResultsVolumeCapabilities', () => {
  it('returns the documented flag set', () => {
    const caps = searchResultsVolumeCapabilities()
    expect(caps).toEqual({
      canPasteInto: false,
      canMkdir: false,
      canMkfile: false,
      canRename: false,
      isSourceOK: true,
    } satisfies SearchResultsCapabilities)
  })

  it('is a pure function: repeated calls return equal values', () => {
    expect(searchResultsVolumeCapabilities()).toEqual(searchResultsVolumeCapabilities())
  })

  it('exposes a friendly toast for shortcut-driven blocks', () => {
    expect(SEARCH_RESULTS_NOT_A_FOLDER_TOAST).toBe("Search results aren't a folder. Paste into a real folder instead.")
  })
})
