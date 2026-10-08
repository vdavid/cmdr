/**
 * The license key delivery mail: the one email in here a customer receives.
 *
 * It is the only one that speaks in the product's voice, the only one with a plain-text
 * alternative (it carries something the reader needs to copy, so it has to survive a client that
 * refuses HTML), and the only one styled with a `<head>` `<style>` block and class names rather
 * than the ops emails' inline chrome. That is why it shares nothing with `layout.ts` but the
 * escaping.
 */

import { Resend } from 'resend'
import type { LicenseType } from '../licensing/license'
import { sendViaResend } from './send'
import { escapeHtml } from './layout'

/**
 * One seat's license. `fullKey` is the signed key itself, which activates offline: the short code
 * only works while `/activate` can exchange it. Optional because a resend reads it back from KV, and
 * a missing entry there must not stop the codes going out.
 */
export interface EmailedLicense {
  shortCode: string
  fullKey?: string
}

interface RenderParams {
  customerName: string
  licenses: EmailedLicense[]
  productName: string
  supportEmail: string
  organizationName?: string
  licenseType?: LicenseType
  /**
   * ISO 8601, and only set for a license with a fixed end date, which today means a hand-issued
   * one. A Paddle subscription renews instead of ending, so it has no date to name here.
   */
  expiresAt?: string
  /**
   * True when we gave this license away rather than sold it. It changes what the email can honestly
   * claim: there was no purchase to thank anyone for.
   */
  issuedManually?: boolean
}

interface EmailParams extends RenderParams {
  to: string
  resendApiKey: string
}

/**
 * The one sentence saying how long the license lasts. Three shapes, and each has to be true on its
 * own, because this is what someone forwards to their IT department:
 *
 * - **Perpetual**: never ends.
 * - **Dated**: ends on a day we name, and nothing renews it. A hand-issued evaluation is this.
 * - **Subscription**: renews on its own until it's canceled, so it names no end date. Only a Paddle
 *   purchase is this, and ❌ its wording must never reach a dated license: promising a renewal to
 *   someone whose license simply stops is the kind of sentence that ends a deal.
 */
export function getLicenseDescription(type: LicenseType | undefined, orgName?: string, expiresAt?: string): string {
  const licensee = orgName ? ` for ${orgName}` : ''
  switch (type) {
    case 'commercial_subscription':
      if (expiresAt) {
        return `Your commercial license${licensee} is valid until ${expiresAt.slice(0, 10)}, and doesn't renew automatically.`
      }
      return `Your commercial license${licensee} is valid for one year and will auto-renew.`
    case 'commercial_perpetual':
      return `Your perpetual commercial license${licensee} is valid forever, with no renewal and no expiry.`
    default:
      return 'This is an unknown license type. This is weird. Please contact support.'
  }
}

/** The opening line. ❌ Never thank someone for a purchase they didn't make. */
export function getIntroText(count: number, productName: string, issuedManually: boolean): string {
  if (issuedManually) {
    return count > 1
      ? `Here are your ${String(count)} license keys for ${productName}:`
      : `Here's your license key for ${productName}:`
  }
  return count > 1
    ? `Thanks for purchasing ${String(count)} licenses for ${productName}! Here are your license keys:`
    : `Thanks for purchasing ${productName}! Here's your license key:`
}

/** The rendered mail, pure so tests can read every word a buyer gets. */
export function renderLicenseEmail(params: RenderParams): { subject: string; html: string; text: string } {
  const escapedCustomerName = escapeHtml(params.customerName)
  const escapedOrgName = params.organizationName ? escapeHtml(params.organizationName) : undefined
  const licenseDescriptionHtml = getLicenseDescription(params.licenseType, escapedOrgName, params.expiresAt)
  const licenseDescriptionText = getLicenseDescription(params.licenseType, params.organizationName, params.expiresAt)
  const orgLine = escapedOrgName ? `<p><strong>Licensed to:</strong> ${escapedOrgName}</p>` : ''
  const orgLineText = params.organizationName ? `Licensed to: ${params.organizationName}\n` : ''

  const count = params.licenses.length
  const isMultiple = count > 1
  const keyWord = isMultiple ? 'keys' : 'key'
  const subject = `Your ${params.productName} license ${keyWord} 🎉`
  const hasOfflineKeys = params.licenses.some((license) => license.fullKey)

  // HTML: one box per seat (numbered if several), each followed by that seat's offline key
  const licenseBoxesHtml = params.licenses
    .map((license, i) => {
      const number = isMultiple ? `<div class="license-number">License ${String(i + 1)} of ${String(count)}</div>` : ''
      const offlineKey = license.fullKey
        ? `<div class="offline-key"><div class="offline-key-label">Offline key</div>${escapeHtml(license.fullKey)}</div>`
        : ''
      return `<div class="license-box">${number}${license.shortCode}</div>${offlineKey}`
    })
    .join('\n')

  // Plain text: the same, with headers if multiple
  const licenseKeysText = params.licenses
    .map((license, i) => {
      const header = isMultiple ? `License ${String(i + 1)} of ${String(count)}:\n` : ''
      const offlineKey = license.fullKey ? `\nOffline key: ${license.fullKey}` : ''
      return `${header}${license.shortCode}${offlineKey}`
    })
    .join('\n\n')

  const offlineNoteHtml = hasOfflineKeys
    ? `<div class="note"><strong>No internet connection?</strong> Paste the offline key instead of the short one. It activates ${params.productName} without contacting our server, so keep this email: it'll set up a new Mac even if our server is ever unreachable.</div>`
    : ''
  const offlineNoteText = hasOfflineKeys
    ? `\nNo internet connection? Paste the offline key instead of the short one. It activates ${params.productName} without contacting our server, so keep this email: it'll set up a new Mac even if our server is ever unreachable.\n`
    : ''

  const introText = getIntroText(count, params.productName, params.issuedManually === true)

  const html = `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; }
        .license-box { background: #f5f5f5; border-radius: 8px; padding: 20px; margin: 20px 0 8px; font-family: monospace; font-size: 18px; text-align: center; letter-spacing: 2px; }
        .license-number { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; color: #666; margin-bottom: 8px; letter-spacing: normal; }
        .offline-key { font-family: monospace; font-size: 11px; color: #666; word-break: break-all; margin: 0 0 20px; padding: 0 4px; }
        .offline-key-label { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; margin-bottom: 4px; }
        .footer { margin-top: 40px; padding-top: 20px; border-top: 1px solid #eee; font-size: 14px; color: #666; }
        .note { background: #e8f4f8; border-left: 4px solid #0ea5e9; padding: 12px 16px; margin: 20px 0; }
    </style>
</head>
<body>
    <h1>Welcome to ${params.productName}! 🚀</h1>

    <p>Hey ${escapedCustomerName},</p>

    <p>${introText}</p>

    ${licenseBoxesHtml}

    ${orgLine}

    <h3>How to activate:</h3>
    <ol>
        <li>Open ${params.productName}</li>
        <li>Go to <strong>Cmdr menu → Enter license key...</strong></li>
        <li>Paste a key and click Activate</li>
    </ol>

    ${offlineNoteHtml}

    <p>${licenseDescriptionHtml}</p>

    <div class="note">
        <strong>Multiple machines?</strong> Each license lets you run ${params.productName} on multiple machines (like a laptop and desktop for remote debugging) as long as you're the only one using that license.
    </div>

    <div class="footer">
        <p>Questions? Just reply to this email or contact <a href="mailto:${params.supportEmail}">${params.supportEmail}</a></p>
        <p>Happy file managing! ⌘</p>
    </div>
</body>
</html>
        `.trim()

  const text = `
Welcome to ${params.productName}!

Hey ${params.customerName},

${introText}

${licenseKeysText}

${orgLineText}
How to activate:
1. Open ${params.productName}
2. Go to Cmdr menu → Enter license key...
3. Paste a key and click Activate
${offlineNoteText}
${licenseDescriptionText}

Multiple machines? Each license lets you run ${params.productName} on multiple machines (like a laptop and desktop for remote debugging) as long as you're the one using that license.

Questions? Contact ${params.supportEmail}

Happy file managing! ⌘
        `.trim()

  return { subject, html, text }
}

export async function sendLicenseEmail(params: EmailParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const { subject, html, text } = renderLicenseEmail(params)
  await sendViaResend(
    resend,
    { from: `${params.productName} <noreply@getcmdr.com>`, to: params.to, subject, html, text },
    'license',
  )
}
