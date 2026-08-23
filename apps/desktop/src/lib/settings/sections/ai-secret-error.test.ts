import { describe, expect, it, beforeEach, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { describeSecretError } from './ai-secret-error'

function setUserAgent(value: string): void {
  Object.defineProperty(navigator, 'userAgent', { value, configurable: true })
}

// The titles/bodies resolve through the i18n catalog (`tString`); pin the base
// locale so the asserted en copy is deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

beforeEach(() => {
  // Default to macOS for tests; overridden case-by-case below. `isMacOS()` reads `userAgent`.
  setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36')
})

describe('describeSecretError', () => {
  it('recognises the typed AiApiKeyError access_denied variant on macOS', () => {
    const err = { type: 'access_denied', message: 'errSecAuthFailed' }
    const out = describeSecretError(err, 'save')
    expect(out.title).toContain('Keychain denied access')
    expect(out.body).toContain('Keychain Access')
    expect(out.level).toBe('error')
    expect(out.detail).toBe('errSecAuthFailed')
  })

  it('recognises access_denied on Linux', () => {
    setUserAgent('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36')
    const err = { type: 'access_denied', message: 'org.freedesktop.Secret.Error.IsLocked' }
    const out = describeSecretError(err, 'save')
    expect(out.title).toContain('keyring denied access')
    expect(out.body).toContain('Unlock')
  })

  it('falls back to generic copy for `other` variants', () => {
    const err = { type: 'other', message: 'disk quota exceeded' }
    const out = describeSecretError(err, 'save')
    expect(out.title).toContain("Couldn't save")
    expect(out.detail).toBe('disk quota exceeded')
  })

  it('switches the verb based on operation', () => {
    const err = { type: 'other', message: 'x' }
    expect(describeSecretError(err, 'save').title.toLowerCase()).toContain('save')
    expect(describeSecretError(err, 'read').title.toLowerCase()).toContain('read')
  })

  it("never guesses the classification out of the store's own words", () => {
    // Pre-fix this read `access_denied` out of the word "cancelled", so a copy
    // edit or a localized OS message silently changed which guidance appeared.
    const err = new Error('Operation was cancelled by the user')
    const out = describeSecretError(err, 'read')

    expect(out.title).toBe(describeSecretError({ type: 'other', message: 'anything' }, 'read').title)
    expect(out.title).not.toContain('Keychain')
  })

  it("keeps an untyped value's text as detail, never as the classification", () => {
    const out = describeSecretError('something went wrong', 'save')

    expect(out.detail).toBe('something went wrong')
    expect(out.title).not.toContain('something went wrong')
    expect(out.level).toBe('error')
  })

  it('treats not_found as generic rather than as denied access', () => {
    const out = describeSecretError({ type: 'not_found', message: 'no such key' }, 'read')

    expect(out.title).not.toContain('Keychain')
    expect(out.detail).toBe('no such key')
  })
})
