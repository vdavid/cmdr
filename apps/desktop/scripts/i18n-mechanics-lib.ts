/**
 * A locale's declared typography, `docs/i18n/<tag>/mechanics.json`: its quotation
 * marks, apostrophes, required spacing, and the hedged-grammar patterns its
 * grammar tempts translators into. Two consumers: the brief prints the
 * declaration beside the style digest (`i18n-brief-lib.ts`), and the mechanics
 * check holds the catalog to it (`i18n-check-mechanics.ts`). Schema and the why:
 * `docs/i18n/termbase.md` § `<tag>/mechanics.json` schema.
 */

import { join } from 'node:path'
import { readJsonIfPresent, resolveDocsRoot } from './i18n-termbase-lib.ts'

/** A pattern a locale's catalog values must NOT contain, and why. */
export interface MechanicsRule {
  /** a JavaScript regex source, compiled with the `u` flag; `INSERT_MARK` stands in for a placeholder */
  pattern: string
  why: string
}

/** An opening and a closing quotation mark. */
export type QuotePair = [string, string]

/** One locale's `mechanics.json`. */
export interface Mechanics {
  quotes: { primary: QuotePair; nested?: QuotePair }
  /** the characters this language writes as an apostrophe (`’`, or `'` where the straight one is the norm) */
  apostrophes?: string[]
  /** spacing the language requires, each written as the pattern that breaks it */
  spacing?: MechanicsRule[]
  /** parenthesized or slashed alternatives this language's grammar tempts translators into */
  hedges?: MechanicsRule[]
  $comment?: string
}

/**
 * Every character that works as a quotation mark in some language. A value may
 * only use the ones its locale declares (as a quote pair or an apostrophe); the
 * straight double quote is never allowed, whatever the declaration.
 */
export const QUOTE_MARKS: ReadonlySet<string> = new Set([
  '"',
  "'",
  '“',
  '”',
  '„',
  '‟',
  '‘',
  '’',
  '‚',
  '‛',
  '«',
  '»',
  '「',
  '」',
  '『',
  '』',
  '〝',
  '〞',
  '〟',
  '＂',
])

/**
 * Single guillemets: a quote pair in some languages, and the breadcrumb separator
 * in every one ("Settings › AI"). A locale may declare them, but a value using
 * one undeclared is never a finding.
 */
export const SEPARATOR_QUOTES: ReadonlySet<string> = new Set(['‹', '›'])

/** A locale's mechanics path. */
export function mechanicsPath(tag: string, docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), tag, 'mechanics.json')
}

/** The raw parsed `mechanics.json`, or `undefined` when the locale hasn't declared one yet. */
export function loadMechanicsRaw(tag: string, docsRoot?: string): unknown {
  return readJsonIfPresent(mechanicsPath(tag, docsRoot))
}

/** The rule list, however loosely the file is written: the brief should still render around a typo. */
function rules(value: unknown): MechanicsRule[] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (rule): rule is MechanicsRule =>
      typeof rule === 'object' && rule !== null && typeof (rule as MechanicsRule).pattern === 'string',
  )
}

/** A quote pair as `‘…’`, or `undefined` when it isn't a pair. */
function pairShown(pair: unknown): string | undefined {
  return Array.isArray(pair) && pair.length === 2 ? `${String(pair[0])}…${String(pair[1])}` : undefined
}

/**
 * One line summarizing a locale's declaration for the brief, or a pointer to the
 * digest when it has none: `quotes «…», nested “…” · apostrophe ’ · spacing: … · hedges: …`.
 */
export function mechanicsSummary(raw: unknown): string {
  if (typeof raw !== 'object' || raw === null) return 'no mechanics.json yet: follow the digest for quotes and spacing'
  const m = raw as Partial<Mechanics>
  const parts: string[] = []
  const primary = pairShown(m.quotes?.primary)
  const nested = pairShown(m.quotes?.nested)
  if (primary) parts.push(nested ? `quotes ${primary}, nested ${nested}` : `quotes ${primary}`)
  if (Array.isArray(m.apostrophes) && m.apostrophes.length > 0) parts.push(`apostrophe ${m.apostrophes.join(' or ')}`)
  const shown = (list: MechanicsRule[]) => list.map((rule) => `\`${rule.pattern}\` (${rule.why})`).join('; ')
  const spacing = rules(m.spacing)
  if (spacing.length > 0) parts.push(`spacing, never: ${shown(spacing)}`)
  const hedges = rules(m.hedges)
  if (hedges.length > 0) parts.push(`hedges to rephrase: ${shown(hedges)}`)
  parts.push('`…` for an ellipsis, never `...` or a straight `"`')
  return parts.join(' · ')
}
