import { describe, it, expect } from 'vitest'
import {
    validateDisallowedChars,
    validateNotEmpty,
    validateNameLength,
    validatePathLength,
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
