/**
 * The wrong-OS-word net over English platform blocks: no `windows` or `other`
 * branch of an `en` value names a macOS thing (Finder, Keychain, ⌘, a
 * `{system_settings}`-style token), and neither borrows the other's words
 * (Trash, Quit, or Terminal on Windows; Recycle Bin or "disconnect" on Linux).
 * The words are `docs/style-guide.md` § "Platform vocabulary". Reads the catalogs
 * from disk, so it guards any hand edit of an `en` block, the way a per-OS string
 * changes (`src/lib/intl/DETAILS.md` § Platform variants).
 *
 * Only blocks are read for words: a plain value shows the same text on every OS,
 * and the ones still in macOS words are `docs/notes/windows-port/copy-decisions.md`
 * § Open rows. A branch that keeps a macOS word on purpose goes in `EXCEPTIONS`,
 * with the reason.
 *
 * Key glyphs are stricter: ⌘, ⌥, ⌃, and ⇧ also never appear in a plain value
 * (`macKeyGlyphsInPlainValue`). A key in a sentence is a `ShortcutChip` tag or a
 * `{combo}` param fed by `shortcutAsText`, which spell it per OS and follow a
 * rebind; a typed glyph shows a Mac key to Windows and Linux users and goes stale
 * the moment the user rebinds. A sentence that names the glyphs themselves keeps
 * them in a `macos` branch.
 */

import { describe, expect, it } from 'vitest'
import { SYSTEM_TOKENS, loadCatalog, platformBranches, visibleText } from './i18n-catalog-lib.ts'

/** The macOS modifier glyphs. Off macOS the same keys are words (Ctrl, Alt, Shift, Win or Super). */
const MAC_KEY_GLYPH = /[⌘⌥⌃⇧]/u

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
  { rule: 'macOS key glyph (⌘, ⌥, ⌃, or ⇧)', pattern: MAC_KEY_GLYPH },
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

/** `<key> <branch>` → why that branch keeps a word the rules flag. */
const EXCEPTIONS: Readonly<Record<string, string>> = {
  'errors.listing.authRequiredEneedauth.suggestion other':
    'unreachable on Linux (ENEEDAUTH is a BSD errno), so it keeps the macOS text',
  'transfer.metadataClass.extendedAttributes windows':
    'open on Windows (no Windows metadata bodies yet), so it keeps the macOS text until they land',
}

/** Every wrong-OS word in `value`'s `windows` and `other` branches, as `<key> <branch>: <rule>`. */
function wrongOsWords(key: string, value: string): string[] {
  const hits: string[] = []
  for (const { name, text } of platformBranches(value)) {
    if (name !== 'windows' && name !== 'other') continue
    const visible = visibleText(key, text)
    const rules = [
      ...MACOS_WORDS.filter(({ pattern }) => pattern.test(visible)).map(({ rule }) => rule),
      ...SYSTEM_TOKENS.filter((token) => text.includes(token)).map((token) => `macOS-only token ${token}`),
      ...(name === 'windows' ? WINDOWS_WRONG_WORDS : LINUX_WRONG_WORDS)
        .filter(({ pattern }) => pattern.test(visible))
        .map(({ rule }) => rule),
    ]
    for (const rule of rules) hits.push(`${key} ${name}: ${rule}`)
  }
  return hits
}

/** `[key]` when a plain (block-less) `value` shows a macOS key glyph to every OS, else `[]`. */
function macKeyGlyphsInPlainValue(key: string, value: string): string[] {
  return platformBranches(value).some(({ name, text }) => name === null && MAC_KEY_GLYPH.test(visibleText(key, text)))
    ? [key]
    : []
}

describe('macKeyGlyphsInPlainValue', () => {
  it('flags a typed macOS key glyph in a plain value', () => {
    expect(macKeyGlyphsInPlainValue('a.b', 'Copy files first with ⌘C.')).toEqual(['a.b'])
    expect(macKeyGlyphsInPlainValue('a.b', 'Press ⇧Space for Quick Look.')).toEqual(['a.b'])
  })

  it('leaves a key cap tag, a combo param, and a platform block alone', () => {
    expect(macKeyGlyphsInPlainValue('a.b', 'Copy files first with <key></key>.')).toEqual([])
    expect(macKeyGlyphsInPlainValue('a.b', 'Copy files first with {combo}.')).toEqual([])
    const block = '{platform, select, macos {Add a modifier (⌘ or ⇧)} other {Add a modifier (Ctrl or Shift)}}'
    expect(macKeyGlyphsInPlainValue('a.b', block)).toEqual([])
  })
})

describe('wrongOsWords', () => {
  it('flags a macOS word in a Windows branch and a Windows word in a Linux one', () => {
    const value = '{platform, select, macos {Show in Finder} windows {Show in Finder} other {Move to the Recycle Bin}}'
    expect(wrongOsWords('commands.x.label', value)).toEqual([
      'commands.x.label windows: macOS word “Finder”',
      'commands.x.label other: “Recycle Bin” on Linux: “Trash”',
    ])
  })

  it('reads a raw block branch by branch and catches a macOS-only token', () => {
    const value = '{platform, select, macos {Open {system_settings}.} other {Open {system_settings}.}}'
    expect(wrongOsWords('errors.x.suggestion', value)).toEqual([
      'errors.x.suggestion other: macOS-only token {system_settings}',
    ])
  })

  it('leaves the macOS branch, a branch in its own words, and plain values alone', () => {
    const value = '{platform, select, macos {Show in Finder} windows {Move to the Recycle Bin} other {Move to Trash}}'
    expect(wrongOsWords('a.b', value)).toEqual([])
    expect(wrongOsWords('a.b', 'Show in Finder')).toEqual([])
  })
})

describe('the en catalog', () => {
  it('has no wrong-OS word in a Windows or Linux branch', () => {
    const { messages } = loadCatalog('en')
    const hits = Object.entries(messages)
      .flatMap(([key, value]) => wrongOsWords(key, value))
      .filter((hit) => !Object.hasOwn(EXCEPTIONS, hit.slice(0, hit.indexOf(':'))))
    expect(hits).toEqual([])
  })

  it('types no macOS key glyph into a plain value', () => {
    const { messages } = loadCatalog('en')
    expect(Object.entries(messages).flatMap(([key, value]) => macKeyGlyphsInPlainValue(key, value))).toEqual([])
  })

  it('names only live branches in its exceptions', () => {
    const { messages } = loadCatalog('en')
    const flagged = new Set(
      Object.entries(messages).flatMap(([key, value]) =>
        wrongOsWords(key, value).map((hit) => hit.slice(0, hit.indexOf(':'))),
      ),
    )
    expect(Object.keys(EXCEPTIONS).filter((entry) => !flagged.has(entry))).toEqual([])
  })
})
