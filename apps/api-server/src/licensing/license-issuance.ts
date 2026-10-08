/**
 * The license ledger (D1 table `license_issuance`): one row per license this server has issued,
 * whether a customer bought it (`source = 'paddle'`) or we handed it out (`source = 'manual'`).
 *
 * **Paddle rows are a fulfillment record.** Minting codes must happen exactly once per transaction;
 * emailing them is fine to repeat. This module keeps those two apart: a row is claimed atomically
 * before any side effect, the minted codes are stored before the email goes out, and the row is
 * marked delivered only after Resend accepts it. A redelivery therefore re-sends the SAME codes
 * instead of minting a second set. Rows never expire: "this purchase was fulfilled" has no useful
 * end date. A full refund or a chargeback sets `revoked_at` (`refunds.ts`), the one fact about a
 * purchase `/validate` reads from here rather than from Paddle.
 *
 * **Manual rows are the license itself.** There is no Paddle transaction to resolve against, so the
 * row is what `/validate` answers from: its type, organization, expiry, and revocation. See the
 * manual section at the bottom of this file.
 */

import { licenseTypes, type LicenseType } from './license'

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
  /** ISO timestamp of a full refund or chargeback (`refunds.ts`); set means never fulfill it again. */
  revokedAt: string | null
}

/**
 * What a delivery should do when it finds an existing row.
 *
 * - `revoked`: refunded or charged back. Acknowledge and do nothing, even if it was never delivered:
 *   a refund can land before a stuck fulfillment's retry does.
 * - `delivered`: fulfilled, forever. Acknowledge and do nothing.
 * - `in_flight`: another delivery holds a fresh claim. Ask Paddle to retry.
 * - `resend`: a stale claim already minted codes. Re-send those, mint nothing.
 * - `remint`: a stale claim died before minting. Start over.
 */
export type IssuanceState = 'revoked' | 'delivered' | 'in_flight' | 'resend' | 'remint'

export function classifyIssuance(record: IssuanceRecord, nowMs: number): IssuanceState {
  if (record.revokedAt) return 'revoked'
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
      `SELECT transaction_id, short_codes, customer_email, claimed_at, emailed_at, revoked_at
       FROM license_issuance WHERE transaction_id = ?`,
    )
    .bind(transactionId)
    .first<{
      transaction_id: string
      short_codes: string | null
      customer_email: string | null
      claimed_at: string
      emailed_at: string | null
      revoked_at: string | null
    }>()
  if (!row) return null

  return {
    transactionId: row.transaction_id,
    shortCodes: parseShortCodes(row.short_codes),
    customerEmail: row.customer_email,
    claimedAt: row.claimed_at,
    emailedAt: row.emailed_at,
    revokedAt: row.revoked_at,
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

/**
 * Revoke a purchase after a full refund or a chargeback (`refunds.ts`). Keeps the first
 * `revoked_at`, so a redelivered refund changes nothing, and returns the row's codes either way so
 * the caller can make sure they're gone from KV.
 *
 * A transaction with no row yet gets one, born revoked: the refund can arrive while the purchase's
 * own fulfillment is still being retried, and that row is what makes the late delivery stand down
 * (`classifyIssuance` → `revoked`) instead of mailing licenses for money already returned.
 */
export async function revokePaddleLicense(
  db: D1Database,
  transactionId: string,
  now: Date,
): Promise<{ newlyRevoked: boolean; shortCodes: string[] }> {
  const revoked = await db
    .prepare(
      `INSERT INTO license_issuance (transaction_id, source, claimed_at, revoked_at) VALUES (?, 'paddle', ?, ?)
       ON CONFLICT(transaction_id) DO UPDATE SET revoked_at = excluded.revoked_at
       WHERE license_issuance.revoked_at IS NULL AND license_issuance.source = 'paddle'
       RETURNING transaction_id`,
    )
    .bind(transactionId, now.toISOString(), now.toISOString())
    .first()
  const row = await db
    .prepare(`SELECT short_codes FROM license_issuance WHERE transaction_id = ? AND source = 'paddle'`)
    .bind(transactionId)
    .first<{ short_codes: string | null }>()
  return { newlyRevoked: revoked !== null, shortCodes: parseShortCodes(row?.short_codes ?? null) }
}

/**
 * Whether a purchase was refunded or charged back. The ledger can only take a Paddle license AWAY:
 * everything else about one (active, expired, its type) is still Paddle's to say.
 */
export async function isPaddleLicenseRevoked(db: D1Database, transactionId: string): Promise<boolean> {
  const row = await db
    .prepare(`SELECT revoked_at FROM license_issuance WHERE transaction_id = ? AND source = 'paddle'`)
    .bind(transactionId)
    .first<{ revoked_at: string | null }>()
  return Boolean(row?.revoked_at)
}

/**
 * A manually issued license, as `/validate` and `/admin/revoke` need to see it. Only `source =
 * 'manual'` rows load through here: a Paddle license's status lives in Paddle, and answering one
 * from this table would let a canceled subscription keep validating.
 */
export interface ManualLicenseRecord {
  transactionId: string
  /** NULL only on a row written by something other than `/admin/generate`. */
  licenseType: LicenseType | null
  organizationName: string | null
  /** ISO 8601; null means perpetual. */
  expiresAt: string | null
  /** ISO 8601; set means the license is dead, whatever its expiry says. */
  revokedAt: string | null
  shortCodes: string[]
  customerEmail: string | null
  note: string | null
}

/** What `/validate` should answer for a manual license. Mirrors `ValidationResponse['status']`. */
export type ManualLicenseState = 'active' | 'expired' | 'invalid'

export function classifyManualLicense(record: ManualLicenseRecord, nowMs: number): ManualLicenseState {
  if (record.revokedAt) return 'invalid'
  if (!record.expiresAt) return 'active'
  const expiresMs = Date.parse(record.expiresAt)
  // An unreadable expiry counts as expired. Unlike a Paddle fulfillment, where failing open costs a
  // paying customer their licenses, a manual row we wrote ourselves is only unreadable if something
  // is wrong, and the holder has a support path (us) to get a fresh one.
  if (Number.isNaN(expiresMs)) return 'expired'
  return expiresMs > nowMs ? 'active' : 'expired'
}

export async function loadManualLicense(db: D1Database, transactionId: string): Promise<ManualLicenseRecord | null> {
  const row = await db
    .prepare(
      `SELECT transaction_id, license_type, organization_name, expires_at, revoked_at, short_codes,
              customer_email, note
       FROM license_issuance WHERE transaction_id = ? AND source = 'manual'`,
    )
    .bind(transactionId)
    .first<{
      transaction_id: string
      license_type: string | null
      organization_name: string | null
      expires_at: string | null
      revoked_at: string | null
      short_codes: string | null
      customer_email: string | null
      note: string | null
    }>()
  if (!row) return null

  return {
    transactionId: row.transaction_id,
    licenseType: isLicenseType(row.license_type) ? row.license_type : null,
    organizationName: row.organization_name,
    expiresAt: row.expires_at,
    revokedAt: row.revoked_at,
    shortCodes: parseShortCodes(row.short_codes),
    customerEmail: row.customer_email,
    note: row.note,
  }
}

/** Find a manual license by one of its short codes, for revoking from the code in a support thread. */
export async function findManualLicenseByCode(db: D1Database, shortCode: string): Promise<ManualLicenseRecord | null> {
  const row = await db
    .prepare(`SELECT transaction_id FROM license_issuance WHERE source = 'manual' AND short_codes LIKE ?`)
    // The column holds a JSON array of codes, and a code is a fixed, unambiguous format, so the
    // quoted match can't hit a different license by accident.
    .bind(`%"${shortCode}"%`)
    .first<{ transaction_id: string }>()
  return row ? await loadManualLicense(db, row.transaction_id) : null
}

/**
 * Write the ledger row for a manual license. Runs BEFORE the code reaches anyone: a row with no
 * key out in the world is invisible, while a key with no row validates as invalid, which would
 * hand someone a license that silently doesn't work.
 */
export async function recordManualLicense(
  db: D1Database,
  params: {
    transactionId: string
    shortCode: string
    licenseType: LicenseType
    customerEmail: string
    organizationName: string | null
    expiresAt: string | null
    note: string
    now: Date
  },
): Promise<void> {
  await db
    .prepare(
      `INSERT INTO license_issuance
         (transaction_id, source, short_codes, quantity, license_type, customer_email,
          organization_name, expires_at, note, claimed_at, issued_at)
       VALUES (?, 'manual', ?, 1, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .bind(
      params.transactionId,
      JSON.stringify([params.shortCode]),
      params.licenseType,
      params.customerEmail,
      params.organizationName,
      params.expiresAt,
      params.note,
      params.now.toISOString(),
      params.now.toISOString(),
    )
    .run()
}

/**
 * Kill a manual license. Returns false when another call got there first, so the caller can say
 * "already revoked" rather than reporting a second revocation.
 */
export async function revokeManualLicense(db: D1Database, transactionId: string, now: Date): Promise<boolean> {
  const revoked = await db
    .prepare(
      `UPDATE license_issuance SET revoked_at = ?
       WHERE transaction_id = ? AND source = 'manual' AND revoked_at IS NULL
       RETURNING transaction_id`,
    )
    .bind(now.toISOString(), transactionId)
    .first()
  return revoked !== null
}

/**
 * What happened to a note edit.
 *
 * - `updated`: the row now carries the new note.
 * - `not_found`: no ledger row has that transaction id.
 * - `manual_needs_note`: the edit would have left a hand-issued license with no note. Minting
 *   refuses without one for a reason (a free license nobody can explain later is worse than no
 *   record), and an edit that can blank it would walk straight around that.
 */
export type NoteUpdate = 'updated' | 'not_found' | 'manual_needs_note'

/**
 * Rewrite one row's note, whatever its source. A purchase starts with no note at all, so this is
 * the only way one gets there: it's where "first sale ever" or "asked about SFTP on 2026-09-16"
 * lives, beside the license it's about.
 */
export async function updateLedgerNote(
  db: D1Database,
  transactionId: string,
  note: string | null,
): Promise<NoteUpdate> {
  const row = await db
    .prepare(`SELECT source FROM license_issuance WHERE transaction_id = ?`)
    .bind(transactionId)
    .first<{ source: string }>()
  if (!row) return 'not_found'
  if (note === null && row.source === 'manual') return 'manual_needs_note'

  await db.prepare(`UPDATE license_issuance SET note = ? WHERE transaction_id = ?`).bind(note, transactionId).run()
  return 'updated'
}

/**
 * One ledger row as `GET /admin/licenses` shows it: every column, whatever the source, with no
 * judgment applied. `admin-licenses.ts` turns it into a state.
 */
export interface LedgerEntry {
  transactionId: string
  source: 'paddle' | 'manual'
  shortCodes: string[]
  licenseType: LicenseType | null
  customerEmail: string | null
  organizationName: string | null
  note: string | null
  quantity: number | null
  claimedAt: string
  issuedAt: string | null
  emailedAt: string | null
  expiresAt: string | null
  revokedAt: string | null
}

/**
 * The whole ledger, newest claim first. Capped: the dashboard renders a list, and a licensing table
 * that ever reaches four digits deserves a paged endpoint rather than a bigger number here.
 */
export const ledgerListLimit = 1000

const ledgerColumns = `transaction_id, source, short_codes, quantity, license_type, customer_email,
                       organization_name, note, claimed_at, issued_at, emailed_at, expires_at, revoked_at`

interface LedgerRow {
  transaction_id: string
  source: string
  short_codes: string | null
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

function toLedgerEntry(row: LedgerRow): LedgerEntry {
  return {
    transactionId: row.transaction_id,
    // The column is written as one of the two, and a row that somehow isn't manual is a purchase.
    source: row.source === 'manual' ? 'manual' : 'paddle',
    shortCodes: parseShortCodes(row.short_codes),
    licenseType: isLicenseType(row.license_type) ? row.license_type : null,
    customerEmail: row.customer_email,
    organizationName: row.organization_name,
    note: row.note,
    quantity: row.quantity,
    claimedAt: row.claimed_at,
    issuedAt: row.issued_at,
    emailedAt: row.emailed_at,
    expiresAt: row.expires_at,
    revokedAt: row.revoked_at,
  }
}

export async function listLedger(db: D1Database): Promise<LedgerEntry[]> {
  const { results } = await db
    .prepare(
      `SELECT ${ledgerColumns}
       FROM license_issuance
       ORDER BY claimed_at DESC
       LIMIT ${String(ledgerListLimit)}`,
    )
    .all<LedgerRow>()

  return results.map(toLedgerEntry)
}

/**
 * Every row, oldest first, with no cap. The backup is the one reader that must not silently drop
 * anything: a listing that truncates costs a scroll, a backup that truncates loses a license.
 */
export async function readWholeLedger(db: D1Database): Promise<LedgerEntry[]> {
  const { results } = await db
    .prepare(`SELECT ${ledgerColumns} FROM license_issuance ORDER BY claimed_at ASC`)
    .all<LedgerRow>()

  return results.map(toLedgerEntry)
}

function isLicenseType(value: string | null): value is LicenseType {
  return value !== null && (licenseTypes as readonly string[]).includes(value)
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
