import { Hono } from 'hono'
import { generateLicenseKey, formatLicenseKey, type LicenseType } from './license'
import { sendLicenseEmail } from './email'
import { verifyPaddleWebhookMulti } from './paddle'
import {
    getSubscriptionStatus,
    getLicenseTypeFromPriceId,
    type ValidationResponse,
    type PriceIdMapping,
} from './paddle-api'

type Bindings = {
    // Paddle webhook secrets (both optional to support gradual rollout)
    PADDLE_WEBHOOK_SECRET_LIVE?: string
    PADDLE_WEBHOOK_SECRET_SANDBOX?: string
    // Paddle API keys for validation
    PADDLE_API_KEY_LIVE?: string
    PADDLE_API_KEY_SANDBOX?: string
    // Crypto keys
    ED25519_PRIVATE_KEY: string
    // Email
    RESEND_API_KEY: string
    // Config
    PRODUCT_NAME: string
    SUPPORT_EMAIL: string
    // Price IDs for license type mapping
    PRICE_ID_SUPPORTER?: string
    PRICE_ID_COMMERCIAL_SUBSCRIPTION?: string
    PRICE_ID_COMMERCIAL_PERPETUAL?: string
}

interface PaddleWebhookPayload {
    event_type: string
    data?: {
        id?: string
        customer?: {
            email?: string
            name?: string
        }
        items?: Array<{
            price?: {
                id?: string
            }
        }>
        custom_data?: {
            organization_name?: string
        }
    }
}

const app = new Hono<{ Bindings: Bindings }>()

// Health check
app.get('/', (c) => {
    return c.json({ status: 'ok', service: 'cmdr-license-server' })
})

// Validate license - called by app to check subscription status
app.post('/validate', async (c) => {
    const { licenseKey, transactionId } = await c.req.json<{ licenseKey?: string; transactionId?: string }>()

    if (!transactionId) {
        const response: ValidationResponse = {
            status: 'invalid',
            type: null,
            organizationName: null,
            expiresAt: null,
        }
        return c.json(response)
    }

    // Try sandbox first, then live (based on transaction ID prefix)
    const isSandbox = transactionId.startsWith('txn_') // Sandbox uses different prefix in practice
    const apiKey = isSandbox ? c.env.PADDLE_API_KEY_SANDBOX : c.env.PADDLE_API_KEY_LIVE
    const environment = isSandbox ? 'sandbox' : 'live'

    if (!apiKey) {
        // Fall back to the other environment
        const fallbackKey = isSandbox ? c.env.PADDLE_API_KEY_LIVE : c.env.PADDLE_API_KEY_SANDBOX
        if (!fallbackKey) {
            console.error('No Paddle API key configured')
            const response: ValidationResponse = {
                status: 'invalid',
                type: null,
                organizationName: null,
                expiresAt: null,
            }
            return c.json(response)
        }
    }

    const result = await getSubscriptionStatus(transactionId, {
        apiKey: apiKey ?? c.env.PADDLE_API_KEY_LIVE ?? c.env.PADDLE_API_KEY_SANDBOX ?? '',
        environment,
    })

    if (!result) {
        const response: ValidationResponse = {
            status: 'invalid',
            type: null,
            organizationName: null,
            expiresAt: null,
        }
        return c.json(response)
    }

    // Determine license type from cached data (we don't have price ID here)
    // For now, assume commercial_subscription for subscriptions, commercial_perpetual otherwise
    const hasExpiration = result.expiresAt !== null
    const licenseType: LicenseType = hasExpiration ? 'commercial_subscription' : 'commercial_perpetual'

    const response: ValidationResponse = {
        status: result.status === 'canceled' ? 'expired' : result.status,
        type: licenseType,
        organizationName: result.customData?.organizationName ?? null,
        expiresAt: result.expiresAt,
    }

    return c.json(response)
})

// Paddle webhook - called when purchase completes
app.post('/webhook/paddle', async (c) => {
    const body = await c.req.text()
    const signature = c.req.header('Paddle-Signature') ?? ''

    // Verify webhook signature against both live and sandbox secrets
    const isValid = await verifyPaddleWebhookMulti(body, signature, [
        c.env.PADDLE_WEBHOOK_SECRET_LIVE,
        c.env.PADDLE_WEBHOOK_SECRET_SANDBOX,
    ])
    if (!isValid) {
        return c.json({ error: 'Invalid signature' }, 401)
    }

    const payload = JSON.parse(body) as PaddleWebhookPayload

    // Only handle completed purchases
    if (payload.event_type !== 'transaction.completed') {
        return c.json({ status: 'ignored', event: payload.event_type })
    }

    const customerEmail = payload.data?.customer?.email
    const customerName = payload.data?.customer?.name ?? 'there'
    const transactionId = payload.data?.id
    const priceId = payload.data?.items?.[0]?.price?.id
    const organizationName = payload.data?.custom_data?.organization_name

    if (!customerEmail || !transactionId) {
        return c.json({ error: 'Missing customer email or transaction ID' }, 400)
    }

    // Determine license type from price ID
    const priceIds: PriceIdMapping = {
        supporter: c.env.PRICE_ID_SUPPORTER,
        commercialSubscription: c.env.PRICE_ID_COMMERCIAL_SUBSCRIPTION,
        commercialPerpetual: c.env.PRICE_ID_COMMERCIAL_PERPETUAL,
    }
    const licenseType = priceId ? getLicenseTypeFromPriceId(priceId, priceIds) : 'commercial_subscription'

    // Generate license key with type
    const licenseData = {
        email: customerEmail,
        transactionId,
        issuedAt: new Date().toISOString(),
        type: licenseType ?? 'commercial_subscription',
    }

    const licenseKey = await generateLicenseKey(licenseData, c.env.ED25519_PRIVATE_KEY)
    const formattedKey = formatLicenseKey(licenseKey)

    // Send license email (include org name and expiration for commercial)
    await sendLicenseEmail({
        to: customerEmail,
        customerName,
        licenseKey: formattedKey,
        productName: c.env.PRODUCT_NAME,
        supportEmail: c.env.SUPPORT_EMAIL,
        resendApiKey: c.env.RESEND_API_KEY,
        organizationName: licenseType !== 'supporter' ? organizationName : undefined,
        licenseType: licenseType ?? undefined,
    })

    return c.json({ status: 'ok', email: customerEmail, licenseType })
})

// Manual license generation (for testing or customer service)
// Protected by bearer token matching either live or sandbox webhook secret
app.post('/admin/generate', async (c) => {
    const authHeader = c.req.header('Authorization')
    const validSecrets = [c.env.PADDLE_WEBHOOK_SECRET_LIVE, c.env.PADDLE_WEBHOOK_SECRET_SANDBOX].filter(Boolean)
    const isAuthorized = validSecrets.some((secret) => authHeader === `Bearer ${secret}`)
    if (!isAuthorized) {
        return c.json({ error: 'Unauthorized' }, 401)
    }

    const { email, type = 'commercial_subscription' } = await c.req.json<{ email: string; type?: LicenseType }>()

    const licenseData = {
        email,
        transactionId: `manual-${String(Date.now())}`,
        issuedAt: new Date().toISOString(),
        type,
    }

    const licenseKey = await generateLicenseKey(licenseData, c.env.ED25519_PRIVATE_KEY)
    const formattedKey = formatLicenseKey(licenseKey)

    return c.json({ licenseKey: formattedKey, type })
})

export default app
