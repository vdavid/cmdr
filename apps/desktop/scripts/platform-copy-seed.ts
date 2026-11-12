#!/usr/bin/env node
/**
 * Seeds `docs/notes/windows-port/copy-final.json`, the resolved platform copy
 * (`platform-copy-rows.ts` has the row shape), from
 * `docs/notes/windows-port/copy-proposal.json` plus what the plan and the
 * verdicts add to it (`platform-copy-seed-data.ts`).
 *
 * What a seeded row starts as:
 *  - a `.mac`/`.other` pair and the two accepted macOS changes: `apply`, resolved;
 *  - a key behind a capability gate: `gated`;
 *  - any other `select` row: `pending`, holding the proposal's three texts for
 *    its milestone to resolve against the decisions, the verdicts, and the evidence;
 *  - a `none`, `neutral`, or ungated `split-feature` row, and a "reviewed, no
 *    change" row: `none`. The `code` rows too: they wait for the keymap work (#1102);
 *  - a row the proposal never listed: `pending`, with today's English as its
 *    macOS text and a note saying what the plan decided.
 *
 * Two modes, neither of which rewrites a row the file already holds, because
 * milestones resolve rows in the file by hand:
 *  - no flag: the first write. Refuses when the file exists.
 *  - `--add-missing`: appends rows the file lacks and refreshes the gate list.
 *
 * Run from `apps/desktop/`: `node scripts/platform-copy-seed.ts [--add-missing]`.
 */

import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { BASE_LOCALE, loadCatalog } from './i18n-catalog-lib.ts'
import { COPY_FINAL_PATH, copyFinalIssues, matchesKeyPattern } from './platform-copy-rows.ts'
import type { CopyFinal, CopyRow, CopyStatus } from './platform-copy-rows.ts'
import {
  ADDED_SEEDS,
  GATED_KEYS,
  PROMOTED_KEYS,
  REGISTERS,
  RESOLVED_SEEDS,
  ROW_NOTES,
} from './platform-copy-seed-data.ts'
import type { ResolvedSeed } from './platform-copy-seed-data.ts'

const PROPOSAL_PATH = join(import.meta.dirname, '..', '..', '..', 'docs', 'notes', 'windows-port', 'copy-proposal.json')

const README =
  'The resolved platform copy: one row per catalog key (a collapsed .mac/.other pair is one row, named by its survivor). `branches` is the final English per OS before folding; `status` is apply (resolved, the script writes it), pending (seeded from the proposal, its milestone resolves it), gated (hidden off macOS), none (no change), or open (the code settles nothing yet). Seeded by apps/desktop/scripts/platform-copy-seed.ts, which never rewrites an existing row; resolved by hand, milestone by milestone; applied by apps/desktop/scripts/apply-platform-copy.ts. Row shape and rules: apps/desktop/scripts/platform-copy-rows.ts. Vocabulary: docs/style-guide.md § "Platform vocabulary".'

/** One `strings` row of the proposal: the fields the seed reads. */
export interface ProposalRow {
  key: string
  keys: string[]
  file: string
  source: 'catalog' | 'rust'
  mechanism: string
  glossaryTerms: string[]
  whereInUi?: string | null
  macos: string
  windows: string | null
  linux: string | null
  macosChanged?: boolean
}

/** One "reviewed, no change" row of the proposal. */
interface ReviewedRow {
  key: string
  file: string
  reason: string
  currentEnglish: string
}

/** The parts of `copy-proposal.json` the seed reads. */
export interface Proposal {
  strings: ProposalRow[]
  reviewedNoChange: ReviewedRow[]
}

/** The catalog file a key lives in: prefix and filename are 1:1. */
function catalogFileOf(key: string): string {
  return `${key.split('.')[0]}.json`
}

function sourceOf(file: string): 'catalog' | 'rust' {
  return file === 'menu.json' ? 'rust' : 'catalog'
}

/** Drops `undefined` fields, so a row's JSON carries only what it has. */
function compact(row: CopyRow): CopyRow {
  return Object.fromEntries(Object.entries(row).filter(([, value]) => value !== undefined)) as unknown as CopyRow
}

function resolvedRow(seed: ResolvedSeed, proposalRow?: ProposalRow): CopyRow {
  return compact({
    key: seed.key,
    proposalKey: proposalRow && proposalRow.key !== seed.key ? proposalRow.key : undefined,
    file: catalogFileOf(seed.key),
    source: proposalRow?.source ?? sourceOf(catalogFileOf(seed.key)),
    mechanism: proposalRow?.mechanism ?? 'select',
    status: 'apply',
    branches: seed.branches,
    collapseFrom: seed.collapseFrom,
    register: REGISTERS[seed.key],
    written: seed.written,
    openBranches: seed.openBranches,
    lintExceptions: seed.lintExceptions,
    macosChanged: seed.macosChanged,
    sources: seed.sources,
    whereInUi: proposalRow?.whereInUi ?? undefined,
    note: seed.note,
  })
}

function resolvedSeedFor(row: ProposalRow): ResolvedSeed | undefined {
  if (row.keys.length === 1) return RESOLVED_SEEDS.find((seed) => !seed.collapseFrom && seed.key === row.keys[0])
  const [mac, other] = row.keys
  return RESOLVED_SEEDS.find((seed) => seed.collapseFrom?.mac === mac && seed.collapseFrom.other === other)
}

/** The status, row source, and note a proposal row that isn't resolved at seeding time starts with. */
function seededState(row: ProposalRow): { status: CopyStatus; rowSource?: string; note?: string } {
  const key = row.keys[0]
  const gate = GATED_KEYS.find(({ keys }) => matchesKeyPattern(keys, key))
  if (gate) return { status: 'gated', rowSource: `verdicts § Gates: hidden off macOS (${gate.capability})` }
  if (row.mechanism === 'select' || PROMOTED_KEYS.has(key)) return { status: 'pending' }
  if (row.mechanism === 'code') {
    return {
      status: 'none',
      note: 'verdicts: waits for the keymap work (#1102), which passes the keys as placeholders.',
    }
  }
  return { status: 'none', rowSource: `proposal: ${row.mechanism}` }
}

function proposalRowSeed(row: ProposalRow): CopyRow {
  const resolved = resolvedSeedFor(row)
  if (resolved) return resolvedRow(resolved, row)
  if (row.keys.length > 1) throw new Error(`The pair ${row.key} has no entry in RESOLVED_SEEDS.`)

  const key = row.keys[0]
  const { status, rowSource, note } = seededState(row)
  const carriesProposalText = status === 'pending' && row.mechanism === 'select'
  const trashSentence = status === 'pending' && row.glossaryTerms.includes('trash') ? 'sentence' : undefined
  return compact({
    key,
    file: row.file,
    source: row.source,
    mechanism: row.mechanism,
    status,
    branches: {
      macos: row.macos,
      windows: carriesProposalText ? row.windows : null,
      other: carriesProposalText ? row.linux : null,
    },
    register: REGISTERS[key] ?? trashSentence,
    sources: carriesProposalText ? { windows: 'proposal', other: 'proposal' } : rowSource ? { row: rowSource } : {},
    whereInUi: row.whereInUi ?? undefined,
    note: [ROW_NOTES[key], note].filter((part) => part !== undefined).join(' ') || undefined,
  })
}

function plainRow(key: string, status: CopyStatus, macos: string, note: string): CopyRow {
  const file = catalogFileOf(key)
  return compact({
    key,
    file,
    source: sourceOf(file),
    mechanism: 'none',
    status,
    branches: { macos, windows: null, other: null },
    register: REGISTERS[key],
    sources: {},
    note,
  })
}

/**
 * Builds the seed file from the proposal and today's English.
 * @param english the `en` catalog's messages: the macOS text of a row the proposal never listed
 * @returns the file, plus the added keys English doesn't hold (their rows are left out)
 */
export function seedCopyFinal(
  proposal: Proposal,
  english: Record<string, string>,
): { file: CopyFinal; missing: string[] } {
  const rows: CopyRow[] = proposal.strings.map(proposalRowSeed)
  const taken = new Set(rows.flatMap((row) => [row.key, ...(row.collapseFrom ? Object.values(row.collapseFrom) : [])]))
  const missing: string[] = []
  const add = (row: CopyRow): void => {
    if (taken.has(row.key)) return
    taken.add(row.key)
    rows.push(row)
  }

  for (const seed of RESOLVED_SEEDS) {
    if (!taken.has(seed.key)) add(resolvedRow(seed))
  }
  const reviewed = new Map(proposal.reviewedNoChange.map((row) => [row.key, row]))
  for (const { key, status, note } of ADDED_SEEDS) {
    const macos = key in english ? english[key] : reviewed.get(key)?.currentEnglish
    if (macos === undefined) missing.push(key)
    else add(plainRow(key, status, macos, note))
  }
  for (const { key, reason, currentEnglish } of proposal.reviewedNoChange) {
    add(plainRow(key, 'none', currentEnglish, `proposal, reviewed, no change: ${reason}`))
  }
  return { file: { readme: README, gated: [...GATED_KEYS], rows }, missing }
}

/**
 * Appends the seeded rows `existing` lacks and takes the seed's gate list,
 * leaving every row `existing` holds exactly as it is.
 * @returns the grown file and the keys it gained
 */
export function addMissingRows(existing: CopyFinal, seeded: CopyFinal): { file: CopyFinal; added: string[] } {
  const held = new Set(existing.rows.map((row) => row.key))
  const fresh = seeded.rows.filter((row) => !held.has(row.key))
  return {
    file: { readme: seeded.readme, gated: seeded.gated, rows: [...existing.rows, ...fresh] },
    added: fresh.map((row) => row.key),
  }
}

function write(file: CopyFinal): void {
  const issues = copyFinalIssues(file)
  if (issues.length > 0) throw new Error(`The seed isn't valid:\n  ${issues.join('\n  ')}`)
  writeFileSync(COPY_FINAL_PATH, `${JSON.stringify(file, null, 2)}\n`, 'utf8')
}

function main(): void {
  const addMissing = process.argv.includes('--add-missing')
  const exists = existsSync(COPY_FINAL_PATH)
  if (exists !== addMissing) {
    console.error(
      exists
        ? 'copy-final.json exists, and milestones resolve rows in it by hand. Pass --add-missing to append rows it lacks.'
        : "copy-final.json doesn't exist yet. Run without --add-missing for the first write.",
    )
    process.exit(1)
  }
  const proposal = JSON.parse(readFileSync(PROPOSAL_PATH, 'utf8')) as Proposal
  const { file: seeded, missing } = seedCopyFinal(proposal, loadCatalog(BASE_LOCALE).messages)

  if (!addMissing) {
    if (missing.length > 0) throw new Error(`English lacks these added keys: ${missing.join(', ')}`)
    write(seeded)
    const count = (status: CopyStatus): number => seeded.rows.filter((row) => row.status === status).length
    console.log(
      `Wrote ${String(seeded.rows.length)} rows: ${String(count('apply'))} apply, ${String(count('pending'))} pending, ${String(count('gated'))} gated, ${String(count('none'))} none.`,
    )
    return
  }
  const existing = JSON.parse(readFileSync(COPY_FINAL_PATH, 'utf8')) as CopyFinal
  const { file, added } = addMissingRows(existing, seeded)
  const unseeded = missing.filter((key) => !file.rows.some((row) => row.key === key))
  if (unseeded.length > 0) throw new Error(`English lacks these added keys: ${unseeded.join(', ')}`)
  write(file)
  console.log(`Added ${String(added.length)} rows; every existing row is unchanged.`)
}

if (import.meta.url === `file://${process.argv[1]}`) main()
