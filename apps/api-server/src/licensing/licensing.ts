import { Hono } from 'hono'
import { generateLicenseKey, generateShortCode, isValidShortCode, licenseTypes, type LicenseType } from './license'
import { sendLicenseEmail } from '../email/license'
import { sendDeviceCountAlert } from '../email/ops-alerts'
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
import {
  claimIssuance,
  classifyIssuance,
  loadIssuance,
  markIssuanceDelivered,
  recordIssuedCodes,
  takeOverIssuance,
} from './license-issuance'
import {
  type Bindings,
  type PaddleWebhookPayload,
  maxOrganizationNameLength,
  activationCountKey,
  maxTransactionIdLength,
  isValidEmail,
  isValidLicenseType,
  redactEmail,
  getPaddleConfig,
} from '../types'

const licensing = new Hono<{ Bindings: Bindings }>()

/** Stored license data in KV */
interface StoredLicense {
  fullKey: string
  organizationName?: string
}

// Activate license - exchange short code for full cryptographic key
licensing.post('/activate', async (c) => {
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
    // executionCtx unavailable (for example, in tests): await inline as fallback
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
licensing.post('/validate', async (c) => {
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

// Paddle webhook - called when purchase completes
licensing.post('/webhook/paddle', async (c) => {
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

/** Process a completed Paddle transaction: claim it, mint licenses if needed, email them. */
async function processCompletedTransaction(payload: PaddleWebhookPayload, env: Bindings): Promise<Response> {
  const purchaseData = extractPurchaseData(payload)
  if (!purchaseData) {
    console.error('Missing customer_id or transaction ID in webhook payload')
    return Response.json({ error: 'Missing customer_id or transaction ID' }, { status: 400 })
  }

  const claim = await claimFulfillment(env.TELEMETRY_DB, purchaseData.transactionId, payload.event_id ?? null)
  if (!claim.proceed) return claim.response

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
  // Unknown price IDs fall back to a subscription, for backwards compatibility
  const licenseType: LicenseType =
    (purchaseData.priceId ? getLicenseTypeFromPriceId(purchaseData.priceId, priceIds) : null) ??
    'commercial_subscription'

  // Get organization name: prefer customer's business name, fall back to custom_data
  const organizationName = customer.businessName ?? purchaseData.organizationName

  // Mint only when this claim has no codes yet. A redelivery that inherited codes re-sends those,
  // so a lost email costs a duplicate message, never a second set of usable licenses.
  let shortCodes = claim.shortCodes
  if (shortCodes.length === 0) {
    shortCodes = await mintLicenses({
      customerEmail: customer.email,
      transactionId: purchaseData.transactionId,
      quantity: purchaseData.quantity,
      licenseType,
      organizationName,
      privateKey: env.ED25519_PRIVATE_KEY,
      kv: env.LICENSE_CODES,
    })
    await recordIssuedCodes(env.TELEMETRY_DB, {
      transactionId: purchaseData.transactionId,
      shortCodes,
      quantity: purchaseData.quantity,
      licenseType,
      customerEmail: customer.email,
      now: new Date(),
    })
  }

  await sendLicenseEmail({
    to: customer.email,
    customerName: customer.name ?? 'there',
    licenseKeys: shortCodes,
    productName: env.PRODUCT_NAME,
    supportEmail: env.SUPPORT_EMAIL,
    resendApiKey: env.RESEND_API_KEY,
    organizationName,
    licenseType,
  })

  await markIssuanceDelivered(env.TELEMETRY_DB, purchaseData.transactionId, new Date())

  console.log('Licenses sent to:', redactEmail(customer.email), 'type:', licenseType, 'quantity:', shortCodes.length)
  return Response.json({
    status: 'ok',
    email: customer.email,
    licenseType,
    quantity: shortCodes.length,
  })
}

/** Either this delivery owns the fulfillment (with any codes it inherited), or it has a response. */
type ClaimOutcome = { proceed: true; shortCodes: string[] } | { proceed: false; response: Response }

/**
 * Decide whether this delivery should fulfill the transaction. Paddle redelivers the same event
 * (60 attempts over 3 days on live), and a captured webhook can be replayed, so the durable
 * `license_issuance` row is what keeps a purchase to one set of licenses. See `license-issuance.ts`.
 */
async function claimFulfillment(db: D1Database, transactionId: string, eventId: string | null): Promise<ClaimOutcome> {
  const now = new Date()
  if (await claimIssuance(db, { transactionId, eventId, now })) {
    return { proceed: true, shortCodes: [] }
  }

  const record = await loadIssuance(db, transactionId)
  if (!record) return { proceed: false, response: retryLater(transactionId) }

  const state = classifyIssuance(record, now.getTime())
  if (state === 'delivered') {
    console.log('Transaction already fulfilled:', transactionId)
    return { proceed: false, response: Response.json({ status: 'already_processed', transactionId }) }
  }
  if (state === 'in_flight') return { proceed: false, response: retryLater(transactionId) }

  // The claim went stale (a delivery died mid-flight). Take it over, unless another delivery got
  // there first, in which case this one steps aside.
  if (!(await takeOverIssuance(db, record, now))) return { proceed: false, response: retryLater(transactionId) }
  return { proceed: true, shortCodes: state === 'resend' ? record.shortCodes : [] }
}

/** Tell Paddle to redeliver: someone else is fulfilling this transaction right now. */
function retryLater(transactionId: string): Response {
  console.log('Fulfillment already in flight, asking Paddle to redeliver:', transactionId)
  return Response.json({ status: 'in_progress', transactionId }, { status: 503 })
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

/** Generate one signed license per seat and store each under its short code in KV. */
async function mintLicenses(params: {
  customerEmail: string
  transactionId: string
  quantity: number
  licenseType: LicenseType
  organizationName: string | undefined
  privateKey: string
  kv: KVNamespace
}): Promise<string[]> {
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

  return licenseCodes
}

// Manual license generation (for testing or customer service)
// Protected by bearer token matching either live or sandbox webhook secret
licensing.post('/admin/generate', async (c) => {
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

export { licensing }
