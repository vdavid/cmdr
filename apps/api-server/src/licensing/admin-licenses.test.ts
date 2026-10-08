import { describe, expect, it } from 'vitest'
import { app } from '../index'
import type { LicenseListing } from './admin-licenses'

const adminToken = 'test-admin-token-secret'
const authHeaders = { Authorization: `Bearer ${adminToken}` }

interface LedgerRow {
  transaction_id: string
  source: string
  short_codes: string
  quantity: number | null
  license_type: string | null
  customer_email: string | null
  organization_name: string | null
  note: string | null
  claimed_at: string
  issued_at: string | null
  emailed_at: string | null
  expires_at: string | null
  revoked_at: string | null
}

/** A ledger row with the shape D1 returns, so only the fields a case cares about need naming. */
function ledgerRow(overrides: Partial<LedgerRow> & { transaction_id: string }): LedgerRow {
  return {
    source: 'paddle',
    short_codes: '[]',
    quantity: 1,
    license_type: 'commercial_perpetual',
    customer_email: 'buyer@example.com',
    organization_name: null,
    note: null,
    claimed_at: '2026-09-01T10:00:00.000Z',
    issued_at: '2026-09-01T10:00:01.000Z',
    emailed_at: '2026-09-01T10:00:02.000Z',
    expires_at: null,
    revoked_at: null,
    ...overrides,
  }
}

function createD1(rows: LedgerRow[]): D1Database {
  return {
    // The ledger, and no refunds: `webhook-runtime.test.ts` covers adjustments against a real D1.
    prepare: (sql: string) => ({
      all: () => Promise.resolve({ results: sql.includes('license_adjustments') ? [] : rows }),
    }),
  } as unknown as D1Database
}

/** KV that pages: each page is one `list` call, so a scan that stops early is visible. */
function createKv(pages: string[][]): KVNamespace {
  return {
    list: ({ cursor }: { cursor?: string } = {}) => {
      const index = cursor ? Number(cursor) : 0
      const isLast = index >= pages.length - 1
      return Promise.resolve({
        keys: (pages[index] ?? []).map((name) => ({ name })),
        list_complete: isLast,
        cursor: isLast ? undefined : String(index + 1),
      })
    },
  } as unknown as KVNamespace
}

function bindings(params: { rows?: LedgerRow[]; kvPages?: string[][]; token?: string | undefined }) {
  return {
    LICENSE_CODES: createKv(params.kvPages ?? [[]]),
    TELEMETRY_DB: createD1(params.rows ?? []),
    ED25519_PRIVATE_KEY: 'ab'.repeat(32),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'david@getcmdr.com',
    ADMIN_API_TOKEN: params.token === undefined ? adminToken : params.token,
  }
}

async function listLicenses(params: Parameters<typeof bindings>[0] = {}): Promise<LicenseListing> {
  const response = await app.request('/admin/licenses', { headers: authHeaders }, bindings(params))
  expect(response.status).toBe(200)
  return await response.json()
}

describe('GET /admin/licenses', () => {
  it('refuses a caller without the admin token', async () => {
    const response = await app.request('/admin/licenses', {}, bindings({}))

    expect(response.status).toBe(401)
  })

  it('returns one row per license, with every field the ledger holds', async () => {
    const listing = await listLicenses({
      rows: [
        ledgerRow({
          transaction_id: 'manual-ABCD2345EFGH',
          source: 'manual',
          short_codes: '["CMDR-2345-6789-ABCD"]',
          customer_email: 'dana@example.com',
          organization_name: 'Acme',
          note: 'Dana Lee, Acme, evaluation',
          emailed_at: null,
        }),
      ],
      kvPages: [['CMDR-2345-6789-ABCD']],
    })

    expect(listing.licenses).toEqual([
      {
        transactionId: 'manual-ABCD2345EFGH',
        source: 'manual',
        shortCodes: ['CMDR-2345-6789-ABCD'],
        licenseType: 'commercial_perpetual',
        customerEmail: 'dana@example.com',
        organizationName: 'Acme',
        note: 'Dana Lee, Acme, evaluation',
        quantity: 1,
        claimedAt: '2026-09-01T10:00:00.000Z',
        issuedAt: '2026-09-01T10:00:01.000Z',
        emailedAt: null,
        expiresAt: null,
        revokedAt: null,
        state: 'active',
        adjustments: [],
      },
    ])
  })

  it('calls a purchase whose codes never reached the buyer undelivered', async () => {
    const listing = await listLicenses({
      rows: [ledgerRow({ transaction_id: 'txn_minted', short_codes: '["CMDR-2345-6789-ABCD"]', emailed_at: null })],
      kvPages: [['CMDR-2345-6789-ABCD']],
    })

    expect(listing.licenses[0].state).toBe('undelivered')
  })

  it('calls a purchase that never got as far as minting unfinished', async () => {
    const listing = await listLicenses({
      rows: [ledgerRow({ transaction_id: 'txn_claimed', short_codes: '[]', issued_at: null, emailed_at: null })],
    })

    expect(listing.licenses[0].state).toBe('unfinished')
  })

  it('separates a revoked license from an expired one', async () => {
    const listing = await listLicenses({
      rows: [
        ledgerRow({
          transaction_id: 'manual-REVOKED',
          source: 'manual',
          short_codes: '["CMDR-2345-6789-ABCD"]',
          revoked_at: '2026-09-02T00:00:00.000Z',
        }),
        ledgerRow({
          transaction_id: 'manual-EXPIRED',
          source: 'manual',
          short_codes: '["CMDR-3456-789A-BCDE"]',
          license_type: 'commercial_subscription',
          expires_at: '2020-01-01T00:00:00.000Z',
        }),
      ],
      kvPages: [['CMDR-3456-789A-BCDE']],
    })

    expect(listing.licenses.map((license) => license.state)).toEqual(['revoked', 'expired'])
  })

  it('reports a code in KV that no ledger row explains', async () => {
    // A license handed out before the ledger existed, or minted by a delivery that died: someone may
    // be holding it, and nothing here says whether it ever worked.
    const listing = await listLicenses({
      rows: [ledgerRow({ transaction_id: 'txn_known', short_codes: '["CMDR-2345-6789-ABCD"]' })],
      kvPages: [['CMDR-2345-6789-ABCD', 'CMDR-4567-89AB-CDEF'], ['CMDR-5678-9ABC-DEF2']],
    })

    expect(listing.orphanCodes).toEqual(['CMDR-4567-89AB-CDEF', 'CMDR-5678-9ABC-DEF2'])
    expect(listing.missingCodes).toEqual([])
  })

  it('ignores the bookkeeping keys that share the namespace', async () => {
    const listing = await listLicenses({
      kvPages: [['_meta:activation_count', 'devices:txn_01hv8x', 'likes:some-post']],
    })

    expect(listing.orphanCodes).toEqual([])
  })

  it('reports a code we issued that is no longer activatable, unless it was revoked', async () => {
    const listing = await listLicenses({
      rows: [
        ledgerRow({ transaction_id: 'txn_gone', short_codes: '["CMDR-2345-6789-ABCD"]' }),
        ledgerRow({
          transaction_id: 'manual-REVOKED',
          source: 'manual',
          short_codes: '["CMDR-3456-789A-BCDE"]',
          revoked_at: '2026-09-02T00:00:00.000Z',
        }),
      ],
      kvPages: [[]],
    })

    // Revoking deletes the code from KV on purpose, so only the first one is a problem.
    expect(listing.missingCodes).toEqual(['CMDR-2345-6789-ABCD'])
  })
})
