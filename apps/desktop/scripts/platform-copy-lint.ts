/**
 * The two nets under the platform copy (`platform-copy-rows.ts`):
 *
 *  - `scanEnglish`: every English value that uses macOS vocabulary and sits in no
 *    row and behind no gate, so a value nobody judged can't slip through.
 *  - `lintRow`: every `windows` and `other` branch held to
 *    `docs/style-guide.md` § "Platform vocabulary": no macOS word in either, no
 *    Trash, Quit, or Terminal on Windows, no Recycle Bin or "disconnect" on
 *    Linux, and the house style on text the row adds. A hit blocks its row unless
 *    the row carries a `lintExceptions` entry for that branch, with the reason.
 *
 * Style hits only count on a NEW branch: one byte-identical to English the key
 * already held stays as it is (the old "e.g. via chmod" in a pair's `other`),
 * since rewording an unchanged branch would throw away its translations. The
 * wrong-OS words count everywhere, so a branch that keeps the macOS text for now
 * says so with an exception.
 *
 * Not linted, because a pattern can't judge it: the Oxford comma, sentence case,
 * and active voice.
 */

import { SYSTEM_TOKENS, platformBranches, visibleText } from './i18n-catalog-lib.ts'
import { BRANCH_NAMES, matchesKeyPattern, rowKeys } from './platform-copy-rows.ts'
import type { ApplyRow, CopyFinal } from './platform-copy-rows.ts'

/** One word rule: what it's called in a report, and what it matches in visible text. */
interface WordRule {
  rule: string
  pattern: RegExp
}

/** macOS words that never belong in a Windows or Linux branch. */
const MACOS_WORDS: readonly WordRule[] = [
  { rule: 'macOS word “Finder”', pattern: /\bFinder\b/ },
  { rule: 'macOS word “macOS”', pattern: /\bmacOS\b/ },
  { rule: 'macOS word “Mac”', pattern: /\bMacs?\b/ },
  { rule: 'macOS word “Quick Look”', pattern: /\bQuick Look\b/i },
  { rule: 'macOS word “Spotlight”', pattern: /\bSpotlight\b/ },
  { rule: 'macOS word “Keychain”', pattern: /\bKeychain\b/ },
  { rule: 'macOS word “Dock”', pattern: /\bDock\b/ },
  { rule: 'macOS key glyph (⌘, ⌥, or ⌃)', pattern: /[⌘⌥⌃]/ },
]

const WINDOWS_WRONG_WORDS: readonly WordRule[] = [
  { rule: '“Trash” on Windows: “Delete” in a label, “the Recycle Bin” in a sentence', pattern: /\btrash\b/i },
  { rule: '“Quit” on Windows: “Exit” for the action, “close” in narration', pattern: /\bquit(s|ting)?\b/i },
  { rule: '“Terminal” on Windows: “PowerShell”', pattern: /\bTerminal\b/ },
]

const LINUX_WRONG_WORDS: readonly WordRule[] = [
  { rule: '“Recycle Bin” on Linux: “Trash”', pattern: /\brecycle bin\b/i },
  { rule: '“disconnect” on Linux: a network mount is unmounted', pattern: /\bdisconnect\b/i },
]

/** The house style, for text a row adds (`docs/style-guide.md`). */
const STYLE_WORDS: readonly WordRule[] = [
  { rule: 'style: no “error”', pattern: /\berrors?\b/i },
  { rule: 'style: no “failed”', pattern: /\bfailed\b/i },
  { rule: 'style: no “just”, “simple”, or “easy”', pattern: /\b(just|simple|simply|easy|easily)\b/i },
  { rule: 'style: no em dash', pattern: /—/ },
  { rule: 'style: “for example”, not “e.g.”', pattern: /\be\.g\./i },
]

/** Microsoft's own wording, for text a row adds to a Windows branch. */
const WINDOWS_STYLE_WORDS: readonly WordRule[] = [
  { rule: 'Windows style: “select”, not “click”', pattern: /(?<!right-|double-)\bclick/i },
  { rule: 'Windows style: “turn on”, not “enable”', pattern: /\benabl(e|es|ed|ing)\b/i },
  { rule: 'Windows style: “drive”, not “disk”, for the PC’s storage', pattern: /\bdisks?\b(?! (image|space))/i },
]

/** A straight quote outside a backticked code span, where it's literal code a user types. */
const STRAIGHT_QUOTE = /['"]/
const CODE_SPAN = /`[^`]*`/g

/** One lint finding on a row. */
export interface LintHit {
  key: string
  branch: 'windows' | 'other'
  rule: string
}

function firstMatches(rules: readonly WordRule[], text: string): string[] {
  return rules.filter(({ pattern }) => pattern.test(text)).map(({ rule }) => rule)
}

/** The style findings on a branch the row wrote. */
function styleHits(branch: 'windows' | 'other', text: string): string[] {
  const rules = [
    ...firstMatches(STYLE_WORDS, text),
    ...(branch === 'windows' ? firstMatches(WINDOWS_STYLE_WORDS, text) : []),
  ]
  if (STRAIGHT_QUOTE.test(text.replaceAll(CODE_SPAN, ' '))) {
    rules.push('style: curly quotes and ’ in prose, never a straight one')
  }
  if (branch === 'windows' && /\bPC\b/.test(text) && /\bcomputer\b/i.test(text)) {
    rules.push('Windows style: never “PC” and “computer” in one string')
  }
  return rules
}

/**
 * Every lint finding on a row's `windows` and `other` branches, exceptions not
 * applied.
 * @param oldEnglish the English the row's keys hold today (both keys of a pair, or the block already applied): a branch equal to any of it isn't new
 */
export function lintRow(row: ApplyRow, oldEnglish: readonly string[]): LintHit[] {
  const held = new Set(oldEnglish.flatMap((value) => platformBranches(value).map(({ text }) => text)))
  const hits: LintHit[] = []
  for (const branch of ['windows', 'other'] as const) {
    const raw = row.branches[branch]
    const text = visibleText(row.key, raw)
    const isNew = !held.has(raw) && raw !== row.branches.macos
    const rules = [
      ...firstMatches(MACOS_WORDS, text),
      ...SYSTEM_TOKENS.filter((token) => raw.includes(token)).map((token) => `macOS-only token ${token}`),
      ...firstMatches(branch === 'windows' ? WINDOWS_WRONG_WORDS : LINUX_WRONG_WORDS, text),
      ...(isNew ? styleHits(branch, text) : []),
    ]
    for (const rule of rules) hits.push({ key: row.key, branch, rule })
  }
  return hits
}

/** The lint findings that block a row: every hit on a branch it has no `lintExceptions` entry for. */
export function blockingLintHits(row: ApplyRow, oldEnglish: readonly string[]): LintHit[] {
  const excepted = new Set((row.lintExceptions ?? []).map(({ branch }) => branch))
  return lintRow(row, oldEnglish).filter(({ branch }) => !excepted.has(branch))
}

/** The macOS vocabulary the scan looks for (the plan's § "Copy resolution" list, plus the Windows-style words). */
const SCAN_WORDS: readonly { term: string; pattern: RegExp }[] = [
  { term: 'Quit', pattern: /\bquit(s|ting)?\b/i },
  { term: 'Finder', pattern: /\bFinder\b/ },
  { term: 'macOS', pattern: /\bmacOS\b/ },
  { term: 'Mac', pattern: /\bMacs?\b/ },
  { term: 'Trash', pattern: /\btrash\b/i },
  { term: 'Keychain', pattern: /\bkeychain\b/i },
  { term: 'Spotlight', pattern: /\bSpotlight\b/ },
  { term: 'Quick Look', pattern: /\bquick look\b/i },
  { term: 'Dock', pattern: /\bDock\b/ },
  { term: 'System Settings', pattern: /\bSystem Settings\b/ },
  { term: 'home folder', pattern: /\bhome folder\b/i },
  { term: 'Terminal', pattern: /\bTerminal\b/ },
  { term: 'Get Info', pattern: /\bget info\b/i },
  { term: 'click', pattern: /(?<!right-|double-)\bclick/i },
  { term: 'enable', pattern: /\benabl(e|es|ed|ing)\b/i },
  { term: 'disk', pattern: /\bdisks?\b(?! (image|space))/i },
]

/** One English value the scan found: its key and the vocabulary it uses. */
export interface ScanHit {
  key: string
  terms: string[]
}

/** The macOS vocabulary an English value uses, in visible text or as a `{system_settings}`-style token. */
export function macVocabularyIn(key: string, value: string): string[] {
  const text = visibleText(key, value)
  return [
    ...SCAN_WORDS.filter(({ pattern }) => pattern.test(text)).map(({ term }) => term),
    ...SYSTEM_TOKENS.filter((token) => value.includes(token)),
  ]
}

/**
 * Every English value that uses macOS vocabulary and that nobody judged: in no
 * row of the resolved copy (whatever its status) and behind no gate.
 * @param english the `en` messages to scan (one catalog's, or all of them)
 */
export function scanEnglish(english: Record<string, string>, file: CopyFinal): ScanHit[] {
  const judged = new Set(file.rows.flatMap(rowKeys))
  const hits: ScanHit[] = []
  for (const [key, value] of Object.entries(english)) {
    if (judged.has(key) || file.gated.some(({ keys }) => matchesKeyPattern(keys, key))) continue
    const terms = macVocabularyIn(key, value)
    if (terms.length > 0) hits.push({ key, terms })
  }
  return hits
}
