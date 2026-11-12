#!/usr/bin/env node
/**
 * PLATFORM-SELECT SHAPE check (i18n maintenance): ERROR class.
 *
 * A value that reads differently per OS is ENTIRELY one block,
 * `{platform, select, macos {…} windows {…} other {…}}`, in both families. ICU
 * values get `platform` injected by the runtime's one format call; raw values
 * (`errors.*`, the native strings) are split by `parsePlatformSelect` in
 * `getMessage()`, the native-strings generator, and the checks. Those readers
 * only agree when the block has exactly one shape, so this check holds every
 * catalog, `en` included, to it:
 *
 *  - Canonical spelling: the value starts `{platform, select, `, branches come
 *    `macos`, `windows`, `other`, one space apart (`getMessage`'s prefix test
 *    relies on it).
 *  - A legal branch set: `other` plus at least one of `macos`/`windows`; in a
 *    SOURCE value, no `macos` or `windows` branch equal to `other` (fold it
 *    into `other`). `macos` and `windows` MAY be equal: that keeps "Windows
 *    says what macOS says" explicit while Linux differs. A locale whose source
 *    is a block takes the source's branch set, so its branches may repeat a
 *    text: two OS words often translate alike (German says „Papierkorb“ for the
 *    Trash and the Recycle Bin), and a carried translation can fill two branches.
 *  - `platform` nowhere else: no block inside a sentence or a branch, no
 *    `{platform}` interpolation, so no OS name leaks as text.
 *  - Raw bodies balance their braces (a malformed block reads as not-a-block)
 *    and never put `'` directly before `{`, `}`, `<`, or `>`: ICU starts a
 *    quoted run there and the raw path doesn't, so the native strings, which
 *    the frontend also reads through ICU, would split one way and render another.
 *  - Keys read without a format call or a split stay plain: `MANIFEST_KEYS` and
 *    the Finder menu item's title (macOS-only, baked into `Info.plist`).
 *  - Every locale's branch set equals its source's (`en`, or an overlay's
 *    layered base). The parity check then compares branch by branch.
 *
 * Mechanism: `src/lib/intl/DETAILS.md` § "Platform variants".
 *
 * Run: `pnpm i18n:check-platform-select` (desktop) or
 * `node scripts/i18n-check-platform-select.ts`. Pass `--messages-root <dir>` to
 * point at a fixture (used by the tests).
 */

import { FINDER_MENU_KEY } from './gen-native-strings-lib.ts'
import { isRawKey, parseMessage } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'
import type { LocaleCheckOptions } from './i18n-locale-check-lib.ts'
import { MANIFEST_KEYS } from './vite-locale-catalogs.ts'
import { PLATFORM_SELECT_PREFIX, parsePlatformSelect } from '../src/lib/intl/platform-select.ts'
import type { PlatformSelectBranch, PlatformSelectBranches } from '../src/lib/intl/platform-select.ts'

const BRANCH_ORDER: readonly PlatformSelectBranch[] = ['macos', 'windows', 'other']

/** Keys whose readers take the value as-is, never formatting or splitting it. */
const PLAIN_ONLY_KEYS: ReadonlySet<string> = new Set([...MANIFEST_KEYS, FINDER_MENU_KEY])

/** A value that opens with a `platform` argument, however it's spelled. */
const LEADS_WITH_PLATFORM = /^\{\s*platform\s*,/

/** A `platform` argument anywhere in raw text: `{platform}` or `{platform, …`. */
const RAW_PLATFORM_ARG = /\{\s*platform\s*[,}]/

/** `'` before a character that starts an ICU quoted run (`platform-select.ts` § QUOTE_OPENERS). */
const QUOTE_BEFORE_OPENER = /'[{}<>]/

const APOSTROPHE_ISSUE =
  "apostrophe directly before {, }, <, or > in a raw branch: ICU starts a quoted run there and the raw path doesn't, so the split and the frontend's ICU read disagree; rephrase, or use a typographic ’"

/** The arguments of `platformSelectIssues`. */
interface PlatformSelectInput {
  key: string
  value: string
  /** the source catalog's value for the key; `undefined` when the source lacks it, or for `en` itself */
  sourceValue?: string
}

/** Whether `text` uses `platform` as an argument, read in its family's grammar. */
function mentionsPlatform(key: string, text: string): boolean {
  if (isRawKey(key)) return RAW_PLATFORM_ARG.test(text)
  const parsed = parseMessage(text)
  return parsed.ok ? parsed.placeholders.has('platform') : RAW_PLATFORM_ARG.test(text)
}

function branchNames(branches: PlatformSelectBranches): PlatformSelectBranch[] {
  return BRANCH_ORDER.filter((name) => branches[name] !== undefined)
}

function formatNames(names: readonly string[]): string {
  return `{${names.join(', ')}}`
}

/** The value spelled canonically, from its branches. */
function canonicalSpelling(branches: PlatformSelectBranches): string {
  return `${PLATFORM_SELECT_PREFIX}${branchNames(branches)
    .map((name) => `${name} {${branches[name] ?? ''}}`)
    .join(' ')}}`
}

/**
 * The issues of a value that is a well-formed block.
 * @param setFromSource whether the value's source is a block too, which dictates its branch set
 */
function blockIssues(key: string, value: string, branches: PlatformSelectBranches, setFromSource: boolean): string[] {
  const issues: string[] = []
  const names = branchNames(branches)
  const bodies = names.map((name) => branches[name] ?? '')
  if (canonicalSpelling(branches) !== value) {
    issues.push(
      'not in canonical spelling: write `{platform, select, ` then the branches in the order macos, windows, other, one space apart',
    )
  }
  if (names.length < 2) {
    issues.push('a block needs at least two branches: `other` alone is a plain value')
  }
  for (const name of ['macos', 'windows'] as const) {
    if (!setFromSource && branches[name] === branches.other) {
      issues.push(`${name} branch equals other: drop it, and that OS reads other`)
    }
  }
  if (bodies.some((body) => mentionsPlatform(key, body))) {
    issues.push('uses `platform` outside a whole-value block (here, inside a branch): one top-level block only')
  }
  if (isRawKey(key) && bodies.some((body) => QUOTE_BEFORE_OPENER.test(body))) {
    issues.push(APOSTROPHE_ISSUE)
  }
  if (PLAIN_ONLY_KEYS.has(key)) {
    issues.push('this key must stay a plain value: its reader takes the value as-is, with no format call or split')
  }
  return issues
}

/** Whether the locale's branch set matches its source's. */
function branchSetIssue(branches: PlatformSelectBranches | null, sourceValue: string): string | null {
  const source = parsePlatformSelect(sourceValue)
  if (branches === null) return source === null ? null : 'a plain value, but its source is a block: use the same block'
  if (source === null) return 'a block, but its source is a plain value'
  const names = branchNames(branches)
  const sourceNames = branchNames(source)
  if (names.join() === sourceNames.join()) return null
  return `branches ${formatNames(names)}, but its source has ${formatNames(sourceNames)}: use the same branch set`
}

/**
 * What's wrong with one value's platform-select shape, or `[]`. Exposed for unit
 * tests.
 */
export function platformSelectIssues({ key, value, sourceValue }: PlatformSelectInput): string[] {
  if (value.startsWith(PLATFORM_SELECT_PREFIX) || LEADS_WITH_PLATFORM.test(value)) {
    if (!value.startsWith(PLATFORM_SELECT_PREFIX)) {
      return ['not in canonical spelling: a block starts with exactly `{platform, select, `']
    }
    const branches = parsePlatformSelect(value)
    if (branches === null) {
      const malformed =
        'not a well-formed block: unbalanced braces, a branch name other than macos/windows/other, a repeated branch, or no other branch'
      // A raw `x'}` hides the brace from the splitter, which is the likelier cause then.
      return isRawKey(key) && QUOTE_BEFORE_OPENER.test(value) ? [malformed, APOSTROPHE_ISSUE] : [malformed]
    }
    const setFromSource = sourceValue !== undefined && parsePlatformSelect(sourceValue) !== null
    const issues = blockIssues(key, value, branches, setFromSource)
    const setIssue = sourceValue === undefined ? null : branchSetIssue(branches, sourceValue)
    return setIssue === null ? issues : [...issues, setIssue]
  }
  const issues: string[] = []
  if (mentionsPlatform(key, value)) {
    issues.push('uses `platform` outside a whole-value block: the whole value must be the block')
  }
  const setIssue = sourceValue === undefined ? null : branchSetIssue(null, sourceValue)
  if (setIssue !== null) issues.push(setIssue)
  return issues
}

/** Runs the platform-select shape check over the catalogs under `messagesRoot`. */
export function runPlatformSelectCheck(opts: LocaleCheckOptions = {}): number {
  return runLocaleCheck({
    title: 'Platform-select shape',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    only: opts.only,
    includeBaseLocale: true,
    summaryLine: (count) => `${String(count)} value(s) not in the one platform-select shape:`,
    inspectLocale: ({ locale, source, catalog, findings }) => {
      for (const [key, value] of Object.entries(catalog.messages)) {
        const sourceValue = locale === 'en' ? undefined : source.messages[key]
        for (const issue of platformSelectIssues({ key, value, sourceValue })) findings.add(key, issue)
      }
    },
  })
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  try {
    process.exit(runPlatformSelectCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the platform-select check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
