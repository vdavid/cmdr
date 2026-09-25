/**
 * Tests for `i18n-brief-history.ts`: the English a stale translation was made
 * from, found in git by its `@key.sourceHash`, and the word diff the brief shows.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { sourceHash } from './i18n-catalog-lib.ts'
import { previousEnglish } from './i18n-brief-history.ts'
import { wordDiff } from './i18n-brief-lib.ts'

describe('wordDiff', () => {
  it('marks removed and added words, keeping the shared ones plain', () => {
    expect(wordDiff('Delete the selected file', 'Delete the selected files now')).toBe(
      'Delete the selected [-file-] {+files now+}',
    )
  })
  it('keeps placeholders and punctuation as words', () => {
    expect(wordDiff('Copy {count} files.', 'Copy {countText} files…')).toBe(
      'Copy [-{count} files.-] {+{countText} files…+}',
    )
  })
  it('returns the text itself when nothing changed', () => {
    expect(wordDiff('Same', 'Same')).toBe('Same')
  })
})

describe('previousEnglish (a git fixture)', () => {
  let root: string
  let messagesRoot: string
  const git = (...args: string[]) => execFileSync('git', ['-C', root, ...args], { stdio: 'ignore' })
  const write = (path: string, data: unknown) => {
    mkdirSync(join(path, '..'), { recursive: true })
    writeFileSync(path, JSON.stringify(data, null, 2))
  }
  const commit = (message: string) => {
    git('add', '-A')
    git('-c', 'user.name=t', '-c', 'user.email=t@example.com', 'commit', '-q', '-m', message)
  }

  beforeAll(() => {
    root = mkdtempSync(join(tmpdir(), 'brief-history-'))
    messagesRoot = join(root, 'messages')
    git('init', '-q')
    write(join(messagesRoot, 'en', 'a.json'), { 'a.one': 'Delete the file', 'a.two': 'Keep it' })
    commit('first')
    write(join(messagesRoot, 'en', 'a.json'), { 'a.one': 'Delete the file forever', 'a.two': 'Keep it' })
    commit('second')
    write(join(messagesRoot, 'en', 'a.json'), { 'a.one': 'Delete the files forever', 'a.two': 'Keep it all' })
    commit('third')
    write(join(messagesRoot, 'sv', 'a.json'), {
      'a.one': 'Radera filen',
      '@a.one': { sourceHash: sourceHash('Delete the file') },
      'a.two': 'Behåll det',
      '@a.two': { sourceHash: sourceHash('Keep it all') },
    })
    commit('sv')
  })
  afterAll(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('finds the English a stale key was translated from, and skips a current one', () => {
    expect(previousEnglish({ keys: ['a.one', 'a.two'], langs: ['sv'], messagesRoot })).toEqual({
      'a.one': { sv: 'Delete the file' },
    })
  })

  it('says nothing for a key whose stamp matches no English in history', () => {
    write(join(messagesRoot, 'sv', 'a.json'), { 'a.one': 'x', '@a.one': { sourceHash: 'fffffff' } })
    expect(previousEnglish({ keys: ['a.one'], langs: ['sv'], messagesRoot })).toEqual({})
  })
})
