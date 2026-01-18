import { Hono } from 'hono'
import { generateLicenseKey, generateShortCode, isValidShortCode, type LicenseType } from './license'
// Note: formatLicenseKey is no longer used - we use short codes now
import { sendLicenseEmail } from './email'
import { verifyPaddleWebhookMulti } from './paddle'
import {
    getSubscriptionStatus,
    getLicenseTypeFromPriceId,
    getCustomerDetails,
    type ValidationResponse,
    type PriceIdMapping,
} from './paddle-api'

type Bindings = {
    // KV namespace for license code -> full key mappings
    LICENSE_CODES: KVNamespace
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
        customer_id?: string
        items?: Array<{
            price?: {
                id?: string
            }
            quantity?: number
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

/** Stored license data in KV */
interface StoredLicense {
    fullKey: string
    organizationName?: string
}

// Activate license - exchange short code for full cryptographic key
app.post('/activate', async (c) => {
    const { code } = await c.req.json<{ code?: string }>()

    if (!code) {
        return c.json({ error: 'Missing license code' }, 400)
    }

    const normalizedCode = code.trim().toUpperCase()

    if (!isValidShortCode(normalizedCode)) {
        return c.json({ error: 'Invalid license code format' }, 400)
    }

    // Look up the license data in KV
    const stored = await c.env.LICENSE_CODES.get<StoredLicense>(normalizedCode, 'json')

    if (!stored) {
        return c.json({ error: 'License code not found or expired' }, 404)
    }

    return c.json({
        licenseKey: stored.fullKey,
        organizationName: stored.organizationName ?? null,
    })
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
        console.error('Webhook signature verification failed')
        return c.json({ error: 'Invalid signature' }, 401)
    }

    const payload = JSON.parse(body) as PaddleWebhookPayload
    console.log('Received webhook:', payload.event_type)

    // Only handle completed purchases
    if (payload.event_type !== 'transaction.completed') {
        return c.json({ status: 'ignored', event: payload.event_type })
    }

    // Extract purchase data from webhook
    const purchaseData = extractPurchaseData(payload)
    if (!purchaseData) {
        console.error('Missing customer_id or transaction ID in webhook payload')
        return c.json({ error: 'Missing customer_id or transaction ID' }, 400)
    }

    console.log('Processing transaction:', purchaseData.transactionId, 'for customer:', purchaseData.customerId)

    // Determine Paddle API config (sandbox vs live based on transaction ID)
    const paddleConfig = getPaddleConfig(purchaseData.transactionId, c.env)
    if (!paddleConfig) {
        console.error('No Paddle API key configured')
        return c.json({ error: 'Server configuration error' }, 500)
    }

    // Fetch customer details from Paddle API
    const customer = await getCustomerDetails(purchaseData.customerId, paddleConfig)
    if (!customer) {
        console.error('Failed to fetch customer details for:', purchaseData.customerId)
        return c.json({ error: 'Failed to fetch customer details' }, 500)
    }

    console.log('Customer email:', customer.email)

    // Determine license type from price ID
    const priceIds: PriceIdMapping = {
        supporter: c.env.PRICE_ID_SUPPORTER,
        commercialSubscription: c.env.PRICE_ID_COMMERCIAL_SUBSCRIPTION,
        commercialPerpetual: c.env.PRICE_ID_COMMERCIAL_PERPETUAL,
    }
    const licenseType = purchaseData.priceId
        ? getLicenseTypeFromPriceId(purchaseData.priceId, priceIds)
        : 'commercial_subscription'

    // Generate and send license(s) - one per quantity
    const result = await generateAndSendLicenses({
        customerEmail: customer.email,
        customerName: customer.name ?? 'there',
        transactionId: purchaseData.transactionId,
        quantity: purchaseData.quantity,
        licenseType: licenseType ?? 'commercial_subscription',
        organizationName: licenseType !== 'supporter' ? purchaseData.organizationName : undefined,
        privateKey: c.env.ED25519_PRIVATE_KEY,
        productName: c.env.PRODUCT_NAME,
        supportEmail: c.env.SUPPORT_EMAIL,
        resendApiKey: c.env.RESEND_API_KEY,
        kv: c.env.LICENSE_CODES,
    })

    console.log('Licenses sent to:', customer.email, 'type:', result.licenseType, 'quantity:', result.quantity)
    return c.json({ status: 'ok', email: customer.email, licenseType: result.licenseType, quantity: result.quantity })
})

/** Extract purchase data from webhook payload (customer fetched separately via API) */
function extractPurchaseData(payload: PaddleWebhookPayload): {
    customerId: string
    transactionId: string
    priceId: string | undefined
    quantity: number
    organizationName: string | undefined
} | null {
    const customerId = payload.data?.customer_id
    const transactionId = payload.data?.id

    if (!customerId || !transactionId) return null

    return {
        customerId,
        transactionId,
        priceId: payload.data?.items?.[0]?.price?.id,
        quantity: payload.data?.items?.[0]?.quantity ?? 1,
        organizationName: payload.data?.custom_data?.organization_name,
    }
}

/** Helper to generate license(s), store in KV, and send email */
async function generateAndSendLicenses(params: {
    customerEmail: string
    customerName: string
    transactionId: string
    quantity: number
    licenseType: LicenseType
    organizationName: string | undefined
    privateKey: string
    productName: string
    supportEmail: string
    resendApiKey: string
    kv: KVNamespace
}): Promise<{ licenseType: LicenseType; quantity: number }> {
    const licenseCodes: string[] = []

    for (let i = 0; i < params.quantity; i++) {
        const licenseData = {
            email: params.customerEmail,
            // Each license gets a unique transaction ID suffix for quantity > 1
            transactionId: params.quantity > 1 ? `${params.transactionId}-${String(i + 1)}` : params.transactionId,
            issuedAt: new Date().toISOString(),
            type: params.licenseType,
            organizationName: params.organizationName,
        }

        // Generate the full cryptographic key (includes organizationName in signed payload)
        const fullKey = await generateLicenseKey(licenseData, params.privateKey)

        // Generate a short code and store the mapping with org name
        const shortCode = generateShortCode()
        const stored: StoredLicense = {
            fullKey,
            organizationName: params.organizationName,
        }
        await params.kv.put(shortCode, JSON.stringify(stored), {
            // Keys never expire - perpetual licenses last forever
            // For subscriptions, server validation handles expiry
        })

        licenseCodes.push(shortCode)
    }

    await sendLicenseEmail({
        to: params.customerEmail,
        customerName: params.customerName,
        licenseKeys: licenseCodes,
        productName: params.productName,
        supportEmail: params.supportEmail,
        resendApiKey: params.resendApiKey,
        organizationName: params.organizationName,
        licenseType: params.licenseType,
    })

    return { licenseType: params.licenseType, quantity: params.quantity }
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

    const {
        email,
        type = 'commercial_subscription',
        organizationName,
    } = await c.req.json<{ email: string; type?: LicenseType; organizationName?: string }>()

    const licenseData = {
        email,
        transactionId: `manual-${String(Date.now())}`,
        issuedAt: new Date().toISOString(),
        type,
        organizationName,
    }

    // Generate the full cryptographic key (includes organizationName in signed payload)
    const fullKey = await generateLicenseKey(licenseData, c.env.ED25519_PRIVATE_KEY)

    // Generate a short code and store the mapping with org name
    const shortCode = generateShortCode()
    const stored: StoredLicense = { fullKey, organizationName }
    await c.env.LICENSE_CODES.put(shortCode, JSON.stringify(stored))

    return c.json({ code: shortCode, type, organizationName: organizationName ?? null })
})

export default app
