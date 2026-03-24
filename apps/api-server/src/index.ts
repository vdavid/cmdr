import { Hono } from 'hono'
import { generateLicenseKey, generateShortCode, isValidShortCode, licenseTypes, type LicenseType } from './license'
import {
    sendDeviceCountAlert,
    sendLicenseEmail,
    sendCrashNotificationEmail,
    sendDbSizeAlert,
    type CrashSummaryEntry,
} from './email'
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
    // Analytics Engine for device count tracking (fair use monitoring)
    DEVICE_COUNTS: AnalyticsEngineDataset
    // D1 database for telemetry persistence (crash reports, downloads, update checks)
    TELEMETRY_DB: D1Database
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
    PRICE_ID_COMMERCIAL_SUBSCRIPTION?: string
    PRICE_ID_COMMERCIAL_PERPETUAL?: string
    // Dedicated admin API token for /admin/stats (separate from Paddle secrets)
    ADMIN_API_TOKEN?: string
    // Crash notification email recipient (for cron-based crash alerts)
    CRASH_NOTIFICATION_EMAIL?: string
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

// KV key for the activation counter, read by /admin/stats.
// Starts from zero on deploy — initialize via the CF API if you need historical count.
const activationCountKey = '_meta:activation_count'
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
    return c.json({ status: 'ok', service: 'cmdr-api-server' })
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

    // Increment activation counter (fire-and-forget, non-blocking)
    const counterPromise = incrementActivationCount(c.env.LICENSE_CODES)
    try {
        c.executionCtx.waitUntil(counterPromise)
    } catch {
        // executionCtx unavailable (for example, in tests) — await inline as fallback
        await counterPromise
    }

    return c.json({
        licenseKey: stored.fullKey,
        organizationName: stored.organizationName ?? null,
    })
})

/** Increment the KV activation counter. Failures are logged but never surface to the caller. */
async function incrementActivationCount(kv: KVNamespace): Promise<void> {
    try {
        const current = parseInt((await kv.get(activationCountKey)) ?? '0', 10)
        await kv.put(activationCountKey, String(current + 1))
    } catch (error) {
        console.error(
            'Activation counter increment failed (non-fatal):',
            error instanceof Error ? error.message : String(error),
        )
    }
}

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
        commercialSubscription: env.PRICE_ID_COMMERCIAL_SUBSCRIPTION,
        commercialPerpetual: env.PRICE_ID_COMMERCIAL_PERPETUAL,
    }
    const licenseType = purchaseData.priceId
        ? getLicenseTypeFromPriceId(purchaseData.priceId, priceIds)
        : 'commercial_subscription'

    // Get organization name: prefer customer's business name, fall back to custom_data
    const organizationName = customer.businessName ?? purchaseData.organizationName

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

// Admin stats — returns activation count and device count
// Auth: dedicated ADMIN_API_TOKEN, separate from the Paddle secrets used by /admin/generate
app.get('/admin/stats', async (c) => {
    const token = c.env.ADMIN_API_TOKEN
    if (!token) {
        return c.json({ error: 'Admin API not configured' }, 500)
    }

    const authHeader = c.req.header('Authorization')
    if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
        return c.json({ error: 'Unauthorized' }, 401)
    }

    const raw = await c.env.LICENSE_CODES.get(activationCountKey)
    const totalActivations = parseInt(raw ?? '0', 10)

    // TODO: `activeDevices` requires querying the CF Analytics Engine SQL API
    // (`POST /v4/accounts/{id}/analytics_engine/sql`), which is an external HTTP call,
    // not available via the `DEVICE_COUNTS` binding (bindings only support `writeDataPoint`).
    // For v1, return null. The analytics dashboard queries CF Analytics Engine directly.
    const activeDevices: number | null = null

    return c.json({ totalActivations, activeDevices })
})

/** Verify admin auth and return error response if unauthorized, or null if authorized. */
function verifyAdminAuth(c: { env: Bindings; req: { header: (name: string) => string | undefined } }): Response | null {
    const token = c.env.ADMIN_API_TOKEN
    if (!token) {
        return Response.json({ error: 'Admin API not configured' }, { status: 500 })
    }
    const authHeader = c.req.header('Authorization')
    if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
        return Response.json({ error: 'Unauthorized' }, { status: 401 })
    }
    return null
}

const validDownloadRanges = new Set(['24h', '7d', '30d', 'all'])
const validActiveUserRanges = new Set(['7d', '30d', '90d', 'all'])
const validCrashRanges = new Set(['7d', '30d', '90d', 'all'])

// Values are hardcoded, never from user input — safe to interpolate into SQL.
const rangeToSqliteInterval: Record<string, string> = {
    '24h': '-1 day',
    '7d': '-7 days',
    '30d': '-30 days',
    '90d': '-90 days',
}

// Admin downloads — aggregated download data from D1
app.get('/admin/downloads', async (c) => {
    const authError = verifyAdminAuth(c)
    if (authError) return authError

    const range = c.req.query('range') ?? '7d'
    if (!validDownloadRanges.has(range)) {
        return c.json({ error: 'Invalid range. Use 24h, 7d, 30d, or all' }, 400)
    }

    const interval = rangeToSqliteInterval[range]
    const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

    const { results } = await c.env.TELEMETRY_DB.prepare(
        `SELECT date(created_at) AS date, app_version AS version, arch, country, COUNT(*) AS count
         FROM downloads ${whereClause}
         GROUP BY date, version, arch, country
         ORDER BY date ASC`,
    ).all<{ date: string; version: string; arch: string; country: string; count: number }>()

    return c.json(results)
})

// Admin active users — aggregated daily active user data from D1
app.get('/admin/active-users', async (c) => {
    const authError = verifyAdminAuth(c)
    if (authError) return authError

    const range = c.req.query('range') ?? '7d'
    if (!validActiveUserRanges.has(range)) {
        return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
    }

    const interval = rangeToSqliteInterval[range]
    const whereClause = interval ? `WHERE date >= date('now', '${interval}')` : ''

    const { results } = await c.env.TELEMETRY_DB.prepare(
        `SELECT date, app_version AS version, arch, unique_users AS uniqueUsers
         FROM daily_active_users ${whereClause}
         ORDER BY date ASC`,
    ).all<{ date: string; version: string; arch: string; uniqueUsers: number }>()

    return c.json(results)
})

// Admin crashes — aggregated crash data from D1
app.get('/admin/crashes', async (c) => {
    const authError = verifyAdminAuth(c)
    if (authError) return authError

    const range = c.req.query('range') ?? '7d'
    if (!validCrashRanges.has(range)) {
        return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
    }

    const interval = rangeToSqliteInterval[range]
    const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

    const { results } = await c.env.TELEMETRY_DB.prepare(
        `SELECT date(created_at) AS date, top_function AS topFunction, signal,
                COUNT(*) AS count, GROUP_CONCAT(DISTINCT app_version) AS versions
         FROM crash_reports ${whereClause}
         GROUP BY date, topFunction, signal
         ORDER BY date ASC`,
    ).all<{ date: string; topFunction: string; signal: string; count: number; versions: string }>()

    return c.json(results)
})

// Crash report ingestion — writes to D1 for crash analysis
const maxCrashReportBytes = 64 * 1024
const crashReportRequiredFields = ['appVersion', 'osVersion', 'arch', 'signal'] as const
const maxBacktraceBytes = 5_000

interface CrashReport {
    appVersion: string
    osVersion: string
    arch: string
    signal: string
    backtraceFrames?: string[]
    [key: string]: unknown
}

/** Extract the first app-code frame from a backtrace (contains `cmdr` or `cmdr_lib`). */
function extractTopFunction(frames: string[] | undefined): string {
    if (!frames || !Array.isArray(frames)) return 'unknown'
    for (const frame of frames) {
        if (typeof frame === 'string' && (frame.includes('cmdr') || frame.includes('cmdr_lib'))) {
            return frame
        }
    }
    return 'unknown'
}

app.post('/crash-report', async (c) => {
    // Reject oversized payloads before parsing
    const contentLength = c.req.header('content-length')
    if (contentLength && parseInt(contentLength, 10) > maxCrashReportBytes) {
        return c.json({ error: 'Report too large' }, 400)
    }

    let rawBody: string
    try {
        rawBody = await c.req.text()
    } catch {
        return c.json({ error: 'Could not read request body' }, 400)
    }

    if (rawBody.length > maxCrashReportBytes) {
        return c.json({ error: 'Report too large' }, 400)
    }

    let report: CrashReport
    try {
        report = JSON.parse(rawBody) as CrashReport
    } catch {
        return c.json({ error: 'Invalid JSON' }, 400)
    }

    // Validate required fields
    for (const field of crashReportRequiredFields) {
        if (typeof report[field] !== 'string' || report[field].length === 0) {
            return c.json({ error: `Missing required field: ${field}` }, 400)
        }
    }

    // Hash IP with daily salt for deduplication (same pattern as update-check)
    const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
    const dailySalt = new Date().toISOString().slice(0, 10) // YYYY-MM-DD
    const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(ip + dailySalt))
    const hashedIp = [...new Uint8Array(hashBuffer)].map((b) => b.toString(16).padStart(2, '0')).join('')

    const topFunction = extractTopFunction(report.backtraceFrames)
    const backtraceTruncated = JSON.stringify(report.backtraceFrames ?? []).slice(0, maxBacktraceBytes)

    // Write to D1 (fire-and-forget)
    const dbWrite = c.env.TELEMETRY_DB.prepare(
        `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
    )
        .bind(
            hashedIp,
            report.appVersion,
            report.osVersion,
            report.arch,
            report.signal,
            topFunction,
            backtraceTruncated,
        )
        .run()
        .catch(() => {}) // Don't let D1 failure block the response

    try {
        c.executionCtx.waitUntil(dbWrite)
    } catch {
        // executionCtx unavailable (for example, in tests) — await inline as fallback
        await dbWrite
    }

    return c.body(null, 204)
})

const versionPattern = /^\d+\.\d+\.\d+$/

// Update check proxy — tracks version and arch for active user counting, then redirects to latest.json
app.get('/update-check/:version', async (c) => {
    const { version } = c.req.param()

    if (!versionPattern.test(version)) {
        return c.json({ error: 'Invalid version' }, 400)
    }

    const arch = c.req.query('arch') ?? 'unknown'

    // Hash IP with daily salt for deduplication without storing PII
    const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
    const dailySalt = new Date().toISOString().slice(0, 10) // YYYY-MM-DD
    const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(ip + dailySalt))
    const hashedIp = [...new Uint8Array(hashBuffer)].map((b) => b.toString(16).padStart(2, '0')).join('')

    // Write to D1 (fire-and-forget). INSERT OR IGNORE deduplicates via UNIQUE constraint.
    const dbWrite = c.env.TELEMETRY_DB.prepare(
        `INSERT OR IGNORE INTO update_checks (date, hashed_ip, app_version, arch) VALUES (?, ?, ?, ?)`,
    )
        .bind(dailySalt, hashedIp, version, arch)
        .run()
        .catch(() => {})

    try {
        c.executionCtx.waitUntil(dbWrite)
    } catch {
        // executionCtx unavailable (for example, in tests) — await inline as fallback
        await dbWrite
    }

    return c.redirect('https://getcmdr.com/latest.json', 302)
})

// Download redirect — tracks version, arch, and country, then redirects to GitHub Releases
const validArchitectures = new Set(['aarch64', 'x86_64', 'universal'])

app.get('/download/:version/:arch', async (c) => {
    const { version, arch } = c.req.param()

    if (!versionPattern.test(version) || !validArchitectures.has(arch)) {
        return c.json({ error: 'Invalid version or architecture' }, 400)
    }

    const cf = c.req.raw.cf as { country?: string; continent?: string } | undefined
    const country = cf?.country ?? 'unknown'
    const continent = cf?.continent ?? 'unknown'

    // Write to D1 (fire-and-forget)
    const dbWrite = c.env.TELEMETRY_DB.prepare(
        `INSERT INTO downloads (app_version, arch, country, continent) VALUES (?, ?, ?, ?)`,
    )
        .bind(version, arch, country, continent)
        .run()
        .catch(() => {})

    try {
        c.executionCtx.waitUntil(dbWrite)
    } catch {
        // executionCtx unavailable (for example, in tests) — await inline as fallback
        await dbWrite
    }

    return c.redirect(`https://github.com/vdavid/cmdr/releases/download/v${version}/Cmdr_${version}_${arch}.dmg`, 302)
})

export { app }

// --- Scheduled handler (cron) ---

const dbSizeThresholdBytes = 100 * 1024 * 1024 // 100 MB

async function handleCrashNotifications(env: Bindings): Promise<void> {
    if (!env.CRASH_NOTIFICATION_EMAIL || !env.RESEND_API_KEY) return

    const { results } = await env.TELEMETRY_DB.prepare(
        `SELECT id, app_version, os_version, arch, signal, top_function, created_at
         FROM crash_reports WHERE notified_at IS NULL`,
    ).all<{
        id: number
        app_version: string
        os_version: string
        arch: string
        signal: string
        top_function: string
        created_at: string
    }>()

    if (results.length === 0) return

    // Group by top_function
    const grouped = new Map<string, { count: number; versions: Set<string>; mostRecent: string }>()
    for (const row of results) {
        const existing = grouped.get(row.top_function)
        if (existing) {
            existing.count++
            existing.versions.add(row.app_version)
            if (row.created_at > existing.mostRecent) existing.mostRecent = row.created_at
        } else {
            grouped.set(row.top_function, {
                count: 1,
                versions: new Set([row.app_version]),
                mostRecent: row.created_at,
            })
        }
    }

    const crashes: CrashSummaryEntry[] = [...grouped.entries()].map(([topFunction, data]) => ({
        topFunction,
        count: data.count,
        versions: [...data.versions],
        mostRecent: data.mostRecent,
    }))

    const ids = results.map((r) => r.id)
    const now = new Date().toISOString()

    // Mark as notified BEFORE sending email (prefer missed notification over duplicate)
    const placeholders = ids.map(() => '?').join(', ')
    await env.TELEMETRY_DB.prepare(`UPDATE crash_reports SET notified_at = ? WHERE id IN (${placeholders})`)
        .bind(now, ...ids)
        .run()

    await sendCrashNotificationEmail({
        crashes,
        totalCount: results.length,
        to: env.CRASH_NOTIFICATION_EMAIL,
        resendApiKey: env.RESEND_API_KEY,
    })
}

async function handleDailyAggregation(env: Bindings): Promise<void> {
    // Compute yesterday's date
    const yesterday = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10)

    // Check if already aggregated
    const existing = await env.TELEMETRY_DB.prepare(`SELECT 1 FROM daily_active_users WHERE date = ? LIMIT 1`)
        .bind(yesterday)
        .first()

    if (existing) return

    // Aggregate raw update checks into daily_active_users
    await env.TELEMETRY_DB.prepare(
        `INSERT OR IGNORE INTO daily_active_users (date, app_version, arch, unique_users)
         SELECT date, app_version, arch, COUNT(*) AS unique_users
         FROM update_checks
         WHERE date = ?
         GROUP BY date, app_version, arch`,
    )
        .bind(yesterday)
        .run()

    // Prune raw update checks older than 7 days
    await env.TELEMETRY_DB.prepare(`DELETE FROM update_checks WHERE date < date('now', '-7 days')`).run()
}

async function handleDbSizeCheck(env: Bindings): Promise<void> {
    if (!env.CRASH_NOTIFICATION_EMAIL || !env.RESEND_API_KEY) return

    const sizeRow = await env.TELEMETRY_DB.prepare(
        `SELECT page_count * page_size AS total_size FROM pragma_page_count, pragma_page_size`,
    ).first<{ total_size: number }>()

    if (!sizeRow || sizeRow.total_size <= dbSizeThresholdBytes) return

    const sizeMb = sizeRow.total_size / (1024 * 1024)

    // Get row counts for each table
    const tables = ['crash_reports', 'downloads', 'update_checks', 'daily_active_users']
    const tableCounts: Record<string, number> = {}
    for (const table of tables) {
        const row = await env.TELEMETRY_DB.prepare(`SELECT COUNT(*) AS cnt FROM ${table}`).first<{ cnt: number }>()
        tableCounts[table] = row?.cnt ?? 0
    }

    await sendDbSizeAlert({
        sizeMb,
        tableCounts,
        to: env.CRASH_NOTIFICATION_EMAIL,
        resendApiKey: env.RESEND_API_KEY,
    })
}

export default {
    fetch: app.fetch.bind(app),
    async scheduled(event: ScheduledEvent, env: Bindings) {
        try {
            await handleCrashNotifications(env)
        } catch (e) {
            console.error('Crash notifications failed:', e)
        }

        // Daily jobs: only run on the 00:00 UTC invocation
        const hour = new Date(event.scheduledTime).getUTCHours()
        if (hour === 0) {
            try {
                await handleDailyAggregation(env)
            } catch (e) {
                console.error('Daily aggregation failed:', e)
            }

            try {
                await handleDbSizeCheck(env)
            } catch (e) {
                console.error('DB size check failed:', e)
            }
        }
    },
}

// Export handler functions for testing
export { handleCrashNotifications, handleDailyAggregation, handleDbSizeCheck }
