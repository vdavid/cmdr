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
 * A platform block on either key is paired per rendered OS (what each OS shows),
 * and a translating locale's OS where either side still shows its English is
 * skipped as pending translation (the stale check's job).
 *
 * Warn-only: a paraphrase is a quality slip, and the label is sometimes bent to
 * fit the sentence on purpose (a case ending); a reviewer judges.
 *
 * Run: `pnpm i18n:check-quoted-labels` (desktop). `--messages-root <dir>` points at a fixture.
 */

import { pickPlatformBranch } from '../src/lib/intl/platform-select.ts'
import {
  BASE_LOCALE,
  RENDERED_PLATFORMS,
  carriesSourceOn,
  loadCatalog,
  pendingTranslationOf,
} from './i18n-catalog-lib.ts'
import type { RenderedPlatform } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck, withoutGeneratedLocales } from './i18n-locale-check-lib.ts'
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
  /** the OSes this pairing renders on, set only when it isn't every OS (a platform block on either side) */
  platforms?: RenderedPlatform[]
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

/**
 * Every English value that quotes another key's whole label, per rendered OS: a
 * platform block (`{platform, select, …}`) on either key pairs what each OS
 * shows, since the two keys' branch names needn't line up. A pairing that holds
 * on every OS (any pair of plain values) is listed once, without `platforms`.
 */
export function quotedLabelPairs(english: Record<string, string>): QuotedLabelPair[] {
  const pairs = new Map<string, QuotedLabelPair>()
  for (const platform of RENDERED_PLATFORMS) {
    const shown = Object.fromEntries(
      Object.entries(english).map(([key, value]) => [key, pickPlatformBranch(value, platform)]),
    )
    const byLabel = new Map<string, string[]>()
    for (const [key, value] of Object.entries(shown)) {
      if (value.length > MAX_LABEL_LENGTH || /[{<]/.test(value)) continue
      const label = fold(value)
      byLabel.set(label, [...(byLabel.get(label) ?? []), key])
    }
    for (const [key, value] of Object.entries(shown)) {
      // A label starts with a capital (sentence case); a quoted lowercase word is prose.
      for (const match of value.matchAll(/“(\p{Lu}[^”]*)”/gu)) {
        const sources = (byLabel.get(fold(match[1])) ?? []).filter((source) => source !== key)
        if (sources.length === 0) continue
        const id = JSON.stringify([key, match[1], sources])
        const seen = pairs.get(id)
        // A value quoting the same label twice is one pairing, held once.
        if (seen) {
          if (!seen.platforms?.includes(platform)) seen.platforms?.push(platform)
        } else pairs.set(id, { key, label: match[1], sources, platforms: [platform] })
      }
    }
  }
  return [...pairs.values()].map(({ platforms, ...pair }) =>
    platforms?.length === RENDERED_PLATFORMS.length ? pair : { ...pair, platforms },
  )
}

/**
 * The pairs whose quoting text, in `messages`, contains none of its sources'
 * labels, judged on each OS the pair renders on. With `source` (the English a
 * translating locale may still show, `pendingTranslationOf`), an OS where the
 * quoting text or a label still shows its English (a platform block's branch
 * pending translation, `carriesSourceOn`) is skipped: the stale check owns it.
 * @param pairs from `quotedLabelPairs`
 * @param messages what the locale renders, key by key
 * @param source the `en` messages, for the pending-branch skip
 */
export function mismatchedQuotes(
  pairs: readonly QuotedLabelPair[],
  messages: Record<string, string>,
  source?: Record<string, string>,
): Issue[] {
  const issues: Issue[] = []
  const pending = (key: string, platform: RenderedPlatform) =>
    source !== undefined && key in source && carriesSourceOn(source[key], messages[key], platform)
  for (const { key, label, sources, platforms } of pairs) {
    if (!(key in messages)) continue
    for (const platform of platforms ?? RENDERED_PLATFORMS) {
      if (pending(key, platform)) continue
      const value = pickPlatformBranch(messages[key], platform)
      const labels = sources.filter((s) => s in messages && !pending(s, platform))
      if (labels.length === 0) continue
      const labelOf = (s: string) => pickPlatformBranch(messages[s], platform)
      if (labels.some((s) => fold(value).includes(fold(labelOf(s))))) continue
      const [first] = labels
      const shown = labelOf(first)
        .replace(/''/g, "'")
        .replace(/[…⋯]$/, '')
      const on = platforms === undefined ? '' : ` on ${platform}`
      issues.push({ key, detail: `quotes “${label}”, but not its label here: “${shown}” (${first})${on}` })
      // A pair that renders alike everywhere breaks alike everywhere: report it once.
      if (platforms === undefined) break
    }
  }
  return issues
}

/**
 * Runs the check over the catalogs under `messagesRoot`.
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.write output sink, one line at a time (for tests)
 * @param opts.only only these locales (a generated one is skipped even when named)
 */
export function runQuotedLabelsCheck(opts: LocaleCheckOptions = {}): number {
  const base = loadCatalog(BASE_LOCALE, opts.messagesRoot).messages
  const pairs = quotedLabelPairs(base)
  const write =
    opts.write ??
    ((line: string) => {
      console.log(line)
    })
  return runLocaleCheck({
    title: 'Quoted labels',
    messagesRoot: opts.messagesRoot,
    write,
    only: withoutGeneratedLocales(opts.only, write),
    summaryLine: (count) => `${String(count)} key(s) quote a label in words the label itself doesn’t use:`,
    inspectLocale: ({ locale, source, isOverlay, catalog, findings }) => {
      // An overlay reads through its base, and answers only for a pair it forks a side of:
      // the base's own findings are the base's to fix.
      const held = isOverlay
        ? pairs.filter(({ key, sources }) => key in catalog.messages || sources.some((s) => s in catalog.messages))
        : pairs
      const effective = isOverlay ? { ...source.messages, ...catalog.messages } : catalog.messages
      const english = pendingTranslationOf(locale, base)
      for (const { key, detail } of mismatchedQuotes(held, effective, english)) findings.add(key, detail)
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
