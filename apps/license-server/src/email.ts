import { Resend } from 'resend'
import type { LicenseType } from './license'

interface EmailParams {
    to: string
    customerName: string
    licenseKey: string
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
    const licenseDescription = getLicenseDescription(params.licenseType, params.organizationName)
    const orgLine = params.organizationName ? `<p><strong>Licensed to:</strong> ${params.organizationName}</p>` : ''
    const orgLineText = params.organizationName ? `Licensed to: ${params.organizationName}\n` : ''

    await resend.emails.send({
        from: `${params.productName} <noreply@getcmdr.com>`,
        to: params.to,
        subject: `Your ${params.productName} license key 🎉`,
        html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; }
        .license-box { background: #f5f5f5; border-radius: 8px; padding: 20px; margin: 20px 0; font-family: monospace; font-size: 18px; text-align: center; letter-spacing: 2px; }
        .footer { margin-top: 40px; padding-top: 20px; border-top: 1px solid #eee; font-size: 14px; color: #666; }
        .note { background: #e8f4f8; border-left: 4px solid #0ea5e9; padding: 12px 16px; margin: 20px 0; }
    </style>
</head>
<body>
    <h1>Welcome to ${params.productName}! 🚀</h1>
    
    <p>Hey ${params.customerName},</p>
    
    <p>Thanks for purchasing ${params.productName}! Here's your license key:</p>
    
    <div class="license-box">
        ${params.licenseKey}
    </div>
    
    ${orgLine}
    
    <h3>How to activate:</h3>
    <ol>
        <li>Open ${params.productName}</li>
        <li>Go to <strong>Menu → Enter License Key</strong></li>
        <li>Paste the key above and click Activate</li>
    </ol>
    
    <p>${licenseDescription}</p>
    
    <div class="note">
        <strong>Multiple machines?</strong> Your license lets you run ${params.productName} on multiple machines — like a laptop and desktop for remote debugging — as long as you're the only one using it.
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

Thanks for purchasing ${params.productName}! Here's your license key:

${params.licenseKey}

${orgLineText}
How to activate:
1. Open ${params.productName}
2. Go to Menu → Enter License Key
3. Paste the key above and click Activate

${licenseDescription}

Multiple machines? Your license lets you run ${params.productName} on multiple machines — like a laptop and desktop for remote debugging — as long as you're the one using it.

Questions? Contact ${params.supportEmail}

Happy file managing! ⌘
        `.trim(),
    })
}
