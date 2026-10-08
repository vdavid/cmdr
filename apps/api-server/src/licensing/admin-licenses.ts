/**
 * `GET /admin/licenses`: every license this server has issued, in one list, for the dashboard's
 * Licenses page. Paddle's dashboard can't answer this — it knows about money, not about license
 * codes, activations, or anything we handed out by hand.
 *
 * It reads the `license_issuance` ledger and the `LICENSE_CODES` KV namespace and reconciles the
 * two, because each can hold a license the other doesn't know about.
 */

import { Hono } from 'hono'
import { isValidShortCode } from './license'
import { listLedger, updateLedgerNote, type LedgerEntry } from './license-issuance'
import { listAdjustments, type AdjustmentEntry } from './refunds'
import { type Bindings, maxLicenseNoteLength, maxTransactionIdLength, verifyAdminAuth } from '../types'

const adminLicenses = new Hono<{ Bindings: Bindings }>()

/**
 * What a license is doing right now, as far as we can tell from our own records.
 *
 * ❌ Not the same vocabulary `/validate` answers the app in, and it can't be: for a Paddle row,
 * `active` means "we fulfilled this purchase", never "the subscription is still running" — only
 * Paddle knows that, and asking it per row would cost one API call per license on every page load.
 * For a manual row the two agree, except that `/validate` calls a revoked license `invalid`.
 *
 * - `active`: codes issued and (for a purchase) delivered, not expired, not revoked.
 * - `expired`: past `expiresAt`, which only a hand-issued license carries.
 * - `revoked`: killed by `/admin/revoke`, or by a full refund or chargeback (`refunds.ts`). Its codes
 *   are gone from KV by design.
 * - `undelivered`: a purchase whose codes were minted but never emailed. Someone paid and is waiting.
 * - `unfinished`: claimed, never minted. A delivery that died, or one in flight this second.
 */
export type LicenseState = 'active' | 'expired' | 'revoked' | 'undelivered' | 'unfinished'

export interface LicenseListEntry extends LedgerEntry {
  state: LicenseState
  /** Refunds, chargebacks, and credits Paddle reported against this purchase, newest first. */
  adjustments: AdjustmentEntry[]
}

export interface LicenseListing {
  licenses: LicenseListEntry[]
  /**
   * Activation codes sitting in KV that no ledger row explains: licenses handed out before the
   * ledger existed, or minted by a delivery that died before recording them. Someone may be holding
   * one, and nothing here says whether it ever worked.
   */
  orphanCodes: string[]
  /**
   * The mirror image: codes a ledger row says we issued that are no longer in KV, so they can't be
   * activated. Revoked licenses are left out, since revoking deletes their codes on purpose.
   */
  missingCodes: string[]
  /**
   * Adjustments against a transaction no ledger row describes: a purchase from before the ledger
   * existed, most likely. Only non-revoking ones end up here, since a revoking one writes a row.
   */
  unmatchedAdjustments: AdjustmentEntry[]
}

export function classifyLedgerEntry(entry: LedgerEntry, nowMs: number): LicenseState {
  if (entry.revokedAt) return 'revoked'
  if (entry.expiresAt) {
    const expiresMs = Date.parse(entry.expiresAt)
    // An unreadable expiry reads as expired, the same call `classifyManualLicense` makes for
    // `/validate`, so the two never disagree about a license in front of the same person.
    if (Number.isNaN(expiresMs) || expiresMs <= nowMs) return 'expired'
  }
  if (entry.shortCodes.length === 0) return 'unfinished'
  // A hand-issued license is often deliberately not emailed (the code goes into a reply by hand),
  // so only a purchase with no `emailedAt` is someone left waiting.
  if (entry.source === 'paddle' && !entry.emailedAt) return 'undelivered'
  return 'active'
}

adminLicenses.get('/admin/licenses', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const [ledger, codesInKv, adjustments] = await Promise.all([
    listLedger(c.env.TELEMETRY_DB),
    listShortCodes(c.env.LICENSE_CODES),
    listAdjustments(c.env.TELEMETRY_DB),
  ])

  const now = Date.now()
  const licenses = ledger.map((entry) => ({
    ...entry,
    state: classifyLedgerEntry(entry, now),
    adjustments: adjustments.filter((adjustment) => adjustment.transactionId === entry.transactionId),
  }))
  const listed = new Set(ledger.map((entry) => entry.transactionId))
  const unmatchedAdjustments = adjustments.filter((adjustment) => !listed.has(adjustment.transactionId))

  const recorded = new Set(ledger.flatMap((entry) => entry.shortCodes))
  const stored = new Set(codesInKv)
  const orphanCodes = codesInKv.filter((code) => !recorded.has(code)).sort()
  const missingCodes = ledger
    .filter((entry) => !entry.revokedAt)
    .flatMap((entry) => entry.shortCodes)
    .filter((code) => !stored.has(code))
    .sort()

  return c.json<LicenseListing>({ licenses, orphanCodes, missingCodes, unmatchedAdjustments })
})

interface NoteBody {
  note?: unknown
}

/**
 * `PUT /admin/licenses/:transactionId/note`: rewrite one row's note, from the dashboard's edit
 * dialog. It takes the whole note rather than appending, because the dialog opens pre-filled with
 * what's already there and the person edits it in place.
 *
 * Any row, either source. A purchase's note is the only place a fact ABOUT that sale lives: Paddle
 * holds the money and knows nothing else.
 */
adminLicenses.put('/admin/licenses/:transactionId/note', async (c) => {
  const unauthorized = verifyAdminAuth(c)
  if (unauthorized) return unauthorized

  const transactionId = c.req.param('transactionId')
  if (transactionId.length === 0 || transactionId.length > maxTransactionIdLength) {
    return c.json({ error: 'Invalid transaction id' }, 400)
  }

  const { note } = await c.req.json<NoteBody>()
  if (note !== null && typeof note !== 'string') {
    return c.json({ error: 'Note must be a string, or null to clear it' }, 400)
  }
  if (typeof note === 'string' && note.length > maxLicenseNoteLength) {
    return c.json({ error: `Note must be at most ${String(maxLicenseNoteLength)} characters` }, 400)
  }

  // Whitespace-only reads as clearing, so the dialog needs no separate "clear" control.
  const trimmed = typeof note === 'string' ? note.trim() : ''
  const outcome = await updateLedgerNote(c.env.TELEMETRY_DB, transactionId, trimmed.length > 0 ? trimmed : null)

  switch (outcome) {
    case 'not_found':
      return c.json({ error: 'No license has that transaction id' }, 404)
    case 'manual_needs_note':
      return c.json({ error: "A hand-issued license needs a note saying who it's for and why" }, 400)
    case 'updated':
      return c.json({ transactionId, note: trimmed.length > 0 ? trimmed : null })
  }
})

/**
 * Every license short code in the KV namespace. It also holds device sets (`devices:…`) and the
 * activation counter, so the code format is what separates a license from bookkeeping.
 */
async function listShortCodes(kv: KVNamespace): Promise<string[]> {
  const codes: string[] = []
  let cursor: string | undefined
  for (;;) {
    const page = await kv.list({ cursor })
    for (const key of page.keys) {
      if (isValidShortCode(key.name)) codes.push(key.name)
    }
    if (page.list_complete) return codes
    cursor = page.cursor
  }
}

export { adminLicenses }
