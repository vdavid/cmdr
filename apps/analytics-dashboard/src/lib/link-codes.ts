/**
 * Client-safe helpers for the Link codes page (`/links`): the `?r=` short-code shape, validation, and
 * the example-link builder. Pure functions only, so the page (browser bundle), the server source, and
 * the tests can all share one copy. The actual admin CRUD fetch lives in `$lib/server/sources/link-codes.ts`.
 *
 * The validation here mirrors the api-server's `link-codes.ts` (`isValidCode`, `sanitizeUtmValue`) so the
 * form rejects bad input before a round-trip; the server re-validates and is the source of truth.
 */

/** One code's stored meaning. `utm_source` is required; the rest are optional. `note` is admin-only. */
export interface LinkCodeEntry {
  utm_source: string
  utm_medium?: string
  /** Free-form admin reminder ("r/macapps launch comment"). Never exposed on the public endpoint. */
  note?: string
}

/** The whole code -> meaning map, as returned by the admin list endpoint. */
export type LinkCodeMap = Record<string, LinkCodeEntry>

/** One row for the table: the code plus its entry, flattened for rendering and sorting. */
export interface LinkCodeRow {
  code: string
  utm_source: string
  utm_medium: string
  note: string
}

const maxCodeLength = 64
const maxUtmLength = 120

// Codes live in an inconspicuous public URL (getcmdr.com/?r=rmc), so the charset stays tight and
// URL-clean: lowercase alphanumerics plus `. _ -`. Mirrors the api-server and the blogs' sanitizer.
const codePattern = /^[a-z0-9._-]+$/

/** Whether a code is a valid map key: lowercase `[a-z0-9._-]`, 1..64 chars. */
export function isValidCode(code: string): boolean {
  return code.length >= 1 && code.length <= maxCodeLength && codePattern.test(code)
}

/**
 * Normalize a UTM value the same way the api-server, the blogs, and the `/download` `ref` handler do:
 * lowercase, drop anything outside `[a-z0-9._-]`, cap length. Returns '' for nullish/empty input.
 */
export function sanitizeUtmValue(value: string | undefined | null): string {
  if (!value) return ''
  return value
    .toLowerCase()
    .replace(/[^a-z0-9._-]/g, '')
    .slice(0, maxUtmLength)
}

/** The validated, server-ready shape of a save, or the first validation error. */
export type ValidatedLinkCode =
  | { ok: true; code: string; utm_source: string; utm_medium?: string; note?: string }
  | { ok: false; error: string }

/**
 * Validate and normalize a save from the form: trims the code, sanitizes UTM values, enforces the
 * code charset and a required source. Used by the server action before proxying; pure so tests cover it.
 */
export function validateLinkCode(input: {
  code?: string
  utm_source?: string
  utm_medium?: string
  note?: string
}): ValidatedLinkCode {
  const code = (input.code ?? '').trim().toLowerCase()
  if (!isValidCode(code)) {
    return { ok: false, error: 'Use lowercase letters, numbers, and . _ - only (up to 64 characters).' }
  }
  const utm_source = sanitizeUtmValue(input.utm_source)
  if (!utm_source) {
    return { ok: false, error: 'A source is required (lowercase letters, numbers, and . _ - only).' }
  }
  const utm_medium = sanitizeUtmValue(input.utm_medium)
  const note = (input.note ?? '').trim().slice(0, 500)

  const result: ValidatedLinkCode = { ok: true, code, utm_source }
  if (utm_medium) result.utm_medium = utm_medium
  if (note) result.note = note
  return result
}

/** Flatten the admin map into sorted rows for the table (by code, ascending). */
export function toRows(map: LinkCodeMap): LinkCodeRow[] {
  return Object.entries(map)
    .map(([code, entry]) => ({
      code,
      utm_source: entry.utm_source,
      utm_medium: entry.utm_medium ?? '',
      note: entry.note ?? '',
    }))
    .sort((a, b) => a.code.localeCompare(b.code))
}

/** The example tracking link a code expands from, shown in the UI so David can copy it. */
export function exampleLink(code: string): string {
  return `getcmdr.com/?r=${code}`
}
