import { describe, it, expect } from 'vitest'
import { binaryWarningKey } from './binary-warning'

describe('binaryWarningKey', () => {
  it('names both keys while both run their commands', () => {
    expect(binaryWarningKey({ quickLookBound: true, openBound: true })).toBe('viewer.binaryWarning.bodyWithKeys')
  })

  it('drops the open key once no key opens the file', () => {
    expect(binaryWarningKey({ quickLookBound: true, openBound: false })).toBe(
      'viewer.binaryWarning.bodyWithQuickLookKey',
    )
  })

  // Off macOS there's no Quick Look, so its key is always missing there.
  it('drops Quick Look once no key runs it', () => {
    expect(binaryWarningKey({ quickLookBound: false, openBound: true })).toBe('viewer.binaryWarning.bodyWithOpenKey')
  })

  it('names no key once neither runs', () => {
    expect(binaryWarningKey({ quickLookBound: false, openBound: false })).toBe('viewer.binaryWarning.bodyWithoutKeys')
  })
})
