/**
 * Tests for `pane-footer.ts`, which decides whether a pane wears the status
 * footer at all and whether that footer talks about disk space.
 *
 * The rule worth pinning: a search-results pane HAS a footer (the user needs the
 * hit count and the selection size) but NO free-space readout and no usage bar,
 * because its rows aren't a place on any one disk the pane is sitting in.
 */

import { describe, it, expect } from 'vitest'
import { paneFooterVisibility } from './pane-footer'

describe('paneFooterVisibility', () => {
  it('gives a normal pane both the counts and the disk-space readout', () => {
    expect(paneFooterVisibility({ kind: 'normal', hasError: false })).toEqual({
      selectionInfo: true,
      volumeSpace: true,
    })
  })

  it('gives a search-results pane the counts without the disk-space readout', () => {
    expect(paneFooterVisibility({ kind: 'search-results', hasError: false })).toEqual({
      selectionInfo: true,
      volumeSpace: false,
    })
  })

  it('keeps the footer off the network view, which has nothing to count', () => {
    expect(paneFooterVisibility({ kind: 'network', hasError: false }).selectionInfo).toBe(false)
  })

  it('drops the whole footer while the pane shows an error', () => {
    expect(paneFooterVisibility({ kind: 'normal', hasError: true })).toEqual({
      selectionInfo: false,
      volumeSpace: false,
    })
    expect(paneFooterVisibility({ kind: 'search-results', hasError: true }).selectionInfo).toBe(false)
  })
})
