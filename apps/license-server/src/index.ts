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
    const { transactionId } = await c.req.json<{ transactionId?: string }>()

    if (!transactionId) {
        return c.json(invalidResponse())
    }

    // Determine which Paddle environment to use
    const paddleConfig = getPaddleConfig(transactionId, c.env)
    if (!paddleConfig) {
        console.error('No Paddle API key configured')
        return c.json(invalidResponse())
    }

    const result = await getSubscriptionStatus(transactionId, paddleConfig)
    if (!result) {
        return c.json(invalidResponse())
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

/** Helper to create invalid response */
function invalidResponse(): ValidationResponse {
    return {
        status: 'invalid',
        type: null,
        organizationName: null,
        expiresAt: null,
    }
}

/** Determine Paddle API config based on transaction ID */
function getPaddleConfig(
    transactionId: string,
    env: Bindings,
): { apiKey: string; environment: 'sandbox' | 'live' } | null {
    // Try sandbox first, then live (based on transaction ID prefix)
    const isSandbox = transactionId.startsWith('txn_') // Sandbox uses different prefix in practice
    const primaryKey = isSandbox ? env.PADDLE_API_KEY_SANDBOX : env.PADDLE_API_KEY_LIVE
    const fallbackKey = isSandbox ? env.PADDLE_API_KEY_LIVE : env.PADDLE_API_KEY_SANDBOX
    const environment = isSandbox ? 'sandbox' : 'live'

    const apiKey = primaryKey ?? fallbackKey
    if (!apiKey) return null

    return { apiKey, environment }
}

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

    // Extract and validate purchase data
    const purchaseData = extractPurchaseData(payload)
    if (!purchaseData) {
        return c.json({ error: 'Missing customer email or transaction ID' }, 400)
    }

    // Determine license type from price ID
    const priceIds: PriceIdMapping = {
        supporter: c.env.PRICE_ID_SUPPORTER,
        commercialSubscription: c.env.PRICE_ID_COMMERCIAL_SUBSCRIPTION,
        commercialPerpetual: c.env.PRICE_ID_COMMERCIAL_PERPETUAL,
    }
    const licenseType = purchaseData.priceId
        ? getLicenseTypeFromPriceId(purchaseData.priceId, priceIds)
        : 'commercial_subscription'

    // Generate and send license
    const result = await generateAndSendLicense({
        customerEmail: purchaseData.customerEmail,
        customerName: purchaseData.customerName,
        transactionId: purchaseData.transactionId,
        licenseType: licenseType ?? 'commercial_subscription',
        organizationName: licenseType !== 'supporter' ? purchaseData.organizationName : undefined,
        privateKey: c.env.ED25519_PRIVATE_KEY,
        productName: c.env.PRODUCT_NAME,
        supportEmail: c.env.SUPPORT_EMAIL,
        resendApiKey: c.env.RESEND_API_KEY,
    })

    return c.json({ status: 'ok', email: purchaseData.customerEmail, licenseType: result.licenseType })
})

/** Extract and validate purchase data from webhook payload */
function extractPurchaseData(payload: PaddleWebhookPayload): {
    customerEmail: string
    customerName: string
    transactionId: string
    priceId: string | undefined
    organizationName: string | undefined
} | null {
    const customerEmail = payload.data?.customer?.email
    const transactionId = payload.data?.id

    if (!customerEmail || !transactionId) return null

    return {
        customerEmail,
        customerName: payload.data?.customer?.name ?? 'there',
        transactionId,
        priceId: payload.data?.items?.[0]?.price?.id,
        organizationName: payload.data?.custom_data?.organization_name,
    }
}

/** Helper to generate license and send email */
async function generateAndSendLicense(params: {
    customerEmail: string
    customerName: string
    transactionId: string
    licenseType: LicenseType
    organizationName: string | undefined
    privateKey: string
    productName: string
    supportEmail: string
    resendApiKey: string
}): Promise<{ licenseType: LicenseType }> {
    const licenseData = {
        email: params.customerEmail,
        transactionId: params.transactionId,
        issuedAt: new Date().toISOString(),
        type: params.licenseType,
    }

    const licenseKey = await generateLicenseKey(licenseData, params.privateKey)
    const formattedKey = formatLicenseKey(licenseKey)

    await sendLicenseEmail({
        to: params.customerEmail,
        customerName: params.customerName,
        licenseKey: formattedKey,
        productName: params.productName,
        supportEmail: params.supportEmail,
        resendApiKey: params.resendApiKey,
        organizationName: params.organizationName,
        licenseType: params.licenseType,
    })

    return { licenseType: params.licenseType }
}

// Manual license generation (for testing or customer service)
// Protected by bearer token matching either live or sandbox webhook secret
app.post('/admin/generate', async (c) => {
    const authHeader = c.req.header('Authorization')
    const validSecrets = [c.env.PADDLE_WEBHOOK_SECRET_LIVE, c.env.PADDLE_WEBHOOK_SECRET_SANDBOX].filter(
        (s): s is string => !!s,
    )
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
