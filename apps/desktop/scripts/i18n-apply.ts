#!/usr/bin/env node
/**
 * `pnpm i18n:apply`: writes a translator's batch into ONE locale's catalogs and
 * stamps each key with its source's current hash, so a translated key leaves the
 * stale list in the same step. Planning and validation live in `i18n-apply-lib.ts`.
 *
 *   --lang <tag>              exactly one locale (parallel translators never touch each other's files)
 *   --values <file.json>      key → value (string, per-OS branches, or null to drop an overlay fork),
 *                             plus optional "@key": { "sameAsSourceJustification": "…" }
 *   --expect-keys <file>      keys the batch should cover, one per line (`#` comments); reports the gaps
 *   --changed-since <ref>     the same, for every English key changed since this branch left <ref>
 *   --dry-run                 validate and report, write nothing
 *   --messages-root <dir>     fixtures
 *
 * Exit 0 when written (or a clean dry run), 1 when any value is refused (nothing
 * is written then), 2 on a usage error. Process: `docs/i18n/translator-instructions.md`.
 */

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import { BASE_LOCALE, loadCatalog } from './i18n-catalog-lib.ts'
import { changedKeys } from './i18n-brief-select.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { BASELINE_RELATIVE_PATH, desktopPath } from './i18n-macos-copy-baseline-lib.ts'
import type { MacosCopyBaseline } from './i18n-macos-copy-baseline-lib.ts'
import { englishAt, readListFile } from './i18n-repo-lib.ts'
import { parseValuesFile, planApply, uncoveredKeys, writePlan } from './i18n-apply-lib.ts'
import type { ApplyPlan } from './i18n-apply-lib.ts'

/** The parsed flags. */
export interface ApplyArgs {
  lang: string
  values: string
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
  if (!values.values) throw new Error('Pass --values <file.json>.')
  return {
    lang,
    values: values.values,
    expectKeys: values['expect-keys'],
    changedSince: values['changed-since'],
    dryRun: values['dry-run'],
    messagesRoot: values['messages-root'],
  }
}

/** The keys the batch is expected to cover, from `--expect-keys` and `--changed-since`, deduplicated. */
function expectedKeys(args: ApplyArgs): string[] | undefined {
  if (!args.expectKeys && !args.changedSince) return undefined
  const listed = args.expectKeys ? readListFile(args.expectKeys) : []
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

function report(plan: ApplyPlan, args: ApplyArgs, expected: string[] | undefined): void {
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
  if (expected !== undefined) {
    const gaps = uncoveredKeys(expected, parseValuesFile(JSON.parse(readFileSync(args.values, 'utf8'))))
    if (gaps.length === 0) console.log(`Covers all ${String(expected.length)} expected key(s).`)
    else {
      console.log(`Not covered by this batch (${String(gaps.length)} of ${String(expected.length)}):`)
      for (const key of gaps) console.log(`  - ${key}`)
    }
  }
  if (plan.refused.length === 0 && !args.dryRun && plan.writes.length > 0) {
    console.log(`Next: pnpm i18n:check-locale ${plan.tag}`)
  }
}

function main(): number {
  const args = parseApplyArgs(process.argv.slice(2))
  const values = parseValuesFile(JSON.parse(readFileSync(args.values, 'utf8')))
  const plan = planApply({
    tag: args.lang,
    values,
    messagesRoot: args.messagesRoot,
    baseline: loadBaseline(args.messagesRoot),
  })
  if (plan.refused.length === 0 && !args.dryRun) writePlan(plan, args.messagesRoot)
  report(plan, args, expectedKeys(args))
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
