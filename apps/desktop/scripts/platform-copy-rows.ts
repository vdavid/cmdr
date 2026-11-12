/**
 * The resolved platform copy, `docs/notes/windows-port/copy-final.json`: its row
 * shape, its validator, and the one function that turns a row into the English
 * catalog value. `apply-platform-copy.ts` reads the file, and its rows are
 * resolved in it by hand, so the review of the English is a diff of that file.
 *
 * A row is one catalog key (a collapsed `.mac`/`.other` pair is one row, named
 * by its survivor key). `status` says what happens to it:
 *
 *  - `apply`: resolved. `branches` holds the final English for all three OSes,
 *    BEFORE folding; the apply script writes it. All three are required, so
 *    "Windows reads what Linux reads" is always a decision somebody wrote down.
 *  - `gated`: the surface is hidden off macOS behind a capability, so the text
 *    stays macOS text.
 *  - `none`: no change (macOS-only by backend, the same words on every OS, or
 *    waiting for other work, as `note` says).
 *  - `open`: the code settles no branch yet. Stays macOS text and is listed in
 *    `copy-decisions.md`. A row with only ONE branch open is `apply`, with that
 *    branch named in `openBranches` and holding the macOS text.
 *
 * Mechanism and vocabulary: `src/lib/intl/DETAILS.md` § "Platform variants",
 * `docs/style-guide.md` § "Platform vocabulary".
 */

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { formatPlatformBranches, isRawKey, parseMessage, platformBranches, visibleText } from './i18n-catalog-lib.ts'
import type { PlatformBranch } from './i18n-catalog-lib.ts'
import { platformSelectIssues } from './i18n-check-platform-select.ts'

/** The resolved-copy file, from this script's dir. */
export const COPY_FINAL_PATH = join(
  import.meta.dirname,
  '..',
  '..',
  '..',
  'docs',
  'notes',
  'windows-port',
  'copy-final.json',
)

export type BranchName = 'macos' | 'windows' | 'other'
export const BRANCH_NAMES: readonly BranchName[] = Object.freeze(['macos', 'windows', 'other'])

export type CopyStatus = 'apply' | 'gated' | 'none' | 'open'
const COPY_STATUSES: readonly string[] = ['apply', 'gated', 'none', 'open']

/** Which Windows register a Trash or Quit row speaks in (`docs/style-guide.md` § "Platform vocabulary"). */
export type CopyRegister = 'label' | 'sentence' | 'action' | 'narration'
const COPY_REGISTERS: readonly string[] = ['label', 'sentence', 'action', 'narration']

/** One accepted lint hit: the branch it sits in, and why it's right there. */
export interface LintException {
  branch: 'windows' | 'other'
  reason: string
}

/** One row of `copy-final.json`. */
export interface CopyRow {
  /** the catalog key the row ends as: a collapsed pair's survivor */
  key: string
  /** the row's `key` in `copy-proposal.json`, where it differs (a pair's `a + b`) */
  proposalKey?: string
  /** the catalog file, like `errors.json` */
  file: string
  source: 'catalog' | 'rust'
  /** the proposal's mechanism, kept for the review page; `status` is what the script acts on */
  mechanism: string
  status: CopyStatus
  /** final English per OS, before folding; `null` only on a row that isn't `apply` */
  branches: Record<BranchName, string | null>
  /** a pair being folded into `key`: the old key macOS read and the one everything else read */
  collapseFrom?: { mac: string; other: string }
  /** required on an `apply` row whose macOS text names the Trash or quitting */
  register?: CopyRegister
  /** branches an agent wrote, where no input doc gave the text; flagged on the review page */
  written?: BranchName[]
  /** branches the code doesn't settle yet: each holds the macOS text until it does */
  openBranches?: ('windows' | 'other')[]
  lintExceptions?: LintException[]
  /** true only for the two accepted macOS English changes */
  macosChanged?: boolean
  /** which doc decided each branch (`row` for the row's status) */
  sources: Partial<Record<BranchName | 'row', string>>
  /** where the string shows, from the proposal, for the review page */
  whereInUi?: string
  note?: string
}

/** Keys hidden off macOS behind a capability: an exact key, or a prefix ending in `*`. */
export interface GatedKeys {
  keys: string
  capability: string
}

/** The whole file. */
export interface CopyFinal {
  readme: string
  gated: GatedKeys[]
  rows: CopyRow[]
}

/** A row the apply script can write: `apply`, with all three branches. */
export type ApplyRow = CopyRow & { status: 'apply'; branches: Record<BranchName, string> }

/** Whether `key` is `pattern` (an exact key) or starts with it (a pattern ending in `*`). */
export function matchesKeyPattern(pattern: string, key: string): boolean {
  return pattern.endsWith('*') ? key.startsWith(pattern.slice(0, -1)) : key === pattern
}

/** Every catalog key a row names: its own, and a pair's two old ones. */
export function rowKeys(row: CopyRow): string[] {
  return row.collapseFrom ? [...new Set([row.key, row.collapseFrom.mac, row.collapseFrom.other])] : [row.key]
}

export function isApplyRow(row: CopyRow): row is ApplyRow {
  return row.status === 'apply' && BRANCH_NAMES.every((name) => typeof row.branches[name] === 'string')
}

const TRASH_WORD = /\btrash\b/i
const QUIT_WORD = /\bquit(s|ting)?\b/i

/** Whether a row's macOS text names the Trash or quitting, so its Windows register must be on record. */
export function touchesTrashOrQuit(row: CopyRow): boolean {
  const macos = row.branches.macos
  if (macos === null) return false
  const text = visibleText(row.key, macos)
  return TRASH_WORD.test(text) || QUIT_WORD.test(text)
}

/**
 * The English catalog value of an `apply` row: its three branches folded into
 * the one legal block. A `macos` or `windows` branch equal to `other` folds into
 * `other`; `macos` and `windows` equal to each other but not to `other` both
 * stay, so Windows never reads Linux words by omission. Three equal branches are
 * a plain value.
 */
export function englishValueOf(row: ApplyRow): string {
  const { macos, windows, other } = row.branches
  const named: PlatformBranch[] = []
  if (macos !== other) named.push({ name: 'macos', text: macos })
  if (windows !== other) named.push({ name: 'windows', text: windows })
  if (named.length === 0) return other
  return formatPlatformBranches([...named, { name: 'other', text: other }])
}

/** What's wrong with the value `englishValueOf` builds for a row, in its family's grammar. */
export function englishValueIssues(row: ApplyRow): string[] {
  const value = englishValueOf(row)
  const issues = platformSelectIssues({ key: row.key, value })
  if (isRawKey(row.key)) {
    if (BRANCH_NAMES.some((name) => row.branches[name].includes("''"))) {
      issues.push("a raw value never doubles an apostrophe (''): no ICU engine reads it")
    }
  } else {
    const parsed = parseMessage(value)
    if (!parsed.ok) issues.push(`not valid ICU: ${parsed.error ?? 'unknown'}`)
  }
  // The block must split back into exactly the branches it was built from.
  const expected = BRANCH_NAMES.map((name) => row.branches[name])
  const read = platformBranches(value)
  const roundTrips = read.every(({ name, text }) =>
    name === null ? expected.every((branch) => branch === text) : row.branches[name] === text,
  )
  if (!roundTrips) issues.push('the built block does not split back into its branches: check braces and apostrophes')
  return issues
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isBranchList(value: unknown, allowed: readonly string[]): boolean {
  return Array.isArray(value) && value.every((name) => typeof name === 'string' && allowed.includes(name))
}

function branchesIssues(row: Record<string, unknown>): string[] {
  const branches = row.branches
  if (!isRecord(branches)) return ['`branches` must be an object with macos, windows, and other']
  const issues: string[] = []
  for (const name of BRANCH_NAMES) {
    const text = branches[name]
    if (typeof text === 'string' || text === null) continue
    issues.push(`\`branches.${name}\` must be a string or null`)
  }
  const unknown = Object.keys(branches).filter((name) => !(BRANCH_NAMES as readonly string[]).includes(name))
  if (unknown.length > 0) issues.push(`unknown branch ${unknown.join(', ')}: only macos, windows, and other`)
  return issues
}

function applyRowIssues(row: CopyRow): string[] {
  const issues: string[] = []
  const missing = BRANCH_NAMES.filter((name) => typeof row.branches[name] !== 'string')
  if (missing.length > 0) {
    issues.push(
      `an apply row carries all three branches, and this one has no ${missing.join(' or ')}: write the text that OS reads, even when it equals another branch`,
    )
    return issues
  }
  for (const name of ['windows', 'other'] as const) {
    if (!row.sources[name]) issues.push(`\`sources.${name}\` must name the doc that decided the ${name} branch`)
  }
  for (const name of row.openBranches ?? []) {
    if (row.branches[name] !== row.branches.macos) {
      issues.push(`the open ${name} branch must hold the macOS text until the code settles it`)
    }
  }
  if (touchesTrashOrQuit(row) && row.register === undefined) {
    issues.push('names the Trash or quitting, so it needs a `register` (label, sentence, action, or narration)')
  }
  return issues
}

function isNonEmptyString(value: unknown): boolean {
  return typeof value === 'string' && value !== ''
}

function isOneOf(allowed: readonly string[]): (value: unknown) => boolean {
  return (value) => typeof value === 'string' && allowed.includes(value)
}

function isLintExceptionList(value: unknown): boolean {
  return (
    Array.isArray(value) &&
    value.every(
      (entry) =>
        isRecord(entry) &&
        (entry.branch === 'windows' || entry.branch === 'other') &&
        typeof entry.reason === 'string' &&
        entry.reason.trim() !== '',
    )
  )
}

function isCollapseFrom(value: unknown): boolean {
  return isRecord(value) && isNonEmptyString(value.mac) && isNonEmptyString(value.other)
}

/** One field's rule: whether it must be there, what a good value is, and what to say otherwise. */
interface FieldRule {
  field: string
  required: boolean
  valid: (value: unknown) => boolean
  issue: string
}

const FIELD_RULES: readonly FieldRule[] = [
  { field: 'key', required: true, valid: isNonEmptyString, issue: '`key` must be a non-empty string' },
  { field: 'file', required: true, valid: isNonEmptyString, issue: '`file` must be a non-empty string' },
  { field: 'mechanism', required: true, valid: isNonEmptyString, issue: '`mechanism` must be a non-empty string' },
  { field: 'source', required: true, valid: isOneOf(['catalog', 'rust']), issue: '`source` must be catalog or rust' },
  {
    field: 'status',
    required: true,
    valid: isOneOf(COPY_STATUSES),
    issue: `\`status\` must be one of ${COPY_STATUSES.join(', ')}`,
  },
  { field: 'sources', required: true, valid: isRecord, issue: '`sources` must be an object' },
  {
    field: 'register',
    required: false,
    valid: isOneOf(COPY_REGISTERS),
    issue: `\`register\` must be one of ${COPY_REGISTERS.join(', ')}`,
  },
  {
    field: 'written',
    required: false,
    valid: (value) => isBranchList(value, BRANCH_NAMES),
    issue: '`written` must list branch names',
  },
  {
    field: 'openBranches',
    required: false,
    valid: (value) => isBranchList(value, ['windows', 'other']),
    issue: '`openBranches` may only name windows and other',
  },
  {
    field: 'collapseFrom',
    required: false,
    valid: isCollapseFrom,
    issue: '`collapseFrom` must be { mac, other }, two catalog keys',
  },
  {
    field: 'lintExceptions',
    required: false,
    valid: isLintExceptionList,
    issue: '`lintExceptions` must list { branch: windows | other, reason }, each with a reason',
  },
]

/** What's wrong with one row's shape, or `[]`. */
export function rowIssues(row: unknown): string[] {
  if (!isRecord(row)) return ['a row must be an object']
  const issues = FIELD_RULES.filter(
    ({ field, required, valid }) => (required || row[field] !== undefined) && !valid(row[field]),
  ).map(({ issue }) => issue)
  issues.push(...branchesIssues(row))
  if (issues.length > 0) return issues
  const typed = row as unknown as CopyRow
  return typed.status === 'apply' ? applyRowIssues(typed) : []
}

/** What's wrong with the whole file's shape, as `key: issue` lines, or `[]`. */
export function copyFinalIssues(file: unknown): string[] {
  if (!isRecord(file) || !Array.isArray(file.rows) || !Array.isArray(file.gated)) {
    return ['copy-final.json must be { readme, gated: [...], rows: [...] }']
  }
  const issues: string[] = []
  for (const entry of file.gated as unknown[]) {
    if (!isRecord(entry) || typeof entry.keys !== 'string' || typeof entry.capability !== 'string') {
      issues.push('gated: every entry must be { keys, capability }')
    }
  }
  const seen = new Set<string>()
  for (const row of file.rows as unknown[]) {
    const key = isRecord(row) && typeof row.key === 'string' ? row.key : '(no key)'
    for (const issue of rowIssues(row)) issues.push(`${key}: ${issue}`)
    if (seen.has(key)) issues.push(`${key}: more than one row has this key`)
    seen.add(key)
  }
  return issues
}

/**
 * Reads and validates the resolved-copy file.
 * @throws listing every shape issue, so a hand edit that broke a row never reaches a catalog
 */
export function readCopyFinal(path: string = COPY_FINAL_PATH): CopyFinal {
  const file = JSON.parse(readFileSync(path, 'utf8')) as unknown
  const issues = copyFinalIssues(file)
  if (issues.length > 0) throw new Error(`${path} isn't valid:\n  ${issues.join('\n  ')}`)
  return file as CopyFinal
}
