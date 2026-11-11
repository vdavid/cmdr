/**
 * The one splitter for platform-variant catalog values: a WHOLE value shaped
 * `{platform, select, macos {…} windows {…} other {…}}` (canonical spelling and
 * order; `other` is Linux and every context without a backend).
 *
 * ICU values never come through here at render time: `t()` injects `platform`
 * and the ICU engine does the select. This exists for the families that have no
 * engine: raw values (`getMessage()`, `errors.*`), the generated Rust native
 * strings, and the checks. Mechanism: `DETAILS.md` § "Platform variants".
 *
 * Dependency-free and ES2020 on purpose: scripts under `apps/desktop/scripts/`
 * import it by relative path, outside Vite's aliases.
 */

/** The branch names a block may carry. */
export type PlatformSelectBranch = 'macos' | 'windows' | 'other'

/** A block's branches, by name: the source text of each, unformatted. */
export interface PlatformSelectBranches {
  macos?: string
  windows?: string
  other: string
}

/** Every block starts with exactly this. The shape check enforces the spelling, so a prefix test is sound. */
export const PLATFORM_SELECT_PREFIX = '{platform, select, '

const BRANCH_NAMES: readonly string[] = ['macos', 'windows', 'other']

/**
 * Characters after a lone `'` that start an ICU quoted run (`intl-messageformat`'s
 * `tryParseQuote`; it adds `#` inside a plural, which never hides a brace). A raw
 * body never has `'` before one of them (the shape check), so this one quoting
 * rule splits both families the same.
 */
const QUOTE_OPENERS = new Set(['{', '}', '<', '>'])

/**
 * The index of the `}` matching the `{` at `open`, or -1 when the braces don't
 * balance. Reads ICU apostrophe quoting: `''` is a literal apostrophe, and `'`
 * before a quote opener starts a run that the next lone `'` ends.
 */
function matchingBrace(value: string, open: number): number {
  let depth = 0
  let i = open
  while (i < value.length) {
    const char = value[i]
    if (char === "'") {
      if (value[i + 1] === "'") {
        i += 2
        continue
      }
      if (QUOTE_OPENERS.has(value[i + 1])) {
        i = quotedRunEnd(value, i + 1)
        continue
      }
    } else if (char === '{') {
      depth += 1
    } else if (char === '}') {
      depth -= 1
      if (depth === 0) return i
    }
    i += 1
  }
  return -1
}

/** The index just past the `'` closing a quoted run that started at `start`, or the value's end (ICU's unterminated quote). */
function quotedRunEnd(value: string, start: number): number {
  let i = start
  while (i < value.length) {
    if (value[i] === "'") {
      if (value[i + 1] === "'") {
        i += 2
        continue
      }
      return i + 1
    }
    i += 1
  }
  return value.length
}

function skipSpaces(value: string, from: number): number {
  let i = from
  while (i < value.length && /\s/.test(value[i])) i += 1
  return i
}

/**
 * A block's branches, or `null` for a value that isn't a well-formed block: a
 * plain value, a block inside a sentence, unbalanced braces, an unknown or
 * repeated branch name, or no `other`. Branch order isn't judged here (the shape
 * check does that).
 */
export function parsePlatformSelect(value: string): PlatformSelectBranches | null {
  if (!value.startsWith(PLATFORM_SELECT_PREFIX)) return null
  if (matchingBrace(value, 0) !== value.length - 1) return null

  const branches: Partial<Record<PlatformSelectBranch, string>> = {}
  let i = PLATFORM_SELECT_PREFIX.length
  for (;;) {
    i = skipSpaces(value, i)
    if (i === value.length - 1) break
    const name = /^[a-z]+/.exec(value.slice(i))?.[0]
    if (name === undefined || !BRANCH_NAMES.includes(name)) return null
    const branch = name as PlatformSelectBranch
    if (branches[branch] !== undefined) return null
    i = skipSpaces(value, i + name.length)
    if (value[i] !== '{') return null
    const close = matchingBrace(value, i)
    if (close === -1) return null
    branches[branch] = value.slice(i + 1, close)
    i = close + 1
  }

  const { other } = branches
  if (other === undefined) return null
  return { ...branches, other }
}

/**
 * The source text `platform` reads from `value`: its branch, or `other` when the
 * block has none for it (Linux always reads `other`). A value that isn't a block
 * comes back unchanged.
 */
export function pickPlatformBranch(value: string, platform: 'macos' | 'windows' | 'linux'): string {
  const branches = parsePlatformSelect(value)
  if (branches === null) return value
  if (platform === 'linux') return branches.other
  return branches[platform] ?? branches.other
}
