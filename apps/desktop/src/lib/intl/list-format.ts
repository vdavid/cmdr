/**
 * Locale-aware "A, B, and C" joining, memoized.
 *
 * The one place a list of names becomes a phrase inside a sentence, so no
 * feature module hand-joins with `', '` and a hardcoded English "and". Today's
 * caller is the eject refusal that names the apps still holding a drive
 * (`$lib/file-explorer/navigation/eject-error-messages.ts`).
 *
 * The locale is {@link getUiLocale}, not the OS formatting one: this is a
 * conjunction inside catalog copy, so it has to speak the language the sentence
 * around it speaks. That's the split `./locale.ts` draws, and it's why sizes and
 * dates resolve the other way.
 *
 * Memoized per locale for the same reason `number-format.ts` is: constructing an
 * `Intl` formatter costs far more than formatting with one.
 */

import { getUiLocale } from './locale'

interface ConjunctionJoiner {
  formatter: Intl.ListFormat
  /** Whether the joins need Han–Latin spacing on top of CLDR's pattern. */
  spacesHan: boolean
}

/** Cache of one conjunction joiner per UI locale. */
const listFormatterCache = new Map<string, ConjunctionJoiner>()

/** A memoized conjunction `Intl.ListFormat` for the active UI locale. */
function getConjunctionJoiner(): ConjunctionJoiner {
  const locale = getUiLocale()
  let joiner = listFormatterCache.get(locale)
  if (joiner === undefined) {
    joiner = {
      formatter: new Intl.ListFormat(locale, { style: 'long', type: 'conjunction' }),
      spacesHan: spacesHanAgainstLatin(locale),
    }
    listFormatterCache.set(locale, joiner)
  }
  return joiner
}

/**
 * Join names the way the UI language joins them: `Preview and Warp`,
 * `Preview, Warp, and Photos`. An empty list answers an empty string, which is
 * the caller's cue that it has no list sentence to say.
 */
export function formatConjunctionList(items: readonly string[]): string {
  if (items.length === 0) return ''
  const { formatter, spacesHan } = getConjunctionJoiner()
  if (!spacesHan) return formatter.format(items)
  return formatter
    .formatToParts(items)
    .map((part, i, parts) =>
      part.type === 'literal' ? spaceLiteral(part.value, parts[i - 1]?.value, parts[i + 1]?.value) : part.value,
    )
    .join('')
}

const HAN = /\p{Script=Han}/u
const LATIN_OR_DIGIT = /[\p{Script=Latin}\p{Nd}]/u

/**
 * Chinese puts a space between Han and a Latin word or number, and CLDR's list
 * patterns don't: `Warp和其他 App`. Both our Chinese style guides rule the
 * spaced form (`docs/i18n/zh/style.md`, `docs/i18n/zh-Hant/style.md`), and the
 * names joined here are app names, often Latin. Japanese runs tight, so this is
 * by language, not by script.
 */
function spacesHanAgainstLatin(locale: string): boolean {
  return locale.split('-')[0].toLowerCase() === 'zh'
}

/** Pads a connective like `和` where a Han edge meets a Latin one. The
 *  enumeration comma `、` is full-width punctuation, so it never gets one. */
function spaceLiteral(literal: string, before: string | undefined, after: string | undefined): string {
  const lead = HAN.test(literal.at(0) ?? '') && LATIN_OR_DIGIT.test(before?.at(-1) ?? '') ? ' ' : ''
  const trail = HAN.test(literal.at(-1) ?? '') && LATIN_OR_DIGIT.test(after?.at(0) ?? '') ? ' ' : ''
  return lead + literal + trail
}

/** Test seam: drop the memoization cache so a memoization assertion starts clean. */
export function _clearListFormatCacheForTests(): void {
  listFormatterCache.clear()
}
