/**
 * Base-locale (en) parity net for the notifications i18n migration.
 *
 * The macOS notification permission-denied toast copy moved from a hardcoded
 * English literal into the `notifications.*` catalog. Behavior-preserving MOVE:
 * the rendered en string must be byte-identical to the pre-migration copy.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('notifications catalog parity (en)', () => {
  it('resolves the permission-denied toast copy', () => {
    expect(tString('notifications.permissionDenied')).toBe(
      'macOS notifications are off. Open System Settings to allow them.',
    )
  })
})
