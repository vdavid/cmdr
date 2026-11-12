import { describe, expect, it, beforeEach, beforeAll, afterAll, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { AiSecretFailure, describeSecretError } from './ai-secret-error'

// The titles/bodies resolve through the i18n catalog (`tString`); pin the base
// locale so the asserted en copy is deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

// macOS by default; a Windows or Linux case says so.
beforeEach(() => withPlatform('macos'))

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
    onTestFinished(withPlatform('linux'))
    const err = { type: 'access_denied', message: 'org.freedesktop.Secret.Error.IsLocked' }
    const out = describeSecretError(err, 'save')
    expect(out.title).toContain('keyring denied access')
    expect(out.body).toContain('Unlock')
  })

  it('recognises access_denied on Windows', () => {
    onTestFinished(withPlatform('windows'))
    const out = describeSecretError({ type: 'access_denied', message: 'ERROR_ACCESS_DENIED' }, 'read')
    expect(out.title).toBe('Couldn’t read your saved API key: Credential Manager denied access')
    expect(out.body).toBe(
      'Open Credential Manager, find the “Cmdr” entry under Windows Credentials, or remove it and enter your key again.',
    )
  })

  it.each([
    ['macos', 'Open Keychain Access, find the “Cmdr” entry, and delete it there.'],
    ['windows', 'Open Credential Manager, find the “Cmdr” entry under Windows Credentials, and remove it there.'],
    ['linux', 'Unlock your keyring (Passwords / Keyrings app) and try again.'],
  ] as const)('tells a %s user where a key that couldn’t be removed lives', (platform, body) => {
    onTestFinished(withPlatform(platform))
    const out = describeSecretError({ type: 'access_denied', message: 'x' }, 'remove')
    expect(out.title).toBe('Couldn’t remove your saved API key.')
    expect(out.body).toBe(body)
  })

  it.each([
    ['macos', 'Check that your Keychain is available, then try again. If it persists, restart the app.'],
    ['windows', 'Check that Credential Manager is available, then try again. If it keeps happening, restart Cmdr.'],
    ['linux', 'Check that your system keyring is available, then try again. If it persists, restart the app.'],
  ] as const)('names the %s secret store in the generic notice', (platform, body) => {
    onTestFinished(withPlatform(platform))
    for (const operation of ['save', 'read', 'remove'] as const) {
      expect(describeSecretError({ type: 'other', message: 'x' }, operation).body).toBe(body)
    }
  })

  it('falls back to generic copy for `other` variants', () => {
    const err = { type: 'other', message: 'disk quota exceeded' }
    const out = describeSecretError(err, 'save')
    expect(out.title).toContain('Couldn’t save')
    expect(out.detail).toBe('disk quota exceeded')
  })

  it('switches the verb based on operation', () => {
    const err = { type: 'other', message: 'x' }
    expect(describeSecretError(err, 'save').title.toLowerCase()).toContain('save')
    expect(describeSecretError(err, 'read').title.toLowerCase()).toContain('read')
  })

  it('never guesses the classification out of the store’s own words', () => {
    // Pre-fix this read `access_denied` out of the word "cancelled", so a copy
    // edit or a localized OS message silently changed which guidance appeared.
    const err = new Error('Operation was cancelled by the user')
    const out = describeSecretError(err, 'read')

    expect(out.title).toBe(describeSecretError({ type: 'other', message: 'anything' }, 'read').title)
    expect(out.title).not.toContain('Keychain')
  })

  it('keeps an untyped value’s text as detail, never as the classification', () => {
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

describe('AiSecretFailure', () => {
  it('carries the store’s own words into its diagnostic, so the log line says why', () => {
    // The Rust save and status paths don't log, so this diagnostic is the only record of
    // what the Keychain or keyring said. The store's message never holds the key itself.
    const failure = new AiSecretFailure({ type: 'access_denied', message: 'User interaction is not allowed.' })

    expect(String(failure)).toContain('access_denied')
    expect(String(failure)).toContain('User interaction is not allowed.')
  })
})
