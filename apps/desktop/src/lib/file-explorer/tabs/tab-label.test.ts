import { describe, it, expect } from 'vitest'
import { deriveTabLabel } from './tab-label'

describe('deriveTabLabel', () => {
  it('shows "/" at an MTP storage root instead of the raw storage id', () => {
    // At the MTP storage root the last path segment is the raw storage id
    // (here 65537 = 0x10001 for Internal Storage), which used to surface as
    // the tab title "65537". A storage root carries no user-meaningful
    // basename, so the tab shows "/" — the same thing a local filesystem
    // root shows.
    expect(deriveTabLabel('mtp://0-5/65537')).toBe('/')
  })

  it('shows the folder name for an MTP subfolder', () => {
    expect(deriveTabLabel('mtp://0-5/65537/DCIM/Camera')).toBe('Camera')
    expect(deriveTabLabel('mtp://0-5/65537/DCIM')).toBe('DCIM')
  })

  it('shows "/" for the local filesystem root (convention pinned)', () => {
    expect(deriveTabLabel('/')).toBe('/')
  })

  it('shows the basename for a local subfolder (convention pinned)', () => {
    expect(deriveTabLabel('/Users/john/Documents')).toBe('Documents')
  })

  it('shows the volume folder name for a mounted-volume root (convention pinned)', () => {
    // A mounted volume root like /Volumes/USB keeps its basename ("USB"),
    // unchanged from the local convention — we only special-case the MTP
    // storage root, not every volume root.
    expect(deriveTabLabel('/Volumes/USB')).toBe('USB')
  })
})
