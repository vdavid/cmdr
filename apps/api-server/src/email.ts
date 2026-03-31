import { Resend } from 'resend'
import type { LicenseType } from './license'

export interface CrashSummaryEntry {
  topFunction: string
  count: number
  versions: string[]
  mostRecent: string
}

interface CrashNotificationParams {
  crashes: CrashSummaryEntry[]
  totalCount: number
  to: string
  resendApiKey: string
}

export async function sendCrashNotificationEmail(params: CrashNotificationParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = `Cmdr: ${String(params.totalCount)} new crash report${params.totalCount === 1 ? '' : 's'}`

  const tableRows = params.crashes
    .map(
      (entry) => `
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace; font-size: 13px;">${escapeHtml(entry.topFunction)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: center;">${String(entry.count)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px;">${escapeHtml(entry.versions.join(', '))}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px;">${escapeHtml(entry.mostRecent)}</td>
        </tr>`,
    )
    .join('\n')

  await resend.emails.send({
    from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
    to: params.to,
    subject,
    html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #dc2626;">${escapeHtml(subject)}</h2>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <thead>
            <tr>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Crash site</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: center; background: #f9fafb;">Count</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Versions</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Most recent</th>
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server.
    </p>
</body>
</html>
        `.trim(),
  })
}

interface DbSizeAlertParams {
  sizeMb: number
  tableCounts: Record<string, number>
  to: string
  resendApiKey: string
}

export async function sendDbSizeAlert(params: DbSizeAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = `Cmdr: telemetry DB is ${String(Math.round(params.sizeMb))} MB`

  const tableRows = Object.entries(params.tableCounts)
    .map(
      ([table, count]) => `
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">${escapeHtml(table)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: right;">${String(count)}</td>
        </tr>`,
    )
    .join('\n')

  await resend.emails.send({
    from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
    to: params.to,
    subject,
    html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #d97706;">${escapeHtml(subject)}</h2>

    <p>The telemetry D1 database has reached <strong>${String(Math.round(params.sizeMb))} MB</strong>. Consider reviewing and pruning old data.</p>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <thead>
            <tr>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Table</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: right; background: #f9fafb;">Row count</th>
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server. Threshold: 100 MB.
    </p>
</body>
</html>
        `.trim(),
  })
}

interface DeviceCountAlertParams {
  seatTransactionId: string
  baseTransactionId: string
  deviceCount: number
  customerEmail: string
  resendApiKey: string
  paddleEnvironment: 'sandbox' | 'live'
}

export async function sendDeviceCountAlert(params: DeviceCountAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const paddleDomain = params.paddleEnvironment === 'sandbox' ? 'sandbox-vendors.paddle.com' : 'vendors.paddle.com'
  const paddleUrl = `https://${paddleDomain}/transactions-v2/${params.baseTransactionId}`

  await resend.emails.send({
    from: 'Cmdr License Alerts <noreply@getcmdr.com>',
    to: 'legal@getcmdr.com',
    subject: `Device count alert: ${params.seatTransactionId} (${String(params.deviceCount)} devices)`,
    html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #d97706;">Device count alert</h2>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Seat transaction ID</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">${escapeHtml(params.seatTransactionId)}</td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Base transaction</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">
                <a href="${escapeHtml(paddleUrl)}" style="color: #2563eb;">${escapeHtml(params.baseTransactionId)}</a>
            </td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Device count</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb;"><strong style="color: #dc2626;">${String(params.deviceCount)}</strong></td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Customer email</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb;">${escapeHtml(params.customerEmail)}</td>
        </tr>
    </table>

    <h3>Next steps</h3>
    <ol>
        <li>Query Analytics Engine to check the pattern: is device count growing or did it spike once?</li>
        <li>Send a friendly email from <code style="background: #f3f4f6; padding: 2px 4px; border-radius: 3px;">support@getcmdr.com</code> asking if they need additional seats.</li>
        <li>If no response after two weeks, follow up once more.</li>
        <li>If still unresolved, consider suspending the subscription via Paddle (last resort).</li>
    </ol>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server. Re-alerts are suppressed for 30 days per seat.
    </p>
</body>
</html>
        `.trim(),
  })
}

const htmlEscapeMap: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => htmlEscapeMap[char])
}

interface EmailParams {
  to: string
  customerName: string
  licenseKeys: string[]
  productName: string
  supportEmail: string
  resendApiKey: string
  organizationName?: string
  licenseType?: LicenseType
}

function getLicenseDescription(type: LicenseType | undefined, orgName?: string): string {
  switch (type) {
    case 'commercial_subscription':
      return orgName
        ? `Your commercial license for ${orgName} is valid for one year and will auto-renew.`
        : 'Your commercial license is valid for one year and will auto-renew.'
    case 'commercial_perpetual':
      return orgName
        ? `Your perpetual commercial license for ${orgName} is valid forever.`
        : 'Your perpetual commercial license is valid forever.'
    default:
      return 'This is an unknown license type. This is weird. Please contact support.'
  }
}

export async function sendLicenseEmail(params: EmailParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const escapedCustomerName = escapeHtml(params.customerName)
  const escapedOrgName = params.organizationName ? escapeHtml(params.organizationName) : undefined
  const licenseDescriptionHtml = getLicenseDescription(params.licenseType, escapedOrgName)
  const licenseDescriptionText = getLicenseDescription(params.licenseType, params.organizationName)
  const orgLine = escapedOrgName ? `<p><strong>Licensed to:</strong> ${escapedOrgName}</p>` : ''
  const orgLineText = params.organizationName ? `Licensed to: ${params.organizationName}\n` : ''

  const count = params.licenseKeys.length
  const isMultiple = count > 1
  const keyWord = isMultiple ? 'keys' : 'key'
  const subject = `Your ${params.productName} license ${keyWord} 🎉`

  // HTML: render keys as numbered boxes if multiple, single box otherwise
  const licenseBoxesHtml = isMultiple
    ? params.licenseKeys
        .map(
          (key, i) => `
            <div class="license-box">
                <div class="license-number">License ${String(i + 1)} of ${String(count)}</div>
                ${key}
            </div>`,
        )
        .join('\n')
    : `<div class="license-box">${params.licenseKeys[0]}</div>`

  // Plain text: render keys with headers if multiple
  const licenseKeysText = isMultiple
    ? params.licenseKeys.map((key, i) => `License ${String(i + 1)} of ${String(count)}:\n${key}`).join('\n\n')
    : params.licenseKeys[0]

  const introText = isMultiple
    ? `Thanks for purchasing ${String(count)} licenses for ${params.productName}! Here are your license keys:`
    : `Thanks for purchasing ${params.productName}! Here's your license key:`

  await resend.emails.send({
    from: `${params.productName} <noreply@getcmdr.com>`,
    to: params.to,
    subject,
    html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; }
        .license-box { background: #f5f5f5; border-radius: 8px; padding: 20px; margin: 20px 0; font-family: monospace; font-size: 18px; text-align: center; letter-spacing: 2px; }
        .license-number { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; color: #666; margin-bottom: 8px; letter-spacing: normal; }
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

    <p>${licenseDescriptionHtml}</p>

    <div class="note">
        <strong>Multiple machines?</strong> Each license lets you run ${params.productName} on multiple machines — like a laptop and desktop for remote debugging — as long as you're the only one using that license.
    </div>

    <div class="footer">
        <p>Questions? Just reply to this email or contact <a href="mailto:${params.supportEmail}">${params.supportEmail}</a></p>
        <p>Happy file managing! ⌘</p>
    </div>
</body>
</html>
        `.trim(),
    text: `
Welcome to ${params.productName}!

Hey ${params.customerName},

${introText}

${licenseKeysText}

${orgLineText}
How to activate:
1. Open ${params.productName}
2. Go to Cmdr menu → Enter license key...
3. Paste a key and click Activate

${licenseDescriptionText}

Multiple machines? Each license lets you run ${params.productName} on multiple machines — like a laptop and desktop for remote debugging — as long as you're the one using that license.

Questions? Contact ${params.supportEmail}

Happy file managing! ⌘
        `.trim(),
  })
}
