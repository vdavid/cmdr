/**
 * Licenses we hand out rather than sell: evaluations for a prospect on a work machine, free seats
 * for partner companies, thank-yous for testimonials and bug reports, and customer-service
 * recovery. `POST /admin/generate` mints one, `POST /admin/revoke` kills it.
 *
 * A manual license has no Paddle transaction behind it, so the `license_issuance` row IS the
 * license: `/validate` reads its type, organization, expiry, and revocation (see `licensing.ts`).
 * That's why the row is written before the code can reach anyone, and why minting refuses to
 * proceed without a note: a free license nobody can explain later is worse than no record.
 *
 * Both routes take `ADMIN_API_TOKEN`, like every other admin route.
 */

import { Hono } from 'hono'
import {
  generateLicenseKey,
  generateManualTransactionId,
  generateShortCode,
  isPaddleTransactionId,
  isValidShortCode,
  licenseTypes,
  type LicenseType,
  type StoredLicense,
} from './license'
import {
  findManualLicenseByCode,
  loadManualLicense,
  markIssuanceDelivered,
  recordManualLicense,
  revokeManualLicense,
  type ManualLicenseRecord,
} from './license-issuance'
import { sendLicenseEmail } from '../email/license'
import {
  type Bindings,
  isValidEmail,
  isValidLicenseType,
  maxLicenseNoteLength,
  maxOrganizationNameLength,
  redactEmail,
  verifyAdminAuth,
} from '../types'

const manualLicenses = new Hono<{ Bindings: Bindings }>()

const maxCustomerNameLength = 200

interface MintRequest {
  email: string
  customerName: string
  licenseType: LicenseType
  organizationName: string | null
  /** ISO 8601, or null for a perpetual license. */
  expiresAt: string | null
  note: string
  sendEmail: boolean
}

interface MintBody {
  email?: unknown
  customerName?: unknown
  type?: unknown
  organizationName?: unknown
  expiresAt?: unknown
  note?: unknown
  sendEmail?: unknown
}

manualLicenses.post('/admin/generate', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const parsed = parseMintRequest(await c.req.json<MintBody>())
  if (!parsed.ok) return c.json({ error: parsed.message }, 400)
  const request = parsed.request

  const transactionId = generateManualTransactionId()
  const shortCode = generateShortCode()
  const now = new Date()

  const fullKey = await generateLicenseKey(
    {
      email: request.email,
      transactionId,
      issuedAt: now.toISOString(),
      type: request.licenseType,
      organizationName: request.organizationName ?? undefined,
      shortCode,
      // Signed in, so the app holds the license to this date with no server to ask.
      expiresAt: request.expiresAt ?? undefined,
    },
    c.env.ED25519_PRIVATE_KEY,
  )

  // Ledger first: a row with no key out in the world is invisible, while a key with no row
  // validates as invalid, which hands someone a license that silently doesn't work.
  await recordManualLicense(c.env.TELEMETRY_DB, {
    transactionId,
    shortCode,
    licenseType: request.licenseType,
    customerEmail: request.email,
    organizationName: request.organizationName,
    expiresAt: request.expiresAt,
    note: request.note,
    now,
  })

  const stored: StoredLicense = { fullKey, organizationName: request.organizationName ?? undefined }
  await c.env.LICENSE_CODES.put(shortCode, JSON.stringify(stored))

  const minted = {
    code: shortCode,
    // The key itself, for a reply written by hand: it activates without our server.
    fullKey,
    transactionId,
    type: request.licenseType,
    organizationName: request.organizationName,
    expiresAt: request.expiresAt,
  }
  console.log('Minted a manual license for', redactEmail(request.email), '-', request.note)

  if (!request.sendEmail) return c.json({ ...minted, emailed: false })

  try {
    await sendLicenseEmail({
      to: request.email,
      customerName: request.customerName,
      licenses: [{ shortCode, fullKey }],
      productName: c.env.PRODUCT_NAME,
      supportEmail: c.env.SUPPORT_EMAIL,
      resendApiKey: c.env.RESEND_API_KEY,
      organizationName: request.organizationName ?? undefined,
      licenseType: request.licenseType,
      expiresAt: request.expiresAt ?? undefined,
      issuedManually: true,
    })
  } catch (error) {
    // The license exists and works; only the delivery didn't. Say so, and hand the code back so
    // the caller can pass it on by hand rather than minting a second one.
    console.error('Manual license email rejected:', error instanceof Error ? error.message : String(error))
    return c.json({ ...minted, emailed: false, error: 'email_not_sent' }, 502)
  }

  await markIssuanceDelivered(c.env.TELEMETRY_DB, transactionId, new Date())
  return c.json({ ...minted, emailed: true })
})

interface RevokeBody {
  transactionId?: unknown
  code?: unknown
}

manualLicenses.post('/admin/revoke', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const body = await c.req.json<RevokeBody>()
  const found = await findLicenseToRevoke(body, c.env.TELEMETRY_DB)
  if ('message' in found) return c.json({ error: found.message }, found.status)
  const record = found.record

  if (record.revokedAt) {
    return c.json({ status: 'already_revoked', transactionId: record.transactionId, revokedAt: record.revokedAt })
  }

  const now = new Date()
  if (!(await revokeManualLicense(c.env.TELEMETRY_DB, record.transactionId, now))) {
    // Another call revoked it between our read and our write. Same outcome, so report it as such.
    return c.json({ status: 'already_revoked', transactionId: record.transactionId, revokedAt: null })
  }

  // Drop the short codes too, so the license can't be activated on a fresh machine while the
  // already-activated copies wait out their revalidation. Reinstating means minting a new license.
  for (const code of record.shortCodes) {
    await c.env.LICENSE_CODES.delete(code)
  }

  console.log('Revoked manual license', record.transactionId, '-', record.note ?? 'no note')
  return c.json({
    status: 'revoked',
    transactionId: record.transactionId,
    codes: record.shortCodes,
    organizationName: record.organizationName,
    note: record.note,
    revokedAt: now.toISOString(),
  })
})

/** Resolve the license a revoke request names, by short code or by manual transaction id. */
async function findLicenseToRevoke(
  body: RevokeBody,
  db: D1Database,
): Promise<{ record: ManualLicenseRecord } | { message: string; status: 400 | 404 }> {
  const { transactionId, code } = body
  if (typeof code === 'string' && code.length > 0) {
    if (typeof transactionId === 'string' && transactionId.length > 0) {
      return { message: 'Name either a code or a transactionId, not both', status: 400 }
    }
    const normalized = code.trim().toUpperCase()
    if (!isValidShortCode(normalized)) return { message: 'Invalid license code format', status: 400 }
    const record = await findManualLicenseByCode(db, normalized)
    return record ? { record } : { message: 'No manually issued license carries that code', status: 404 }
  }

  if (typeof transactionId !== 'string' || transactionId.length === 0) {
    return { message: 'Name the license to revoke, by code or transactionId', status: 400 }
  }
  if (isPaddleTransactionId(transactionId)) {
    // Revoking here would do nothing: `/validate` resolves a `txn_` id against Paddle and never
    // reads `revoked_at`. Saying so beats a success that leaves the license working.
    return { message: 'That is a Paddle license: cancel or refund it in Paddle instead', status: 400 }
  }
  const record = await loadManualLicense(db, transactionId)
  return record ? { record } : { message: 'No manually issued license has that transaction id', status: 404 }
}

function parseMintRequest(body: MintBody): { ok: true; request: MintRequest } | { ok: false; message: string } {
  const { email, note } = body
  if (typeof email !== 'string' || !isValidEmail(email)) {
    return { ok: false, message: 'Invalid email format' }
  }
  if (typeof note !== 'string' || note.trim().length === 0) {
    return { ok: false, message: 'A note is required: say who this license is for and why' }
  }
  if (note.length > maxLicenseNoteLength) {
    return { ok: false, message: `Note must be at most ${String(maxLicenseNoteLength)} characters` }
  }

  const optional = parseOptionalFields(body)
  if ('message' in optional) return { ok: false, message: optional.message }

  const licenseType = resolveLicenseType(body.type, optional.expiresAt)
  if (typeof licenseType !== 'string') return { ok: false, message: licenseType.message }

  return {
    ok: true,
    request: {
      email,
      customerName: optional.customerName,
      licenseType,
      organizationName: optional.organizationName,
      expiresAt: optional.expiresAt,
      note: note.trim(),
      sendEmail: body.sendEmail === true,
    },
  }
}

function parseOptionalFields(
  body: MintBody,
): { customerName: string; organizationName: string | null; expiresAt: string | null } | { message: string } {
  const { customerName, organizationName, expiresAt } = body

  if (customerName !== undefined && (typeof customerName !== 'string' || customerName.length > maxCustomerNameLength)) {
    return { message: `Customer name must be a string of at most ${String(maxCustomerNameLength)} characters` }
  }
  if (
    organizationName !== undefined &&
    (typeof organizationName !== 'string' || organizationName.length > maxOrganizationNameLength)
  ) {
    return { message: `Organization name must be a string of at most ${String(maxOrganizationNameLength)} characters` }
  }

  let normalizedExpiry: string | null = null
  if (expiresAt !== undefined && expiresAt !== null) {
    if (typeof expiresAt !== 'string') return { message: 'Expiry must be an ISO 8601 date string' }
    const parsedExpiry = Date.parse(expiresAt)
    if (Number.isNaN(parsedExpiry)) return { message: 'Expiry must be an ISO 8601 date string' }
    if (parsedExpiry <= Date.now()) return { message: 'Expiry is in the past, so the license would be born expired' }
    normalizedExpiry = new Date(parsedExpiry).toISOString()
  }

  return {
    customerName: typeof customerName === 'string' && customerName.length > 0 ? customerName : 'there',
    organizationName: typeof organizationName === 'string' ? organizationName : null,
    expiresAt: normalizedExpiry,
  }
}

/**
 * The type follows the expiry, the way it does for a Paddle purchase: dated means subscription,
 * undated means perpetual. An explicit `type` is accepted only when it agrees, so nobody can mint
 * a "perpetual" license that stops working in March.
 */
function resolveLicenseType(type: unknown, expiresAt: string | null): LicenseType | { message: string } {
  const implied: LicenseType = expiresAt ? 'commercial_subscription' : 'commercial_perpetual'
  if (type === undefined) return implied
  if (typeof type !== 'string' || !isValidLicenseType(type)) {
    return { message: `Invalid license type. Must be one of: ${licenseTypes.join(', ')}` }
  }
  if (type !== implied) {
    return {
      message:
        type === 'commercial_perpetual'
          ? 'A perpetual license cannot have an expiry'
          : 'A subscription license needs an expiry',
    }
  }
  return type
}

export { manualLicenses }
