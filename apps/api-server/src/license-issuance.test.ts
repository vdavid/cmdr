import { describe, expect, it } from 'vitest'
import { classifyIssuance, issuanceStaleAfterMs, type IssuanceRecord } from './license-issuance'

const claimedAt = '2026-08-12T10:00:00.000Z'
const claimedAtMs = Date.parse(claimedAt)

function record(overrides: Partial<IssuanceRecord> = {}): IssuanceRecord {
  return {
    transactionId: 'txn_01hv8x',
    shortCodes: [],
    customerEmail: null,
    claimedAt,
    emailedAt: null,
    ...overrides,
  }
}

describe('classifyIssuance', () => {
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
