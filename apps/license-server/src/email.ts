import { Resend } from 'resend'
import type { LicenseType } from './license'

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
        case 'supporter':
            return 'Your supporter license is valid forever for personal use. Love you! ❤️'
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
