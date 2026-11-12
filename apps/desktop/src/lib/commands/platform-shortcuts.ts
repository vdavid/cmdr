/**
 * A command's default keys, one list per platform, each in that platform's own canonical form
 * (what `formatKeyCombo` writes there: `⌘⇧P` on macOS, `Ctrl+Shift+P` on Windows and Linux).
 *
 * The helpers are constructors that return all three arms at module load, so the registry data
 * is always the full three-way table and nothing converts a combo at read time. Each names the
 * rule it applies. They take macOS glyph spelling, so the macOS arm is the literal in the source,
 * and write the other two arms in canonical `Ctrl+Alt+Shift+Super` order. A list that fits no
 * helper (it holds `⌃`, or its keys truly differ per platform) is written as the literal
 * `{ macos, windows, linux }`.
 */
import type { Platform } from '#lib/platform/index.js'

/** A command's default keys on each platform, in that platform's canonical spelling. */
export type PlatformShortcuts = Readonly<Record<Platform, readonly string[]>>

/** Every platform, in the order the arms are written. */
export const PLATFORMS = ['macos', 'windows', 'linux'] as const satisfies readonly Platform[]

type HoldsGlyph<S extends string, G extends string> = S extends `${string}${G}${string}` ? true : false

/** Each combo, or `never` where it holds ⌘ or ⌃ (`sameOnAll` refuses those). */
type WithoutCommandGlyphs<T extends readonly string[]> = {
  [K in keyof T]: HoldsGlyph<T[K], '⌘' | '⌃'> extends true ? never : T[K]
}

/** Each combo, or `never` where it lacks ⌘ or holds ⌃ (`cmdOrCtrl` needs the first, refuses the second). */
type CommandChords<T extends readonly string[]> = {
  [K in keyof T]: HoldsGlyph<T[K], '⌃'> extends true ? never : HoldsGlyph<T[K], '⌘'> extends true ? T[K] : never
}

/** The macOS modifier glyphs, in the order Windows and Linux write their tokens. */
const offMacTokens: readonly [glyph: string, token: string][] = [
  ['⌘', 'Ctrl'],
  ['⌥', 'Alt'],
  ['⇧', 'Shift'],
]

/** Splits a macOS-spelled combo into its leading modifier glyphs and its key. */
function splitMacCombo(combo: string): { glyphs: Set<string>; key: string } {
  const chars = Array.from(combo)
  const glyphs = new Set<string>()
  let i = 0
  // The key itself may be one character, so the last character is always the key.
  while (i < chars.length - 1 && '⌘⌃⌥⇧'.includes(chars[i])) glyphs.add(chars[i++])
  return { glyphs, key: chars.slice(i).join('') }
}

/** The Windows and Linux spelling of a macOS combo holding no ⌃, in canonical token order. */
function offMacSpelling(combo: string): string {
  const { glyphs, key } = splitMacCombo(combo)
  const tokens = offMacTokens.filter(([glyph]) => glyphs.has(glyph)).map(([, token]) => token)
  return [...tokens, key].join('+')
}

function refuse(helper: string, combo: string, why: string): never {
  throw new Error(`${helper}('${combo}'): ${why}`)
}

/** One list on macOS, its off-macOS spelling on Windows and Linux. */
function spreadFromMac(macos: readonly string[]): PlatformShortcuts {
  const offMac = macos.map(offMacSpelling)
  return { macos: [...macos], windows: offMac, linux: [...offMac] }
}

/**
 * The same physical chord on every platform: a bare key, or one with ⌥ / ⇧ (Alt / Shift off
 * macOS). Refuses ⌘ and ⌃, whose meaning differs per platform. `sameOnAll()` is "no key".
 */
export function sameOnAll<const T extends readonly string[]>(
  ...combos: T & WithoutCommandGlyphs<T>
): PlatformShortcuts {
  for (const combo of combos) {
    const { glyphs } = splitMacCombo(combo)
    if (glyphs.has('⌘') || glyphs.has('⌃')) refuse('sameOnAll', combo, 'holds ⌘ or ⌃; use cmdOrCtrl or a literal')
  }
  return spreadFromMac(combos)
}

/**
 * ⌘ on macOS, the same chord on Ctrl on Windows and Linux. Each combo needs ⌘ and refuses ⌃, so
 * `⌘D` and `⌃D` can never land on one key by accident: a command that needs ⌃ writes the literal.
 */
export function cmdOrCtrl<const T extends readonly string[]>(...combos: T & CommandChords<T>): PlatformShortcuts {
  for (const combo of combos) {
    const { glyphs } = splitMacCombo(combo)
    if (!glyphs.has('⌘')) refuse('cmdOrCtrl', combo, 'has no ⌘; use sameOnAll')
    if (glyphs.has('⌃')) refuse('cmdOrCtrl', combo, 'holds ⌃; write the literal')
  }
  return spreadFromMac(combos)
}

/** Keys on macOS, none on Windows or Linux. */
export function macOnly(...combos: readonly string[]): PlatformShortcuts {
  return { macos: [...combos], windows: [], linux: [] }
}

/**
 * Joins lists arm by arm, in argument order, for a list that mixes rules (`Backspace` plus
 * `⌘↑`). Throws on a combo repeated within an arm.
 */
export function concatShortcuts(...parts: readonly PlatformShortcuts[]): PlatformShortcuts {
  const joined = { macos: [] as string[], windows: [] as string[], linux: [] as string[] }
  for (const platform of PLATFORMS) {
    for (const part of parts) {
      for (const combo of part[platform]) {
        if (joined[platform].includes(combo)) refuse('concatShortcuts', combo, `repeated on ${platform}`)
        joined[platform].push(combo)
      }
    }
  }
  return joined
}
