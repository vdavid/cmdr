/**
 * The app-path grammar, driven by the case table Rust's suite reads too
 * (`crates/cmdr-fs/src/volume/path_grammar_cases.json`), so the two languages
 * can't drift apart. A new case goes into that file, ❌ never into one suite.
 */
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import {
  basenameInVolume,
  fromUserInput,
  hostPathFlavor,
  isLocalPath,
  isVolumeRoot,
  joinPath,
  lastName,
  normalizePath,
  parentInVolume,
  parentOrRoot,
  parsePath,
  pathRelativeTo,
  rootOf,
  samePath,
  shownPath,
  shownSeparator,
  volumePrefix,
  type PathFlavor,
} from './grammar'

interface GrammarCase {
  path: string
  flavor: PathFlavor
  kind: 'posix' | 'drive' | 'unc' | 'scheme' | null
  root: string | null
  normalized: string | null
  isRoot: boolean | null
  parent: string | null
  basename: string | null
  segments: string[] | null
  shown: string
}

interface UserInputCase {
  text: string
  flavor: PathFlavor
  wire: string
}

const table = JSON.parse(
  readFileSync(resolve(process.cwd(), '../../crates/cmdr-fs/src/volume/path_grammar_cases.json'), 'utf8'),
) as { cases: GrammarCase[]; userInput: UserInputCase[] }

const label = (c: { flavor: string; path?: string; text?: string }) => `${c.flavor} ${JSON.stringify(c.path ?? c.text)}`

describe('the shared grammar table', () => {
  it.each(table.cases.map((c) => [label(c), c] as const))('%s', (_name, c) => {
    const parsed = parsePath(c.path, c.flavor)
    expect(parsed?.kind ?? null).toBe(c.kind)
    expect(rootOf(c.path, c.flavor)).toBe(c.root)
    expect(normalizePath(c.path, c.flavor)).toBe(c.normalized)
    expect(parsed?.names ?? null).toEqual(c.segments)
    expect(isLocalPath(c.path, c.flavor)).toBe(c.kind !== null && c.kind !== 'scheme')
    if (c.isRoot !== null) expect(isVolumeRoot(c.path, c.flavor)).toBe(c.isRoot)
    expect(parentInVolume(c.path, c.flavor)).toBe(c.parent)
    expect(basenameInVolume(c.path, c.flavor)).toBe(c.basename)
    expect(shownPath(c.path, c.flavor)).toBe(c.shown)
  })

  it.each(table.userInput.map((c) => [label(c), c] as const))('user input %s', (_name, c) => {
    expect(fromUserInput(c.text, c.flavor)).toBe(c.wire)
  })
})

describe('hostPathFlavor', () => {
  it('is windows only on Windows', () => {
    onTestFinished(withPlatform('windows'))
    expect(hostPathFlavor()).toBe('windows')
  })

  it.each(['macos', 'linux'] as const)('is posix on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(hostPathFlavor()).toBe('posix')
  })

  it('makes the helpers follow the platform when no flavor is passed', () => {
    onTestFinished(withPlatform('windows'))
    expect(parentInVolume('C:/Users')).toBe('C:/')
    expect(shownPath('C:/Users')).toBe('C:\\Users')
    onTestFinished(withPlatform('macos'))
    expect(parentInVolume('C:/Users')).toBeNull()
    expect(shownPath('C:/Users')).toBe('C:/Users')
  })
})

describe('joinPath', () => {
  it.each([
    ['/', 'Users', '/Users'],
    ['/Users', 'test_user_1', '/Users/test_user_1'],
    ['C:/', 'Users', 'C:/Users'],
    ['C:/Users', 'test_user_1', 'C:/Users/test_user_1'],
    ['//server/share/', 'x', '//server/share/x'],
    ['sftp://test_user_1@testnas.local:22/', 'home', 'sftp://test_user_1@testnas.local:22/home'],
    ['mtp://dev/65537', 'Music', 'mtp://dev/65537/Music'],
    ['/Users', 'a/b', '/Users/a/b'],
  ])('%s + %s = %s', (base, name, joined) => {
    expect(joinPath(base, name)).toBe(joined)
  })
})

describe('pathRelativeTo', () => {
  it('answers the part below the ancestor, by whole components', () => {
    expect(pathRelativeTo('/Users/test_user_1/x', '/Users/test_user_1')).toBe('x')
    expect(pathRelativeTo('/Users/test_user_1', '/Users/test_user_1')).toBe('')
    expect(pathRelativeTo('/Users/test_user_1-1', '/Users/test_user_1')).toBeNull()
    expect(pathRelativeTo('/Users/test_user_1', '/')).toBe('Users/test_user_1')
    expect(pathRelativeTo('C:/Users/test_user_1', 'C:/')).toBe('Users/test_user_1')
    expect(pathRelativeTo('//server/share/x/y', '//server/share/')).toBe('x/y')
    expect(pathRelativeTo('//server/share-1/x', '//server/share')).toBeNull()
  })

  it('folds case only on a case-insensitive volume', () => {
    expect(pathRelativeTo('C:/users/TEST_USER_1/x', 'C:/Users/test_user_1')).toBeNull()
    expect(pathRelativeTo('C:/users/TEST_USER_1/x', 'C:/Users/test_user_1', { caseSensitive: false })).toBe('x')
    expect(pathRelativeTo('//SERVER/Share/x', '//server/share/', { caseSensitive: false })).toBe('x')
  })
})

describe('samePath', () => {
  it('compares exactly on a case-sensitive volume', () => {
    expect(samePath('/Users/test_user_1', '/Users/test_user_1', { caseSensitive: true })).toBe(true)
    expect(samePath('/Users/test_user_1', '/users/test_user_1', { caseSensitive: true })).toBe(false)
  })

  it('folds case and Unicode form on a case-insensitive one', () => {
    expect(samePath('C:/Users/Test_user_1', 'C:/users/test_user_1', { caseSensitive: false })).toBe(true)
    expect(samePath('/Caf\u00e9', '/cafe\u0301', { caseSensitive: false })).toBe(true)
    expect(samePath('C:/Users/Test_user_1', 'C:/Users/Bea', { caseSensitive: false })).toBe(false)
  })
})

describe('shownSeparator', () => {
  it('is a backslash only for a Windows local path on Windows', () => {
    expect(shownSeparator('C:/Users', 'windows')).toBe('\\')
    expect(shownSeparator('//server/share/x', 'windows')).toBe('\\')
    expect(shownSeparator('sftp://test_user_1@testnas.local:22/home', 'windows')).toBe('/')
    expect(shownSeparator('/Users/test_user_1', 'posix')).toBe('/')
  })
})

describe('volumePrefix', () => {
  it('drops a drive or share root its slash, and leaves everything else', () => {
    expect(volumePrefix('C:/', 'windows')).toBe('C:')
    expect(volumePrefix('//server/share/', 'windows')).toBe('//server/share')
    expect(volumePrefix('//server/share', 'windows')).toBe('//server/share')
    expect(volumePrefix('C:/Users', 'windows')).toBe('C:/Users')
    expect(volumePrefix('/', 'posix')).toBe('/')
    expect(volumePrefix('/Volumes/testshare', 'posix')).toBe('/Volumes/testshare')
    expect(volumePrefix('sftp://test_user_1@testnas.local:22/', 'windows')).toBe('sftp://test_user_1@testnas.local:22/')
  })
})

describe('parentOrRoot and lastName', () => {
  it.each([
    ['/Users/test_user_1', 'posix', '/Users', 'test_user_1'],
    ['/Users', 'posix', '/', 'Users'],
    ['/', 'posix', '/', ''],
    ['name.txt', 'posix', '/', 'name.txt'],
    ['mtp://dev/65537/Music', 'posix', 'mtp://dev/65537', 'Music'],
    ['C:/Users/test_user_1', 'windows', 'C:/Users', 'test_user_1'],
    ['C:/Users', 'windows', 'C:/', 'Users'],
    ['C:/', 'windows', 'C:/', ''],
    ['//server/share/x', 'windows', '//server/share/', 'x'],
    ['//server/share', 'windows', '//server/share/', ''],
    ['C:/Users', 'posix', 'C:', 'Users'],
  ] as const)('%s (%s): parent %s, name %s', (path, flavor, parent, name) => {
    expect(parentOrRoot(path, flavor)).toBe(parent)
    expect(lastName(path, flavor)).toBe(name)
  })
})
