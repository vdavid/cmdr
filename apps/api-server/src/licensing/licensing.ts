import { Hono } from 'hono'
import {
  generateLicenseKey,
  generateShortCode,
  isPaddleTransactionId,
  isValidNonce,
  isValidShortCode,
  signValidationAnswer,
  type LicenseType,
  type StoredLicense,
} from './license'
import { manualLicenses } from './manual-licenses'
import { adminLicenses } from './admin-licenses'
import { sendLicenseEmail, type EmailedLicense } from '../email/license'
import { sendDeviceCountAlert } from '../email/ops-alerts'
import { verifyPaddleWebhookMulti } from './paddle'
import {
  getSubscriptionStatus,
  getLicenseTypeFromPriceId,
  getCustomerDetails,
  PaddleApiError,
  type ValidationResponse,
  type PriceIdMapping,
} from './paddle-api'
import { pruneStaleDevices, shouldAlert, type DeviceSet } from './device-tracking'
import { parseAdjustment, processAdjustment } from './refunds'
import {
  claimIssuance,
  classifyIssuance,
  classifyManualLicense,
  isPaddleLicenseRevoked,
  loadIssuance,
  loadManualLicense,
  markIssuanceDelivered,
  recordIssuedCodes,
  takeOverIssuance,
} from './license-issuance'
import {
  type Bindings,
  type PaddleWebhookPayload,
  maxOrganizationNameLength,
  activationCountKey,
  enforceIpRateLimit,
  maxTransactionIdLength,
  redactEmail,
  getPaddleConfig,
} from '../types'

const licensing = new Hono<{ Bindings: Bindings }>()

// Activate license - exchange short code for full cryptographic key
licensing.post('/activate', async (c) => {
  const limited = await enforceIpRateLimit(c.env.ACTIVATE_LIMITER, c.req)
  if (limited) return limited

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
  const limited = await enforceIpRateLimit(c.env.VALIDATE_LIMITER, c.req)
  if (limited) return limited

  const body = await c.req.json<{ transactionId?: string; deviceId?: string; nonce?: unknown }>()
  const { response, trackingPromise } = await handleValidation(body.transactionId, body.deviceId, c.env)
  if (trackingPromise) {
    c.executionCtx.waitUntil(trackingPromise)
  }
  // Only an answer bound to the app's nonce gets signed: without one it could be replayed. Apps
  // before signed answers send no nonce and read the plain fields, which stay beside it.
  const { transactionId, nonce } = body
  if (
    response.status === 200 &&
    'status' in response.body &&
    typeof transactionId === 'string' &&
    transactionId.length <= maxTransactionIdLength &&
    isValidNonce(nonce)
  ) {
    const signedAnswer = await signValidationAnswer(
      { ...response.body, transactionId, nonce },
      c.env.ED25519_PRIVATE_KEY,
      new Date(),
    )
    return c.json({ ...response.body, signedAnswer }, 200)
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

  // Anything outside Paddle's id namespace is one we issued by hand, and Paddle would 404 on it.
  if (!isPaddleTransactionId(transactionId)) {
    return { response: await validateManualLicense(transactionId, env.TELEMETRY_DB), trackingPromise: null }
  }

  const baseTransactionId = transactionId.replace(/-\d+$/, '')

  // A refund leaves the transaction `completed` in Paddle, so only our ledger knows about it. The
  // ledger can take a Paddle license away, never grant one: anything else still comes from Paddle.
  const revocation = await readPaddleRevocation(baseTransactionId, env.TELEMETRY_DB)
  if (revocation === 'revoked') return { response: { body: invalidResponse(), status: 200 }, trackingPromise: null }
  if (revocation === 'unreadable') {
    return { response: { body: { error: 'upstream_error' }, status: 502 }, trackingPromise: null }
  }

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

/**
 * Whether the purchase was refunded. `unreadable` (the ledger read threw) answers 502
 * `upstream_error`, the same as a Paddle outage, so the app keeps its cached status rather than
 * reading a D1 blip as anything.
 */
async function readPaddleRevocation(
  baseTransactionId: string,
  db: D1Database,
): Promise<'revoked' | 'not_revoked' | 'unreadable'> {
  try {
    return (await isPaddleLicenseRevoked(db, baseTransactionId)) ? 'revoked' : 'not_revoked'
  } catch (error) {
    console.error('Ledger read failed during validation:', error instanceof Error ? error.message : String(error))
    return 'unreadable'
  }
}

/**
 * Answer for a manually issued license out of the `license_issuance` ledger, which is the whole
 * record of it: there is no Paddle transaction behind a manual license.
 *
 * A ledger read that throws answers 502 `upstream_error`, exactly like a Paddle outage does, so the
 * app falls back to its cached status instead of dropping a working license to Personal.
 *
 * Device tracking doesn't run here: the fair-use alert resolves the customer through the Paddle
 * API, and a manual license has no Paddle customer to resolve.
 */
async function validateManualLicense(
  transactionId: string,
  db: D1Database,
): Promise<{ body: ValidationResponse | { error: string }; status: 200 | 502 }> {
  let record
  try {
    record = await loadManualLicense(db, transactionId)
  } catch (error) {
    console.error('Ledger read failed during validation:', error instanceof Error ? error.message : String(error))
    return { body: { error: 'upstream_error' }, status: 502 }
  }

  if (!record) return { body: invalidResponse(), status: 200 }

  const state = classifyManualLicense(record, Date.now())
  if (state === 'invalid') return { body: invalidResponse(), status: 200 }

  return {
    body: {
      status: state,
      type: record.licenseType,
      organizationName: record.organizationName,
      expiresAt: record.expiresAt,
    },
    status: 200,
  }
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

// Paddle webhook: a completed purchase mints licenses, a refund or chargeback revokes them
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

  try {
    return await dispatchWebhookEvent(payload, c.env)
  } catch (error) {
    console.error('Webhook processing failed:', error instanceof Error ? error.message : String(error))
    return c.json({ error: 'Internal server error' }, 500)
  }
})

/** Route a verified webhook to its handler: a purchase mints, an adjustment may revoke, the rest is acknowledged. */
async function dispatchWebhookEvent(payload: PaddleWebhookPayload, env: Bindings): Promise<Response> {
  if (payload.event_type === 'transaction.completed') {
    if (isSubscriptionFollowUp(payload.data?.origin)) {
      console.log('Subscription follow-up transaction, nothing to issue:', payload.data?.id, payload.data?.origin)
      return Response.json({ status: 'ignored', event: payload.event_type, origin: payload.data?.origin })
    }
    return await processCompletedTransaction(payload, env)
  }
  if (payload.event_type === 'adjustment.created' || payload.event_type === 'adjustment.updated') {
    const adjustment = parseAdjustment(payload.data)
    if (!adjustment) {
      console.error('Adjustment webhook without a usable adjustment id, transaction id, action, or status')
      return Response.json({ error: 'Invalid adjustment' }, { status: 400 })
    }
    return await processAdjustment(adjustment, payload.event_id ?? null, env)
  }
  return Response.json({ status: 'ignored', event: payload.event_type })
}

/**
 * Transactions Paddle creates on its own from an existing subscription: a renewal, a one-off charge,
 * a plan or seat change, a payment-method update. Each completes with a NEW `txn_` id, but the
 * buyer's key names the subscription's FIRST transaction and keeps validating through the
 * subscription's status, so fulfilling one would only mail a second, redundant set of licenses.
 * A missing or unknown origin still fulfills: a paying buyer left without a key is the worse miss.
 * (Origins from https://developer.paddle.com/webhooks/transactions/transaction-completed, 2026-10-06.)
 */
const subscriptionFollowUpOrigins: ReadonlySet<string> = new Set([
  'subscription_recurring',
  'subscription_charge',
  'subscription_update',
  'subscription_payment_method_change',
])

function isSubscriptionFollowUp(origin: string | undefined): boolean {
  return origin !== undefined && subscriptionFollowUpOrigins.has(origin)
}

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
  let licenses: EmailedLicense[]
  if (claim.shortCodes.length === 0) {
    licenses = await mintLicenses({
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
      shortCodes: licenses.map((license) => license.shortCode),
      quantity: purchaseData.quantity,
      licenseType,
      customerEmail: customer.email,
      now: new Date(),
    })
  } else {
    licenses = await readStoredLicenses(env.LICENSE_CODES, claim.shortCodes)
  }
  const shortCodes = licenses.map((license) => license.shortCode)

  await sendLicenseEmail({
    to: customer.email,
    customerName: customer.name ?? 'there',
    licenses,
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
  if (state === 'revoked') {
    console.log('Transaction was refunded, not fulfilling it:', transactionId)
    return { proceed: false, response: Response.json({ status: 'revoked', transactionId }) }
  }
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
}): Promise<EmailedLicense[]> {
  const licenses: EmailedLicense[] = []

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

    licenses.push({ shortCode, fullKey })
  }

  return licenses
}

/**
 * The full keys behind codes an earlier delivery minted, so a resent email carries them too. A code
 * missing from KV still goes out on its own: holding back the whole email over it would leave the
 * buyer with nothing.
 */
async function readStoredLicenses(kv: KVNamespace, shortCodes: string[]): Promise<EmailedLicense[]> {
  return Promise.all(
    shortCodes.map(async (shortCode) => {
      const stored = await kv.get<StoredLicense>(shortCode, 'json')
      return stored ? { shortCode, fullKey: stored.fullKey } : { shortCode }
    }),
  )
}

// Minting and revoking hand-issued licenses, and listing everything we've ever issued, are their own
// features, mounted here so the licensing area stays one route module from `index.ts`'s point of view.
licensing.route('/', manualLicenses)
licensing.route('/', adminLicenses)

export { licensing }
