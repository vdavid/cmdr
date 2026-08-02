/**
 * Base-locale (en) parity net for the shared attach-email label.
 *
 * The crash-report, error-report, and feedback dialogs all render this one string, so it
 * lives in `common.attachEmail` and is frozen here once. An intended copy edit lands in
 * the catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('attach-email label copy parity (en)', () => {
  it('resolves the interpolated label', () => {
    expect(t('common.attachEmail', { email: 'alex@example.com' })).toBe(
      'Attach my email (alex@example.com) so we can reply',
    )
  })
})
