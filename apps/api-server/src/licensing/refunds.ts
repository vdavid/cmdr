/**
 * Refunds and chargebacks: the `adjustment.created` / `adjustment.updated` half of
 * `/webhook/paddle`.
 *
 * A refund in Paddle is an adjustment BESIDE the transaction, which itself stays `completed`, so
 * `/validate` asking Paddle about a refunded one-time purchase keeps hearing "fine". This module
 * writes the adjustment into our own records (`license_adjustments`) and, when it takes the money
 * back for good, revokes the purchase in the ledger. `/validate` then answers the license `invalid`,
 * signed, which is the one answer that drops a perpetual license to Personal on the Mac.
 */

import { revokePaddleLicense } from './license-issuance'
import { isPaddleTransactionId } from './license'
import type { Bindings } from '../types'

/** The adjustment fields we read, as Paddle sends them in `data`. Everything else is ignored. */
export interface PaddleAdjustment {
  id: string
  transaction_id: string
  action: string
  /** `full` or `partial`. Optional in Paddle's schema, where a missing one means `partial`. */
  type?: string
  status: string
  items?: { type?: string }[]
  reason?: string | null
  totals?: { total?: string; currency_code?: string } | null
  updated_at?: string
}

/**
 * - `revoke`: the money went back for good, so the license goes with it.
 * - `record`: worth knowing, not worth a license. A human reads it in the dashboard.
 */
export type AdjustmentVerdict = 'revoke' | 'record'

/**
 * Decide what an adjustment does to the license.
 *
 * - **An approved full refund revokes.** Pending ones wait for Paddle (most live refunds need its
 *   approval, and a rejected one never moved money).
 * - **A partial refund only records.** In practice it's a goodwill amount or a price correction, and
 *   which seat of a multi-seat purchase it stands for is unknowable from an amount. If one really is
 *   "give back two of five seats", a human revokes by hand.
 * - **An approved chargeback revokes, full or partial.** The buyer took the money back through
 *   their bank rather than asking, which is not the goodwill case a partial refund is.
 * - **Warnings, credits, and reversals only record.** A warning moves no money yet; a reversal
 *   (Paddle won the dispute) doesn't reinstate either, because revocation is one-way here like
 *   everywhere else: reinstating means minting a new license.
 */
export function classifyAdjustment(
  adjustment: Pick<PaddleAdjustment, 'action' | 'type' | 'status' | 'items'>,
): AdjustmentVerdict {
  if (adjustment.status !== 'approved') return 'record'
  if (adjustment.action === 'chargeback') return 'revoke'
  if (adjustment.action === 'refund' && adjustmentScope(adjustment) === 'full') return 'revoke'
  return 'record'
}

/**
 * `full` when Paddle says so, either at the top or on every line item. `type` is optional in its
 * schema, and a full refund still marks each item `full`, so a payload without it still revokes.
 */
export function adjustmentScope(adjustment: Pick<PaddleAdjustment, 'type' | 'items'>): 'full' | 'partial' {
  if (adjustment.type === 'full') return 'full'
  if (adjustment.type === undefined) {
    const items = adjustment.items ?? []
    if (items.length > 0 && items.every((item) => item.type === 'full')) return 'full'
  }
  return 'partial'
}

/** Pull the adjustment out of a webhook's `data`, or null when it isn't one we can act on. */
export function parseAdjustment(data: unknown): PaddleAdjustment | null {
  if (typeof data !== 'object' || data === null) return null
  const candidate = data as Partial<PaddleAdjustment>
  if (typeof candidate.id !== 'string' || candidate.id.length === 0 || candidate.id.length > 200) return null
  if (typeof candidate.transaction_id !== 'string' || !isPaddleTransactionId(candidate.transaction_id)) return null
  if (candidate.transaction_id.length > 200) return null
  if (typeof candidate.action !== 'string' || typeof candidate.status !== 'string') return null
  return candidate as PaddleAdjustment
}

/** One adjustment as `GET /admin/licenses` shows it. */
export interface AdjustmentEntry {
  adjustmentId: string
  transactionId: string
  action: string
  type: 'full' | 'partial'
  status: string
  reason: string | null
  total: string | null
  currencyCode: string | null
  /** Whether this adjustment, at its current status, takes the license away. */
  revokes: boolean
  updatedAt: string
  receivedAt: string
}

/**
 * Handle one adjustment delivery: record it, then revoke the purchase if it calls for that.
 *
 * Idempotent end to end, so a redelivery or a replay inside the signature window is harmless: the
 * record is an upsert on the adjustment id, the revocation keeps the first `revoked_at`, and
 * deleting codes that are already gone is a no-op.
 */
export async function processAdjustment(
  adjustment: PaddleAdjustment,
  eventId: string | null,
  env: Pick<Bindings, 'TELEMETRY_DB' | 'LICENSE_CODES'>,
): Promise<Response> {
  const now = new Date()
  await recordAdjustment(env.TELEMETRY_DB, adjustment, eventId, now)

  const verdict = classifyAdjustment(adjustment)
  if (verdict === 'record') {
    console.log(
      'Recorded adjustment:',
      adjustment.id,
      adjustment.action,
      adjustment.status,
      'on',
      adjustment.transaction_id,
    )
    return Response.json({ status: 'recorded', adjustmentId: adjustment.id })
  }

  const { newlyRevoked, shortCodes } = await revokePaddleLicense(env.TELEMETRY_DB, adjustment.transaction_id, now)
  // Every time, not only on the first revocation: a delivery that died between the ledger write and
  // these deletes left codes behind, and its retry finds the row already revoked.
  await Promise.all(shortCodes.map((code) => env.LICENSE_CODES.delete(code)))

  console.log(
    newlyRevoked ? 'Revoked license on adjustment:' : 'License already revoked, adjustment:',
    adjustment.id,
    adjustment.action,
    'on',
    adjustment.transaction_id,
  )
  return Response.json({ status: 'revoked', adjustmentId: adjustment.id, transactionId: adjustment.transaction_id })
}

/**
 * Upsert the adjustment at its latest status. Paddle can deliver `adjustment.created` after the
 * `adjustment.updated` that followed it (retries are independent), so a row only moves forward in
 * Paddle's own `updated_at` order.
 */
async function recordAdjustment(
  db: D1Database,
  adjustment: PaddleAdjustment,
  eventId: string | null,
  now: Date,
): Promise<void> {
  const updatedAt = typeof adjustment.updated_at === 'string' ? adjustment.updated_at : now.toISOString()
  await db
    .prepare(
      `INSERT INTO license_adjustments
         (adjustment_id, transaction_id, action, scope, status, reason, total, currency_code, event_id,
          paddle_updated_at, received_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(adjustment_id) DO UPDATE SET
         action = excluded.action, scope = excluded.scope, status = excluded.status, reason = excluded.reason,
         total = excluded.total, currency_code = excluded.currency_code, event_id = excluded.event_id,
         paddle_updated_at = excluded.paddle_updated_at
       WHERE excluded.paddle_updated_at >= license_adjustments.paddle_updated_at`,
    )
    .bind(
      adjustment.id,
      adjustment.transaction_id,
      adjustment.action,
      adjustmentScope(adjustment),
      adjustment.status,
      typeof adjustment.reason === 'string' ? adjustment.reason : null,
      typeof adjustment.totals?.total === 'string' ? adjustment.totals.total : null,
      typeof adjustment.totals?.currency_code === 'string' ? adjustment.totals.currency_code : null,
      eventId,
      updatedAt,
      now.toISOString(),
    )
    .run()
}

interface AdjustmentRow {
  adjustment_id: string
  transaction_id: string
  action: string
  scope: string
  status: string
  reason: string | null
  total: string | null
  currency_code: string | null
  paddle_updated_at: string
  received_at: string
}

/** Every recorded adjustment, newest first. A few per year, so no cap. */
export async function listAdjustments(db: D1Database): Promise<AdjustmentEntry[]> {
  const { results } = await db
    .prepare(
      `SELECT adjustment_id, transaction_id, action, scope, status, reason, total, currency_code,
              paddle_updated_at, received_at
       FROM license_adjustments ORDER BY paddle_updated_at DESC`,
    )
    .all<AdjustmentRow>()

  return results.map((row) => {
    const type = row.scope === 'full' ? 'full' : 'partial'
    return {
      adjustmentId: row.adjustment_id,
      transactionId: row.transaction_id,
      action: row.action,
      type,
      status: row.status,
      reason: row.reason,
      total: row.total,
      currencyCode: row.currency_code,
      revokes: classifyAdjustment({ action: row.action, type, status: row.status }) === 'revoke',
      updatedAt: row.paddle_updated_at,
      receivedAt: row.received_at,
    }
  })
}
