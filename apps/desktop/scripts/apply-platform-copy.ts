#!/usr/bin/env node
/**
 * Applies resolved platform copy (`docs/notes/windows-port/copy-final.json`) to
 * every catalog: `en`, the 13 full locales, and the three overlays, plus the
 * files outside the catalogs that name a collapsed pair's keys. The rewrite rules
 * are `apply-platform-copy-lib.ts`'s; this file selects rows, lints them, proves
 * the result, writes it, and says what's left for a person.
 *
 * It writes nothing unless the planned result passes, on a scratch copy of the
 * catalogs: the frozen macOS copy comparison (`macosCopyDrift`, the one
 * `macos-copy-unchanged.test.ts` runs), the platform-select, parity, ICU, plural,
 * and coverage checks, and "`sync-locale-keys.ts` has nothing left to do".
 *
 * Run from `apps/desktop/`:
 *
 *   node scripts/apply-platform-copy.ts <selection> [--dry-run]
 *   node scripts/apply-platform-copy.ts --scan [<catalog> …]
 *   node scripts/apply-platform-copy.ts --restamp-english-overlays <selection>
 *
 * `<selection>` is any mix of `--catalog <name>` (like `errors`), `--key <key>`,
 * `--pairs` (the rows that fold a `.mac`/`.other` pair), and `--all`. Only
 * `apply` rows are written.
 *
 *  - `--dry-run` prints the report and writes nothing.
 *  - `--scan` lists every English value using macOS vocabulary that no row and no
 *    gate covers, and every lint hit on the resolved rows.
 *  - `--restamp-english-overlays` restamps the selected keys in `en-GB` and
 *    `en-AU` ONLY, after their new branches have been spelled. ❌ Never a bare
 *    `sync-locale-keys.ts --restamp <key>`: it restamps every full locale too and
 *    wipes the translators' worklist.
 *
 * After a real run it formats the non-catalog files it rewrote and regenerates
 * `keys.gen.ts` and the native strings. It runs all three even when one exits
 * non-zero: `pnpm intl:keys` does after a pair collapse, until the call sites
 * stop naming the old keys.
 */

import { spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative } from 'node:path'
import { parseArgs } from 'node:util'
import { planPlatformCopy } from './apply-platform-copy-lib.ts'
import type { CopyPlan, PlanPaths } from './apply-platform-copy-lib.ts'
import {
  BASE_LOCALE,
  TYPE,
  astOrUndefined,
  isRawKey,
  listLocales,
  loadCatalog,
  resolveLocaleSource,
  resolveMessagesRoot,
} from './i18n-catalog-lib.ts'
import type { AstElement } from './i18n-catalog-lib.ts'
import { runCoverageCheck, EXIT_WARN } from './i18n-check-coverage.ts'
import { runIcuCheck } from './i18n-check-icu.ts'
import { runParityCheck } from './i18n-check-parity.ts'
import { runPlatformSelectCheck } from './i18n-check-platform-select.ts'
import { runPluralCheck } from './i18n-check-plural.ts'
import { EXIT_CLEAN } from './i18n-locale-check-lib.ts'
import type { LocaleCheckOptions } from './i18n-locale-check-lib.ts'
import { BASELINE_RELATIVE_PATH, desktopPath, macosCopyDrift } from './i18n-macos-copy-baseline-lib.ts'
import type { MacosCopyBaseline } from './i18n-macos-copy-baseline-lib.ts'
import { blockingLintHits, scanEnglish } from './platform-copy-lint.ts'
import type { LintHit } from './platform-copy-lint.ts'
import { isApplyRow, readCopyFinal, rowKeys } from './platform-copy-rows.ts'
import type { ApplyRow, CopyFinal, CopyRow } from './platform-copy-rows.ts'
import { restampOverlay, syncLocale } from './sync-locale-keys.ts'

/** Which rows a run names. */
export interface Selection {
  catalogs: readonly string[]
  keys: readonly string[]
  pairs: boolean
  all: boolean
}

/** The rows of the resolved copy a selection names, in file order. */
export function selectRows(file: CopyFinal, selection: Selection): CopyRow[] {
  const files = new Set(selection.catalogs.map((name) => (name.endsWith('.json') ? name : `${name}.json`)))
  const keys = new Set(selection.keys)
  return file.rows.filter(
    (row) =>
      selection.all ||
      files.has(row.file) ||
      rowKeys(row).some((key) => keys.has(key)) ||
      (selection.pairs && row.collapseFrom !== undefined),
  )
}

/** The English a row's keys hold today: what its branches are "new" against. */
function oldEnglishOf(row: CopyRow, english: Record<string, string>): string[] {
  return rowKeys(row).flatMap((key) => (key in english ? [english[key]] : []))
}

/** The lint hits that block each row, across `rows`. */
export function lintRows(rows: readonly ApplyRow[], english: Record<string, string>): LintHit[] {
  return rows.flatMap((row) => blockingLintHits(row, oldEnglishOf(row, english)))
}

const CHECKS: readonly { name: string; run: (opts: LocaleCheckOptions) => number; passing: readonly number[] }[] = [
  { name: 'platform-select', run: runPlatformSelectCheck, passing: [EXIT_CLEAN] },
  { name: 'parity', run: runParityCheck, passing: [EXIT_CLEAN] },
  { name: 'icu', run: runIcuCheck, passing: [EXIT_CLEAN] },
  { name: 'plural', run: runPluralCheck, passing: [EXIT_CLEAN] },
  { name: 'coverage', run: runCoverageCheck, passing: [EXIT_CLEAN, EXIT_WARN] },
]

function catalogBytes(root: string, locale: string): string {
  const dir = join(root, locale)
  return readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => `${name}\n${readFileSync(join(dir, name), 'utf8')}`)
    .join('\n')
}

/**
 * Proves a plan on a scratch copy of the catalogs: macOS text unmoved, the shape
 * and structure checks green, and the key sync left with nothing to do.
 * @param baseline the frozen macOS copy to compare against; `null` skips that comparison (a fixture root)
 * @returns one line per problem, or `[]`
 */
export function verifyPlan(plan: CopyPlan, messagesRoot: string, baseline: MacosCopyBaseline | null): string[] {
  const scratch = mkdtempSync(join(tmpdir(), 'cmdr-platform-copy-'))
  try {
    cpSync(messagesRoot, scratch, { recursive: true })
    for (const [path, text] of plan.catalogFiles) writeFileSync(join(scratch, relative(messagesRoot, path)), text)

    const problems: string[] =
      baseline === null ? [] : macosCopyDrift(baseline, scratch).map((line) => `macOS text moved: ${line}`)
    for (const { name, run, passing } of CHECKS) {
      const lines: string[] = []
      const code = run({ messagesRoot: scratch, write: (line) => lines.push(line) })
      if (!passing.includes(code)) problems.push(`the ${name} check fails on the result:\n${lines.join('\n')}`)
    }
    const locales = listLocales(scratch)
    for (const locale of locales) {
      if (locale === BASE_LOCALE || resolveLocaleSource(locale, locales).isOverlay) continue
      const before = catalogBytes(scratch, locale)
      syncLocale(locale, { messagesRoot: scratch })
      if (catalogBytes(scratch, locale) !== before) {
        problems.push(`sync-locale-keys.ts would still change ${locale}: its keys or their order don't match en`)
      }
    }
    return problems
  } finally {
    rmSync(scratch, { recursive: true, force: true })
  }
}

/** The shortest literal run worth searching a file for. */
const MIN_RUN_LENGTH = 12

/**
 * The literal runs of a catalog text a test might assert: the text between
 * placeholders, tags, and line breaks, 12 characters or longer.
 */
export function literalRuns(key: string, text: string): string[] {
  const ast = isRawKey(key) ? undefined : astOrUndefined(text, BASE_LOCALE)
  const pieces: string[] = []
  if (ast) {
    const collect = (elements: readonly AstElement[]): void => {
      for (const el of elements) {
        if (el.type === TYPE.literal) pieces.push(el.value)
        else if (el.type === TYPE.select || el.type === TYPE.plural) {
          for (const branch of Object.values(el.options ?? {})) collect(branch.value)
        } else if (el.type === TYPE.tag) collect(el.children ?? [])
      }
    }
    collect(ast)
  } else {
    pieces.push(...text.split(/\{[^{}]*\}/))
  }
  return [...new Set(pieces.flatMap((piece) => piece.split('\n')).map((piece) => piece.trim()))].filter(
    (piece) => piece.length >= MIN_RUN_LENGTH,
  )
}

/** Files the search never reads: the catalogs and what's generated from them, which hold every string by design. */
const SEARCH_SKIPS: readonly string[] = [
  'node_modules',
  join('src', 'lib', 'intl', 'messages'),
  join('src', 'lib', 'intl', 'keys.gen.ts'),
  join('src', 'lib', 'intl', '__fixtures__', 'macos-copy-baseline.json'),
  join('test', 'fixtures', 'i18n-pseudolocale'),
  join('src', 'lib', 'ipc', 'bindings.ts'),
  // The migration's own tooling names the keys and texts by design.
  join('scripts', 'apply-platform-copy.test.ts'),
  join('scripts', 'platform-copy-rows.test.ts'),
  join('scripts', 'i18n-macos-copy-baseline-lib.ts'),
]

const SEARCHED_EXTENSIONS = /\.(ts|js|svelte|json)$/

function searchableFiles(dir: string, base: string, out: string[]): void {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    const rel = relative(base, path)
    if (SEARCH_SKIPS.some((skip) => rel === skip || rel.startsWith(`${skip}/`) || name === skip)) continue
    if (statSync(path).isDirectory()) searchableFiles(path, base, out)
    else if (SEARCHED_EXTENSIONS.test(name)) out.push(path)
  }
}

/**
 * The files that assert a migrated key's copy or name the key: every file under
 * `roots` holding a literal run of the key's old English or of a new branch, or
 * the key itself (a pair's old keys included). Each catalog milestone runs the
 * tests among them by name, since a JSON edit has no import edge for
 * `svelte-tests-related` to follow.
 * @param texts key → its old English and new branch texts (`CopyPlan.texts`)
 * @param keyNames every key name to search for
 * @param base the dir `roots` and the result are relative to
 */
export function filesAssertingCopy(
  texts: ReadonlyMap<string, readonly string[]>,
  keyNames: readonly string[],
  base: string,
  roots: readonly string[] = ['src', 'test', 'scripts'],
): string[] {
  const runs = [...texts].flatMap(([key, values]) => values.flatMap((value) => literalRuns(key, value)))
  // One pass per file: index every run by its first 12 characters, then confirm the rest.
  const byPrefix = new Map<string, string[]>()
  for (const run of new Set(runs)) {
    const prefix = run.slice(0, MIN_RUN_LENGTH)
    byPrefix.set(prefix, [...(byPrefix.get(prefix) ?? []), run])
  }
  const files: string[] = []
  for (const root of roots) {
    if (existsSync(join(base, root))) searchableFiles(join(base, root), base, files)
  }
  const hasRun = (content: string): boolean => {
    for (let i = 0; i + MIN_RUN_LENGTH <= content.length; i++) {
      const candidates = byPrefix.get(content.slice(i, i + MIN_RUN_LENGTH))
      if (candidates?.some((run) => content.startsWith(run, i))) return true
    }
    return false
  }
  return files
    .filter((path) => {
      const content = readFileSync(path, 'utf8')
      return keyNames.some((key) => content.includes(key)) || (byPrefix.size > 0 && hasRun(content))
    })
    .map((path) => relative(base, path))
    .sort()
}

/** The English overlays: written and restamped here, since an English fork of English isn't a translation. */
const ENGLISH_OVERLAYS: readonly string[] = ['en-GB', 'en-AU']

/**
 * Restamps `keys` in the English overlays only, against the `en` value each fork
 * overrides. Full locales and `es-419` are never touched: their stale hashes are
 * the translators' worklist.
 */
export function restampEnglishOverlays(keys: readonly string[], messagesRoot?: string): Record<string, string[]> {
  const available = listLocales(messagesRoot)
  const restamped: Record<string, string[]> = {}
  for (const tag of ENGLISH_OVERLAYS) {
    if (!available.includes(tag)) continue
    restamped[tag] = restampOverlay(tag, { messagesRoot, restampKeys: keys }).restampedKeys
  }
  return restamped
}

/** Writes a plan's files. */
export function writePlan(plan: CopyPlan): void {
  for (const [path, text] of [...plan.catalogFiles, ...plan.otherFiles]) writeFileSync(path, text, 'utf8')
}

const DESKTOP_DIR = desktopPath('.')
const REPO_ROOT = join(DESKTOP_DIR, '..', '..')

function printList(title: string, items: readonly string[]): void {
  if (items.length === 0) return
  console.log(`\n${title} (${String(items.length)}):`)
  for (const item of items) console.log(`  ${item}`)
}

function printReport(plan: CopyPlan): void {
  console.log(
    `\n${String(plan.applied.length)} row(s) to apply, ${String(plan.alreadyApplied.length)} already applied, ${String(plan.refused.length)} refused; ${String(plan.catalogFiles.size)} catalog file(s) and ${String(plan.otherFiles.size)} other file(s) change.`,
  )
  printList(
    'Refused',
    plan.refused.map(({ key, reason }) => `${key}: ${reason}`),
  )
  console.log('\nPer locale (branches holding the locale’s own text / holding English, pending translation):')
  for (const report of plan.locales) {
    const extras = [
      report.preStale.length > 0 ? `${String(report.preStale.length)} already stale` : '',
      report.missingOther.length > 0 ? `${String(report.missingOther.length)} without a .other key` : '',
      report.droppedForks.length > 0 ? `${String(report.droppedForks.length)} fork(s) dropped` : '',
    ].filter((part) => part !== '')
    console.log(
      `  ${report.locale.padEnd(8)} ${String(report.carried).padStart(5)} carried  ${String(report.english).padStart(5)} English${extras.length > 0 ? `  (${extras.join(', ')})` : ''}`,
    )
  }
  for (const report of plan.locales) {
    printList(
      `${report.locale}: translation was already stale, so it went into the macOS-read branch only`,
      report.preStale,
    )
    printList(`${report.locale}: no .other key, so that branch is English`, report.missingOther)
    printList(`${report.locale}: fork dropped, it now equals what it overrides`, report.droppedForks)
    printList(
      `${report.locale}: spell these new branches per docs/i18n/${report.locale}/glossary.md, then --restamp-english-overlays`,
      report.spellingPass,
    )
  }
  printList(
    'Renamed outside the catalogs',
    [...plan.renames].map(([oldKey, survivors]) => `${oldKey} → ${survivors.join(', ')}`),
  )
  printList(
    'Guides still naming an old key in shorthand (hand pass)',
    plan.leftoverMentions.map((line) => relative(REPO_ROOT, line)),
  )
}

/** One command the script runs after writing. */
export interface AfterWriteStep {
  label: string
  command: string
  args: readonly string[]
  cwd: string
}

function spawnStep(step: AfterWriteStep): number {
  return spawnSync(step.command, step.args, { cwd: step.cwd, stdio: 'inherit' }).status ?? 1
}

/**
 * Runs every step and returns the labels of those that exited non-zero. Never stops at the
 * first: after a pair collapse the key generator exits 1 until the call sites stop naming the
 * old keys (it has written `keys.gen.ts` by then), and the native strings still have to be
 * regenerated, or the Rust table keeps the keys the catalogs no longer have.
 */
export function regenerate(
  steps: readonly AfterWriteStep[],
  runStep: (step: AfterWriteStep) => number = spawnStep,
): string[] {
  return steps.filter((step) => runStep(step) !== 0).map((step) => step.label)
}

function scan(file: CopyFinal, catalogs: readonly string[]): number {
  const english = loadCatalog(BASE_LOCALE).messages
  const prefixes = catalogs.map((name) => `${name.replace(/\.json$/, '')}.`)
  const inScope = (key: string): boolean => prefixes.length === 0 || prefixes.some((prefix) => key.startsWith(prefix))
  const scoped = Object.fromEntries(Object.entries(english).filter(([key]) => inScope(key)))
  const unjudged = scanEnglish(scoped, file)
  printList(
    'English values using macOS vocabulary that no row and no gate covers',
    unjudged.map(({ key, terms }) => `${key}: ${terms.join(', ')}`),
  )
  const hits = lintRows(
    file.rows.filter(isApplyRow).filter((row) => inScope(row.key)),
    english,
  )
  printList(
    'Lint hits on resolved rows (fix the branch, or add a lintExceptions entry with the reason)',
    hits.map(({ key, branch, rule }) => `${key} [${branch}]: ${rule}`),
  )
  if (unjudged.length === 0 && hits.length === 0) console.log('Scan and lint: clean.')
  return unjudged.length > 0 || hits.length > 0 ? 1 : 0
}

function main(): number {
  const { values } = parseArgs({
    options: {
      catalog: { type: 'string', multiple: true },
      key: { type: 'string', multiple: true },
      pairs: { type: 'boolean' },
      all: { type: 'boolean' },
      'dry-run': { type: 'boolean' },
      scan: { type: 'boolean' },
      'restamp-english-overlays': { type: 'boolean' },
    },
    allowPositionals: true,
  })
  const file = readCopyFinal()
  const selection: Selection = {
    catalogs: values.catalog ?? [],
    keys: values.key ?? [],
    pairs: values.pairs === true,
    all: values.all === true,
  }
  if (values.scan) return scan(file, selection.catalogs)

  const selected = selectRows(file, selection)
  if (selected.length === 0) {
    console.error('No rows selected. Name --catalog <name>, --key <key>, --pairs, or --all.')
    return 2
  }
  if (values['restamp-english-overlays']) {
    const restamped = restampEnglishOverlays(selected.filter(isApplyRow).map((row) => row.key))
    for (const [tag, keys] of Object.entries(restamped)) printList(`Restamped in ${tag}`, keys)
    return 0
  }

  return applyRows(selected, values['dry-run'] === true)
}

/** Lints, plans, proves, and (outside a dry run) writes the selected rows. */
function applyRows(selected: readonly CopyRow[], dryRun: boolean): number {
  const rows = selected.filter(isApplyRow)

  const messagesRoot = resolveMessagesRoot()
  const english = loadCatalog(BASE_LOCALE).messages
  const hits = lintRows(rows, english)
  printList(
    'Lint hits: fix the branch, or add a lintExceptions entry',
    hits.map(({ key, branch, rule }) => `${key} [${branch}]: ${rule}`),
  )
  if (hits.length > 0) return 1

  const paths: PlanPaths = {}
  const plan = planPlatformCopy(rows, paths)
  printReport(plan)
  const baseline = JSON.parse(readFileSync(desktopPath(BASELINE_RELATIVE_PATH), 'utf8')) as MacosCopyBaseline
  const problems = verifyPlan(plan, messagesRoot, baseline)
  printList('The result does not pass', problems)
  if (problems.length === 0) {
    console.log(
      '\nVerified on a scratch copy: macOS text unmoved in every locale; platform-select, parity, ICU, plural, and coverage pass; sync-locale-keys.ts has nothing to do.',
    )
  }

  const oldKeys = [...plan.renames.keys()]
  printList(
    'Files asserting this copy or naming these keys (run the tests among them by name)',
    filesAssertingCopy(plan.texts, [...plan.applied, ...oldKeys], DESKTOP_DIR),
  )
  if (plan.refused.length > 0) return 1
  if (problems.length > 0) return 1
  if (dryRun) {
    console.log('\nDry run: nothing written.')
    return 0
  }

  writePlan(plan)
  const steps: AfterWriteStep[] = [
    { label: 'pnpm intl:keys', command: 'pnpm', args: ['intl:keys'], cwd: DESKTOP_DIR },
    { label: 'pnpm intl:native-strings', command: 'pnpm', args: ['intl:native-strings'], cwd: DESKTOP_DIR },
  ]
  if (plan.otherFiles.size > 0) {
    steps.unshift({
      label: 'oxfmt on the rewritten guides',
      command: 'pnpm',
      args: ['exec', 'oxfmt', ...plan.otherFiles.keys()],
      cwd: REPO_ROOT,
    })
  }
  const stepsFailed = regenerate(steps)
  console.log(
    '\nWritten. Next: fix the call sites and tests listed above, spell the English overlays’ new branches and run --restamp-english-overlays, then the i18n check lane.',
  )
  printList(
    'Exited non-zero (`pnpm intl:keys` does until no call site names a collapsed key; rerun it after fixing them)',
    stepsFailed,
  )
  return stepsFailed.length > 0 ? 1 : 0
}

if (import.meta.url === `file://${process.argv[1]}`) process.exit(main())
