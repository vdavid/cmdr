import { describe, it, expect, beforeEach, vi } from 'vitest'

// icon-cache imports these from ./tauri-commands at module load. We never call them
// in these tests (we drive the cache via the test-only helpers), but they must exist
// as mocks so the import resolves without a real Tauri runtime.
vi.mock('./tauri-commands', () => ({
  getIcons: vi.fn(),
  getCustomFolderIconIds: vi.fn(),
  refreshDirectoryIcons: vi.fn(),
  clearExtensionIconCache: vi.fn(),
  clearDirectoryIconCache: vi.fn(),
}))

import {
  getCachedIcon,
  evictPerPathIconsForDir,
  prefetchCustomFolderIcons,
  _resetIconCacheForTests,
  _applyIconsToCacheForTests,
  _pathKeyCapForTests,
} from './icon-cache'
import { getIcons, getCustomFolderIconIds } from './tauri-commands'

const STORAGE_KEY = 'cmdr-icon-cache'

function storedKeys(): string[] {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return []
  return Object.keys(JSON.parse(raw) as Record<string, string>)
}

describe('icon-cache path: key bounding', () => {
  beforeEach(() => {
    _resetIconCacheForTests()
    localStorage.clear()
  })

  it('does not persist path: keys to localStorage, but persists bounded keys', () => {
    _applyIconsToCacheForTests({
      dir: 'dir-url',
      'ext:txt': 'txt-url',
      'symlink-dir': 'symlink-url',
      'path:/Users/me/Folder': 'folder-url',
      'path:/Users/me/Other': 'other-url',
    })

    const keys = storedKeys()
    // path: keys are session-only.
    expect(keys).not.toContain('path:/Users/me/Folder')
    expect(keys).not.toContain('path:/Users/me/Other')
    expect(keys.some((k) => k.startsWith('path:'))).toBe(false)
    // Bounded keys still persist.
    expect(keys).toEqual(expect.arrayContaining(['dir', 'ext:txt', 'symlink-dir']))
  })

  it('keeps path: keys available in the in-memory cache (just not persisted)', () => {
    _applyIconsToCacheForTests({ 'path:/Users/me/Folder': 'folder-url' })
    expect(getCachedIcon('path:/Users/me/Folder')).toBe('folder-url')
  })

  it('LRU-caps path: keys in memory, evicting the oldest first', () => {
    const cap = _pathKeyCapForTests
    const icons: Record<string, string> = {}
    // Insert one more than the cap. Map preserves insertion order, so /folder/0 is oldest.
    for (let n = 0; n <= cap; n++) {
      icons[`path:/folder/${n}`] = `url-${n}`
    }
    // Apply one at a time so insertion order is deterministic (a single object would
    // preserve order anyway, but per-call mirrors real batched fetches).
    for (const [id, url] of Object.entries(icons)) {
      _applyIconsToCacheForTests({ [id]: url })
    }

    // Oldest evicted, newest retained.
    expect(getCachedIcon('path:/folder/0')).toBeUndefined()
    expect(getCachedIcon(`path:/folder/${cap}`)).toBe(`url-${cap}`)

    // Exactly `cap` path: keys remain (count via a fresh probe set).
    let remaining = 0
    for (let n = 0; n <= cap; n++) {
      if (getCachedIcon(`path:/folder/${n}`) !== undefined) remaining++
    }
    expect(remaining).toBe(cap)
  })

  it('never evicts non-path: keys regardless of how many path: keys arrive', () => {
    _applyIconsToCacheForTests({ dir: 'dir-url', 'ext:png': 'png-url', file: 'file-url' })

    for (let n = 0; n < _pathKeyCapForTests * 3; n++) {
      _applyIconsToCacheForTests({ [`path:/folder/${n}`]: `url-${n}` })
    }

    expect(getCachedIcon('dir')).toBe('dir-url')
    expect(getCachedIcon('ext:png')).toBe('png-url')
    expect(getCachedIcon('file')).toBe('file-url')
  })
})

describe('icon-cache pkg: keys (Tier C packages)', () => {
  beforeEach(() => {
    _resetIconCacheForTests()
    localStorage.clear()
  })

  it('does not persist pkg: keys to localStorage', () => {
    _applyIconsToCacheForTests({
      'pkg:/Applications/Safari.app': 'safari-url',
      dir: 'dir-url',
    })
    const keys = storedKeys()
    expect(keys.some((k) => k.startsWith('pkg:'))).toBe(false)
    expect(keys).toContain('dir')
  })

  it('keeps pkg: keys available in memory', () => {
    _applyIconsToCacheForTests({ 'pkg:/Applications/Safari.app': 'safari-url' })
    expect(getCachedIcon('pkg:/Applications/Safari.app')).toBe('safari-url')
  })

  it('shares one LRU budget across path: and pkg: keys', () => {
    const cap = _pathKeyCapForTests
    // Fill the whole budget with pkg: keys.
    for (let n = 0; n < cap; n++) {
      _applyIconsToCacheForTests({ [`pkg:/Applications/App${n}.app`]: `url-${n}` })
    }
    // A single path: key now evicts the oldest pkg: key — both share the cap.
    _applyIconsToCacheForTests({ 'path:/Users/me/Custom': 'custom-url' })
    expect(getCachedIcon('pkg:/Applications/App0.app')).toBeUndefined()
    expect(getCachedIcon('path:/Users/me/Custom')).toBe('custom-url')
  })
})

describe('evictPerPathIconsForDir', () => {
  beforeEach(() => {
    _resetIconCacheForTests()
    localStorage.clear()
  })

  it('evicts only the direct children of the ended directory', () => {
    _applyIconsToCacheForTests({
      'path:/Users/me/Work/CustomA': 'a',
      'pkg:/Users/me/Work/Foo.app': 'foo',
      'path:/Users/me/Other/CustomB': 'b', // a sibling dir, must survive
      dir: 'dir-url',
    })

    evictPerPathIconsForDir('/Users/me/Work')

    expect(getCachedIcon('path:/Users/me/Work/CustomA')).toBeUndefined()
    expect(getCachedIcon('pkg:/Users/me/Work/Foo.app')).toBeUndefined()
    // Sibling dir's icon and bounded keys untouched.
    expect(getCachedIcon('path:/Users/me/Other/CustomB')).toBe('b')
    expect(getCachedIcon('dir')).toBe('dir-url')
  })

  it('handles a trailing slash and a no-op empty path', () => {
    _applyIconsToCacheForTests({ 'path:/a/b/C': 'c' })
    evictPerPathIconsForDir('') // no-op
    expect(getCachedIcon('path:/a/b/C')).toBe('c')
    evictPerPathIconsForDir('/a/b/')
    expect(getCachedIcon('path:/a/b/C')).toBeUndefined()
  })
})

describe('prefetchCustomFolderIcons', () => {
  beforeEach(() => {
    _resetIconCacheForTests()
    localStorage.clear()
    vi.mocked(getCustomFolderIconIds).mockReset()
    vi.mocked(getIcons).mockReset()
  })

  it('asks the backend only about dirs without a cached path: icon', async () => {
    // Pre-seed one dir as already having a custom icon.
    _applyIconsToCacheForTests({ 'path:/Users/me/Known': 'known' })
    vi.mocked(getCustomFolderIconIds).mockResolvedValue({ data: [], timedOut: false })

    await prefetchCustomFolderIcons(['/Users/me/Known', '/Users/me/Unknown'], true)

    expect(getCustomFolderIconIds).toHaveBeenCalledTimes(1)
    // Only the uncached dir is queried.
    expect(getCustomFolderIconIds).toHaveBeenCalledWith(['/Users/me/Unknown'])
  })

  it('fetches the returned custom-folder ids through getIcons', async () => {
    vi.mocked(getCustomFolderIconIds).mockResolvedValue({
      data: ['path:/Users/me/Custom'],
      timedOut: false,
    })
    vi.mocked(getIcons).mockResolvedValue({
      data: { 'path:/Users/me/Custom': 'custom-url' },
      timedOut: false,
    })

    await prefetchCustomFolderIcons(['/Users/me/Custom'], false)

    expect(getIcons).toHaveBeenCalledWith(['path:/Users/me/Custom'], false)
    expect(getCachedIcon('path:/Users/me/Custom')).toBe('custom-url')
  })

  it('does nothing on an empty input and never throws on backend error', async () => {
    await prefetchCustomFolderIcons([], true)
    expect(getCustomFolderIconIds).not.toHaveBeenCalled()

    vi.mocked(getCustomFolderIconIds).mockRejectedValue(new Error('boom'))
    await expect(prefetchCustomFolderIcons(['/x'], true)).resolves.toBeUndefined()
  })
})

describe('icon-cache special: keys (Tier B)', () => {
  beforeEach(() => {
    _resetIconCacheForTests()
    localStorage.clear()
  })

  it('persists special: keys to localStorage alongside the bounded keys', () => {
    _applyIconsToCacheForTests({
      'special:downloads': 'dl-url',
      'special:applications': 'apps-url',
      dir: 'dir-url',
    })

    const keys = storedKeys()
    expect(keys).toEqual(expect.arrayContaining(['special:downloads', 'special:applications', 'dir']))
  })

  it('never evicts special: keys when many path: keys arrive (they are not LRU-capped)', () => {
    _applyIconsToCacheForTests({ 'special:downloads': 'dl-url' })

    for (let n = 0; n < _pathKeyCapForTests * 3; n++) {
      _applyIconsToCacheForTests({ [`path:/folder/${n}`]: `url-${n}` })
    }

    expect(getCachedIcon('special:downloads')).toBe('dl-url')
  })
})
