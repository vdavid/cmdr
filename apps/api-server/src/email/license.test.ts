import { describe, expect, it } from 'vitest'
import { getIntroText, getLicenseDescription, renderLicenseEmail } from './license'

/**
 * The copy in the one email a customer receives. It gets forwarded to purchasing departments and
 * IT reviewers, so each sentence has to be true on its own terms, and a claim that belongs to one
 * kind of license must not reach another.
 */
describe('getLicenseDescription', () => {
  it('promises no end date for a perpetual license', () => {
    const line = getLicenseDescription('commercial_perpetual', 'Acme')

    expect(line).toBe('Your perpetual commercial license for Acme is valid forever, with no renewal and no expiry.')
  })

  it('names the end date for a dated license, and says nothing renews it', () => {
    const line = getLicenseDescription('commercial_subscription', 'Acme', '2026-12-15T18:05:57.922Z')

    expect(line).toBe("Your commercial license for Acme is valid until 2026-12-15, and doesn't renew automatically.")
  })

  it('never tells a dated license it will auto-renew', () => {
    // The sentence someone whose evaluation license simply stops must never receive.
    const line = getLicenseDescription('commercial_subscription', undefined, '2026-12-15T18:05:57.922Z')

    expect(line).not.toContain('auto-renew')
    expect(line).toContain('2026-12-15')
  })

  it('still promises the renewal to a real subscription, which does renew', () => {
    expect(getLicenseDescription('commercial_subscription')).toBe(
      'Your commercial license is valid for one year and will auto-renew.',
    )
  })

  it('leaves the organization out when there isn’t one', () => {
    expect(getLicenseDescription('commercial_perpetual')).toBe(
      'Your perpetual commercial license is valid forever, with no renewal and no expiry.',
    )
  })
})

/**
 * The short code needs our server to turn into a key; the full key doesn't. Carrying both means a
 * buyer can set up a new Mac even when `api.getcmdr.com` is unreachable or gone.
 */
describe('renderLicenseEmail', () => {
  const base = {
    customerName: 'Dana',
    productName: 'Cmdr',
    supportEmail: 'hello@getcmdr.com',
    licenseType: 'commercial_perpetual' as const,
  }

  it('carries the full offline key beside the short code, in HTML and in plain text', () => {
    const email = renderLicenseEmail({
      ...base,
      licenses: [{ shortCode: 'CMDR-ABCD-EFGH-2345', fullKey: 'eyJwYXlsb2FkIjp0cnVlfQ==.c2lnbmF0dXJl' }],
    })

    for (const body of [email.html, email.text]) {
      expect(body).toContain('CMDR-ABCD-EFGH-2345')
      expect(body).toContain('eyJwYXlsb2FkIjp0cnVlfQ==.c2lnbmF0dXJl')
    }
  })

  it('pairs each seat’s full key with its own code', () => {
    const email = renderLicenseEmail({
      ...base,
      licenses: [
        { shortCode: 'CMDR-AAAA-AAAA-AAAA', fullKey: 'first.key' },
        { shortCode: 'CMDR-BBBB-BBBB-BBBB', fullKey: 'second.key' },
      ],
    })

    expect(email.text.indexOf('first.key')).toBeLessThan(email.text.indexOf('CMDR-BBBB-BBBB-BBBB'))
    expect(email.text.indexOf('second.key')).toBeGreaterThan(email.text.indexOf('CMDR-BBBB-BBBB-BBBB'))
  })

  it('still sends the code alone when a seat’s full key is missing', () => {
    const email = renderLicenseEmail({ ...base, licenses: [{ shortCode: 'CMDR-ABCD-EFGH-2345' }] })

    expect(email.text).toContain('CMDR-ABCD-EFGH-2345')
    expect(email.text).not.toContain('Offline key')
  })
})

describe('getIntroText', () => {
  it('thanks a buyer for the purchase they made', () => {
    expect(getIntroText(1, 'Cmdr', false)).toBe("Thanks for purchasing Cmdr! Here's your license key:")
    expect(getIntroText(3, 'Cmdr', false)).toBe(
      'Thanks for purchasing 3 licenses for Cmdr! Here are your license keys:',
    )
  })

  it('thanks nobody for a license we gave away', () => {
    expect(getIntroText(1, 'Cmdr', true)).toBe("Here's your license key for Cmdr:")
    expect(getIntroText(1, 'Cmdr', true)).not.toContain('purchas')
  })
})
