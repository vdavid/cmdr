#!/usr/bin/env node
/**
 * `pnpm i18n:check-locale <tag> [<tag>…]`: every locale-scoped i18n check for the
 * given locales, in one command, the way a translator needs it after a batch: one
 * line per check when it's clean, the full report only for a check that isn't.
 *
 * In process, each check scoped to the tags: parity, stale, icu, plural, coverage,
 * dont-translate, aria-label, quoted-labels (`runLocaleCheck`'s `only`), and
 * term-consistency, termbase, mechanics (their per-locale outcomes, filtered).
 * Then, as subprocesses: the doc citations (repo-wide, it's one fast scan) and the
 * overlay tests that guard the tags (`i18n-es-overlay.test.ts` for es / es-419,
 * `i18n-en-overlays.test.ts` for en-GB / en-AU).
 *
 * It never ratchets a baseline or an allowlist (a scoped run can't see the other
 * locales, so shrink-wrapping would drop them): the full checks do that.
 *
 * Exit 0 when every check is clean, 1 otherwise (a warn-class finding included:
 * a translator's batch isn't done while its locale carries one).
 */

import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { baseLanguageOf, listLocales } from './i18n-catalog-lib.ts'
import { runAriaCheck } from './i18n-check-aria-label.ts'
import { runCoverageCheck } from './i18n-check-coverage.ts'
import { runDontTranslateCheck } from './i18n-check-dont-translate.ts'
import { runIcuCheck } from './i18n-check-icu.ts'
import * as mechanics from './i18n-check-mechanics.ts'
import { runParityCheck } from './i18n-check-parity.ts'
import { runPluralCheck } from './i18n-check-plural.ts'
import { runQuotedLabelsCheck } from './i18n-check-quoted-labels.ts'
import { runStaleCheck } from './i18n-check-stale.ts'
import * as termConsistency from './i18n-check-term-consistency.ts'
import * as termbase from './i18n-check-termbase.ts'
import { EXIT_CLEAN, EXIT_ERROR } from './i18n-locale-check-lib.ts'

/** One check: its name, and a run that writes its report and returns an exit code (0 = clean). */
interface Step {
  name: string
  run: (write: (line: string) => void) => number
}

/** Keeps a schema error that belongs to one of `tags` (or their base languages), or to no locale at all. */
function forTags(errors: readonly string[], tags: ReadonlySet<string>): string[] {
  return errors.filter((error) => {
    const owner = /^([A-Za-z]{2,3}(?:-[A-Za-z0-9]+)*)\//.exec(error)?.[1]
    return owner === undefined || tags.has(owner)
  })
}

function inProcessSteps(
  tags: readonly string[],
  {
    messagesRoot,
    docsRoot,
    termAllowlistPath,
  }: { messagesRoot?: string; docsRoot?: string; termAllowlistPath?: string },
): Step[] {
  const only = tags
  const wanted = new Set(tags)
  const withBases = new Set([...tags, ...tags.map(baseLanguageOf)])
  return [
    { name: 'parity', run: (write) => runParityCheck({ messagesRoot, write, only }) },
    { name: 'stale', run: (write) => runStaleCheck({ messagesRoot, write, only }) },
    { name: 'icu', run: (write) => runIcuCheck({ messagesRoot, write, only }) },
    { name: 'plural', run: (write) => runPluralCheck({ messagesRoot, write, only }) },
    { name: 'coverage', run: (write) => runCoverageCheck({ messagesRoot, write, only }) },
    { name: 'dont-translate', run: (write) => runDontTranslateCheck({ messagesRoot, write, only }) },
    { name: 'aria-label', run: (write) => runAriaCheck({ messagesRoot, write, only }) },
    { name: 'quoted-labels', run: (write) => runQuotedLabelsCheck({ messagesRoot, write, only }) },
    {
      name: 'term-consistency',
      run: (write) => {
        const outcomes = termConsistency.inspectLocales({
          messagesRoot,
          allowlist: termConsistency.loadAllowlist(termAllowlistPath),
        })
        return termConsistency.report(
          outcomes.filter(({ locale }) => wanted.has(locale)),
          write,
        )
      },
    },
    {
      name: 'termbase',
      run: (write) => {
        const outcome = termbase.inspectTermbase({ messagesRoot, docsRoot, baseline: termbase.loadBaseline() })
        const scoped = {
          schemaErrors: forTags(outcome.schemaErrors, withBases),
          locales: outcome.locales.filter(({ locale }) => wanted.has(locale)),
        }
        return termbase.report(scoped, write)
      },
    },
    {
      name: 'mechanics',
      run: (write) => {
        const outcome = mechanics.inspectMechanics({ messagesRoot, docsRoot, baseline: mechanics.loadBaseline() })
        const scoped = {
          schemaErrors: forTags(outcome.schemaErrors, withBases),
          locales: outcome.locales.filter(({ locale }) => wanted.has(locale)),
        }
        return mechanics.report(scoped, write)
      },
    },
  ]
}

const DESKTOP = join(import.meta.dirname, '..')

/** A subprocess step: its combined output, and its exit code. */
function command(name: string, cwd: string, argv: string[]): Step {
  return {
    name,
    run: (write) => {
      const result = spawnSync(argv[0], argv.slice(1), { cwd, encoding: 'utf8' })
      for (const line of `${result.stdout}${result.stderr}`.trimEnd().split('\n')) write(line)
      return result.status ?? EXIT_ERROR
    },
  }
}

function externalSteps(tags: readonly string[]): Step[] {
  const steps = [command('citations (repo-wide)', join(DESKTOP, '..', '..'), ['./scripts/check.sh', 'i18n-citations'])]
  const overlayTests: [string, string[]][] = [
    ['scripts/i18n-es-overlay.test.ts', ['es', 'es-419']],
    ['scripts/i18n-en-overlays.test.ts', ['en-GB', 'en-AU']],
  ]
  for (const [test, guarded] of overlayTests) {
    if (tags.some((tag) => guarded.includes(tag))) {
      steps.push(command(`overlay test (${test.replace('scripts/', '')})`, DESKTOP, ['npx', 'vitest', 'run', test]))
    }
  }
  return steps
}

/**
 * Runs every locale-scoped check for `tags` and prints the digest.
 *
 * @param opts.external also run the subprocess steps (citations, overlay tests); tests turn it off
 * @throws when a tag has no catalog
 * @returns 0 when every check is clean, else 1
 */
export function runLocaleChecks({
  tags,
  messagesRoot,
  docsRoot,
  termAllowlistPath,
  external = true,
  write = (line: string) => {
    console.log(line)
  },
}: {
  tags: readonly string[]
  messagesRoot?: string
  docsRoot?: string
  /** the term-consistency allowlist (a fixture's own; default the real one) */
  termAllowlistPath?: string
  external?: boolean
  write?: (line: string) => void
}): number {
  const available = listLocales(messagesRoot)
  const unknown = tags.filter((tag) => !available.includes(tag))
  if (unknown.length > 0) throw new Error(`No catalog for ${unknown.join(', ')} (have: ${available.join(', ')})`)
  const steps = [
    ...inProcessSteps(tags, { messagesRoot, docsRoot, termAllowlistPath }),
    ...(external ? externalSteps(tags) : []),
  ]
  let failed = 0
  for (const { name, run } of steps) {
    const lines: string[] = []
    const code = run((line) => void lines.push(line))
    if (code === EXIT_CLEAN) {
      write(`✓ ${name}`)
      continue
    }
    failed++
    write(`✗ ${name}`)
    for (const line of lines) write(`    ${line}`)
  }
  return failed === 0 ? EXIT_CLEAN : 1
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  const tags = process.argv
    .slice(2)
    .flatMap((arg) => arg.split(','))
    .map((tag) => tag.trim())
    .filter(Boolean)
  try {
    if (tags.length === 0) throw new Error('Usage: pnpm i18n:check-locale <tag> [<tag>…]')
    process.exit(runLocaleChecks({ tags }))
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
