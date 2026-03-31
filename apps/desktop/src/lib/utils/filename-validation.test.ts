import { describe, it, expect } from 'vitest'
import {
  validateDisallowedChars,
  validateNotEmpty,
  validateNameLength,
  validatePathLength,
  validateDirectoryPath,
  validateExtensionChange,
  validateConflict,
  validateFilename,
  getExtension,
} from './filename-validation'

describe('validateDisallowedChars', () => {
  it('allows normal filenames', () => {
    expect(validateDisallowedChars('hello.txt')).toEqual({ severity: 'ok', message: '' })
  })

  it('rejects slash', () => {
    const result = validateDisallowedChars('foo/bar')
    expect(result.severity).toBe('error')
  })

  it('rejects null character', () => {
    const result = validateDisallowedChars('foo\0bar')
    expect(result.severity).toBe('error')
  })

  it('allows dots, spaces, dashes, underscores', () => {
    expect(validateDisallowedChars('my file-name_v2.0.txt').severity).toBe('ok')
  })

  it('uses folder label when isDir is true', () => {
    const result = validateDisallowedChars('foo/bar', true)
    expect(result.message).toContain('Folder name')
  })
})

describe('validateNotEmpty', () => {
  it('rejects empty string', () => {
    expect(validateNotEmpty('').severity).toBe('error')
  })

  it('rejects whitespace-only', () => {
    expect(validateNotEmpty('   ').severity).toBe('error')
  })

  it('allows non-empty string', () => {
    expect(validateNotEmpty('a').severity).toBe('ok')
  })
})

describe('validateNameLength', () => {
  it('allows short names', () => {
    expect(validateNameLength('hello.txt').severity).toBe('ok')
  })

  it('rejects names at 255 bytes', () => {
    const longName = 'a'.repeat(255)
    expect(validateNameLength(longName).severity).toBe('error')
  })

  it('allows names just under 255 bytes', () => {
    const name = 'a'.repeat(254)
    expect(validateNameLength(name).severity).toBe('ok')
  })

  it('counts multi-byte characters correctly', () => {
    // Each emoji is 4 bytes in UTF-8
    const emojiName = '\u{1F600}'.repeat(64) // 256 bytes
    expect(validateNameLength(emojiName).severity).toBe('error')
  })
})

describe('validatePathLength', () => {
  it('allows short paths', () => {
    expect(validatePathLength('/Users/test', 'file.txt').severity).toBe('ok')
  })

  it('rejects paths at 1024 bytes', () => {
    const parent = '/a'.repeat(500) // 1000 bytes
    expect(validatePathLength(parent, 'a'.repeat(30)).severity).toBe('error')
  })

  it('handles trailing slash in parent', () => {
    expect(validatePathLength('/Users/test/', 'file.txt').severity).toBe('ok')
  })
})

describe('validateDirectoryPath', () => {
  it('rejects empty string', () => {
    const result = validateDirectoryPath('')
    expect(result.severity).toBe('error')
    expect(result.message).toBe("Path can't be empty")
  })

  it('rejects whitespace-only string', () => {
    const result = validateDirectoryPath('   ')
    expect(result.severity).toBe('error')
    expect(result.message).toBe("Path can't be empty")
  })

  it('rejects relative path', () => {
    const result = validateDirectoryPath('Documents/folder')
    expect(result.severity).toBe('error')
    expect(result.message).toBe('Path must be absolute (start with /)')
  })

  it('rejects path with null byte', () => {
    const result = validateDirectoryPath('/Users/test\0/folder')
    expect(result.severity).toBe('error')
    expect(result.message).toBe('Path contains a null character')
  })

  it('rejects path at 1024 bytes', () => {
    const longPath = '/' + 'a'.repeat(1023)
    const result = validateDirectoryPath(longPath)
    expect(result.severity).toBe('error')
    expect(result.message).toMatch(/Path is too long/)
  })

  it('rejects path with a component at 255 bytes', () => {
    const longComponent = 'a'.repeat(255)
    const result = validateDirectoryPath(`/Users/${longComponent}/folder`)
    expect(result.severity).toBe('error')
    expect(result.message).toMatch(/A folder name in the path is too long/)
  })

  it('allows valid absolute path', () => {
    expect(validateDirectoryPath('/Users/test/Documents').severity).toBe('ok')
  })

  it('allows root path', () => {
    expect(validateDirectoryPath('/').severity).toBe('ok')
  })

  it('handles trailing slashes', () => {
    expect(validateDirectoryPath('/Users/test/').severity).toBe('ok')
  })

  it('handles double slashes', () => {
    expect(validateDirectoryPath('/Users//test').severity).toBe('ok')
  })

  it('counts multi-byte characters correctly', () => {
    // Each emoji is 4 bytes
    const emojiDir = '\u{1F600}'.repeat(64) // 256 bytes
    const result = validateDirectoryPath(`/Users/${emojiDir}`)
    expect(result.severity).toBe('error')
    expect(result.message).toMatch(/A folder name in the path is too long/)
  })

  it('allows path just under 1024 bytes', () => {
    // Build a long path from many short segments to avoid per-component limit
    const segment = 'a'.repeat(100)
    const path = ('/' + segment).repeat(10) + '/' + 'b'.repeat(12) // 10 * 101 + 13 = 1023 bytes
    expect(validateDirectoryPath(path).severity).toBe('ok')
  })
})

describe('getExtension', () => {
  it('returns extension with dot', () => {
    expect(getExtension('file.txt')).toBe('.txt')
  })

  it('returns empty for no extension', () => {
    expect(getExtension('Makefile')).toBe('')
  })

  it('returns empty for hidden files without ext', () => {
    expect(getExtension('.gitignore')).toBe('')
  })

  it('returns last extension for double extensions', () => {
    expect(getExtension('archive.tar.gz')).toBe('.gz')
  })

  it('handles hidden files with extensions', () => {
    expect(getExtension('.config.json')).toBe('.json')
  })
})

describe('validateExtensionChange', () => {
  it('allows when setting is yes', () => {
    expect(validateExtensionChange('file.txt', 'file.md', 'yes').severity).toBe('ok')
  })

  it('errors when setting is no and ext changed', () => {
    const result = validateExtensionChange('file.txt', 'file.md', 'no')
    expect(result.severity).toBe('error')
  })

  it('allows when setting is no but ext unchanged', () => {
    expect(validateExtensionChange('file.txt', 'renamed.txt', 'no').severity).toBe('ok')
  })

  it('allows when setting is ask (dialog handles it)', () => {
    expect(validateExtensionChange('file.txt', 'file.md', 'ask').severity).toBe('ok')
  })
})

describe('validateConflict', () => {
  it('warns on case-insensitive match with a different sibling', () => {
    const result = validateConflict('README.md', ['readme.md', 'other.txt'], 'old.txt')
    expect(result.severity).toBe('warning')
  })

  it('no warning for case-only rename of same file', () => {
    const result = validateConflict('README.md', ['readme.md'], 'readme.md')
    expect(result.severity).toBe('ok')
  })

  it('no warning when no conflict', () => {
    const result = validateConflict('unique.txt', ['other.txt', 'file.md'], 'original.txt')
    expect(result.severity).toBe('ok')
  })

  it('trims before comparing', () => {
    const result = validateConflict('  conflict.txt  ', ['conflict.txt'], 'original.txt')
    expect(result.severity).toBe('warning')
  })
})

describe('validateFilename', () => {
  const parentPath = '/Users/test'
  const siblings: string[] = []

  it('returns ok for valid rename', () => {
    const result = validateFilename('newname.txt', 'old.txt', parentPath, siblings, 'yes')
    expect(result.severity).toBe('ok')
  })

  it('returns error for empty name', () => {
    const result = validateFilename('', 'old.txt', parentPath, siblings, 'yes')
    expect(result.severity).toBe('error')
  })

  it('returns error for disallowed chars', () => {
    const result = validateFilename('bad/name', 'old.txt', parentPath, siblings, 'yes')
    expect(result.severity).toBe('error')
  })

  it('returns warning for conflict', () => {
    const result = validateFilename('conflict.txt', 'old.txt', parentPath, ['conflict.txt'], 'yes')
    expect(result.severity).toBe('warning')
  })

  it('returns first error before warnings', () => {
    // Empty AND conflicting — error takes precedence
    const result = validateFilename('', 'old.txt', parentPath, [''], 'yes')
    expect(result.severity).toBe('error')
  })

  it('validates extension change with no setting', () => {
    const result = validateFilename('file.md', 'file.txt', parentPath, siblings, 'no')
    expect(result.severity).toBe('error')
  })
})
