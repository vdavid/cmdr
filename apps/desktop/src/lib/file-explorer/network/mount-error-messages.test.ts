/**
 * Every typed refusal a share mount can ship has words, in the person's own
 * language, and they follow the error-copy rules.
 *
 * The record in `mount-error-messages.ts` is exhaustive at the type level, so
 * what these catch is the other half: a variant whose catalog key was never
 * added (which renders the key itself) and copy that breaks
 * `docs/guides/error-handling.md`. Both fail silently at runtime.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import type { MountError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { renderMountError } from './mount-error-messages'
import { MountFailure, asMountError } from './mount-error'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

const ADDRESS = '192.168.1.111'
const SHARE = 'data'

/** One value per `MountError` variant. A mapped type, so a new variant stops this compiling until it's here. */
const CASES: { [K in MountError['type']]: Extract<MountError, { type: K }> } = {
  host_unreachable: { type: 'host_unreachable', server: ADDRESS },
  timeout: { type: 'timeout', server: ADDRESS },
  share_not_found: { type: 'share_not_found', server: ADDRESS, share: SHARE },
  auth_required: { type: 'auth_required', server: ADDRESS, share: SHARE },
  auth_failed: { type: 'auth_failed', server: ADDRESS },
  permission_denied: { type: 'permission_denied', server: ADDRESS, share: SHARE, username: 'ada' },
  cancelled: { type: 'cancelled', share: SHARE },
  unsupported_protocol: { type: 'unsupported_protocol', server: ADDRESS },
  mount_refused: { type: 'mount_refused', server: ADDRESS, share: SHARE },
  mount_missing: { type: 'mount_missing', server: ADDRESS, share: SHARE },
  gvfs_missing: { type: 'gvfs_missing' },
  unexpected: { type: 'unexpected', server: ADDRESS, share: SHARE, detail: 'NetFS answered -1234' },
}

const ALL: MountError[] = Object.values(CASES)

describe('renderMountError', () => {
  it('gives every variant a sentence from the catalog', () => {
    for (const error of ALL) {
      const message = renderMountError(error)
      expect(message, `${error.type} must have words`).not.toBe('')
      expect(message, `${error.type} must resolve, not echo its key`).not.toMatch(/^errors\./)
      expect(message, `${error.type} must not leave a placeholder unfilled`).not.toMatch(/\{[a-zA-Z]+\}/)
    }
  })

  it('follows the error-copy writing rules', () => {
    for (const error of ALL) {
      const message = renderMountError(error)
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
    const rendered = new Set(ALL.map((error) => renderMountError(error)))
    expect(rendered.size).toBe(ALL.length)
  })

  it('names the share and the server wherever the variant knows them', () => {
    for (const error of ALL) {
      const message = renderMountError(error)
      if ('share' in error) expect(message, `the share, in ${error.type}`).toContain(`"${SHARE}"`)
      if ('server' in error) expect(message, `the server, in ${error.type}`).toContain(`"${ADDRESS}"`)
    }
    expect(renderMountError(CASES.permission_denied), 'the refused account').toContain('"ada"')
  })

  it('prefers the name the pane shows for the server over the address the mount used', () => {
    for (const error of ALL.filter((e) => 'server' in e)) {
      const message = renderMountError(error, 'Naspolya')
      expect(message, error.type).toContain('"Naspolya"')
      expect(message, error.type).not.toContain(ADDRESS)
    }
  })

  it('never shows the diagnostic detail', () => {
    expect(renderMountError(CASES.unexpected)).not.toContain('-1234')
  })
})

describe('MountFailure', () => {
  it('carries the typed refusal across a throw', () => {
    const caught: unknown = (() => {
      try {
        throw new MountFailure(CASES.mount_missing)
      } catch (e) {
        return e
      }
    })()
    expect(caught).toBeInstanceOf(Error)
    expect(asMountError(caught)).toEqual(CASES.mount_missing)
  })

  it('reads nothing typed out of a value that is not one', () => {
    expect(asMountError(new Error('ipc down'))).toBeNull()
    expect(asMountError({ type: 'timeout', server: ADDRESS })).toBeNull()
  })
})
