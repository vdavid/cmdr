import { describe, it, expect } from 'vitest'
import { binaryWarningKey } from './binary-warning'

describe('binaryWarningKey', () => {
  it('names both keys while both run their commands', () => {
    expect(binaryWarningKey({ quickLookKey: '⇧Space', openKey: '↩' })).toBe('viewer.binaryWarning.bodyWithKeys')
  })

  it('drops the open key once no key opens the file', () => {
    expect(binaryWarningKey({ quickLookKey: '⇧Space', openKey: undefined })).toBe(
      'viewer.binaryWarning.bodyWithQuickLookKey',
    )
  })

  // Off macOS there's no Quick Look, so its key is always missing there.
  it('drops Quick Look once no key runs it', () => {
    expect(binaryWarningKey({ quickLookKey: undefined, openKey: 'Enter' })).toBe('viewer.binaryWarning.bodyWithOpenKey')
  })

  it('names no key once neither runs', () => {
    expect(binaryWarningKey({ quickLookKey: undefined, openKey: undefined })).toBe(
      'viewer.binaryWarning.bodyWithoutKeys',
    )
  })
})
