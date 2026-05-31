import { describe, it, expect, beforeEach, vi } from 'vitest'

// icon-cache imports these from ./tauri-commands at module load. We never call them
// in these tests (we drive the cache via the test-only helpers), but they must exist
// as mocks so the import resolves without a real Tauri runtime.
vi.mock('./tauri-commands', () => ({
  getIcons: vi.fn(),
  refreshDirectoryIcons: vi.fn(),
  clearExtensionIconCache: vi.fn(),
  clearDirectoryIconCache: vi.fn(),
}))

import { getCachedIcon, _resetIconCacheForTests, _applyIconsToCacheForTests, _pathKeyCapForTests } from './icon-cache'

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
