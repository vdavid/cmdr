import { describe, expect, it } from 'vitest'
import { classifyAdjustment, type PaddleAdjustment } from './refunds'

function adjustment(overrides: Partial<PaddleAdjustment> = {}): PaddleAdjustment {
  return {
    id: 'adj_01hvgf2s84dr6reszzg29zbvcm',
    transaction_id: 'txn_01hvcc93znj3mpqt1tenkjb04y',
    action: 'refund',
    type: 'full',
    status: 'approved',
    ...overrides,
  }
}

describe('classifyAdjustment', () => {
  it('revokes on an approved full refund', () => {
    expect(classifyAdjustment(adjustment())).toBe('revoke')
  })

  it('revokes on a full refund recognizable only from its items', () => {
    // `type` is optional in Paddle's schema; a full refund still says so on every line item.
    const withoutType = adjustment({ type: undefined, items: [{ type: 'full' }, { type: 'full' }] })

    expect(classifyAdjustment(withoutType)).toBe('revoke')
  })

  it('only records a partial refund: it is a goodwill amount, not a returned license', () => {
    expect(classifyAdjustment(adjustment({ type: 'partial' }))).toBe('record')
    expect(classifyAdjustment(adjustment({ type: undefined, items: [{ type: 'full' }, { type: 'partial' }] }))).toBe(
      'record',
    )
    // Paddle's documented default for a missing type is `partial`.
    expect(classifyAdjustment(adjustment({ type: undefined }))).toBe('record')
  })

  it('waits for Paddle to approve a refund before revoking', () => {
    expect(classifyAdjustment(adjustment({ status: 'pending_approval' }))).toBe('record')
    expect(classifyAdjustment(adjustment({ status: 'rejected' }))).toBe('record')
  })

  it('revokes on an approved chargeback, full or partial: the buyer took the money back by force', () => {
    expect(classifyAdjustment(adjustment({ action: 'chargeback' }))).toBe('revoke')
    expect(classifyAdjustment(adjustment({ action: 'chargeback', type: 'partial' }))).toBe('revoke')
  })

  it('only records the adjustments that move no money out for good', () => {
    for (const action of ['chargeback_warning', 'chargeback_warning_reverse', 'chargeback_reverse', 'credit']) {
      expect(classifyAdjustment(adjustment({ action })), action).toBe('record')
    }
  })
})
