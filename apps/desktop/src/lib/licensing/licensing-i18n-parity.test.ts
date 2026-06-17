/**
 * Base-locale (en) parity net for the licensing i18n migration.
 *
 * The About window, commercial-reminder and expiration modals, the license-key
 * dialog, and the settings License section moved their hardcoded English into the
 * `licensing.*` catalog (resolved through `t()` / `tString` / `<Trans>`). This is
 * a behavior-preserving MOVE: every rendered en string must be byte-identical to
 * the pre-migration copy. The goldens below are the literals that lived in those
 * components before the move; an intended copy edit lands in the catalog AND here
 * together, never silently. Apostrophe-doubling (`''`) and the inline-component
 * (`<Trans>`) tag handling are the ICU hazards this net guards.
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

/** Render a rich-text (`<Trans>`) message to a flat string for assertion: tag handlers wrap their inner chunks. */
function renderRich(key: Parameters<typeof t>[0], params: Record<string, unknown>, tags: string[]): string {
  const handlers: Record<string, unknown> = { ...params }
  for (const tag of tags) handlers[tag] = (chunks: unknown[]) => chunks.join('')
  const result = t(key, handlers as Parameters<typeof t>[1])
  return Array.isArray(result) ? result.join('') : String(result)
}

describe('About window copy (en)', () => {
  it('static strings', () => {
    expect(tString('licensing.about.srTitle')).toBe('About Cmdr')
    expect(tString('licensing.about.appName')).toBe('Cmdr')
    expect(tString('licensing.about.tagline')).toBe('Keyboard-driven file manager')
    expect(tString('licensing.about.noLicense')).toBe('No license – only personal use allowed')
    expect(tString('licensing.about.fallbackOrg')).toBe('your organization')
    expect(tString('licensing.about.aiAttribution')).toBe(
      'AI powered by Falcon-H1R-7B by Technology Innovation Institute (TII)',
    )
    expect(tString('licensing.about.copyright')).toBe('© 2024-2026 David Veszelovszki')
  })

  it('version + license-description interpolation', () => {
    expect(tString('licensing.about.version', { version: '1.4.2' })).toBe('Version 1.4.2 (open beta)')
    expect(tString('licensing.about.perpetual', { org: 'Acme' })).toBe('Perpetual commercial license for Acme')
    expect(tString('licensing.about.commercial', { org: 'Acme' })).toBe('Commercial license for Acme')
    expect(tString('licensing.about.commercialUntil', { org: 'Acme', date: 'June 15, 2026' })).toBe(
      'Commercial license for Acme, valid until June 15, 2026',
    )
  })

  it('beta note renders the GitHub link inline (apostrophe + tag hazard)', () => {
    expect(renderRich('licensing.about.betaNote', {}, ['github'])).toBe(
      'Found something bad? Tell me on GitHub. I read every report!',
    )
  })
})

describe('Commercial reminder modal copy (en)', () => {
  it('static strings, with apostrophes intact', () => {
    expect(tString('licensing.commercialReminder.title')).toBe('Thanks for using Cmdr!')
    expect(tString('licensing.commercialReminder.usingPersonal')).toBe("You're using a Personal license.")
    expect(tString('licensing.commercialReminder.askCommercial')).toBe(
      "If you're using Cmdr at work, please get a Commercial license to stay compliant.",
    )
    expect(tString('licensing.commercialReminder.priceInfo')).toBe(
      'Commercial licenses are $59/year/user and support continued development.',
    )
    expect(tString('licensing.commercialReminder.getCommercial')).toBe('Get commercial license')
  })

  it('decline button keeps the inline line break', () => {
    expect(renderRich('licensing.commercialReminder.declinePersonal', {}, ['break'])).toBe(
      'I only use Cmdrfor personal purposes',
    )
  })
})

describe('Expiration modal copy (en)', () => {
  it('static + interpolated strings', () => {
    expect(tString('licensing.expiration.title')).toBe('Your commercial license has expired')
    expect(tString('licensing.expiration.info')).toBe(
      "Cmdr is now running in personal use mode. If you're still using it for work, please renew your license.",
    )
    expect(tString('licensing.expiration.renew')).toBe('Renew license')
    expect(tString('licensing.expiration.continue')).toBe('Continue in personal mode')
  })

  it('emphasized org + date render inline', () => {
    expect(renderRich('licensing.expiration.orgName', { org: 'Acme' }, ['strong'])).toBe('License for: Acme')
    expect(renderRich('licensing.expiration.message', { date: 'March 1, 2026' }, ['strong'])).toBe(
      'Your commercial subscription expired on March 1, 2026.',
    )
  })
})

describe('License key dialog copy (en)', () => {
  it('titles, labels, and validity values', () => {
    expect(tString('licensing.dialog.loading')).toBe('Loading...')
    expect(tString('licensing.dialog.detailsTitle')).toBe('License details')
    expect(tString('licensing.dialog.enterTitle')).toBe('Enter license key')
    expect(tString('licensing.dialog.validityNotYetVerified')).toBe('Not yet verified')
    expect(tString('licensing.dialog.validityPerpetualUntil', { date: 'June 15, 2026' })).toBe(
      'Perpetual: updates until June 15, 2026',
    )
    expect(tString('licensing.dialog.validityValidUntil', { date: 'June 15, 2026' })).toBe('Valid until June 15, 2026')
    expect(tString('licensing.dialog.validityExpiredOn', { date: 'June 15, 2026' })).toBe('Expired on June 15, 2026')
  })

  it('pending hint with the day count', () => {
    expect(tString('licensing.dialog.pendingHint', { days: 7 })).toBe(
      "We'll verify with the server automatically within 7 days.",
    )
  })

  it('enter prompt + placeholder', () => {
    expect(renderRich('licensing.dialog.enterPrompt', {}, ['getLicense'])).toBe(
      "Paste your license key from the email you received after purchase. Don't have one yet? Get a license.",
    )
    expect(tString('licensing.dialog.inputPlaceholder')).toBe('Example: CMDR-ABCD-EFGH-1234')
  })

  it('activation toasts and errors', () => {
    expect(tString('licensing.dialog.activatedToast')).toBe('License activated. Thanks for your support! ❤️')
    expect(tString('licensing.dialog.activatedToastNamed', { org: 'Acme' })).toBe(
      'Welcome aboard, Acme! Thanks for your support. ❤️',
    )
    expect(tString('licensing.dialog.expiredOnError', { date: 'June 15, 2026' })).toBe(
      'This license expired on June 15, 2026.',
    )
    expect(tString('licensing.dialog.serverInvalidError')).toBe(
      "We know this key but when we checked it with our payment provider, it didn't recognize it. This can happen if the purchase was refunded or not cleared.",
    )
  })

  it('contact-support help lines render the email link inline', () => {
    expect(renderRich('licensing.dialog.serverInvalidBanner', { email: 'hello@getcmdr.com' }, ['supportEmail'])).toBe(
      "This key couldn't be verified with the server. Please try a different key or email us at hello@getcmdr.com.",
    )
    expect(
      renderRich('licensing.dialog.retryExhausted', { count: 3, email: 'hello@getcmdr.com' }, ['supportEmail']),
    ).toBe(
      "We've tried 3 times and it didn't work. We're sorry for the trouble. Please drop us a message at hello@getcmdr.com and we'll sort it out.",
    )
    expect(renderRich('licensing.dialog.serverInvalidHelp', { email: 'hello@getcmdr.com' }, ['supportEmail'])).toBe(
      "If you believe this is a mistake, email us at hello@getcmdr.com and we'll sort it out.",
    )
    expect(renderRich('licensing.dialog.genericHelp', { email: 'hello@getcmdr.com' }, ['supportEmail'])).toBe(
      'If you need help, contact us at hello@getcmdr.com.',
    )
  })

  it('prefilled support email body keeps newlines and the key', () => {
    expect(tString('licensing.dialog.mailtoSubject')).toBe('License key issue')
    expect(tString('licensing.dialog.mailtoBody', { key: 'CMDR-1234' })).toBe(
      "Hi,\n\nI'm having trouble activating my license key:\nCMDR-1234\n\n",
    )
  })
})

describe('Activation error messages (en)', () => {
  it('classified error + hint pairs', () => {
    expect(tString('licensing.error.badSignature')).toBe(
      "This license key failed our signature verification, meaning that it doesn't look like a valid key.",
    )
    expect(tString('licensing.error.badFormatHint')).toBe(
      'License keys are either a short code (CMDR-XXXX-XXXX-XXXX) or a longer cryptographic key from your purchase email.',
    )
    expect(tString('licensing.error.network')).toBe("Ouch, we couldn't reach the license server this time.")
    expect(tString('licensing.error.server')).toBe(
      "Hmm, the license server responded with something weird. We're sorry about that.",
    )
    expect(tString('licensing.error.generic')).toBe('Something went wrong when activating this key.')
    expect(tString('licensing.error.genericHint')).toBe(
      "Please try again. If the problem persists, email us and we'll help.",
    )
  })
})

describe('Settings License section copy (en)', () => {
  it('section chrome and labels', () => {
    expect(tString('licensing.section.title')).toBe('License')
    expect(tString('licensing.section.manageKey')).toBe('Manage license key')
    expect(tString('licensing.section.enterKey')).toBe('Enter license key')
    expect(tString('licensing.section.getLicense')).toBe('Get a license')
    expect(tString('licensing.section.typePersonal')).toBe('Personal (free)')
    expect(tString('licensing.section.statusValidUntil', { date: 'June 15, 2026' })).toBe('Valid until June 15, 2026')
    expect(tString('licensing.section.statusUpdatesUntil', { date: 'June 15, 2026' })).toBe(
      'Updates until June 15, 2026',
    )
  })
})
