/**
 * The one fuzzy matcher for user-typed lookups over short labels: Settings search, the command
 * palette (and the Keyboard shortcuts section through it), and the recent-items dropdown.
 *
 * Three things on top of plain uFuzzy, all for languages other than English:
 *
 * - **Every script is a letter.** uFuzzy's default config only knows `a-z`, so `язык` or `语言`
 *   found nothing. The `\p{L}` options below are uFuzzy's documented universal variant, plus
 *   `\p{M}` so a script whose vowels are combining marks doesn't split words mid-letter.
 * - **Accents don't have to match.** `foldForSearch` strips them from both sides, so `tema` finds
 *   `téma` and `ngon ngu` finds `ngôn ngữ`.
 * - **No typos in a CJK query.** A Chinese or Japanese term is a few characters long, and one
 *   "typo" turns it into a different word, so such a query matches exactly.
 */

import uFuzzy from '@leeoniya/ufuzzy'
import { INTRA_MODE_MULTI_INSERT, INTRA_MODE_SINGLE_ERROR } from './ufuzzy-modes.js'

/**
 * Text that went through `foldForSearch`. Branded so a haystack can't skip the fold: an unfolded
 * haystack would silently miss every accented word.
 */
export type FoldedText = string & { readonly __folded: unique symbol }

/** Scripts whose accents are combining marks we may drop. Others (Hangul, kana) decompose into letters, not accents. */
const FOLDABLE_SCRIPT = /[\p{Script=Latin}\p{Script=Greek}\p{Script=Cyrillic}]/u
const COMBINING_MARKS = /\p{M}/gu

/** Letters that aren't a base plus a mark, so NFD leaves them alone. Each maps to ONE character. */
const STANDALONE_FOLDS: Partial<Record<string, string>> = { ø: 'o', ł: 'l', đ: 'd', æ: 'a', œ: 'o' }

/** Folds one BMP character: lowercased, its accents dropped, still exactly one code unit. */
function foldChar(char: string): string {
  const lower = char.toLowerCase()
  // `İ` lowercases to two code units; keeping it as is keeps every later offset valid.
  if (lower.length !== 1) return char
  if (lower < '\u0080') return lower
  const standalone = STANDALONE_FOLDS[lower]
  if (standalone !== undefined) return standalone
  if (!FOLDABLE_SCRIPT.test(lower)) return lower
  const base = lower.normalize('NFD').replace(COMBINING_MARKS, '')
  return base.length === 1 ? base : lower
}

/**
 * Lowercases `text` and strips its accents, ONE code unit in, ONE out, so a match range in the
 * folded text indexes the original text too (highlighting reads the original).
 *
 * Marks are stripped only from Latin, Greek, and Cyrillic letters (`ő` → `o`, `ё` → `е`). A Hangul
 * syllable decomposes into jamo and `が` into `か` plus a mark, and neither is an accent. `ß` stays:
 * `ss` would shift every offset after it. ❌ Not `uFuzzy.latinize`: its fixed table misses `ő`,
 * stacked Vietnamese diacritics, `ø`, and `đ`.
 */
export function foldForSearch(text: string): FoldedText {
  let out = ''
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    // A surrogate (half of an astral char like an emoji) passes through untouched.
    out += code >= 0xd800 && code <= 0xdfff ? text[i] : foldChar(text[i])
  }
  return out as FoldedText
}

/** uFuzzy's universal-script variant (its README's "Unicode / universal" options). */
const UNICODE_OPTIONS: uFuzzy.Options = {
  unicode: true,
  interSplit: "[^\\p{L}\\p{M}\\d']+",
  intraSplit: '\\p{Ll}\\p{Lu}',
  interBound: '[^\\p{L}\\p{M}\\d]',
  intraBound: '\\p{L}\\d|\\d\\p{L}|\\p{Ll}\\p{Lu}',
  intraChars: "[\\p{L}\\p{M}\\d']",
  intraContr: "'\\p{L}{1,2}\\b",
}

/** Typo-tolerant within words ("tyoe" → "type"), at most three characters inserted between terms. */
const tolerant = new uFuzzy({ ...UNICODE_OPTIONS, intraMode: INTRA_MODE_SINGLE_ERROR, interIns: 3 })

/** Each term must appear as typed. */
const exact = new uFuzzy({ ...UNICODE_OPTIONS, intraMode: INTRA_MODE_MULTI_INSERT, intraIns: 0, interIns: 3 })

const CJK = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u

/**
 * Searches a folded haystack for `query` (folded here). The result is uFuzzy's own, and its
 * `info.ranges` index the original, unfolded strings as well as the folded ones.
 */
export function fuzzySearch(haystack: FoldedText[], query: string): uFuzzy.SearchResult {
  const engine = CJK.test(query) ? exact : tolerant
  return engine.search(haystack, foldForSearch(query))
}
