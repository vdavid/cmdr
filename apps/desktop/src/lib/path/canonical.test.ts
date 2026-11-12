import { describe, expect, it, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { basenameOf, isPathOnVolume, isPlainFilesystemPath, parentOf, toCanonical } from './canonical'

const HOME = '/Users/foo'

describe('toCanonical', () => {
  it('expands bare ~ to home dir', () => {
    expect(toCanonical('~', HOME)).toBe(HOME)
  })

  it('expands ~/sub to home + sub', () => {
    expect(toCanonical('~/Documents', HOME)).toBe('/Users/foo/Documents')
  })

  it('passes absolute POSIX paths through', () => {
    expect(toCanonical('/Users/foo/bar', HOME)).toBe('/Users/foo/bar')
    expect(toCanonical('/', HOME)).toBe('/')
  })

  it('passes virtual-volume URLs through', () => {
    expect(toCanonical('mtp://dev/65537/Music', HOME)).toBe('mtp://dev/65537/Music')
    expect(toCanonical('adb://R58M12345/sdcard/Music', HOME)).toBe('adb://R58M12345/sdcard/Music')
    expect(toCanonical('smb://host/share/dir', HOME)).toBe('smb://host/share/dir')
    expect(toCanonical('search-results://snap-1', HOME)).toBe('search-results://snap-1')
  })

  it('throws on relative paths', () => {
    expect(() => toCanonical('foo/bar', HOME)).toThrow(/not absolute/)
    expect(() => toCanonical('.', HOME)).toThrow(/not absolute/)
  })

  it('throws when expanding ~ without a home dir', () => {
    expect(() => toCanonical('~', '')).toThrow(/homeDir is empty/)
    expect(() => toCanonical('~/Docs', '')).toThrow(/homeDir is empty/)
  })
})

describe('parentOf', () => {
  it('returns the parent of an absolute path', () => {
    expect(parentOf(toCanonical('/Users/foo/bar', HOME))).toBe('/Users/foo')
    expect(parentOf(toCanonical('/Users', HOME))).toBe('/')
  })

  it('returns / for /', () => {
    expect(parentOf(toCanonical('/', HOME))).toBe('/')
  })

  // The bug this whole module exists to prevent: navigating `..` from `~`
  // used to land on `/` because `~.lastIndexOf('/')` is -1.
  it('returns /Users when called on canonicalised ~', () => {
    expect(parentOf(toCanonical('~', HOME))).toBe('/Users')
  })

  it('handles virtual-volume URLs by slash arithmetic', () => {
    expect(parentOf(toCanonical('mtp://dev/65537/Music', HOME))).toBe('mtp://dev/65537')
    expect(parentOf(toCanonical('adb://R58M12345/sdcard', HOME))).toBe('adb://R58M12345')
  })
})

describe('basenameOf', () => {
  it('returns the final segment', () => {
    expect(basenameOf(toCanonical('/Users/foo/bar', HOME))).toBe('bar')
    expect(basenameOf(toCanonical('/Users', HOME))).toBe('Users')
  })

  it('returns "" for /', () => {
    expect(basenameOf(toCanonical('/', HOME))).toBe('')
  })

  // Same root cause as the parentOf bug: `~.split('/').pop()` used to return '~'.
  it('returns the home folder name when called on canonicalised ~', () => {
    expect(basenameOf(toCanonical('~', HOME))).toBe('foo')
  })
})

describe('isPlainFilesystemPath', () => {
  it('accepts an absolute POSIX path', () => {
    expect(isPlainFilesystemPath('/Users/foo/bar.txt')).toBe(true)
    expect(isPlainFilesystemPath('/')).toBe(true)
  })

  it('rejects every virtual-volume URL, which no OS API can resolve', () => {
    expect(isPlainFilesystemPath('mtp://dev/65537/Music/a.mp3')).toBe(false)
    expect(isPlainFilesystemPath('adb://R58M12345/sdcard/a.jpg')).toBe(false)
    expect(isPlainFilesystemPath('smb://host/share/a.txt')).toBe(false)
    expect(isPlainFilesystemPath('search-results://snap-1')).toBe(false)
  })

  it('rejects ~-rooted and relative paths, which need a base the OS API lacks', () => {
    expect(isPlainFilesystemPath('~')).toBe(false)
    expect(isPlainFilesystemPath('~/Documents/a.txt')).toBe(false)
    expect(isPlainFilesystemPath('foo/bar')).toBe(false)
    expect(isPlainFilesystemPath('')).toBe(false)
  })
})

describe('isPathOnVolume', () => {
  it('accepts an exact match', () => {
    expect(isPathOnVolume('/Volumes/testshare', '/Volumes/testshare')).toBe(true)
  })

  it('accepts a descendant path', () => {
    expect(isPathOnVolume('/Volumes/testshare/photos/2024', '/Volumes/testshare')).toBe(true)
  })

  it('rejects a sibling whose name shares a prefix', () => {
    expect(isPathOnVolume('/Volumes/testshare-backup', '/Volumes/testshare')).toBe(false)
  })

  it('rejects a foreign path', () => {
    expect(isPathOnVolume('/Users/test/project', '/Volumes/testshare')).toBe(false)
  })

  it('treats root volume `/` as matching any absolute path', () => {
    expect(isPathOnVolume('/Users/test', '/')).toBe(true)
    expect(isPathOnVolume('/', '/')).toBe(true)
  })

  it('handles a volumePath that already ends in `/`', () => {
    expect(isPathOnVolume('smb://host/share/foo', 'smb://')).toBe(true)
    expect(isPathOnVolume('/Users/test', 'smb://')).toBe(false)
  })

  // A remote volume rooted at `/` carries a trailing slash in its own root, so
  // this is the everyday shape for SFTP, WebDAV, and ADB, not a curiosity.
  it('accepts a descendant of a remote volume rooted at /', () => {
    expect(
      isPathOnVolume('sftp://test_user_1@testnas.local:22/home/test_user_1', 'sftp://test_user_1@testnas.local:22/'),
    ).toBe(true)
  })

  it('rejects a sibling server-side root that merely shares a prefix', () => {
    expect(
      isPathOnVolume('sftp://test_user_1@testnas.local:22/srv/data-1', 'sftp://test_user_1@testnas.local:22/srv/data'),
    ).toBe(false)
  })
})

describe('on Windows', () => {
  const WIN_HOME = 'C:/Users/test_user_1'

  it('admits drive and UNC paths, and no bare POSIX one', () => {
    onTestFinished(withPlatform('windows'))
    expect(toCanonical('C:/Users/test_user_1/x', WIN_HOME)).toBe('C:/Users/test_user_1/x')
    expect(toCanonical('//server/share/x', WIN_HOME)).toBe('//server/share/x')
    expect(toCanonical('~/Documents', WIN_HOME)).toBe('C:/Users/test_user_1/Documents')
    expect(toCanonical('mtp://dev/65537/Music', WIN_HOME)).toBe('mtp://dev/65537/Music')
    expect(() => toCanonical('/Users/test_user_1', WIN_HOME)).toThrow(/not absolute/)
    expect(() => toCanonical('C:\\Users', WIN_HOME)).toThrow(/not absolute/)
  })

  it('stops parentOf at the volume root, a root being its own parent', () => {
    onTestFinished(withPlatform('windows'))
    expect(parentOf(toCanonical('C:/Users', WIN_HOME))).toBe('C:/')
    expect(parentOf(toCanonical('C:/', WIN_HOME))).toBe('C:/')
    expect(parentOf(toCanonical('//server/share/x', WIN_HOME))).toBe('//server/share/')
    expect(parentOf(toCanonical('//server/share', WIN_HOME))).toBe('//server/share/')
    expect(parentOf(toCanonical('~', WIN_HOME))).toBe('C:/Users')
  })

  it('answers no basename at a root', () => {
    onTestFinished(withPlatform('windows'))
    expect(basenameOf(toCanonical('C:/', WIN_HOME))).toBe('')
    expect(basenameOf(toCanonical('//server/share/', WIN_HOME))).toBe('')
    expect(basenameOf(toCanonical('C:/Users/test_user_1', WIN_HOME))).toBe('test_user_1')
  })

  it('reads drive and UNC paths as OS-resolvable, and a bare /x as not', () => {
    onTestFinished(withPlatform('windows'))
    expect(isPlainFilesystemPath('C:/Users/test_user_1')).toBe(true)
    expect(isPlainFilesystemPath('//server/share/a.txt')).toBe(true)
    expect(isPlainFilesystemPath('/Users/test_user_1')).toBe(false)
    expect(isPlainFilesystemPath('smb://host/share/a.txt')).toBe(false)
  })

  it('places a path on its drive or share volume', () => {
    onTestFinished(withPlatform('windows'))
    expect(isPathOnVolume('C:/Users/test_user_1', 'C:/')).toBe(true)
    expect(isPathOnVolume('D:/x', 'C:/')).toBe(false)
    expect(isPathOnVolume('//server/share/x', '//server/share/')).toBe(true)
    expect(isPathOnVolume('//server/share-1/x', '//server/share')).toBe(false)
  })
})

describe('on macOS and Linux, the new roots stay unknown', () => {
  it.each(['macos', 'linux'] as const)('%s keeps a drive-shaped path relative', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(() => toCanonical('C:/Users', HOME)).toThrow(/not absolute/)
    expect(isPlainFilesystemPath('C:/Users')).toBe(false)
    expect(parentOf(toCanonical('//server/share', HOME))).toBe('//server')
  })
})

describe('isPathOnVolume with a case rule', () => {
  it('stays exact without a rule or on a case-sensitive volume', () => {
    expect(isPathOnVolume('/volumes/TESTSHARE/x', '/Volumes/testshare')).toBe(false)
    expect(isPathOnVolume('/volumes/TESTSHARE/x', '/Volumes/testshare', { caseSensitive: true })).toBe(false)
  })

  it('folds case on a case-insensitive volume, still by whole components', () => {
    expect(isPathOnVolume('/volumes/TESTSHARE/x', '/Volumes/testshare', { caseSensitive: false })).toBe(true)
    expect(isPathOnVolume('/volumes/TESTSHARE-1', '/Volumes/testshare', { caseSensitive: false })).toBe(false)
  })
})
