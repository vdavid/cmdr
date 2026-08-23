/**
 * Every typed refusal an eject or a disconnect can ship has words, in every
 * locale that ships.
 *
 * The message table is exhaustive at the type level, so what these tests catch
 * is the other half: a variant whose catalog key was never added (which renders
 * the key itself, or an empty string), a message that breaks the error-copy
 * writing rules, and — the one that used to reach users — `diskutil`'s raw
 * English stderr leaking into the sentence a person reads.
 */
import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest'
import type { EjectError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { renderEjectError, ejectTechnicalDetail, wordEjectRefusal } from './eject-error-messages'
import { EjectFailure, asEjectError, throwEjectError } from './eject-error'

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** One value per `EjectError` variant, in declaration order. Adding a variant makes this list fail to typecheck. */
const EJECT_CASES: EjectError[] = [
  { type: 'busy' },
  { type: 'volumeNotFound', volumeId: 'volumes-usb-drive' },
  { type: 'mtpIdMissingDevicePrefix', volumeId: 'no-colon-id' },
  { type: 'notEjectable', volumeId: 'root' },
  { type: 'notAnSmbVolume', volumeId: 'volumes-usb-drive' },
  { type: 'mtpDisconnectRefused', detail: 'PTP CloseSession timed out' },
  { type: 'unmountRefused', detail: 'Unmount failed for /Volumes/Trip: in use by process 1234 (mds)' },
  { type: 'timedOut' },
  { type: 'unexpected', detail: 'the eject task panicked' },
]

/** The writing rules for error copy (`docs/guides/error-handling.md`). */
function assertErrorCopyRules(message: string, label: string): void {
  expect(message, `${label} must have words`).not.toBe('')
  expect(message, `${label} must resolve, not echo its key`).not.toMatch(/^errors\./)
  expect(message.toLowerCase(), `${label} must not say "error"`).not.toMatch(/\berror\b/)
  expect(message.toLowerCase(), `${label} must not say "failed"`).not.toMatch(/\bfailed\b/)
  for (const trivializer of ['just ', 'simply', 'simple ', 'easy ']) {
    expect(message.toLowerCase(), `${label} must not trivialize with "${trivializer.trim()}"`).not.toContain(
      trivializer,
    )
  }
  expect(message, `${label} must not leave a placeholder unfilled`).not.toMatch(/\{[a-zA-Z]+\}/)
}

describe('renderEjectError', () => {
  for (const error of EJECT_CASES) {
    it(`words ${error.type}`, () => {
      assertErrorCopyRules(renderEjectError(error), `eject ${error.type}`)
    })
  }

  it("says a timeout may still land, because the backend's deadline detaches rather than cancels", () => {
    expect(renderEjectError({ type: 'timedOut' }).toLowerCase()).toContain('may still')
  })

  it('never renders the untranslated OS text as the message', () => {
    const rendered = renderEjectError({
      type: 'unmountRefused',
      detail: 'Unmount failed for /Volumes/Trip: in use by process 1234 (mds)',
    })
    expect(rendered).not.toContain('mds')
    expect(rendered).not.toContain('/Volumes/Trip')
  })

  it('tells a busy drive apart from a drive the OS refused, which used to read the same', () => {
    expect(renderEjectError({ type: 'busy' })).not.toBe(renderEjectError({ type: 'unmountRefused', detail: 'x' }))
  })
})

describe('ejectTechnicalDetail', () => {
  it("hands back the OS's own words, which often name the process holding the drive", () => {
    expect(ejectTechnicalDetail({ type: 'unmountRefused', detail: 'in use by process 1234 (mds)' })).toBe(
      'in use by process 1234 (mds)',
    )
  })

  it('has nothing to add for a refusal that carries no diagnostic', () => {
    expect(ejectTechnicalDetail({ type: 'busy' })).toBeNull()
    expect(ejectTechnicalDetail({ type: 'notEjectable', volumeId: 'root' })).toBeNull()
  })
})

describe('EjectFailure', () => {
  it('survives the throw with its typed value intact', () => {
    try {
      throwEjectError({ type: 'unmountRefused', detail: 'in use by process 1234 (mds)' })
      expect.unreachable('throwEjectError must throw')
    } catch (e) {
      expect(asEjectError(e)).toEqual({ type: 'unmountRefused', detail: 'in use by process 1234 (mds)' })
      expect(e).toBeInstanceOf(Error)
    }
  })

  it('is not mistaken for some other error', () => {
    expect(asEjectError(new Error('something else'))).toBeNull()
    expect(asEjectError('a string')).toBeNull()
  })
})

describe('wordEjectRefusal', () => {
  it('words a typed refusal from the catalog', () => {
    expect(wordEjectRefusal(new EjectFailure({ type: 'busy' }))).toBe(renderEjectError({ type: 'busy' }))
  })

  it('falls back honestly when the transport itself broke, without showing the raw value', () => {
    const rendered = wordEjectRefusal(new Error('IPC channel closed'))
    expect(rendered).toBe(renderEjectError({ type: 'unexpected', detail: '' }))
    expect(rendered).not.toContain('IPC channel')
  })
})
