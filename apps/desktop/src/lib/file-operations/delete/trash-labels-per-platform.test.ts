/**
 * The F8 (trash) and Shift+F8 (permanent delete) actions read in each OS's own words, and
 * never under one label. Windows labels the trash action "Delete" (File Explorer's word), so
 * every surface that also offers the permanent delete must say "permanently" there, or the two
 * read the same. Sentences name the place: "the Recycle Bin" on Windows, the Trash elsewhere.
 * Vocabulary: `docs/style-guide.md` § "Platform vocabulary".
 */

import { afterAll, beforeAll, describe, expect, it, onTestFinished } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { withPlatform } from '#lib/platform/test-platform.js'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

const PLATFORMS = ['macos', 'windows', 'linux'] as const
const count = { count: 3, countText: '3' }

/** Every surface showing both actions side by side or in turn: [trash label, permanent label]. */
const PAIRS: readonly { surface: string; trash: () => string; permanent: () => string }[] = [
  {
    surface: 'delete dialog button',
    trash: () => tString('fileOperations.delete.confirmMoveToTrash'),
    permanent: () => tString('fileOperations.delete.confirmDelete'),
  },
  {
    surface: 'function-key bar',
    trash: () => tString('fileExplorer.functionKeyBar.deleteLabel'),
    permanent: () => tString('fileExplorer.functionKeyBar.permanentlyLabel'),
  },
  {
    surface: 'command palette',
    trash: () => tString('commands.fileDelete.label'),
    permanent: () => tString('commands.fileDeletePermanently.label'),
  },
  {
    surface: 'File menu',
    trash: () => tString('menu.file.delete'),
    permanent: () => tString('menu.file.deletePermanently'),
  },
  {
    surface: 'sources-gone dialog button',
    trash: () => tString('fileOperations.sourcesGone.continue', { type: 'trash' }),
    permanent: () => tString('fileOperations.sourcesGone.continue', { type: 'delete' }),
  },
  {
    surface: 'suggested-ops action chip',
    trash: () => tString('suggestedOps.action.trash'),
    permanent: () => tString('suggestedOps.action.delete'),
  },
  {
    surface: 'suggested-ops approve button',
    trash: () => tString('suggestedOps.approve.trash', count),
    permanent: () => tString('suggestedOps.approve.delete', count),
  },
  {
    surface: 'rename conflict buttons',
    trash: () => tString('fileExplorer.renameConflict.overwriteTrash'),
    permanent: () => tString('fileExplorer.renameConflict.overwriteDelete'),
  },
  {
    surface: 'progress title',
    trash: () => tString('fileOperations.transferProgress.titleActive', { gerund: 'trash' }),
    permanent: () => tString('fileOperations.transferProgress.titleActive', { gerund: 'delete' }),
  },
]

describe('the trash and permanent-delete labels', () => {
  it.each(PLATFORMS)('never read the same on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    for (const { surface, trash, permanent } of PAIRS) {
      expect(trash(), surface).not.toBe(permanent())
    }
  })

  it('say “Delete” and “Delete permanently” on Windows', () => {
    onTestFinished(withPlatform('windows'))
    expect(tString('fileOperations.delete.confirmMoveToTrash')).toBe('Delete')
    expect(tString('fileOperations.delete.confirmDelete')).toBe('Delete permanently')
    expect(tString('fileOperations.sourcesGone.continue', { type: 'trash' })).toBe('Delete the rest')
    expect(tString('fileOperations.sourcesGone.continue', { type: 'delete' })).toBe('Delete the rest permanently')
    expect(tString('suggestedOps.action.trash')).toBe('Delete')
    expect(tString('suggestedOps.action.delete')).toBe('Delete permanently')
    expect(tString('suggestedOps.approve.trash', count)).toBe('Delete 3 files')
    expect(tString('suggestedOps.approve.delete', count)).toBe('Delete 3 files permanently')
    expect(tString('fileExplorer.renameConflict.overwriteTrash')).toBe('Overwrite and delete old file')
    expect(tString('fileExplorer.renameConflict.overwriteTrashFolder')).toBe('Overwrite and delete old folder')
    expect(tString('fileExplorer.renameConflict.overwriteDelete')).toBe('Overwrite and permanently delete old file')
    // The note that the button turned permanent names the button's own Windows label.
    expect(tString('fileOperations.delete.cloudOnlineOnlyHandedBack')).toContain('now offers Delete permanently.')
  })

  it.each(['macos', 'linux'] as const)('keep the trash words on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(tString('fileOperations.delete.confirmMoveToTrash')).toBe('Move to trash')
    expect(tString('fileOperations.delete.confirmDelete')).toBe('Delete')
    expect(tString('fileOperations.sourcesGone.continue', { type: 'trash' })).toBe('Move the rest to trash')
    expect(tString('suggestedOps.approve.trash', count)).toBe('Move 3 files to Trash')
    expect(tString('fileExplorer.renameConflict.overwriteTrash')).toBe('Overwrite and trash old file')
    expect(tString('fileExplorer.renameConflict.overwriteDelete')).toBe('Overwrite and delete old file')
    expect(tString('fileOperations.delete.cloudOnlineOnlyHandedBack')).toContain('now offers Delete.')
  })
})

describe('the sentences around a trash', () => {
  const expected: Record<(typeof PLATFORMS)[number], Partial<Record<MessageKey, string>>> = {
    macos: {
      'fileOperations.delete.trashSwitch': 'Move to trash',
      'fileOperations.trash.goToTrashAction': 'Go to trash',
      'fileOperations.transferProgress.scanTitleTrash': 'Counting items to trash…',
    },
    windows: {
      'fileOperations.delete.trashSwitch': 'Move to the Recycle Bin',
      'fileOperations.trash.goToTrashAction': 'Go to Recycle Bin',
      'fileOperations.transferProgress.scanTitleTrash': 'Counting items to move to the Recycle Bin…',
    },
    linux: {
      'fileOperations.delete.trashSwitch': 'Move to trash',
      'fileOperations.trash.goToTrashAction': 'Go to trash',
      'fileOperations.transferProgress.scanTitleTrash': 'Counting items to trash…',
    },
  }

  it.each(PLATFORMS)('name the place in %s’s words', (platform) => {
    onTestFinished(withPlatform(platform))
    for (const [key, text] of Object.entries(expected[platform])) {
      expect(tString(key as MessageKey), key).toBe(text)
    }
    const place = platform === 'windows' ? 'the Recycle Bin' : 'trash'
    expect(tString('transfer.trash', count)).toBe(`Moved 3 files to ${place}`)
    expect(tString('fileOperations.transferDialog.titleVerbOnly', { verb: 'trash' })).toBe(
      platform === 'windows' ? 'Move to the Recycle Bin' : 'Move to trash',
    )
    expect(tString('fileOperations.transferDialog.titleWithCounts', { verb: 'trash', phrase: '3 files' })).toBe(
      platform === 'windows' ? 'Move 3 files to the Recycle Bin' : 'Move to trash 3 files',
    )
    expect(tString('fileOperations.transferDialog.titleWithCounts', { verb: 'copy', phrase: '3 files' })).toBe(
      'Copy 3 files',
    )
  })
})
