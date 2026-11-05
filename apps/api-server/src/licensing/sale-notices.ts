/**
 * The sale notice: an email to David for every completed purchase (`../email/sale.ts` has the copy).
 *
 * Best-effort by design. The webhook schedules it AFTER the buyer's licenses are mailed and the row
 * is marked delivered, in the background, and `notifySale` never throws, so a failure here can't
 * cost a buyer their key or Paddle its 200. One sale is one email: `claimSaleNotice` is a
 * conditional UPDATE on `sale_notified_at`, so Paddle's redeliveries and a rerun backfill find it
 * taken. A failed send gives the claim back and leaves an ops event for the digest, and
 * `POST /admin/sale-notices` retries it, which is also how sales from before the notice existed get
 * theirs.
 */

import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import { sendSaleNotice } from '../email/sale'
import { humanReportRecipient } from '../email/send'
import { recordOpsEvent } from '../ops-events'
import { type Bindings, getPaddleConfig, maxTransactionIdLength } from '../types'
import { isPaddleTransactionId } from './license'
import { claimSaleNotice, countSalesThrough, releaseSaleNotice } from './license-issuance'
import { getSaleDetails } from './paddle-api'

export type SaleNoticeOutcome =
  /** Mailed. */
  | 'sent'
  /** An earlier delivery or backfill run already mailed (or is mailing) this one. */
  | 'already_sent'
  /** `PADDLE_ENVIRONMENT` isn't `live`: a sandbox purchase is a test, not a sale. */
  | 'not_live'
  /** No `CRASH_NOTIFICATION_EMAIL` / `FEEDBACK_NOTIFICATION_EMAIL` to send it to. */
  | 'no_recipient'
  /** Paddle doesn't know the transaction, or the ledger holds no purchase row for it. */
  | 'not_found'
  /** Paddle says the transaction isn't `completed`. */
  | 'not_completed'
  /** A $0 purchase (a 100% discount): a license handed out, not a sale. */
  | 'free'
  /** Paddle or SMTP2Go failed. The claim is released, so a backfill run can retry. */
  | 'failed'

/** Send the sale notice for one Paddle transaction. Never throws. */
export async function notifySale(env: Bindings, transactionId: string): Promise<SaleNoticeOutcome> {
  try {
    return await notifySaleUnsafe(env, transactionId)
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error)
    console.error('Sale notice failed (non-fatal):', transactionId, reason)
    await recordOpsEvent(env.TELEMETRY_DB, 'sale-notice-failed', saleNoticeFailedMessage(transactionId, reason))
    return 'failed'
  }
}

export function saleNoticeFailedMessage(transactionId: string, reason: string): string {
  return `The sale notice for ${transactionId} didn't go out (${reason.slice(0, 300)}). Resend it with POST /admin/sale-notices.`
}

async function notifySaleUnsafe(env: Bindings, transactionId: string): Promise<SaleNoticeOutcome> {
  if (env.PADDLE_ENVIRONMENT !== 'live') return 'not_live'
  const to = humanReportRecipient(env)
  if (!to) return 'no_recipient'
  const paddleConfig = getPaddleConfig(env)
  if (!paddleConfig) throw new Error('No Paddle API key configured')

  const details = await getSaleDetails(transactionId, paddleConfig)
  if (!details) return 'not_found'
  if (details.status !== 'completed') return 'not_completed'
  if (details.grandTotalMinor <= 0) return 'free'

  const claimedAt = new Date()
  const claim = await claimSaleNotice(env.TELEMETRY_DB, transactionId, claimedAt)
  if (claim === 'not_in_ledger') return 'not_found'
  if (claim === 'already_notified') return 'already_sent'

  try {
    const saleNumber = await countSalesThrough(env.TELEMETRY_DB, transactionId)
    await sendSaleNotice({
      notice: { ...details, saleNumber, paddleEnvironment: paddleConfig.environment },
      to,
      smtp2goApiKey: env.SMTP2GO_API_KEY,
    })
  } catch (error) {
    await releaseSaleNotice(env.TELEMETRY_DB, transactionId, claimedAt)
    throw error
  }
  console.log('Sale notice sent:', transactionId)
  return 'sent'
}

/** How many transactions one backfill call may name. Each costs a Paddle call and maybe an email. */
const maxBackfillTransactions = 50

/**
 * `POST /admin/sale-notices` with `{ "transactionIds": ["txn_..."] }`: send the notice for each named
 * purchase that hasn't had one, and answer each id's outcome. Safe to rerun: a sent one answers
 * `already_sent`. Explicit ids only, so the sales David already knew about stay quiet.
 */
const saleNotices = new Hono<{ Bindings: Bindings }>()

saleNotices.post('/admin/sale-notices', async (c) => {
  const unauthorized = await verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const body = await c.req.json<{ transactionIds?: unknown }>().catch(() => null)
  const ids = body?.transactionIds
  if (
    !Array.isArray(ids) ||
    ids.length === 0 ||
    ids.length > maxBackfillTransactions ||
    !ids.every(
      (id): id is string => typeof id === 'string' && id.length <= maxTransactionIdLength && isPaddleTransactionId(id),
    )
  ) {
    return c.json(
      { error: `transactionIds must be 1–${String(maxBackfillTransactions)} Paddle transaction ids (txn_...)` },
      400,
    )
  }

  const results: Record<string, SaleNoticeOutcome> = {}
  // One at a time, oldest first if the caller lists them that way, so the running counts read in order.
  for (const id of ids) {
    results[id] = await notifySale(c.env, id)
  }
  return c.json({ results })
})

export { saleNotices }
