/**
 * Durable fulfillment record for a paid Paddle transaction (D1 table `license_issuance`).
 *
 * Minting codes must happen exactly once per transaction; emailing them is fine to repeat. This
 * module keeps those two apart: a row is claimed atomically before any side effect, the minted
 * codes are stored before the email goes out, and the row is marked delivered only after Resend
 * accepts it. A redelivery therefore re-sends the SAME codes instead of minting a second set.
 * Rows never expire: "this purchase was fulfilled" has no useful end date.
 */

/** How long a claim may sit unfinished before another delivery may take it over. */
export const issuanceStaleAfterMs = 5 * 60 * 1000

export interface IssuanceRecord {
  transactionId: string
  /** Codes minted for this transaction, one per seat. Empty until the mint step lands. */
  shortCodes: string[]
  customerEmail: string | null
  /** ISO timestamp of the claim (or of the last take-over). */
  claimedAt: string
  /** ISO timestamp of the accepted license email; set means the purchase is fully fulfilled. */
  emailedAt: string | null
}

/**
 * What a delivery should do when it finds an existing row.
 *
 * - `delivered`: fulfilled, forever. Acknowledge and do nothing.
 * - `in_flight`: another delivery holds a fresh claim. Ask Paddle to retry.
 * - `resend`: a stale claim already minted codes. Re-send those, mint nothing.
 * - `remint`: a stale claim died before minting. Start over.
 */
export type IssuanceState = 'delivered' | 'in_flight' | 'resend' | 'remint'

export function classifyIssuance(record: IssuanceRecord, nowMs: number): IssuanceState {
  if (record.emailedAt) return 'delivered'
  // An unparseable timestamp reads as stale (NaN fails the comparison), so a broken row still
  // ends in a delivered license rather than a purchase nobody ever completes.
  if (nowMs - Date.parse(record.claimedAt) < issuanceStaleAfterMs) return 'in_flight'
  return record.shortCodes.length > 0 ? 'resend' : 'remint'
}

/**
 * Claim the transaction. Returns true when this delivery owns it and should do the work; false
 * when a row already existed (the caller then loads it and classifies).
 *
 * The conditional insert is the whole atomicity guarantee: two concurrent deliveries race on one
 * primary key, and SQLite hands exactly one of them the row.
 */
export async function claimIssuance(
  db: D1Database,
  params: { transactionId: string; eventId: string | null; now: Date },
): Promise<boolean> {
  const claimed = await db
    .prepare(
      `INSERT INTO license_issuance (transaction_id, event_id, claimed_at) VALUES (?, ?, ?)
       ON CONFLICT(transaction_id) DO NOTHING
       RETURNING transaction_id`,
    )
    .bind(params.transactionId, params.eventId, params.now.toISOString())
    .first()
  return claimed !== null
}

export async function loadIssuance(db: D1Database, transactionId: string): Promise<IssuanceRecord | null> {
  const row = await db
    .prepare(
      `SELECT transaction_id, short_codes, customer_email, claimed_at, emailed_at
       FROM license_issuance WHERE transaction_id = ?`,
    )
    .bind(transactionId)
    .first<{
      transaction_id: string
      short_codes: string | null
      customer_email: string | null
      claimed_at: string
      emailed_at: string | null
    }>()
  if (!row) return null

  return {
    transactionId: row.transaction_id,
    shortCodes: parseShortCodes(row.short_codes),
    customerEmail: row.customer_email,
    claimedAt: row.claimed_at,
    emailedAt: row.emailed_at,
  }
}

/**
 * Take over a stale claim. The update is conditional on the claim timestamp we read, so when two
 * deliveries decide to take over at once, only one wins and the other is told to retry.
 */
export async function takeOverIssuance(db: D1Database, record: IssuanceRecord, now: Date): Promise<boolean> {
  const takenOver = await db
    .prepare(
      `UPDATE license_issuance SET claimed_at = ?
       WHERE transaction_id = ? AND claimed_at = ? AND emailed_at IS NULL
       RETURNING transaction_id`,
    )
    .bind(now.toISOString(), record.transactionId, record.claimedAt)
    .first()
  return takenOver !== null
}

/** Store the minted codes. Runs BEFORE the email, so a lost email can reuse them. */
export async function recordIssuedCodes(
  db: D1Database,
  params: {
    transactionId: string
    shortCodes: string[]
    quantity: number
    licenseType: string
    customerEmail: string | null
    now: Date
  },
): Promise<void> {
  await db
    .prepare(
      `UPDATE license_issuance SET short_codes = ?, quantity = ?, license_type = ?, customer_email = ?, issued_at = ?
       WHERE transaction_id = ?`,
    )
    .bind(
      JSON.stringify(params.shortCodes),
      params.quantity,
      params.licenseType,
      params.customerEmail,
      params.now.toISOString(),
      params.transactionId,
    )
    .run()
}

/** Mark the purchase fulfilled. Only reached once Resend has accepted the license email. */
export async function markIssuanceDelivered(db: D1Database, transactionId: string, now: Date): Promise<void> {
  await db
    .prepare(`UPDATE license_issuance SET emailed_at = ? WHERE transaction_id = ?`)
    .bind(now.toISOString(), transactionId)
    .run()
}

function parseShortCodes(raw: string | null): string[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((code): code is string => typeof code === 'string') : []
  } catch {
    return []
  }
}
