/**
 * How the i18n tooling reads a per-OS catalog value, the whole-value
 * `{platform, select, macos {…} windows {…} other {…}}` block, branch by branch:
 * split (`platformBranches`, over the runtime's own `parsePlatformSelect`), write
 * back (`formatPlatformBranches`), pair a locale with its source
 * (`pairPlatformBranches`), and the two forms of the judging checks' "pending
 * translation" skip (`judgedBranchPairs` per branch, `carriesSourceOn` per
 * rendered OS). Pure, and dependent on nothing but the runtime splitter, so
 * `i18n-catalog-lib.ts` (which re-exports all of it: the one door the checks
 * import through) builds its catalog-aware readers on top. The mechanism:
 * `src/lib/intl/DETAILS.md` § Platform variants.
 */

import { PLATFORM_SELECT_PREFIX, parsePlatformSelect, pickPlatformBranch } from '../src/lib/intl/platform-select.ts'
import type { PlatformSelectBranch } from '../src/lib/intl/platform-select.ts'

/** One branch of a value, as `platformBranches` reads it. `name` is `null` for a value that isn't a block. */
export interface PlatformBranch {
  name: PlatformSelectBranch | null
  text: string
}

/** An OS `pickPlatformBranch` renders for. */
export type RenderedPlatform = 'macos' | 'windows' | 'linux'

/** Every OS a reader renders on, for a check that compares what two KEYS show side by side. */
export const RENDERED_PLATFORMS: readonly RenderedPlatform[] = Object.freeze(['macos', 'windows', 'linux'])

const BRANCH_ORDER: readonly PlatformSelectBranch[] = ['macos', 'windows', 'other']

/** The OS that reads a branch name: `other` is Linux's (and every backend-less context's). */
const BRANCH_READER: Readonly<Record<PlatformSelectBranch, RenderedPlatform>> = {
  macos: 'macos',
  windows: 'windows',
  other: 'linux',
}

/** The OS that reads a platform branch (`other` is Linux's, and every backend-less context's). */
export function branchReader(name: PlatformSelectBranch): RenderedPlatform {
  return BRANCH_READER[name]
}

/**
 * The text a check must judge, branch by branch. A platform block
 * (`{platform, select, macos {…} windows {…} other {…}}`, split by the runtime's
 * own `parsePlatformSelect`, so both families split the same way) yields its
 * branches in canonical order; any other value, a malformed block included,
 * yields itself as one unnamed branch. Every check that reads catalog TEXT goes
 * through here: a whole-value reader sees a raw block's branch bodies as
 * `{token}`s (`rawTokens`) or strips them as spans, going blind to them.
 * Whether a block is well formed is the platform-select check's question.
 * @param value a catalog value, either family
 */
export function platformBranches(value: string): PlatformBranch[] {
  const branches = parsePlatformSelect(value)
  if (branches === null) return [{ name: null, text: value }]
  return BRANCH_ORDER.flatMap((name) => {
    const text = branches[name]
    return text === undefined ? [] : [{ name, text }]
  })
}

/**
 * The inverse of `platformBranches`: writes branches back as one canonical block
 * (`{platform, select, macos {…} windows {…} other {…}}`), or a lone unnamed
 * branch as the plain value it came from. For generators that transform each
 * branch's text and must keep the block's frame and branch names untouched.
 * @param branches what `platformBranches` returned, each `text` possibly rewritten
 */
export function formatPlatformBranches(branches: readonly PlatformBranch[]): string {
  if (branches.length === 1 && branches[0].name === null) return branches[0].text
  const body = branches.map(({ name, text }) => `${String(name)} {${text}}`).join(' ')
  return `${PLATFORM_SELECT_PREFIX}${body}}`
}

/** One pairing of `pairPlatformBranches`: the source and locale text a reader of `name`'s OS sees. */
export interface PlatformBranchPair {
  name: PlatformSelectBranch | null
  source: string
  locale: string
}

/**
 * Pairs a locale value with its source branch by branch, for the checks that
 * compare the two (parity, and every rule that asks "is this branch the source's
 * same branch?"). With matching branch sets each branch meets its namesake. When
 * the sets differ (that mismatch is the platform-select check's finding), every
 * branch name either side carries is paired by what its OS READS on each side
 * (`pickPlatformBranch`: a missing branch reads `other`, a plain value reads
 * itself), so a comparison still judges what each OS would render. Two plain
 * values give one unnamed pair.
 * @param sourceValue the source (usually English) value
 * @param localeValue the locale's value
 */
export function pairPlatformBranches(sourceValue: string, localeValue: string): PlatformBranchPair[] {
  const sourceBranches = platformBranches(sourceValue)
  const localeBranches = platformBranches(localeValue)
  const present = new Set([...sourceBranches, ...localeBranches].map(({ name }) => name))
  if (present.size === 1 && present.has(null)) return [{ name: null, source: sourceValue, locale: localeValue }]
  return BRANCH_ORDER.filter((name) => present.has(name)).map((name) => ({
    name,
    source: pickPlatformBranch(sourceValue, BRANCH_READER[name]),
    locale: pickPlatformBranch(localeValue, BRANCH_READER[name]),
  }))
}

/**
 * The branch pairs a JUDGING check reads (dont-translate, termbase, mechanics):
 * `pairPlatformBranches` minus every named branch the locale carries
 * byte-identical from the source's same branch. That branch is English pending
 * translation (the shape a migrated key lands in, `src/lib/intl/DETAILS.md` §
 * Platform variants): the stale check owns it, and judging its English against a
 * locale's rulings would read "File Explorer" in a German `windows` branch as a
 * missing German term. A plain pair is always kept, equal or not: a whole value
 * identical to its source is coverage's question, and the judging checks read it
 * exactly as they always have.
 * @param sourceValue the source (usually English) value
 * @param localeValue the locale's value
 */
export function judgedBranchPairs(sourceValue: string, localeValue: string): PlatformBranchPair[] {
  return pairPlatformBranches(sourceValue, localeValue).filter(
    ({ name, source, locale }) => name === null || locale !== source,
  )
}

/**
 * Whether a locale still renders its source's text on `platform` for a key whose
 * source is a platform block: the per-OS form of `judgedBranchPairs`' skip, for a
 * check that pairs two KEYS (aria containment, quoted labels) or groups keys by
 * their English (term consistency), where branch names don't line up across keys
 * and only the rendered OS does. A plain source never counts (coverage's question).
 * @param sourceValue the source (usually English) value
 * @param localeValue the locale's value
 * @param platform the OS rendering it
 */
export function carriesSourceOn(sourceValue: string, localeValue: string, platform: RenderedPlatform): boolean {
  if (parsePlatformSelect(sourceValue) === null) return false
  return pickPlatformBranch(localeValue, platform) === pickPlatformBranch(sourceValue, platform)
}
