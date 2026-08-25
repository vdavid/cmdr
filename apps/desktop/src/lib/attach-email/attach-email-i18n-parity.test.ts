/**
 * Base-locale (en) parity net for the shared attach-email strings.
 *
 * The crash-report, error-report, and feedback dialogs all render these, so they live
 * under `common.attachEmail*` and are frozen here once. An intended copy edit lands in
 * the catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('attach-email copy parity (en)', () => {
  it('resolves the interpolated label', () => {
    expect(t('common.attachEmail', { email: 'alex@example.com' })).toBe(
      'Attach my email (alex@example.com) so we can reply',
    )
  })

  it('resolves the label that invites an address', () => {
    expect(tString('common.attachEmailPrompt')).toBe('Attach my email so we can reply')
  })

  it('resolves the field label and placeholder', () => {
    expect(tString('common.attachEmailInputLabel')).toBe('Your email address')
    expect(tString('common.attachEmailPlaceholder')).toBe('you@example.com')
  })

  it('resolves the validation message', () => {
    expect(tString('common.attachEmailInvalid')).toBe("That doesn't look like an email address")
  })
})
