/* eslint-disable-next-line no-restricted-imports -- This file is the Node-side DRIVER: it reads the migration files off disk and talks to the Worker over HTTP. Nothing here is bundled into the Worker, which is what makes the runtime under test honest. */
import { readFile } from 'node:fs/promises'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createTestHarness } from 'wrangler'
import * as ed from '@noble/ed25519'
import { isValidShortCode, validationAnswerSignaturePrefix, type LicenseData } from './license'
import type { ValidationResponse } from './paddle-api'

/**
 * Mints a license through the REAL Worker: wrangler builds `src/index.ts` and runs it in workerd
 * under `wrangler.toml`, so the compatibility date and flags are production's. Node globals the
 * deployed Worker doesn't have (`Buffer`, `process`) are missing here too, which is the point:
 * every other test in this app runs in Node, where they exist.
 *
 * ❌ Never give this harness its own compatibility settings, and never reach for `nodejs_compat`
 * to make it pass: a test runtime richer than the deployed one is the hole that let a
 * `Buffer.from()` call ship in the minting path and throw on every purchase.
 *
 * `/admin/generate` is the smallest route that runs the whole path (sign, encode, store), so it
 * stands in for the Paddle webhook without needing Paddle.
 */

const webhookSecret = 'test-webhook-secret'
const adminToken = 'test-admin-token'
const privateKey = ed.utils.randomSecretKey()
const privateKeyHex = Array.from(privateKey, (byte) => byte.toString(16).padStart(2, '0')).join('')

const server = createTestHarness({
  workers: [
    {
      configPath: new URL('../../wrangler.toml', import.meta.url),
      // Test-only, and deliberately not the production signer: this key only has to verify here.
      secrets: {
        ED25519_PRIVATE_KEY: privateKeyHex,
        PADDLE_WEBHOOK_SECRET_LIVE: webhookSecret,
        ADMIN_API_TOKEN: adminToken,
      },
    },
  ],
})

interface HarnessEnv {
  LICENSE_CODES: KVNamespace
  TELEMETRY_DB: D1Database
}

/** Starting the harness builds the Worker and boots workerd, so it's slower than a normal test. */
beforeAll(async () => {
  await server.listen()
  await applyLicenseIssuanceSchema()
}, 60_000)

afterAll(async () => {
  await server.close()
})

/**
 * Create `license_issuance` in the harness's D1 by running the real migration files, so a column
 * this suite relies on can't exist only in the test.
 */
async function applyLicenseIssuanceSchema(): Promise<void> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  for (const file of ['0012_license_issuance.sql', '0017_manual_licenses.sql']) {
    const sql = await readFile(new URL(`../../migrations/${file}`, import.meta.url), 'utf8')
    const statements = sql
      .split('\n')
      .filter((line) => !line.trimStart().startsWith('--'))
      .join('\n')
      .split(';')
      .map((statement) => statement.trim())
      .filter((statement) => statement.length > 0)
    for (const statement of statements) {
      await env.TELEMETRY_DB.prepare(statement).run()
    }
  }
}

type ValidateBody = Partial<ValidationResponse> & {
  error?: string
  signedAnswer?: { payload: string; signature: string }
}

async function validate(transactionId: string, nonce?: string): Promise<{ status: number; body: ValidateBody }> {
  const response = await server.fetch('http://api.getcmdr.com/validate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ transactionId, nonce }),
  })
  const text = await response.text()
  return { status: response.status, body: JSON.parse(text) as ValidateBody }
}

/** Insert a manual license row the way `/admin/generate` would, minus the minting. */
async function insertManualLicense(row: {
  transactionId: string
  expiresAt?: string | null
  revokedAt?: string | null
  licenseType?: string
}): Promise<void> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  await env.TELEMETRY_DB.prepare(
    `INSERT INTO license_issuance
       (transaction_id, source, short_codes, quantity, license_type, customer_email,
        organization_name, note, claimed_at, issued_at, expires_at, revoked_at)
     VALUES (?, 'manual', '["CMDR-2345-6789-ABCD"]', 1, ?, 'friend@example.com',
             'Acme Inc', 'a friend of the project', ?, ?, ?, ?)`,
  )
    .bind(
      row.transactionId,
      row.licenseType ?? 'commercial_perpetual',
      new Date().toISOString(),
      new Date().toISOString(),
      row.expiresAt ?? null,
      row.revokedAt ?? null,
    )
    .run()
}

function base64ToBytes(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}

interface MintedLicense {
  code: string
  fullKey?: string
  transactionId: string
  type: string
  organizationName: string | null
  expiresAt: string | null
  emailed: boolean
  error?: string
}

/** The body comes back as text as well as parsed: a route that throws answers with bare 500 text. */
async function generate(
  body: Record<string, unknown>,
  token = adminToken,
): Promise<{ status: number; text: string; minted: MintedLicense }> {
  const response = await server.fetch('http://api.getcmdr.com/admin/generate', {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const text = await response.text()
  return { status: response.status, text, minted: parseMinted(text) }
}

/**
 * Parse, tolerating a non-JSON body. A route that throws inside workerd answers with bare 500 text,
 * and a parse error thrown here would bury the status assertion that explains what happened.
 */
function parseMinted(text: string): MintedLicense {
  try {
    return JSON.parse(text) as MintedLicense
  } catch {
    return {} as MintedLicense
  }
}

/** Insert a Paddle fulfillment row, so a note can be written on a purchase rather than a gift. */
async function insertPaddleLicense(transactionId: string): Promise<void> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  await env.TELEMETRY_DB.prepare(
    `INSERT INTO license_issuance
       (transaction_id, source, short_codes, quantity, license_type, customer_email, claimed_at,
        issued_at, emailed_at)
     VALUES (?, 'paddle', '["CMDR-3456-789A-BCDE"]', 1, 'commercial_subscription',
             'buyer@example.com', ?, ?, ?)`,
  )
    .bind(transactionId, new Date().toISOString(), new Date().toISOString(), new Date().toISOString())
    .run()
}

async function setNote(
  transactionId: string,
  note: unknown,
  token = adminToken,
): Promise<{ status: number; body: Record<string, unknown> }> {
  const response = await server.fetch(
    `http://api.getcmdr.com/admin/licenses/${encodeURIComponent(transactionId)}/note`,
    {
      method: 'PUT',
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ note }),
    },
  )
  return { status: response.status, body: JSON.parse(await response.text()) as Record<string, unknown> }
}

/** The ledger row for one transaction, read back from D1 rather than from the listing endpoint. */
async function readNote(transactionId: string): Promise<string | null> {
  const env = await server.getWorker<HarnessEnv>().getEnv()
  const row = await env.TELEMETRY_DB.prepare(`SELECT note FROM license_issuance WHERE transaction_id = ?`)
    .bind(transactionId)
    .first<{ note: string | null }>()
  return row?.note ?? null
}

async function revoke(body: Record<string, unknown>): Promise<{ status: number; body: Record<string, unknown> }> {
  const response = await server.fetch('http://api.getcmdr.com/admin/revoke', {
    method: 'POST',
    headers: { Authorization: `Bearer ${adminToken}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  return { status: response.status, body: JSON.parse(await response.text()) as Record<string, unknown> }
}

describe('minting a license in the Worker runtime', () => {
  it('signs, encodes, and stores a license key', async () => {
    const { status, text, minted } = await generate({
      email: 'buyer@example.com',
      note: 'Runtime test: the whole signing path',
    })

    // Carry the body and the runtime logs into the failure message: a route that throws inside
    // workerd answers with a bare 500, and the reason only exists in the logs.
    const logs = server.getLogs().map((log) => JSON.stringify(log))
    expect(status, `Body: ${text}\nWorker logs:\n${logs.join('\n')}`).toBe(200)

    const code = minted.code
    expect(isValidShortCode(code)).toBe(true)

    // The key the buyer would receive, read back from the KV namespace the route wrote it to.
    const env = await server.getWorker<{ LICENSE_CODES: KVNamespace }>().getEnv()
    const stored = await env.LICENSE_CODES.get<{ fullKey: string }>(code, 'json')
    const [payloadBase64, signatureBase64] = (stored?.fullKey ?? '').split('.')

    const payloadBytes = base64ToBytes(payloadBase64)
    const publicKey = await ed.getPublicKeyAsync(privateKey)
    expect(await ed.verifyAsync(base64ToBytes(signatureBase64), payloadBytes, publicKey)).toBe(true)

    const payload = JSON.parse(new TextDecoder().decode(payloadBytes)) as LicenseData
    expect(payload.email).toBe('buyer@example.com')
    expect(payload.type).toBe('commercial_perpetual')
    expect(payload.shortCode).toBe(code)
  })

  it('mints a license that validates as active, which is the whole point of minting one', async () => {
    const { status, minted } = await generate({
      email: 'prospect@example.com',
      customerName: 'Dana',
      organizationName: 'Acme',
      note: 'Dana Lee, Acme, evaluation',
    })
    expect(status).toBe(200)
    expect(minted.emailed).toBe(false)

    const { body } = await validate(minted.transactionId)

    expect(body.status).toBe('active')
    expect(body.type).toBe('commercial_perpetual')
    expect(body.organizationName).toBe('Acme')
  })

  it('takes an expiry, and calls that license a subscription', async () => {
    const expiresAt = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString()

    const { minted } = await generate({ email: 'trial@example.com', note: 'trial', expiresAt })

    expect(minted.type).toBe('commercial_subscription')
    expect(minted.expiresAt).toBe(expiresAt)
    expect((await validate(minted.transactionId)).body.status).toBe('active')
  })

  it('signs the end date into a dated key, and hands the full key back for offline activation', async () => {
    const expiresAt = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString()

    const { minted } = await generate({ email: 'dated@example.com', note: 'dated', expiresAt })

    const [payloadBase64] = (minted.fullKey ?? '').split('.')
    const payload = JSON.parse(new TextDecoder().decode(base64ToBytes(payloadBase64))) as LicenseData
    expect(payload.expiresAt).toBe(expiresAt)
    expect(payload.shortCode).toBe(minted.code)
  })

  it('refuses to mint without a note, so no license is untraceable', async () => {
    expect((await generate({ email: 'anonymous@example.com' })).status).toBe(400)
  })

  it('refuses a perpetual license with an expiry, which would be a lie either way', async () => {
    const { status } = await generate({
      email: 'contradiction@example.com',
      note: 'contradiction',
      type: 'commercial_perpetual',
      expiresAt: new Date(Date.now() + 1000 * 60).toISOString(),
    })

    expect(status).toBe(400)
  })

  it('refuses an expiry in the past', async () => {
    const { status } = await generate({
      email: 'past@example.com',
      note: 'past',
      expiresAt: '2020-01-01T00:00:00.000Z',
    })

    expect(status).toBe(400)
  })

  it('keeps the license when delivery falls over, and hands the code back', async () => {
    // The harness has no Resend key, so asking for the email is a guaranteed delivery failure.
    const { status, minted } = await generate({
      email: 'undeliverable@example.com',
      note: 'delivery test',
      sendEmail: true,
    })

    expect(status).toBe(502)
    expect(minted.error).toBe('email_not_sent')
    expect(isValidShortCode(minted.code)).toBe(true)
    // The license itself is real, so it can be handed over by hand instead of minted again.
    expect((await validate(minted.transactionId)).body.status).toBe('active')
  })

  it('no longer accepts the Paddle webhook secret as an admin credential', async () => {
    const { status } = await generate({ email: 'buyer@example.com', note: 'wrong credential' }, webhookSecret)

    expect(status).toBe(401)
  })
})

describe('revoking a manual license', () => {
  it('turns a live license invalid and takes its activation code out of circulation', async () => {
    const { minted } = await generate({ email: 'leaked@example.com', note: 'posted the key on a forum' })
    expect((await validate(minted.transactionId)).body.status).toBe('active')

    const revoked = await revoke({ code: minted.code })

    expect(revoked.status).toBe(200)
    expect(revoked.body.status).toBe('revoked')
    expect((await validate(minted.transactionId)).body.status).toBe('invalid')

    const env = await server.getWorker<HarnessEnv>().getEnv()
    expect(await env.LICENSE_CODES.get(minted.code)).toBeNull()
  })

  it('says so rather than revoking twice', async () => {
    const { minted } = await generate({ email: 'twice@example.com', note: 'mistake' })
    await revoke({ transactionId: minted.transactionId })

    const second = await revoke({ transactionId: minted.transactionId })

    expect(second.status).toBe(200)
    expect(second.body.status).toBe('already_revoked')
  })

  it('refuses a Paddle transaction id, which it could not actually revoke', async () => {
    const response = await revoke({ transactionId: 'txn_01abcdef' })

    expect(response.status).toBe(400)
  })

  it('reports an unknown code as not found', async () => {
    const response = await revoke({ code: 'CMDR-2345-6789-ABCD' })

    expect(response.status).toBe(404)
  })
})

/**
 * A manually issued license has no Paddle transaction behind it, so `/validate` has to answer for
 * it from the `license_issuance` ledger. The harness has no Paddle API key, which doubles as the
 * dispatch proof: anything that reaches the Paddle branch answers 502 `upstream_error`.
 */
describe('validating a manual license', () => {
  it('reports a live manual license as active, with its type and organization', async () => {
    await insertManualLicense({ transactionId: 'manual-ACTIVE01' })

    const { status, body } = await validate('manual-ACTIVE01')

    expect(status).toBe(200)
    expect(body.status).toBe('active')
    expect(body.type).toBe('commercial_perpetual')
    expect(body.organizationName).toBe('Acme Inc')
    expect(body.expiresAt).toBeNull()
  })

  it('reports a manual license past its expiry as expired', async () => {
    await insertManualLicense({
      transactionId: 'manual-EXPIRED1',
      licenseType: 'commercial_subscription',
      expiresAt: '2020-01-01T00:00:00.000Z',
    })

    const { body } = await validate('manual-EXPIRED1')

    expect(body.status).toBe('expired')
    expect(body.type).toBe('commercial_subscription')
    expect(body.expiresAt).toBe('2020-01-01T00:00:00.000Z')
  })

  it('reports a revoked manual license as invalid', async () => {
    await insertManualLicense({ transactionId: 'manual-REVOKED1', revokedAt: '2026-09-01T00:00:00.000Z' })

    const { body } = await validate('manual-REVOKED1')

    expect(body.status).toBe('invalid')
    expect(body.type).toBeNull()
  })

  it('reports an unknown manual id as invalid without asking Paddle', async () => {
    const { status, body } = await validate('manual-NOSUCHID')

    expect(status).toBe(200)
    expect(body.status).toBe('invalid')
  })

  it('signs its verdict over the app’s nonce, so a revocation can’t be forged or replayed', async () => {
    await insertManualLicense({ transactionId: 'manual-SIGNEDRV', revokedAt: '2026-09-01T00:00:00.000Z' })
    const nonce = 'aaaabbbbccccdddd0000111122223333'

    const { body } = await validate('manual-SIGNEDRV', nonce)

    const payloadBytes = base64ToBytes(body.signedAnswer?.payload ?? '')
    const prefixed = new Uint8Array([...new TextEncoder().encode(validationAnswerSignaturePrefix), ...payloadBytes])
    const publicKey = await ed.getPublicKeyAsync(privateKey)
    expect(await ed.verifyAsync(base64ToBytes(body.signedAnswer?.signature ?? ''), prefixed, publicKey)).toBe(true)
    expect(JSON.parse(new TextDecoder().decode(payloadBytes))).toMatchObject({
      transactionId: 'manual-SIGNEDRV',
      nonce,
      status: 'invalid',
    })
  })

  it('signs nothing for a caller that sent no nonce, since that answer could be replayed', async () => {
    await insertManualLicense({ transactionId: 'manual-NONONCE1' })

    const { body } = await validate('manual-NONONCE1')

    expect(body.status).toBe('active')
    expect(body.signedAnswer).toBeUndefined()
  })

  it('still resolves a Paddle transaction id against Paddle', async () => {
    // No Paddle API key in the harness, so the Paddle branch can only answer 502. That failure IS
    // the assertion: a `txn_` id must never be answered from the ledger.
    const { status, body } = await validate('txn_01abcdef')

    expect(status).toBe(502)
    expect(body).toEqual({ error: 'upstream_error' })
  })
})

/**
 * The note is the ledger's running record of one license: why it exists, what happened since, what
 * to do next. It's editable from the dashboard, which is the only reason a purchase ever gets one.
 */
describe('editing a license note', () => {
  it('writes a note onto a purchase, which starts without one', async () => {
    await insertPaddleLicense('txn_note_first_sale')

    const { status, body } = await setNote('txn_note_first_sale', 'First purchase ever!!')

    expect(status).toBe(200)
    expect(body.note).toBe('First purchase ever!!')
    expect(await readNote('txn_note_first_sale')).toBe('First purchase ever!!')
  })

  it('replaces the whole note, so the dialog can open pre-filled and save what it holds', async () => {
    await insertManualLicense({ transactionId: 'manual-NOTEEDIT' })

    await setNote('manual-NOTEEDIT', 'a friend of the project\n2026-09-17: asked about SFTP')

    expect(await readNote('manual-NOTEEDIT')).toBe('a friend of the project\n2026-09-17: asked about SFTP')
  })

  it('clears a purchase note when the text is blank', async () => {
    await insertPaddleLicense('txn_note_clearable')
    await setNote('txn_note_clearable', 'written by mistake')

    const { status, body } = await setNote('txn_note_clearable', '   ')

    expect(status).toBe(200)
    expect(body.note).toBeNull()
    expect(await readNote('txn_note_clearable')).toBeNull()
  })

  it('refuses to blank a hand-issued license, which must stay explainable', async () => {
    await insertManualLicense({ transactionId: 'manual-KEEPNOTE' })

    const { status } = await setNote('manual-KEEPNOTE', '')

    expect(status).toBe(400)
    expect(await readNote('manual-KEEPNOTE')).toBe('a friend of the project')
  })

  it('refuses a note past the length cap', async () => {
    await insertPaddleLicense('txn_note_too_long')

    const { status } = await setNote('txn_note_too_long', 'x'.repeat(2001))

    expect(status).toBe(400)
    expect(await readNote('txn_note_too_long')).toBeNull()
  })

  it('reports an unknown transaction id as not found', async () => {
    const { status } = await setNote('txn_note_nosuchid', 'nobody is listening')

    expect(status).toBe(404)
  })

  it('refuses a caller without the admin token', async () => {
    await insertPaddleLicense('txn_note_unauthorized')

    const { status } = await setNote('txn_note_unauthorized', 'should not land', 'wrong-token')

    expect(status).toBe(401)
    expect(await readNote('txn_note_unauthorized')).toBeNull()
  })
})
