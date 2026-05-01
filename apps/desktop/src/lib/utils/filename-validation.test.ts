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
  extensionsDifferMeaningfully,
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

describe('extensionsDifferMeaningfully', () => {
  it('detects a real extension change', () => {
    expect(extensionsDifferMeaningfully('file.txt', 'file.json')).toBe(true)
  })

  it('ignores case-only changes', () => {
    expect(extensionsDifferMeaningfully('photo.JPG', 'photo.jpg')).toBe(false)
    expect(extensionsDifferMeaningfully('archive.TAR.GZ', 'archive.TAR.gz')).toBe(false)
  })

  it('ignores no-op changes', () => {
    expect(extensionsDifferMeaningfully('file.txt', 'renamed.txt')).toBe(false)
  })

  it('ignores changes within the JPEG group', () => {
    expect(extensionsDifferMeaningfully('photo.jpeg', 'photo.jpg')).toBe(false)
    expect(extensionsDifferMeaningfully('photo.jpg', 'photo.jpe')).toBe(false)
    expect(extensionsDifferMeaningfully('photo.jfif', 'photo.JPEG')).toBe(false)
  })

  it('ignores changes within the TIFF group', () => {
    expect(extensionsDifferMeaningfully('scan.tif', 'scan.tiff')).toBe(false)
  })

  it('ignores changes within the HTML group', () => {
    expect(extensionsDifferMeaningfully('page.htm', 'page.html')).toBe(false)
  })

  it('ignores changes within the YAML group', () => {
    expect(extensionsDifferMeaningfully('config.yml', 'config.yaml')).toBe(false)
  })

  it('ignores changes within the MPEG group', () => {
    expect(extensionsDifferMeaningfully('clip.mpg', 'clip.mpeg')).toBe(false)
  })

  it('ignores changes within the MIDI group', () => {
    expect(extensionsDifferMeaningfully('song.mid', 'song.midi')).toBe(false)
  })

  it('ignores changes within the AIFF group', () => {
    expect(extensionsDifferMeaningfully('sound.aif', 'sound.aiff')).toBe(false)
  })

  it('ignores changes within the QuickTime group', () => {
    expect(extensionsDifferMeaningfully('movie.qt', 'movie.mov')).toBe(false)
  })

  it('ignores changes within the markdown/text group', () => {
    expect(extensionsDifferMeaningfully('notes.md', 'notes.txt')).toBe(false)
    expect(extensionsDifferMeaningfully('notes.markdown', 'notes.md')).toBe(false)
    expect(extensionsDifferMeaningfully('notes.TXT', 'notes.Markdown')).toBe(false)
  })

  it('flags changes that cross groups', () => {
    expect(extensionsDifferMeaningfully('photo.jpg', 'photo.tif')).toBe(true)
    expect(extensionsDifferMeaningfully('notes.md', 'notes.html')).toBe(true)
  })

  it('flags adding an extension to a name that had none', () => {
    expect(extensionsDifferMeaningfully('Makefile', 'Makefile.txt')).toBe(true)
  })

  it('flags removing an extension', () => {
    expect(extensionsDifferMeaningfully('readme.txt', 'readme')).toBe(true)
  })

  it('treats a no-extension to no-extension rename as no change', () => {
    expect(extensionsDifferMeaningfully('Makefile', 'Dockerfile')).toBe(false)
  })

  it('trims the new name before comparing', () => {
    expect(extensionsDifferMeaningfully('photo.jpg', '  photo.jpeg  ')).toBe(false)
  })

  it('treats a dotfile without extension as no extension', () => {
    // getExtension('.gitignore') === '', so renaming to .gitkeep is also no extension
    expect(extensionsDifferMeaningfully('.gitignore', '.gitkeep')).toBe(false)
  })

  it('uses only the last extension on multi-dot names', () => {
    expect(extensionsDifferMeaningfully('archive.tar.gz', 'archive.tar.bz2')).toBe(true)
    expect(extensionsDifferMeaningfully('photo.backup.jpeg', 'photo.backup.jpg')).toBe(false)
  })
})

describe('validateExtensionChange', () => {
  it('allows when setting is yes', () => {
    expect(validateExtensionChange('file.txt', 'file.json', 'yes').severity).toBe('ok')
  })

  it('errors when setting is no and ext changed', () => {
    const result = validateExtensionChange('file.txt', 'file.json', 'no')
    expect(result.severity).toBe('error')
  })

  it('allows when setting is no but ext unchanged', () => {
    expect(validateExtensionChange('file.txt', 'renamed.txt', 'no').severity).toBe('ok')
  })

  it('allows when setting is ask (dialog handles it)', () => {
    expect(validateExtensionChange('file.txt', 'file.json', 'ask').severity).toBe('ok')
  })

  it('allows case-only extension change when setting is no', () => {
    expect(validateExtensionChange('photo.JPG', 'photo.jpg', 'no').severity).toBe('ok')
  })

  it('still errors on real extension change when setting is no', () => {
    expect(validateExtensionChange('file.txt', 'file.json', 'no').severity).toBe('error')
  })

  it('allows equivalent-extension changes when setting is no', () => {
    expect(validateExtensionChange('photo.jpeg', 'photo.jpg', 'no').severity).toBe('ok')
    expect(validateExtensionChange('notes.md', 'notes.txt', 'no').severity).toBe('ok')
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
    const result = validateFilename('file.json', 'file.txt', parentPath, siblings, 'no')
    expect(result.severity).toBe('error')
  })
})
