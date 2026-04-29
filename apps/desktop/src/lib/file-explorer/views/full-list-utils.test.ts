/**
 * Tests for full-list-utils.ts
 */
import { describe, it, expect, vi } from 'vitest'
import {
  getVisibleItemsCount,
  FULL_LIST_ROW_HEIGHT,
  getVirtualizationBufferRows,
  getDisplayExtension,
  getDisplayName,
  pickSizeDisplay,
} from './full-list-utils'
import type { FileEntry } from '../types'

// Mock the settings store
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn().mockReturnValue(20), // Default buffer size
}))

describe('constants', () => {
  it('has expected row height', () => {
    expect(FULL_LIST_ROW_HEIGHT).toBe(20)
  })

  it('has expected buffer size from settings', () => {
    expect(getVirtualizationBufferRows()).toBe(20)
  })
})

describe('getVisibleItemsCount', () => {
  it('calculates visible items with default row height', () => {
    expect(getVisibleItemsCount(400)).toBe(20) // 400 / 20 = 20
  })

  it('rounds up partial items', () => {
    expect(getVisibleItemsCount(410)).toBe(21) // ceil(410 / 20) = 21
  })

  it('handles exact multiple', () => {
    expect(getVisibleItemsCount(200)).toBe(10)
  })

  it('handles small container', () => {
    expect(getVisibleItemsCount(15)).toBe(1) // ceil(15 / 20) = 1
  })

  it('handles zero height', () => {
    expect(getVisibleItemsCount(0)).toBe(0)
  })

  it('accepts custom row height', () => {
    expect(getVisibleItemsCount(400, 40)).toBe(10) // 400 / 40 = 10
  })

  it('calculates with custom row height and rounding', () => {
    expect(getVisibleItemsCount(410, 40)).toBe(11) // ceil(410 / 40) = 11
  })
})

describe('getDisplayExtension / getDisplayName', () => {
  it('splits a plain filename', () => {
    expect(getDisplayExtension('photo.jpg', false)).toBe('jpg')
    expect(getDisplayName('photo.jpg', false)).toBe('photo')
  })

  it('keeps dotfiles intact (no secondary dot)', () => {
    expect(getDisplayExtension('.bashrc', false)).toBe('')
    expect(getDisplayName('.bashrc', false)).toBe('.bashrc')
  })

  it('treats only the last segment of a multi-dot name as the extension', () => {
    expect(getDisplayExtension('file.tar.gz', false)).toBe('gz')
    expect(getDisplayName('file.tar.gz', false)).toBe('file.tar')
  })

  it('returns empty ext for directories and keeps the full name', () => {
    expect(getDisplayExtension('My Folder.d', true)).toBe('')
    expect(getDisplayName('My Folder.d', true)).toBe('My Folder.d')
  })

  it('keeps trailing-dot names intact', () => {
    expect(getDisplayExtension('foo.', false)).toBe('')
    expect(getDisplayName('foo.', false)).toBe('foo.')
  })

  it('handles names with no dot at all', () => {
    expect(getDisplayExtension('README', false)).toBe('')
    expect(getDisplayName('README', false)).toBe('README')
  })

  it('splits a dotfile with a secondary dot', () => {
    expect(getDisplayExtension('.env.local', false)).toBe('local')
    expect(getDisplayName('.env.local', false)).toBe('.env')
  })
})

describe('pickSizeDisplay', () => {
  function makeEntry(extra: Partial<FileEntry> = {}): FileEntry {
    return {
      name: 'main',
      path: '/repo/.git/branches/main',
      isDirectory: true,
      isSymlink: false,
      permissions: 0o755,
      owner: '',
      group: '',
      iconId: 'git:branch',
      extendedMetadataLoaded: true,
      ...extra,
    }
  }

  it('returns an empty pick for normal rows', () => {
    expect(pickSizeDisplay(makeEntry({ size: 1234 }))).toEqual({})
  })

  it('returns the override when a virtual entry sets displaySize', () => {
    const pick = pickSizeDisplay(makeEntry({ displaySize: '+3 / -1' }))
    expect(pick.override).toBe('+3 / -1')
    expect(pick.tooltip).toBeUndefined()
  })

  it('forwards the rich tooltip when present', () => {
    const pick = pickSizeDisplay(
      makeEntry({
        displaySize: '+3 / -1',
        displaySizeTooltip: '3 commits ahead, 1 commit behind `origin/main`',
      }),
    )
    expect(pick.override).toBe('+3 / -1')
    expect(pick.tooltip).toBe('3 commits ahead, 1 commit behind `origin/main`')
  })

  it('prefers override even when size is also set (sort key)', () => {
    // Branches use `size` as the within-category sort key while showing the
    // override string in the cell. The picker honors the override.
    const pick = pickSizeDisplay(makeEntry({ size: 12, displaySize: '12 items' }))
    expect(pick.override).toBe('12 items')
  })
})
