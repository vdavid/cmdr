#!/usr/bin/env node
/**
 * `pnpm i18n:apply`: writes a translator's batch into ONE locale's catalogs and
 * stamps each key with its source's current hash, so a translated key leaves the
 * stale list in the same step. Planning and validation live in `i18n-apply-lib.ts`.
 *
 *   --lang <tag>              exactly one locale (parallel translators never touch each other's files)
 *   --values <file.json>      key → value (string, per-OS branches, or null to drop an overlay fork); default:
 *                             `values.json` in the language's newest run directory (`pnpm i18n:brief` makes it),
 *                             plus optional "@key": { "sameAsSourceJustification": "…" }
 *   --expect-keys <file>      keys the batch should cover, one per line (`#` comments); reports the gaps.
 *                             Default: the `keys.txt` beside the values file, when there is one
 *   --changed-since <ref>     the same, for every English key changed since this branch left <ref>
 *   --dry-run                 validate and report, write nothing
 *   --messages-root <dir>     fixtures
 *
 * Exit 0 when written (or a clean dry run), 1 when any value is refused (nothing
 * is written then), 2 on a usage error. Process: `docs/i18n/translator-instructions.md`.
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { parseArgs } from 'node:util'
import { BASE_LOCALE, loadCatalog } from './i18n-catalog-lib.ts'
import { changedKeys } from './i18n-brief-select.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { BASELINE_RELATIVE_PATH, desktopPath } from './i18n-macos-copy-baseline-lib.ts'
import type { MacosCopyBaseline } from './i18n-macos-copy-baseline-lib.ts'
import { englishAt, latestWorkDir, readListFile } from './i18n-repo-lib.ts'
import { parseValuesFile, planApply, uncoveredKeys, writePlan } from './i18n-apply-lib.ts'
import type { ApplyPlan } from './i18n-apply-lib.ts'

/** The parsed flags. */
export interface ApplyArgs {
  lang: string
  /** the values file; `undefined` means the newest run directory's `values.json` */
  values?: string
  expectKeys?: string
  changedSince?: string
  dryRun: boolean
  messagesRoot?: string
}

/**
 * Parses the flags strictly: one locale, a values file, nothing stray.
 * @param argv `process.argv.slice(2)`
 */
export function parseApplyArgs(argv: string[]): ApplyArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      lang: { type: 'string' },
      values: { type: 'string' },
      'expect-keys': { type: 'string' },
      'changed-since': { type: 'string' },
      'dry-run': { type: 'boolean', default: false },
      'messages-root': { type: 'string' },
    },
    allowPositionals: true,
  })
  if (positionals.length > 0) throw new Error(`Unexpected argument(s): ${positionals.join(' ')}.`)
  const lang = values.lang?.trim()
  if (!lang) throw new Error('Pass --lang <tag>.')
  if (lang === 'all' || lang.includes(',')) {
    throw new Error(
      'i18n:apply writes one locale per call, so parallel translators never share a file: pass one --lang.',
    )
  }
  return {
    lang,
    values: values.values,
    expectKeys: values['expect-keys'],
    changedSince: values['changed-since'],
    dryRun: values['dry-run'],
    messagesRoot: values['messages-root'],
  }
}

/**
 * The keys the batch is expected to cover, from `--expect-keys` and `--changed-since`, deduplicated; with neither,
 * the `keys.txt` the brief left beside the values file.
 */
function expectedKeys(args: ApplyArgs, valuesPath: string): string[] | undefined {
  const beside = join(dirname(valuesPath), 'keys.txt')
  const expectKeys = args.expectKeys ?? (!args.changedSince && existsSync(beside) ? beside : undefined)
  if (!expectKeys && !args.changedSince) return undefined
  const listed = expectKeys ? readListFile(expectKeys) : []
  const changed = args.changedSince
    ? changedKeys(loadCatalog(BASE_LOCALE, args.messagesRoot).messages, englishAt(args.changedSince, args.messagesRoot))
    : []
  return [...new Set([...listed, ...changed])]
}

/** The frozen macOS copy, read only against the shipped catalogs (a fixture root has none). */
function loadBaseline(messagesRoot: string | undefined): MacosCopyBaseline | undefined {
  if (messagesRoot !== undefined) return undefined
  return JSON.parse(readFileSync(desktopPath(BASELINE_RELATIVE_PATH), 'utf8')) as MacosCopyBaseline
}

function report(
  plan: ApplyPlan,
  args: ApplyArgs,
  { valuesPath, expected }: { valuesPath: string; expected?: string[] },
): void {
  const where = join('messages', plan.tag)
  if (plan.refused.length > 0) {
    console.log(`Refused ${String(plan.refused.length)} value(s), so nothing was written to ${where}/:`)
    for (const { key, reason } of plan.refused) console.log(`  ✗ ${key}: ${reason}`)
  } else {
    const verb = args.dryRun ? 'Would write' : 'Wrote'
    const restamped = plan.writes.filter((write) => write.restamped).length
    const files = [...plan.files.keys()].join(', ') || 'no files'
    console.log(
      `${verb} ${String(plan.writes.length)} key(s) to ${where}/ (${files}), ${String(restamped)} restamped; ${String(plan.unchanged.length)} already current.`,
    )
  }
  for (const note of plan.notes) console.log(`  · ${note}`)
  reportAccepted(plan, args.dryRun)
  if (expected !== undefined) reportCoverage(expected, valuesPath)
  if (plan.refused.length === 0 && !args.dryRun && plan.writes.length > 0) {
    const keysFile = join(dirname(valuesPath), 'keys.txt')
    const scope = existsSync(keysFile) ? ` --keys-file ${keysFile}` : ''
    console.log(`Next: pnpm i18n:check-locale ${plan.tag}${scope}`)
  }
}

/** The accepted macOS-text changes a clean plan records (or drops). */
function reportAccepted(plan: ApplyPlan, dryRun: boolean): void {
  if (plan.refused.length > 0 || plan.accepted.size === 0) return
  const kept = [...plan.accepted].map(([key, entry]) =>
    entry === null ? `${key} (dropped: back to the frozen text)` : key,
  )
  console.log(
    `${dryRun ? 'Would record' : 'Recorded'} the macOS text changes in ${plan.acceptedPath ?? ''}: ${kept.join(', ')}`,
  )
}

/** The expected keys the values file gives no value for. */
function reportCoverage(expected: readonly string[], valuesPath: string): void {
  const gaps = uncoveredKeys(expected, parseValuesFile(JSON.parse(readFileSync(valuesPath, 'utf8'))))
  if (gaps.length === 0) {
    console.log(`Covers all ${String(expected.length)} expected key(s).`)
    return
  }
  console.log(`Not covered by this batch (${String(gaps.length)} of ${String(expected.length)}):`)
  for (const key of gaps) console.log(`  - ${key}`)
}

/** `values.json` in the language's newest run directory, the brief's default place for it. */
function defaultValuesPath(lang: string): string {
  const dir = latestWorkDir(lang)
  if (dir === undefined) {
    throw new Error(
      `No run directory for ${lang} yet: run pnpm i18n:brief --lang ${lang} first, or pass --values <file>.`,
    )
  }
  const path = join(dir, 'values.json')
  if (!existsSync(path)) throw new Error(`No values file at ${path}: write your values there, or pass --values <file>.`)
  return path
}

function main(): number {
  const args = parseApplyArgs(process.argv.slice(2))
  const valuesPath = args.values ?? defaultValuesPath(args.lang)
  console.log(`Values: ${valuesPath}`)
  const values = parseValuesFile(JSON.parse(readFileSync(valuesPath, 'utf8')))
  const plan = planApply({
    tag: args.lang,
    values,
    messagesRoot: args.messagesRoot,
    baseline: loadBaseline(args.messagesRoot),
  })
  if (plan.refused.length === 0 && !args.dryRun) writePlan(plan, args.messagesRoot)
  report(plan, args, { valuesPath, expected: expectedKeys(args, valuesPath) })
  return plan.refused.length > 0 ? EXIT_ISSUES : EXIT_CLEAN
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    process.exit(main())
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
