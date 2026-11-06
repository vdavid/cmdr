import { describe, it, expect } from 'vitest'
import { defaultMacOnlyFiles, macOnlyFilesFor, macOnlyLinkKey, macOnlyToggleStartsOn } from './mac-only-files'

describe('the “Skip Mac-only hidden files” setting', () => {
  it('ticks the toggle only for a skip default; “Always ask” starts it off', () => {
    expect(macOnlyToggleStartsOn('skip')).toBe(true)
    expect(macOnlyToggleStartsOn('include')).toBe(false)
    expect(macOnlyToggleStartsOn('ask')).toBe(false)
  })

  it('sends what the toggle says', () => {
    expect(macOnlyFilesFor(true)).toBe('skip')
    expect(macOnlyFilesFor(false)).toBe('include')
  })

  it('includes them when nobody saw the toggle, unless the default is to skip', () => {
    expect(defaultMacOnlyFiles('skip')).toBe('skip')
    expect(defaultMacOnlyFiles('include')).toBe('include')
    expect(defaultMacOnlyFiles('ask')).toBe('include')
  })

  it('offers to stop asking, or names the default it started from', () => {
    expect(macOnlyLinkKey('ask')).toBe('fileOperations.transferDialog.macOnlyFiles.dontAskAgain')
    expect(macOnlyLinkKey('include')).toBe('fileOperations.transferDialog.macOnlyFiles.includingByDefault')
    expect(macOnlyLinkKey('skip')).toBe('fileOperations.transferDialog.macOnlyFiles.skippingByDefault')
  })
})
