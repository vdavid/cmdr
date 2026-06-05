import { describe, expect, it } from 'vitest'
import { searchResultsVolumeCapabilities, SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from './capabilities'
import { capabilitiesForKind } from '$lib/file-explorer/pane/volume-capabilities'

describe('searchResultsVolumeCapabilities', () => {
  it('returns the search-results row of the per-kind capability table', () => {
    expect(searchResultsVolumeCapabilities()).toEqual(capabilitiesForKind('search-results'))
  })

  it('encodes the search-results pane rules (no destination ops, source ops OK)', () => {
    const caps = searchResultsVolumeCapabilities()
    expect(caps.canPasteInto).toBe(false)
    expect(caps.canCreateChild).toBe(false)
    expect(caps.canRenameInPlace).toBe(false)
    expect(caps.canBeSource).toBe(true)
  })

  it('is a pure function: repeated calls return equal values', () => {
    expect(searchResultsVolumeCapabilities()).toEqual(searchResultsVolumeCapabilities())
  })

  it('exposes a friendly toast for shortcut-driven blocks', () => {
    expect(SEARCH_RESULTS_NOT_A_FOLDER_TOAST).toBe("Search results aren't a folder. Paste into a real folder instead.")
  })
})
