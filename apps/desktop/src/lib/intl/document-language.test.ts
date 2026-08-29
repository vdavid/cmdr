/**
 * The `<html lang>` writer. Its whole job is one guarded attribute write, and
 * the guard is the part worth pinning: `setLocale()` runs under the SvelteKit
 * static adapter's Node pass too, where there is no document to write to.
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import { setDocumentLanguage } from './document-language'

afterEach(() => {
  document.documentElement.lang = 'en'
})

describe('setDocumentLanguage', () => {
  it('writes the tag onto the root element', () => {
    setDocumentLanguage('zh-Hant')
    expect(document.documentElement.lang).toBe('zh-Hant')
  })

  it('overwrites a previous tag rather than appending', () => {
    setDocumentLanguage('hu')
    setDocumentLanguage('sv')
    expect(document.documentElement.lang).toBe('sv')
  })

  it('does nothing when there is no document', () => {
    vi.stubGlobal('document', undefined)
    try {
      expect(() => {
        setDocumentLanguage('hu')
      }).not.toThrow()
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
