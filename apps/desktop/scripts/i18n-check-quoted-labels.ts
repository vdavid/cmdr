#!/usr/bin/env node
/**
 * QUOTED-LABEL check (i18n maintenance): WARN class.
 *
 * Text that names a button, setting, or command in quotes ("Click “Don’t show
 * again”") sends the reader looking for that exact control, so the quote must copy
 * the label as the locale writes it. A synonym, a different article, or a stale
 * form leaves them hunting for a button that doesn't exist (principles § names
 * vs prose; the sweeps found it in five locales).
 *
 * Pairs come from English, the same way the aria check finds its pairs: an English
 * value quoting (“…”) text that is the WHOLE value of another key (a trailing
 * ellipsis aside) quotes that key's label. No metadata to maintain, and a quote
 * of an ordinary word never pairs. When several keys share that English, the
 * locale may copy any of their translations.
 *
 * The comparison ignores case, whitespace kind, ellipsis glyphs, and the ICU
 * doubled apostrophe, and reads ' as ’. An overlay is read through its base, so a
 * forked label whose quoting sentence wasn't forked with it is a finding.
 *
 * Warn-only: a paraphrase is a quality slip, and the label is sometimes bent to
 * fit the sentence on purpose (a case ending); a reviewer judges.
 *
 * Run: `pnpm i18n:check-quoted-labels` (desktop). `--messages-root <dir>` points at a fixture.
 */

import { BASE_LOCALE, loadCatalog } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'
import type { LocaleCheckOptions } from './i18n-locale-check-lib.ts'
import type { Issue } from './i18n-locale-check-lib.ts'

/** A value longer than this is prose, never a label someone quotes. */
const MAX_LABEL_LENGTH = 60

/** One English value quoting another key's label. */
export interface QuotedLabelPair {
  key: string
  label: string
  /** the keys whose English value is that label */
  sources: string[]
}

/** How two strings compare for "is this the same label": see the file comment. */
const fold = (text: string): string =>
  text
    .replace(/''/g, "'")
    .replace(/'/g, '’')
    .replace(/[…⋯]|\.\.\./g, '')
    .replace(/\s+/gu, ' ')
    .trim()
    .toLowerCase()

/** Every English value that quotes another key's whole label. */
export function quotedLabelPairs(english: Record<string, string>): QuotedLabelPair[] {
  const byLabel = new Map<string, string[]>()
  for (const [key, value] of Object.entries(english)) {
    if (value.length > MAX_LABEL_LENGTH || /[{<]/.test(value)) continue
    const label = fold(value)
    byLabel.set(label, [...(byLabel.get(label) ?? []), key])
  }
  const pairs: QuotedLabelPair[] = []
  for (const [key, value] of Object.entries(english)) {
    // A label starts with a capital (sentence case); a quoted lowercase word is prose.
    for (const match of value.matchAll(/“(\p{Lu}[^”]*)”/gu)) {
      const sources = (byLabel.get(fold(match[1])) ?? []).filter((source) => source !== key)
      if (sources.length > 0) pairs.push({ key, label: match[1], sources })
    }
  }
  return pairs
}

/** The pairs whose quoting text, in `messages`, contains none of its sources' labels. */
export function mismatchedQuotes(pairs: readonly QuotedLabelPair[], messages: Record<string, string>): Issue[] {
  const issues: Issue[] = []
  for (const { key, label, sources } of pairs) {
    const value = messages[key] as string | undefined
    const labels = sources.filter((source) => source in messages)
    if (value === undefined || labels.length === 0) continue
    if (labels.some((source) => fold(value).includes(fold(messages[source])))) continue
    const [first] = labels
    const shown = messages[first].replace(/''/g, "'").replace(/[…⋯]$/, '')
    issues.push({ key, detail: `quotes “${label}”, but not its label here: “${shown}” (${first})` })
  }
  return issues
}

/**
 * Runs the check over the catalogs under `messagesRoot`.
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.write output sink, one line at a time (for tests)
 */
export function runQuotedLabelsCheck(opts: LocaleCheckOptions = {}): number {
  const pairs = quotedLabelPairs(loadCatalog(BASE_LOCALE, opts.messagesRoot).messages)
  return runLocaleCheck({
    title: 'Quoted labels',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    only: opts.only,
    summaryLine: (count) => `${String(count)} key(s) quote a label in words the label itself doesn’t use:`,
    inspectLocale: ({ source, isOverlay, catalog, findings }) => {
      // An overlay reads through its base, and answers only for a pair it forks a side of:
      // the base's own findings are the base's to fix.
      const held = isOverlay
        ? pairs.filter(({ key, sources }) => key in catalog.messages || sources.some((s) => s in catalog.messages))
        : pairs
      const effective = isOverlay ? { ...source.messages, ...catalog.messages } : catalog.messages
      for (const { key, detail } of mismatchedQuotes(held, effective)) findings.add(key, detail)
    },
  })
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  try {
    process.exit(runQuotedLabelsCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the quoted-label check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
