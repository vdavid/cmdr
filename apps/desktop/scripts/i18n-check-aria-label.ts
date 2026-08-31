#!/usr/bin/env node
/**
 * ARIA-LABEL CONTAINMENT check (i18n a11y): ERROR class, run in CI on every push.
 *
 * WCAG 2.5.3 (Label in Name): when a control's accessible name is one key and its
 * visible label is another, the name must CONTAIN the label verbatim. Voice-control
 * users say what they can SEE, so an accessible name that paraphrases the label
 * leaves them unable to press the button at all.
 *
 * English gets this right by construction (the author writes both). Translation is
 * where it breaks, silently and per-language, because inflection pulls the two
 * apart: a German case the label doesn't have, a Hungarian suffix, a Swedish
 * definite form, or simply a translator picking a smoother verb for the sentence
 * than for the button. `docs/guides/i18n-translation.md` § An `*Aria` key must
 * contain its visible label spells out the failure shapes and the fix (bend the
 * LABEL to the form the natural aria sentence already uses, then cut the label out
 * of that sentence).
 *
 * ## What makes a pair "real"
 *
 * Pairs are found by naming convention (`foo` + `fooAria` / `fooAriaLabel`), then
 * **gated on English containment**. That gate is what keeps this quiet: plenty of
 * `*Aria` keys aren't accessible names for a sibling label at all
 * (`main.quit.countdownAria` describes a timer next to a countdown SENTENCE;
 * `updates.toast.versionChangeAria` narrates a `v{prev} → v{next}` badge). English
 * doesn't satisfy those either, so they're not pairs and are never reported. Only
 * a pair English itself honours is held to the standard in every locale, which
 * means zero judgment and no allowlist.
 *
 * Comparison ignores case, `{placeholders}`, `<tag>` names, whitespace, and
 * punctuation in both scripts, and folds the ICU doubled apostrophe: none of
 * those is what a voice-control user pronounces.
 *
 * A report fails the build, and CI runs this on every push. The English-containment
 * gate above is what earns that: a reported pair is one English itself honours, so
 * it's a real regression rather than a judgment call, and no pair has a legitimate
 * reason to stay broken. ❌ Don't soften this to a warn.
 *
 * Run: `pnpm i18n:check-aria-label` (desktop). Pass `--messages-root <dir>` to
 * point at a fixture (used by the tests).
 */

import {
  BASE_LOCALE,
  isRawKey,
  layerCatalogs,
  listLocales,
  loadCatalog,
  resolveLocaleSource,
} from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'

/** A visible-label key and the accessible-name key that must contain it. */
export interface AriaPair {
  labelKey: string
  ariaKey: string
}

/** Strips everything a voice-control user doesn't pronounce. */
function speakable(value: string, key: string): string {
  const unescaped = isRawKey(key) ? value : value.replace(/''/g, "'")
  return unescaped
    .replace(/\{[^{}]*\}/g, ' ')
    .replace(/<\/?[^<>]*>/g, ' ')
    .replace(/[\s…⋯]/g, '')
    .replace(/[.,:!?;'"()[\]{}·—–-]/g, '')
    .replace(/[。，、！？：；「」『』（）]/g, '')
    .toLowerCase()
}

/**
 * Whether an accessible name contains its visible label, ignoring case,
 * placeholders, tags, whitespace, and punctuation.
 *
 * @param aria the accessible-name value
 * @param label the visible-label value
 */
export function containsLabel(aria: string, label: string): boolean {
  const needle = speakable(label, 'x')
  if (needle.length === 0) return true
  return speakable(aria, 'x').includes(needle)
}

/**
 * The REAL label/aria pairs in a source catalog: a `fooAria` (or `fooAriaLabel`)
 * whose sibling `foo` exists AND whose English already contains it. The English
 * gate is what distinguishes an accessible name from an unrelated description
 * that merely ends in `Aria`.
 *
 * @param source the `en` catalog (or an overlay's effective source)
 */
export function ariaPairs(source: Catalog): AriaPair[] {
  const pairs: AriaPair[] = []
  for (const ariaKey of Object.keys(source.messages)) {
    const match = /^(.*?)(?:Aria|AriaLabel)$/.exec(ariaKey)
    if (!match || match[1].length === 0) continue
    const labelKey = match[1]
    if (!(labelKey in source.messages)) continue
    if (!containsLabel(source.messages[ariaKey], source.messages[labelKey])) continue
    pairs.push({ labelKey, ariaKey })
  }
  return pairs.sort((a, b) => a.ariaKey.localeCompare(b.ariaKey))
}

/**
 * What a locale actually renders for a key: its own value, or for an OVERLAY the
 * source value it falls through to. `undefined` when nothing renders.
 */
function effectiveValue(source: Catalog, catalog: Catalog, key: string, isOverlay: boolean): string | undefined {
  if (key in catalog.messages) return catalog.messages[key]
  if (isOverlay && key in source.messages) return source.messages[key]
  return undefined
}

/** One locale's broken pair, with the two values that no longer line up. */
export interface AriaFinding extends AriaPair {
  label: string
  aria: string
}

/**
 * Checks one locale's rendering of every real pair in `source`.
 *
 * @param source the catalog the locale is checked against
 * @param catalog the locale's own catalog
 * @param isOverlay whether an absent key falls through to the source value
 */
export function checkLocale(source: Catalog, catalog: Catalog, isOverlay = false): AriaFinding[] {
  const findings: AriaFinding[] = []
  for (const { labelKey, ariaKey } of ariaPairs(source)) {
    const label = effectiveValue(source, catalog, labelKey, isOverlay)
    const aria = effectiveValue(source, catalog, ariaKey, isOverlay)
    if (label === undefined || aria === undefined) continue
    if (containsLabel(aria, label)) continue
    findings.push({ labelKey, ariaKey, label, aria })
  }
  return findings
}

function main(): void {
  const args = process.argv.slice(2)
  const rootFlag = args.indexOf('--messages-root')
  const messagesRoot = rootFlag === -1 ? undefined : args[rootFlag + 1]

  const available = listLocales(messagesRoot)
  const loaded = new Map<string, Catalog>()
  const load = (tag: string): Catalog => {
    const hit = loaded.get(tag)
    if (hit) return hit
    const fresh = loadCatalog(tag, messagesRoot)
    loaded.set(tag, fresh)
    return fresh
  }

  const locales = available.filter((tag) => tag !== BASE_LOCALE)
  if (locales.length === 0) {
    console.log(`Aria label containment: no non-${BASE_LOCALE} locales to check.`)
    process.exit(EXIT_CLEAN)
  }

  let total = 0
  const pairCount = ariaPairs(load(BASE_LOCALE)).length
  for (const locale of locales) {
    const { overrides, isOverlay } = resolveLocaleSource(locale, available)
    const source = isOverlay ? layerCatalogs(load(BASE_LOCALE), load(overrides)) : load(BASE_LOCALE)
    const findings = checkLocale(source, load(locale), isOverlay)
    if (findings.length === 0) {
      console.log(`${locale}: clean.`)
      continue
    }
    total += findings.length
    const noun = findings.length === 1 ? 'name that no longer contains its' : 'names that no longer contain their'
    console.log(`${locale}: ${String(findings.length)} accessible ${noun} visible label`)
    for (const { labelKey, ariaKey, label, aria } of findings) {
      console.log(`  - ${ariaKey} → ${JSON.stringify(aria)} doesn't contain ${labelKey} = ${JSON.stringify(label)}`)
    }
  }

  if (total === 0) {
    console.log(`Aria label containment: all ${String(pairCount)} label/name pairs hold in every locale.`)
    process.exit(EXIT_CLEAN)
  }
  process.exit(EXIT_ISSUES)
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    main()
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
