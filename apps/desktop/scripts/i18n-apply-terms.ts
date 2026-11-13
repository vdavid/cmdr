#!/usr/bin/env node
/**
 * `pnpm i18n:apply-terms`: patches one termbase file, schema-checked, formatting
 * kept. The patch shape and the rules are in `i18n-apply-terms-lib.ts`.
 *
 *   --lang <tag> --patch <file.json>              the locale's rulings (`docs/i18n/<tag>/terms.json`)
 *   --concepts --patch <file.json>                the shared registry (`docs/i18n/concepts.json`)
 *   --lang <tag> --proposed --patch <file.json>   a parallel fan-out's new concepts (`<tag>/concepts-proposed.json`)
 *   --drop-stale-exceptions                       with --concepts: also drop every locale exception the edit made stale
 *   --dry-run                                     report, write nothing
 *   --messages-root <dir>, --docs-root <dir>      fixtures
 *
 * Exit 0 when written (or a clean dry run), 1 when the patch adds a schema
 * problem (nothing is written then), 2 on a usage error or a patch that can't apply.
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { formatJson } from './i18n-repo-lib.ts'
import { dropExceptionPatches, planTermsPatch, staleExceptionsUnder } from './i18n-apply-terms-lib.ts'
import type { TermbasePatch, TermbaseTarget, TermsPlan } from './i18n-apply-terms-lib.ts'
import type { Concepts } from './i18n-termbase-lib.ts'

/** The parsed flags. */
export interface ApplyTermsArgs {
  target: TermbaseTarget
  patch: string
  dropStaleExceptions: boolean
  dryRun: boolean
  messagesRoot?: string
  docsRoot?: string
}

/**
 * Parses the flags: exactly one target (`--lang`, `--concepts`, or `--lang --proposed`) and a patch file.
 * @param argv `process.argv.slice(2)`
 */
export function parseApplyTermsArgs(argv: string[]): ApplyTermsArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      lang: { type: 'string' },
      concepts: { type: 'boolean', default: false },
      proposed: { type: 'boolean', default: false },
      patch: { type: 'string' },
      'drop-stale-exceptions': { type: 'boolean', default: false },
      'dry-run': { type: 'boolean', default: false },
      'messages-root': { type: 'string' },
      'docs-root': { type: 'string' },
    },
    allowPositionals: true,
  })
  if (positionals.length > 0) throw new Error(`Unexpected argument(s): ${positionals.join(' ')}.`)
  if (!values.patch) throw new Error('Pass --patch <file.json>.')
  const lang = values.lang?.trim()
  if (values.concepts === (lang !== undefined)) throw new Error('Pass exactly one of --lang <tag> or --concepts.')
  if (lang?.includes(',') || lang === 'all') throw new Error('One locale per call: pass one --lang.')
  if (values.proposed && !lang) throw new Error('--proposed needs --lang <tag>.')
  if (values['drop-stale-exceptions'] && !values.concepts) {
    throw new Error('--drop-stale-exceptions goes with --concepts.')
  }
  let target: TermbaseTarget = { kind: 'concepts' }
  if (lang) target = values.proposed ? { kind: 'proposed', tag: lang } : { kind: 'terms', tag: lang }
  return {
    target,
    patch: values.patch,
    dropStaleExceptions: values['drop-stale-exceptions'],
    dryRun: values['dry-run'],
    messagesRoot: values['messages-root'],
    docsRoot: values['docs-root'],
  }
}

/** Prints one plan's outcome; true when it's clean. */
function reportPlan(plan: TermsPlan, verb: string): boolean {
  if (plan.added.length > 0) {
    console.log(`Refused: the patch would add ${String(plan.added.length)} schema problem(s) to ${plan.path}:`)
    for (const line of plan.added) console.log(`  ✗ ${line}`)
    return false
  }
  console.log(`${verb} ${plan.path}.`)
  if (plan.preexisting.length > 0) {
    console.log(`  (${String(plan.preexisting.length)} problem(s) were already there, not from this patch:)`)
    for (const line of plan.preexisting) console.log(`  · ${line}`)
  }
  return true
}

function write(plans: readonly TermsPlan[]): void {
  for (const plan of plans) writeFileSync(plan.path, JSON.stringify(plan.content, null, 2) + '\n', 'utf8')
  if (!formatJson(plans.map((plan) => plan.path))) {
    console.warn('  warning: oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the files above.')
  }
}

function main(): number {
  const args = parseApplyTermsArgs(process.argv.slice(2))
  const opts = { messagesRoot: args.messagesRoot, docsRoot: args.docsRoot }
  const patch = JSON.parse(readFileSync(args.patch, 'utf8')) as TermbasePatch
  const verb = args.dryRun ? 'Would write' : 'Wrote'
  const plan = planTermsPatch(args.target, patch, opts)
  if (!reportPlan(plan, verb)) return EXIT_ISSUES

  const plans = [plan]
  if (args.target.kind === 'concepts') {
    const stale = staleExceptionsUnder(plan.content as Concepts, opts)
    if (stale.length > 0 && !args.dropStaleExceptions) {
      console.log(
        `This edit makes ${String(stale.length)} locale exception(s) stale (the termbase check fails on them):`,
      )
      for (const { tag, concept, key } of stale) console.log(`  - ${tag} ${concept}: ${key}`)
      console.log('Rerun with --drop-stale-exceptions to drop them in the same step.')
      return EXIT_ISSUES
    }
    for (const [tag, dropPatch] of dropExceptionPatches(stale)) {
      const dropPlan = planTermsPatch({ kind: 'terms', tag }, dropPatch, {
        ...opts,
        concepts: plan.content as Concepts,
      })
      if (!reportPlan(dropPlan, `${verb} (dropping stale exceptions)`)) return EXIT_ISSUES
      plans.push(dropPlan)
    }
  }
  if (!args.dryRun) write(plans)
  return EXIT_CLEAN
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    process.exit(main())
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
