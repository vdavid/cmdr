/**
 * Base-locale (en) parity net for the shared attach-email strings.
 *
 * The crash-report, error-report, and feedback dialogs all render these, so they live
 * under `common.attachEmail*` and are frozen here once. An intended copy edit lands in
 * the catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { getMessage, t, tString } from '$lib/intl/messages.svelte'

/** Render a rich-text message the way `<Trans>` would, flattened back to plain text. */
function renderFlat(key: 'common.attachEmail', params: Record<string, unknown>): string {
  const parts = t(key, { ...params, change: (chunks: unknown[]) => chunks.join('') })
  return Array.isArray(parts) ? parts.join('') : String(parts)
}

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('attach-email copy parity (en)', () => {
  it('freezes the label source, tag and placeholder included', () => {
    // The `<change>` tag is what `AttachEmailCheckbox` maps to the Settings link, and the
    // param is named apart from it so `<Trans>` can't resolve one as the other.
    expect(getMessage('common.attachEmail')).toBe(
      'Attach my email address ({emailAddress} – <change>change</change>) so you can follow up',
    )
  })

  it('renders the label around the address and the link text', () => {
    expect(renderFlat('common.attachEmail', { emailAddress: 'alex@example.com' })).toBe(
      'Attach my email address (alex@example.com – change) so you can follow up',
    )
  })

  it('resolves the label that invites an address', () => {
    expect(tString('common.attachEmailPrompt')).toBe('Attach my email address so you can follow up')
  })

  it('resolves the field label and placeholder', () => {
    expect(tString('common.attachEmailInputLabel')).toBe('Your email address')
    expect(tString('common.attachEmailPlaceholder')).toBe('you@example.com')
  })

  it('resolves the validation message', () => {
    expect(tString('common.attachEmailInvalid')).toBe("That doesn't look like an email address")
  })
})
