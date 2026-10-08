import { describe, expect, it } from 'vitest'
import {
  classifyIssuance,
  classifyManualLicense,
  issuanceStaleAfterMs,
  type IssuanceRecord,
  type ManualLicenseRecord,
} from './license-issuance'

const claimedAt = '2026-08-12T10:00:00.000Z'
const claimedAtMs = Date.parse(claimedAt)

function record(overrides: Partial<IssuanceRecord> = {}): IssuanceRecord {
  return {
    transactionId: 'txn_01hv8x',
    shortCodes: [],
    customerEmail: null,
    claimedAt,
    emailedAt: null,
    revokedAt: null,
    ...overrides,
  }
}

describe('classifyIssuance', () => {
  it('reports a refunded purchase as revoked, so a late delivery never mints or mails it', () => {
    const refunded = record({ revokedAt: '2026-08-12T10:03:00.000Z' })

    expect(classifyIssuance(refunded, claimedAtMs + 1000)).toBe('revoked')
    expect(classifyIssuance(refunded, claimedAtMs + issuanceStaleAfterMs + 1000)).toBe('revoked')
  })

  it('reports a delivered purchase as done, however old the row is', () => {
    const delivered = record({ shortCodes: ['CMDR-2345-6789-ABCD'], emailedAt: '2026-08-12T10:00:05.000Z' })

    expect(classifyIssuance(delivered, claimedAtMs + 1000)).toBe('delivered')
    expect(classifyIssuance(delivered, claimedAtMs + 400 * 24 * 60 * 60 * 1000)).toBe('delivered')
  })

  it('treats a fresh claim as still in flight', () => {
    expect(classifyIssuance(record(), claimedAtMs + issuanceStaleAfterMs - 1)).toBe('in_flight')
  })

  it('re-sends the stored codes once a claim that already minted them goes stale', () => {
    const issued = record({ shortCodes: ['CMDR-2345-6789-ABCD'] })

    expect(classifyIssuance(issued, claimedAtMs + issuanceStaleAfterMs + 1)).toBe('resend')
  })

  it('mints again once a stale claim never got as far as storing codes', () => {
    expect(classifyIssuance(record(), claimedAtMs + issuanceStaleAfterMs + 1)).toBe('remint')
  })

  it('treats an unreadable claim timestamp as stale, so a purchase is never stuck undelivered', () => {
    expect(classifyIssuance(record({ claimedAt: 'not a date' }), claimedAtMs)).toBe('remint')
  })
})

const expiresAt = '2026-09-01T00:00:00.000Z'
const expiresAtMs = Date.parse(expiresAt)

function manual(overrides: Partial<ManualLicenseRecord> = {}): ManualLicenseRecord {
  return {
    transactionId: 'manual-A2B3C4D5E6F7',
    licenseType: 'commercial_perpetual',
    organizationName: 'Acme Inc',
    expiresAt: null,
    revokedAt: null,
    shortCodes: ['CMDR-2345-6789-ABCD'],
    customerEmail: 'friend@example.com',
    note: 'evaluation',
    ...overrides,
  }
}

describe('classifyManualLicense', () => {
  it('keeps a license with no expiry active forever', () => {
    expect(classifyManualLicense(manual(), expiresAtMs + 100 * 365 * 24 * 60 * 60 * 1000)).toBe('active')
  })

  it('holds a dated license active right up to its expiry', () => {
    expect(classifyManualLicense(manual({ expiresAt }), expiresAtMs - 1)).toBe('active')
    expect(classifyManualLicense(manual({ expiresAt }), expiresAtMs)).toBe('expired')
  })

  it('reports a revoked license as invalid, however far off its expiry is', () => {
    const revoked = manual({ revokedAt: '2026-08-01T00:00:00.000Z', expiresAt })

    expect(classifyManualLicense(revoked, expiresAtMs - 1000)).toBe('invalid')
  })

  it('reports a revoked perpetual license as invalid', () => {
    expect(classifyManualLicense(manual({ revokedAt: '2026-08-01T00:00:00.000Z' }), expiresAtMs)).toBe('invalid')
  })

  it('treats an unreadable expiry as expired, so a broken row never grants a license', () => {
    expect(classifyManualLicense(manual({ expiresAt: 'whenever' }), expiresAtMs)).toBe('expired')
  })
})
