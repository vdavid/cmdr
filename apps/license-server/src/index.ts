import { Hono } from 'hono'
import { generateLicenseKey, generateShortCode, isValidShortCode, licenseTypes, type LicenseType } from './license'
import { sendDeviceCountAlert, sendLicenseEmail } from './email'
import { constantTimeEqual, verifyPaddleWebhookMulti } from './paddle'
import {
    getSubscriptionStatus,
    getLicenseTypeFromPriceId,
    getCustomerDetails,
    PaddleApiError,
    type ValidationResponse,
    type PriceIdMapping,
} from './paddle-api'
import { pruneStaleDevices, shouldAlert, type DeviceSet } from './device-tracking'

type Bindings = {
    // KV namespace for license code -> full key mappings
    LICENSE_CODES: KVNamespace
    // Analytics Engine for download tracking
    DOWNLOADS: AnalyticsEngineDataset
    // Analytics Engine for device count tracking (fair use monitoring)
    DEVICE_COUNTS: AnalyticsEngineDataset
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
    // "sandbox" (default) or "live" — controls which Paddle API to use for /validate
    PADDLE_ENVIRONMENT?: string
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
            // Paddle preserves the key casing from checkout - we use camelCase
            organizationName?: string
        }
    }
}

const maxOrganizationNameLength = 500
const maxTransactionIdLength = 200

function isValidEmail(email: string): boolean {
    const atIndex = email.indexOf('@')
    return atIndex > 0 && email.indexOf('.', atIndex) > atIndex + 1
}

function isValidLicenseType(type: string): type is LicenseType {
    return (licenseTypes as readonly string[]).includes(type)
}

/** Redact an email for logging: "john@example.com" -> "j***@example.com" */
function redactEmail(email: string): string {
    const atIndex = email.indexOf('@')
    if (atIndex <= 0) return '***'
    return email[0] + '***' + email.slice(atIndex)
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

    if (!code || typeof code !== 'string' || code.length > 50) {
        return c.json({ error: 'Missing or invalid license code' }, 400)
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

const maxDeviceIdLength = 200
const deviceAlertThreshold = 6

// Validate license - called by app to check subscription status
app.post('/validate', async (c) => {
    const body = await c.req.json<{ transactionId?: string; deviceId?: string }>()
    const { response, trackingPromise } = await handleValidation(body.transactionId, body.deviceId, c.env)
    if (trackingPromise) {
        c.executionCtx.waitUntil(trackingPromise)
    }
    return c.json(response.body, response.status)
})

/** Track a device for fair use monitoring. Never throws to callers (errors are logged). */
async function trackDevice(params: {
    seatTransactionId: string
    baseTransactionId: string
    customerId: string | undefined
    deviceId: string
    kv: KVNamespace
    deviceCounts: AnalyticsEngineDataset
    paddleConfig: { apiKey: string; environment: 'sandbox' | 'live' }
    resendApiKey: string
}): Promise<void> {
    const kvKey = `devices:${params.seatTransactionId}`
    const now = new Date().toISOString()

    // Read current device set
    const stored = await params.kv.get<DeviceSet>(kvKey, 'json')
    const deviceSet: DeviceSet = stored ?? { devices: {} }

    // Add/update the device entry
    deviceSet.devices[params.deviceId] = now

    // Prune stale entries (older than 90 days)
    deviceSet.devices = pruneStaleDevices(deviceSet.devices, 90)

    const deviceCount = Object.keys(deviceSet.devices).length

    // Write Analytics Engine data point (fire-and-forget, non-blocking)
    params.deviceCounts.writeDataPoint({
        indexes: [params.seatTransactionId],
        blobs: [params.seatTransactionId, params.deviceId],
        doubles: [deviceCount],
    })

    // Alert if threshold crossed and not recently alerted
    if (shouldAlert(deviceCount, deviceSet.lastAlertedAt, deviceAlertThreshold)) {
        let customerEmail = 'unknown'
        if (params.customerId) {
            const customer = await getCustomerDetails(params.customerId, params.paddleConfig)
            if (customer) {
                customerEmail = customer.email
            }
        }

        await sendDeviceCountAlert({
            seatTransactionId: params.seatTransactionId,
            baseTransactionId: params.baseTransactionId,
            deviceCount,
            customerEmail,
            resendApiKey: params.resendApiKey,
            paddleEnvironment: params.paddleConfig.environment,
        })

        deviceSet.lastAlertedAt = now
    }

    // Single KV write (includes lastAlertedAt if alert was sent)
    await params.kv.put(kvKey, JSON.stringify(deviceSet))
}

/** Fetch subscription status, returning an error response on failure. */
async function fetchSubscriptionResult(
    baseTransactionId: string,
    paddleConfig: { apiKey: string; environment: 'sandbox' | 'live' },
): Promise<
    | { ok: true; result: NonNullable<Awaited<ReturnType<typeof getSubscriptionStatus>>> }
    | { ok: false; body: ValidationResponse | { error: string }; status: 200 | 502 }
> {
    let result
    try {
        result = await getSubscriptionStatus(baseTransactionId, paddleConfig)
    } catch (error) {
        if (error instanceof PaddleApiError) {
            console.error('Paddle API error during validation:', error.message)
            return { ok: false, body: { error: 'upstream_error' }, status: 502 }
        }
        throw error
    }

    if (!result) {
        return { ok: false, body: invalidResponse(), status: 200 }
    }

    return { ok: true, result }
}

/** Core validation logic, extracted to keep route handler complexity low. */
async function handleValidation(
    transactionId: string | undefined,
    deviceId: string | undefined,
    env: Bindings,
): Promise<{
    response: { body: ValidationResponse | { error: string }; status: 200 | 502 }
    trackingPromise: Promise<void> | null
}> {
    if (!transactionId || typeof transactionId !== 'string' || transactionId.length > maxTransactionIdLength) {
        return { response: { body: invalidResponse(), status: 200 }, trackingPromise: null }
    }

    const baseTransactionId = transactionId.replace(/-\d+$/, '')

    const paddleConfig = getPaddleConfig(env)
    if (!paddleConfig) {
        console.error('No Paddle API key configured')
        return { response: { body: { error: 'upstream_error' }, status: 502 }, trackingPromise: null }
    }

    const fetchResult = await fetchSubscriptionResult(baseTransactionId, paddleConfig)
    if (!fetchResult.ok) {
        return { response: { body: fetchResult.body, status: fetchResult.status }, trackingPromise: null }
    }

    const { result } = fetchResult
    const hasExpiration = result.expiresAt !== null
    const licenseType: LicenseType = hasExpiration ? 'commercial_subscription' : 'commercial_perpetual'

    const body: ValidationResponse = {
        status: result.status === 'canceled' ? 'expired' : result.status,
        type: licenseType,
        organizationName: result.customData?.organizationName ?? null,
        expiresAt: result.expiresAt,
    }

    // Device tracking: runs after the response is sent via waitUntil, never affects latency
    const validDeviceId = isValidDeviceId(deviceId)
    const trackingPromise = validDeviceId
        ? trackDeviceSafe({
              seatTransactionId: transactionId,
              baseTransactionId,
              customerId: result.customerId ?? undefined,
              deviceId: validDeviceId,
              kv: env.LICENSE_CODES,
              deviceCounts: env.DEVICE_COUNTS,
              paddleConfig,
              resendApiKey: env.RESEND_API_KEY,
          })
        : null

    return { response: { body, status: 200 }, trackingPromise }
}

function isValidDeviceId(deviceId: unknown): string | null {
    if (typeof deviceId === 'string' && deviceId.length > 0 && deviceId.length <= maxDeviceIdLength) {
        return deviceId
    }
    return null
}

/** Wraps trackDevice in a try/catch so it never affects the validation response. */
async function trackDeviceSafe(params: Parameters<typeof trackDevice>[0]): Promise<void> {
    try {
        await trackDevice(params)
    } catch (error) {
        console.error('Device tracking error (non-fatal):', error instanceof Error ? error.message : String(error))
    }
}

/** Helper to create invalid response */
function invalidResponse(): ValidationResponse {
    return {
        status: 'invalid',
        type: null,
        organizationName: null,
        expiresAt: null,
    }
}

/** Determine Paddle API config from PADDLE_ENVIRONMENT var (default: sandbox). */
function getPaddleConfig(env: Bindings): { apiKey: string; environment: 'sandbox' | 'live' } | null {
    const environment = env.PADDLE_ENVIRONMENT === 'live' ? 'live' : 'sandbox'
    const apiKey = environment === 'live' ? env.PADDLE_API_KEY_LIVE : env.PADDLE_API_KEY_SANDBOX
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

    let payload: PaddleWebhookPayload
    try {
        payload = JSON.parse(body) as PaddleWebhookPayload
    } catch {
        console.error('Failed to parse webhook body as JSON')
        return c.json({ error: 'Invalid JSON' }, 400)
    }
    console.log('Received webhook:', payload.event_type)

    // Only handle completed purchases
    if (payload.event_type !== 'transaction.completed') {
        return c.json({ status: 'ignored', event: payload.event_type })
    }

    try {
        return await processCompletedTransaction(payload, c.env)
    } catch (error) {
        console.error('Webhook processing failed:', error instanceof Error ? error.message : String(error))
        return c.json({ error: 'Internal server error' }, 500)
    }
})

/** Process a completed Paddle transaction: validate, generate licenses, send email. */
async function processCompletedTransaction(payload: PaddleWebhookPayload, env: Bindings): Promise<Response> {
    const purchaseData = extractPurchaseData(payload)
    if (!purchaseData) {
        console.error('Missing customer_id or transaction ID in webhook payload')
        return Response.json({ error: 'Missing customer_id or transaction ID' }, { status: 400 })
    }

    // Idempotency: skip if this transaction was already processed
    const idempotencyKey = `transaction:${purchaseData.transactionId}`
    const alreadyProcessed = await env.LICENSE_CODES.get(idempotencyKey)
    if (alreadyProcessed) {
        console.log('Transaction already processed:', purchaseData.transactionId)
        return Response.json({ status: 'already_processed', transactionId: purchaseData.transactionId })
    }

    console.log('Processing transaction:', purchaseData.transactionId, 'for customer:', purchaseData.customerId)

    // Determine Paddle API config (sandbox vs live based on PADDLE_ENVIRONMENT)
    const paddleConfig = getPaddleConfig(env)
    if (!paddleConfig) {
        console.error('No Paddle API key configured')
        return Response.json({ error: 'Server configuration error' }, { status: 500 })
    }

    // Fetch customer details from Paddle API
    const customer = await getCustomerDetails(purchaseData.customerId, paddleConfig)
    if (!customer) {
        console.error('Failed to fetch customer details for:', purchaseData.customerId)
        return Response.json({ error: 'Failed to fetch customer details' }, { status: 500 })
    }

    console.log('Customer:', redactEmail(customer.email))

    // Determine license type from price ID
    const priceIds: PriceIdMapping = {
        supporter: env.PRICE_ID_SUPPORTER,
        commercialSubscription: env.PRICE_ID_COMMERCIAL_SUBSCRIPTION,
        commercialPerpetual: env.PRICE_ID_COMMERCIAL_PERPETUAL,
    }
    const licenseType = purchaseData.priceId
        ? getLicenseTypeFromPriceId(purchaseData.priceId, priceIds)
        : 'commercial_subscription'

    // Get organization name: prefer customer's business name, fall back to custom_data
    const organizationName =
        licenseType !== 'supporter' ? (customer.businessName ?? purchaseData.organizationName) : undefined

    // Generate and send license(s) - one per quantity
    // If email fails after KV writes, we intentionally don't mark the transaction as processed
    // so the next Paddle retry will re-generate and re-send the licenses.
    const result = await generateAndSendLicenses({
        customerEmail: customer.email,
        customerName: customer.name ?? 'there',
        transactionId: purchaseData.transactionId,
        quantity: purchaseData.quantity,
        licenseType: licenseType ?? 'commercial_subscription',
        organizationName,
        privateKey: env.ED25519_PRIVATE_KEY,
        productName: env.PRODUCT_NAME,
        supportEmail: env.SUPPORT_EMAIL,
        resendApiKey: env.RESEND_API_KEY,
        kv: env.LICENSE_CODES,
    })

    // Mark transaction as processed (7-day TTL)
    const sevenDaysInSeconds = 604_800
    await env.LICENSE_CODES.put(idempotencyKey, 'processed', { expirationTtl: sevenDaysInSeconds })

    console.log(
        'Licenses sent to:',
        redactEmail(customer.email),
        'type:',
        result.licenseType,
        'quantity:',
        result.quantity,
    )
    return Response.json({
        status: 'ok',
        email: customer.email,
        licenseType: result.licenseType,
        quantity: result.quantity,
    })
}

/** Truncate organization name to max allowed length */
function truncateOrgName(name: string | undefined): string | undefined {
    return typeof name === 'string' ? name.slice(0, maxOrganizationNameLength) : undefined
}

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
        organizationName: truncateOrgName(payload.data?.custom_data?.organizationName),
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
        // Generate the short code first so it can be embedded in the signed payload
        const shortCode = generateShortCode()

        const licenseData = {
            email: params.customerEmail,
            // Each license gets a unique transaction ID suffix for quantity > 1
            transactionId: params.quantity > 1 ? `${params.transactionId}-${String(i + 1)}` : params.transactionId,
            issuedAt: new Date().toISOString(),
            type: params.licenseType,
            organizationName: params.organizationName,
            shortCode,
        }

        const fullKey = await generateLicenseKey(licenseData, params.privateKey)
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
    const isAuthorized = validSecrets.some((secret) => constantTimeEqual(authHeader ?? '', `Bearer ${secret}`))
    if (!isAuthorized) {
        return c.json({ error: 'Unauthorized' }, 401)
    }

    const {
        email,
        type = 'commercial_subscription',
        organizationName,
    } = await c.req.json<{ email: string; type?: string; organizationName?: string }>()

    if (!email || typeof email !== 'string' || !isValidEmail(email)) {
        return c.json({ error: 'Invalid email format' }, 400)
    }
    if (!isValidLicenseType(type)) {
        return c.json({ error: `Invalid license type. Must be one of: ${licenseTypes.join(', ')}` }, 400)
    }
    if (
        organizationName !== undefined &&
        (typeof organizationName !== 'string' || organizationName.length > maxOrganizationNameLength)
    ) {
        return c.json(
            { error: `Organization name must be a string of at most ${String(maxOrganizationNameLength)} characters` },
            400,
        )
    }

    // Generate the short code first so it can be embedded in the signed payload
    const shortCode = generateShortCode()

    const licenseData = {
        email,
        transactionId: `manual-${String(Date.now())}`,
        issuedAt: new Date().toISOString(),
        type,
        organizationName,
        shortCode,
    }

    const fullKey = await generateLicenseKey(licenseData, c.env.ED25519_PRIVATE_KEY)
    const stored: StoredLicense = { fullKey, organizationName }
    await c.env.LICENSE_CODES.put(shortCode, JSON.stringify(stored))

    return c.json({ code: shortCode, type, organizationName: organizationName ?? null })
})

// Download redirect — tracks version, arch, and country, then redirects to GitHub Releases
const validArchitectures = new Set(['aarch64', 'x86_64', 'universal'])
const versionPattern = /^\d+\.\d+\.\d+$/

app.get('/download/:version/:arch', (c) => {
    const { version, arch } = c.req.param()

    if (!versionPattern.test(version) || !validArchitectures.has(arch)) {
        return c.json({ error: 'Invalid version or architecture' }, 400)
    }

    const cf = c.req.raw.cf as { country?: string; continent?: string } | undefined
    const country = cf?.country ?? 'unknown'
    const continent = cf?.continent ?? 'unknown'

    // Fire-and-forget — writeDataPoint is non-blocking
    c.env.DOWNLOADS.writeDataPoint({
        indexes: [version],
        blobs: [version, arch, country, continent],
        doubles: [1],
    })

    return c.redirect(`https://github.com/vdavid/cmdr/releases/download/v${version}/Cmdr_${version}_${arch}.dmg`, 302)
})

export default app
