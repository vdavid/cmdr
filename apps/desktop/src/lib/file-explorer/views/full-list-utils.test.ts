/**
 * Tests for full-list-utils.ts
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import {
  getVisibleItemsCount,
  FULL_LIST_ROW_HEIGHT,
  getVirtualizationBufferRows,
  getDisplayExtension,
  getDisplayName,
  getNameColumnText,
  pickSizeDisplay,
  isHiddenRowDimmed,
  extCellText,
  wordResticSnapshot,
  showsPausedSizeWarning,
} from './full-list-utils'
import type { SizeDisplayPick } from './full-list-utils'
import type { FileEntry } from '../types'
import type { GitCountKind } from '#lib/ipc/bindings.js'
import { _setLocaleForTests } from '#lib/intl/locale.js'

// Mock the settings store
vi.mock('#lib/settings/settings-store.js', () => ({
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
    expect(getDisplayExtension('photo.jpg', 'file')).toBe('jpg')
    expect(getDisplayName('photo.jpg', 'file')).toBe('photo')
  })

  it('keeps dotfiles intact (no secondary dot)', () => {
    expect(getDisplayExtension('.bashrc', 'file')).toBe('')
    expect(getDisplayName('.bashrc', 'file')).toBe('.bashrc')
  })

  it('treats only the last segment of a multi-dot name as the extension', () => {
    expect(getDisplayExtension('my.file.name.txt', 'file')).toBe('txt')
    expect(getDisplayName('my.file.name.txt', 'file')).toBe('my.file.name')
  })

  it('shows a compressed tar as one extension', () => {
    expect(getDisplayExtension('file.tar.gz', 'file')).toBe('tar.gz')
    expect(getDisplayName('file.tar.gz', 'file')).toBe('file')
  })

  it('keeps words after a dot in the name', () => {
    expect(getDisplayExtension('Smlouva v. 2 final', 'file')).toBe('')
    expect(getDisplayName('Smlouva v. 2 final', 'file')).toBe('Smlouva v. 2 final')
  })

  it('returns empty ext for directories and keeps the full name', () => {
    expect(getDisplayExtension('My Folder.d', 'folder')).toBe('')
    expect(getDisplayName('My Folder.d', 'folder')).toBe('My Folder.d')
  })

  it('splits a macOS package folder at its extension', () => {
    expect(getDisplayExtension('Cmdr.app', 'package')).toBe('app')
    expect(getDisplayName('Cmdr.app', 'package')).toBe('Cmdr')
  })

  it('keeps trailing-dot names intact', () => {
    expect(getDisplayExtension('foo.', 'file')).toBe('')
    expect(getDisplayName('foo.', 'file')).toBe('foo.')
  })

  it('handles names with no dot at all', () => {
    expect(getDisplayExtension('README', 'file')).toBe('')
    expect(getDisplayName('README', 'file')).toBe('README')
  })

  it('splits a dotfile with a secondary dot', () => {
    expect(getDisplayExtension('.env.local', 'file')).toBe('local')
    expect(getDisplayName('.env.local', 'file')).toBe('.env')
  })
})

describe('getNameColumnText', () => {
  it('strips the extension when the split mode is on (default)', () => {
    expect(getNameColumnText('launch.json', 'file', false)).toBe('launch')
    expect(getNameColumnText('file.tar.gz', 'file', false)).toBe('file')
  })

  it('keeps the full filename when showExtensionInName is on', () => {
    expect(getNameColumnText('launch.json', 'file', true)).toBe('launch.json')
    expect(getNameColumnText('file.tar.gz', 'file', true)).toBe('file.tar.gz')
  })

  it('returns the full name in both modes when there is no extension to split', () => {
    expect(getNameColumnText('README', 'file', false)).toBe('README')
    expect(getNameColumnText('README', 'file', true)).toBe('README')
    expect(getNameColumnText('My Folder', 'folder', false)).toBe('My Folder')
    expect(getNameColumnText('My Folder', 'folder', true)).toBe('My Folder')
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
      ...extra,
    }
  }

  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('returns an empty pick for normal rows', () => {
    expect(pickSizeDisplay(makeEntry({ size: 1234 }))).toEqual({})
  })

  it('words an ahead/behind pair, cell and tooltip', () => {
    const pick = pickSizeDisplay(
      makeEntry({ gitMeta: { kind: 'aheadBehind', ahead: 3, behind: 1, vs: 'origin/main' } }),
    )
    expect(pick.override).toBe('+3 / -1')
    expect(pick.tooltip).toBe('3 commits ahead, 1 commit behind “origin/main”')
  })

  it('says "1 commit", not "1 commits"', () => {
    // The old backend built this string with a bare `{n} commits`, so a branch
    // one commit apart read as "1 commits ahead". Wording it from the catalog
    // is what fixes it, in every locale at once.
    const pick = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'aheadBehind', ahead: 1, behind: 0, vs: 'main' } }))
    expect(pick.tooltip).toBe('1 commit ahead, 0 commits behind “main”')
  })

  it('shows a pending count as an ellipsis, with a tooltip saying it’s on its way', () => {
    const pick = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'countPending', counted: 'filesChanged' } }))
    expect(pick.override).toBe('…')
    expect(pick.tooltip).toBe('Counting the files this commit changed…')
  })

  it('words each count kind on its own noun', () => {
    const cell = (counted: GitCountKind, n: number) =>
      pickSizeDisplay(makeEntry({ gitMeta: { kind: 'count', counted, n } })).override
    expect(cell('branches', 3)).toBe('3 branches')
    expect(cell('tags', 5)).toBe('5 tags')
    expect(cell('commits', 123)).toBe('123 commits')
    expect(cell('stashEntries', 2)).toBe('2 stash entries')
    expect(cell('linkedWorktrees', 2)).toBe('2 linked worktrees')
    expect(cell('submodules', 4)).toBe('4 submodules')
    expect(cell('filesChanged', 7)).toBe('7 files')
  })

  it('picks the singular form for each count kind', () => {
    const cell = (counted: GitCountKind) =>
      pickSizeDisplay(makeEntry({ gitMeta: { kind: 'count', counted, n: 1 } })).override
    expect(cell('branches')).toBe('1 branch')
    expect(cell('tags')).toBe('1 tag')
    expect(cell('commits')).toBe('1 commit')
    expect(cell('stashEntries')).toBe('1 stash entry')
    expect(cell('linkedWorktrees')).toBe('1 linked worktree')
    expect(cell('submodules')).toBe('1 submodule')
    expect(cell('filesChanged')).toBe('1 file')
  })

  it('gives the three repo-wide counts a tooltip that says where they come from', () => {
    const tip = (counted: GitCountKind, n: number) =>
      pickSizeDisplay(makeEntry({ gitMeta: { kind: 'count', counted, n } })).tooltip
    expect(tip('branches', 3)).toBe('3 branches on this repo')
    expect(tip('tags', 1)).toBe('1 tag on this repo')
    expect(tip('commits', 42)).toBe('42 commits reachable from HEAD')
    expect(tip('filesChanged', 1)).toBe('1 file changed compared to the parent commit')
  })

  it('reuses the cell text as the tooltip where there is nothing to add', () => {
    for (const counted of ['stashEntries', 'linkedWorktrees', 'submodules'] as const) {
      const pick = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'count', counted, n: 2 } }))
      expect(pick.tooltip).toBe(pick.override)
    }
  })

  it('shortens a commit id for the cell and keeps the full one in the tooltip', () => {
    const id = '0123456789abcdef0123456789abcdef01234567'
    const tagged = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'taggedCommit', id } }))
    expect(tagged.override).toBe('0123456')
    expect(tagged.tooltip).toBe(`Tagged commit ${id}`)

    const pinned = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'pinnedCommit', id } }))
    expect(pinned.override).toBe('0123456')
    expect(pinned.tooltip).toBe(`Pinned at commit ${id}`)

    const detached = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'worktreeDetachedAt', id } }))
    expect(detached.override).toBe('0123456')
    expect(detached.tooltip).toBe(`Detached at ${id}`)
  })

  it('names the branch a stash entry and a worktree belong to', () => {
    const stashed = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'stashedOnBranch', branch: 'main' } }))
    expect(stashed.override).toBe('on main')
    expect(stashed.tooltip).toBe('Created on branch “main”')

    const worktree = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'worktreeOnBranch', branch: 'feature-x' } }))
    expect(worktree.override).toBe('on feature-x')
    expect(worktree.tooltip).toBe('Branch “feature-x” is checked out')
  })

  it('prefers the git wording even when size is also set (the sort key)', () => {
    // Branches use `size` as the within-category sort key while the cell shows
    // the wording. The picker honors the wording.
    const pick = pickSizeDisplay(makeEntry({ size: 12, gitMeta: { kind: 'count', counted: 'branches', n: 12 } }))
    expect(pick.override).toBe('12 branches')
  })

  it('lets the restricted-folder override win over a git row', () => {
    const pick = pickSizeDisplay(makeEntry({ gitMeta: { kind: 'count', counted: 'branches', n: 3 } }), true)
    expect(pick.override).not.toBe('3 branches')
  })
})

describe('pickSizeDisplay across locales', () => {
  function cell(counted: GitCountKind, n: number): SizeDisplayPick {
    return pickSizeDisplay({
      name: 'branches',
      path: '/repo/.git/branches',
      isDirectory: true,
      isSymlink: false,
      permissions: 0o755,
      owner: '',
      group: '',
      iconId: 'git:branch',
      gitMeta: { kind: 'count', counted, n },
    })
  }

  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('keeps the noun unchanged in Hungarian, which never pluralizes after a number', () => {
    // Hungarian counts with the singular: "3 ág", never "3 ágak". The old
    // backend built one English string for every language, so this reading was
    // unreachable no matter what the user picked.
    _setLocaleForTests('hu')
    expect(cell('branches', 1).override).toBe('1 ág')
    expect(cell('branches', 3).override).toBe('3 ág')
  })

  it('picks Portuguese forms from a category set English does not have', () => {
    // Portuguese carries a `many` category English lacks, so its catalog
    // branches on its own set rather than English's one/other.
    _setLocaleForTests('pt')
    expect(cell('stashEntries', 1).override).toBe('1 entrada de stash')
    expect(cell('stashEntries', 3).override).toBe('3 entradas de stash')
    expect(cell('branches', 2).tooltip).toBe('2 branches neste repositório')
  })

  it('drops the plural machinery entirely in Chinese, which has one form', () => {
    _setLocaleForTests('zh')
    expect(cell('branches', 1).override).toBe('1 个分支')
    expect(cell('branches', 7).override).toBe('7 个分支')
    expect(cell('commits', 7).tooltip).toBe('从 HEAD 可追溯到 7 次提交')
  })
})

describe('isHiddenRowDimmed', () => {
  const noExclusions = { isRestricted: false, isSelected: false, isUnderCursor: false }

  it('dims a hidden entry with none of the exclusions', () => {
    expect(isHiddenRowDimmed({ isHidden: true }, noExclusions)).toBe(true)
  })

  it('does not dim an entry that is not hidden', () => {
    expect(isHiddenRowDimmed({ isHidden: false }, noExclusions)).toBe(false)
    expect(isHiddenRowDimmed({}, noExclusions)).toBe(false)
  })

  it('does not dim a hidden entry that is selected', () => {
    expect(isHiddenRowDimmed({ isHidden: true }, { ...noExclusions, isSelected: true })).toBe(false)
  })

  it('does not dim a hidden entry that is under the cursor', () => {
    expect(isHiddenRowDimmed({ isHidden: true }, { ...noExclusions, isUnderCursor: true })).toBe(false)
  })

  it('does not dim a hidden entry that is also TCC-restricted', () => {
    // Restricted rows already carry their own italic + opacity treatment;
    // stacking the hidden-dim color on top would fade toward the contrast
    // floor that opacity treatment already skirts. Restricted wins.
    expect(isHiddenRowDimmed({ isHidden: true }, { ...noExclusions, isRestricted: true })).toBe(false)
  })
})

describe('restic snapshot rows', () => {
  function snapshotRow(meta: FileEntry['resticMeta']): FileEntry {
    return {
      name: '2026-10-12 21.00.03 (a1b2c3d4)',
      path: '/repo/Browse snapshots/2026-10-12 21.00.03 (a1b2c3d4)',
      isDirectory: true,
      isSymlink: false,
      permissions: 0o755,
      owner: '',
      group: '',
      iconId: 'dir',
      resticMeta: meta,
    }
  }

  it('shows the hostname in the Ext cell, which a folder otherwise leaves empty', () => {
    expect(extCellText(snapshotRow({ hostname: 'naspolya', tags: [], paths: [] }))).toBe('naspolya')
    expect(extCellText(snapshotRow({ hostname: '', tags: [], paths: [] }))).toBe('')
  })

  it('keeps every other row’s Ext cell its extension', () => {
    expect(extCellText({ ...snapshotRow(undefined), name: 'photo.jpg', isDirectory: false })).toBe('jpg')
    expect(extCellText(snapshotRow(undefined))).toBe('')
  })

  it('words the host, tags, and paths one line each', () => {
    const tooltip = wordResticSnapshot({
      hostname: 'm3',
      tags: ['daily', 'home'],
      paths: ['/Users/david', '/etc'],
    })
    expect(tooltip).toBe('Backed up from m3\nTags: daily, home\nBacked-up paths: /Users/david, /etc')
  })

  it('picks the singular noun for one tag or one path', () => {
    expect(wordResticSnapshot({ hostname: 'm3', tags: ['daily'], paths: ['/Users/david'] })).toBe(
      'Backed up from m3\nTag: daily\nBacked-up path: /Users/david',
    )
  })

  it('leaves out a line the snapshot has nothing for', () => {
    expect(wordResticSnapshot({ hostname: 'm3', tags: [], paths: [] })).toBe('Backed up from m3')
    expect(wordResticSnapshot({ hostname: '', tags: ['daily'], paths: [] })).toBe('Tag: daily')
  })
})

describe('showsPausedSizeWarning', () => {
  it('warns on every folder size while the drive is paused for disk space', () => {
    for (const state of ['size', 'size-stale', 'lower-bound'] as const) {
      expect(showsPausedSizeWarning(state, true)).toBe(true)
      expect(showsPausedSizeWarning(state, false)).toBe(false)
    }
  })

  it('stays off the `<dir>` placeholder, which claims no size', () => {
    expect(showsPausedSizeWarning('dir', true)).toBe(false)
    expect(showsPausedSizeWarning('scanning', true)).toBe(false)
    expect(showsPausedSizeWarning(undefined, true)).toBe(false)
  })
})
