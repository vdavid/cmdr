/**
 * Every way a server's share list can stay empty has words, in the person's own
 * language, and they follow the error-copy rules.
 *
 * The record in `share-list-error-messages.ts` is exhaustive at the type level;
 * these catch a variant whose catalog key was never added (the key itself
 * renders), copy that breaks `docs/guides/error-handling.md`, and the backend's
 * diagnostic `message` leaking into what a person reads.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import type { ShareListError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { renderShareListError } from './share-list-error-messages'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** The backend's own diagnostic, which must never reach the screen. */
const DIAGNOSTIC = 'smbutil failed: Connection refused (os error 61)'

/** One value per `ShareListError` variant. A mapped type, so a new variant stops this compiling until it's here. */
const CASES: { [K in ShareListError['type']]: Extract<ShareListError, { type: K }> } = {
  host_unreachable: { type: 'host_unreachable', message: DIAGNOSTIC },
  timeout: { type: 'timeout', message: DIAGNOSTIC },
  auth_required: { type: 'auth_required', message: DIAGNOSTIC },
  signing_required: { type: 'signing_required', message: DIAGNOSTIC },
  auth_failed: { type: 'auth_failed', message: DIAGNOSTIC },
  protocol_error: { type: 'protocol_error', message: DIAGNOSTIC },
  resolution_failed: { type: 'resolution_failed', message: DIAGNOSTIC },
  missing_dependency: { type: 'missing_dependency', message: DIAGNOSTIC, installCommand: 'sudo apt install smbclient' },
}

const ALL: ShareListError[] = Object.values(CASES)

describe('renderShareListError', () => {
  it('gives every variant a sentence from the catalog', () => {
    for (const error of ALL) {
      const message = renderShareListError(error, 'Naspolya')
      expect(message, `${error.type} must have words`).not.toBe('')
      expect(message, `${error.type} must resolve, not echo its key`).not.toMatch(/^errors\./)
      expect(message, `${error.type} must not leave a placeholder unfilled`).not.toMatch(/\{[a-zA-Z]+\}/)
    }
  })

  it('follows the error-copy writing rules', () => {
    for (const error of ALL) {
      const message = renderShareListError(error, 'Naspolya')
      const lower = message.toLowerCase()
      expect(lower, `${error.type} must not say "error"`).not.toMatch(/\berrors?\b/)
      expect(lower, `${error.type} must not say "failed"`).not.toMatch(/\bfail(ed|ure)?\b/)
      for (const trivializer of ['just', 'simply', 'simple', 'easy']) {
        expect(lower, `${error.type} must not trivialize with "${trivializer}"`).not.toMatch(
          new RegExp(`\\b${trivializer}\\b`),
        )
      }
      expect(message, `${error.type} must not use an em dash`).not.toContain('—')
    }
  })

  it('gives each variant its own words', () => {
    const rendered = new Set(ALL.map((error) => renderShareListError(error, 'Naspolya')))
    expect(rendered.size).toBe(ALL.length)
  })

  it('never shows the backend diagnostic, or the install command the pane shows on its own', () => {
    for (const error of ALL) {
      const message = renderShareListError(error, 'Naspolya')
      expect(message, error.type).not.toContain('smbutil')
      expect(message, error.type).not.toContain('os error')
      expect(message, error.type).not.toContain('sudo')
    }
  })

  it('names the server for everything but a missing local tool', () => {
    for (const error of ALL.filter((e) => e.type !== 'missing_dependency')) {
      expect(renderShareListError(error, 'Naspolya'), error.type).toContain('"Naspolya"')
    }
  })
})
