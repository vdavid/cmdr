import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { composeTransferCompleteToast } from './transfer-complete-toast'
import { _setLocaleForTests } from '$lib/intl/locale'

// The composer resolves its wording through `t()` (catalog + ICU), which reads
// the active locale. Pin en-US so these golden assertions are the parity net:
// the ICU-rendered output must equal the historic English copy byte-for-byte.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('composeTransferCompleteToast', () => {
  describe('copy with selection split (fileCount / folderCount available)', () => {
    it('files only, all copied', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 2,
          filesSkipped: 0,
          fileCount: 2,
          folderCount: 0,
        }),
      ).toBe('Copied 2 files.')
    })

    it('reads as an ordinary copy when the item was duplicated in place', () => {
      // Copying an item into the folder it already lives in duplicates it under
      // a free ` (N)` name. That never enters the skip or conflict counters
      // (pinned backend-side in `self_collision_tests.rs`), so the toast is the
      // plain one. Without this, a future counter change could quietly turn a
      // duplicate into "Copied 0 files."
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 1,
          filesSkipped: 0,
          fileCount: 1,
          folderCount: 0,
        }),
      ).toBe('Copied 1 file.')
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 12,
          filesSkipped: 0,
          fileCount: 12,
          folderCount: 0,
        }),
      ).toBe('Copied 12 files.')
    })

    it('single file, all copied', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 1,
          filesSkipped: 0,
          fileCount: 1,
          folderCount: 0,
        }),
      ).toBe('Copied 1 file.')
    })

    it('folders only, all copied', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 10,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 3,
        }),
      ).toBe('Copied 3 folders.')
    })

    it('single folder, all copied', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 4,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 1,
        }),
      ).toBe('Copied 1 folder.')
    })

    it('mixed files and folders, all copied', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 7,
          filesSkipped: 0,
          fileCount: 2,
          folderCount: 1,
        }),
      ).toBe('Copied 2 files and 1 folder.')
    })

    it('the canonical "1 file and 3 folders" example', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 99,
          filesSkipped: 0,
          fileCount: 1,
          folderCount: 3,
        }),
      ).toBe('Copied 1 file and 3 folders.')
    })

    it('omits the zero file part', () => {
      // Never "0 files and 3 folders".
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 5,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 3,
        }),
      ).toBe('Copied 3 folders.')
    })
  })

  describe('move with selection split', () => {
    it('the canonical "Moved 1 file and 3 folders" example', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 50,
          filesSkipped: 0,
          fileCount: 1,
          folderCount: 3,
        }),
      ).toBe('Moved 1 file and 3 folders.')
    })

    it('mixed files and folders, all moved', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 9,
          filesSkipped: 0,
          fileCount: 2,
          folderCount: 1,
        }),
      ).toBe('Moved 2 files and 1 folder.')
    })
  })

  describe('selection split with skipped files (skips are file-level; folders always merge)', () => {
    it('mixed: some files skipped, folder merged', () => {
      // Selected 2 files + 1 folder; 1 file skipped. The folder always merges.
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 8,
          filesSkipped: 1,
          fileCount: 2,
          folderCount: 1,
        }),
      ).toBe('Moved 1 file and 1 folder, skipped 1 file (already at the target).')
    })

    it('all selected files skipped, folder still merged', () => {
      // Selected 2 files + 1 folder; both files skipped. The folder merges.
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 6,
          filesSkipped: 2,
          fileCount: 2,
          folderCount: 1,
        }),
      ).toBe('Moved 1 folder, skipped 2 files (already at the target).')
    })

    it('files-only selection, all skipped', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 3,
          filesSkipped: 3,
          fileCount: 3,
          folderCount: 0,
        }),
      ).toBe('Copy complete: skipped all 3 files (already at the target), nothing was copied.')
    })

    it('files-only selection, single file skipped', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 1,
          filesSkipped: 1,
          fileCount: 1,
          folderCount: 0,
        }),
      ).toBe('Copy complete: file already at the target, not copied.')
    })

    it('mixed copy with a single skipped file uses singular noun', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 5,
          filesSkipped: 1,
          fileCount: 2,
          folderCount: 1,
        }),
      ).toBe('Copied 1 file and 1 folder, skipped 1 file (already at the target).')
    })
  })

  // The engine that can tell a top-level skip from a nested one says so, and
  // then each selection count is reduced only by its own kind. Without this the
  // leaf count eats top-level items that actually landed.
  describe('selection split when the engine reports the selection`s own skips', () => {
    it('a nested skip leaves the top-level file it used to eat', () => {
      // Selected `readme.txt` + `only-in-source.txt` + `docs/`. Skip All keeps
      // the clashing readme plus two files inside docs/: 3 leaves skipped, but
      // only ONE of the things the user picked.
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 5,
          filesSkipped: 3,
          fileCount: 2,
          folderCount: 1,
          topLevelSkipped: { files: 1, folders: 0 },
        }),
      ).toBe('Copied 1 file and 1 folder, skipped 3 files (already at the target).')
    })

    it('a skipped folder is not reported as copied', () => {
      // A folder landing on a same-named file is refused, so nothing arrived.
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 1,
          filesSkipped: 1,
          fileCount: 0,
          folderCount: 1,
          topLevelSkipped: { files: 0, folders: 1 },
        }),
      ).toBe('Copy complete: file already at the target, not copied.')
    })

    it('a folder whose child was skipped still counts as copied', () => {
      // One refused child doesn't make the folder a skip: it partly arrived.
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 3,
          filesSkipped: 1,
          fileCount: 0,
          folderCount: 1,
          topLevelSkipped: { files: 0, folders: 0 },
        }),
      ).toBe('Copied 1 folder, skipped 1 file (already at the target).')
    })

    it('reporting no top-level skips keeps every selected item in the phrase', () => {
      // The counterweight to the fallback: an engine that reports `0/0` means
      // "nothing you picked was skipped", and must NOT have `filesSkipped`
      // subtracted from its files the way an engine that stays silent does.
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 8,
          filesSkipped: 2,
          fileCount: 2,
          folderCount: 1,
          topLevelSkipped: { files: 0, folders: 0 },
        }),
      ).toBe('Moved 2 files and 1 folder, skipped 2 files (already at the target).')
    })

    it('every selected item skipped collapses to the flat wording', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'copy',
          filesProcessed: 4,
          filesSkipped: 4,
          fileCount: 1,
          folderCount: 1,
          topLevelSkipped: { files: 1, folders: 1 },
        }),
      ).toBe('Copy complete: skipped all 4 files (already at the target), nothing was copied.')
    })
  })

  describe('copy fallback (no selection counts — clipboard paste)', () => {
    it('all copied, multi-file', () => {
      expect(composeTransferCompleteToast({ operationType: 'copy', filesProcessed: 5, filesSkipped: 0 })).toBe(
        'Copy complete: copied 5 files.',
      )
    })

    it('all copied, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'copy', filesProcessed: 1, filesSkipped: 0 })).toBe(
        'Copy complete: copied 1 file.',
      )
    })

    it('all skipped, multi-file', () => {
      expect(composeTransferCompleteToast({ operationType: 'copy', filesProcessed: 5, filesSkipped: 5 })).toBe(
        'Copy complete: skipped all 5 files (already at the target), nothing was copied.',
      )
    })

    it('all skipped, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'copy', filesProcessed: 1, filesSkipped: 1 })).toBe(
        'Copy complete: file already at the target, not copied.',
      )
    })

    it('mixed: some copied, some skipped', () => {
      expect(composeTransferCompleteToast({ operationType: 'copy', filesProcessed: 5, filesSkipped: 2 })).toBe(
        'Copy complete: copied 3, skipped 2. All 5 of your selected files are now at the target.',
      )
    })
  })

  describe('move fallback (no selection counts — clipboard paste)', () => {
    it('all moved, multi-file', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 5, filesSkipped: 0 })).toBe(
        'Move complete: moved 5 files.',
      )
    })

    it('all moved, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 1, filesSkipped: 0 })).toBe(
        'Move complete: moved 1 file.',
      )
    })

    it('all skipped, multi-file', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 5, filesSkipped: 5 })).toBe(
        'Move complete: skipped all 5 files (already at the target), nothing was moved.',
      )
    })

    it('all skipped, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 1, filesSkipped: 1 })).toBe(
        'Move complete: file already at the target, not moved.',
      )
    })

    it('mixed: phrased as "already at target", not "now at target"', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 5, filesSkipped: 2 })).toBe(
        'Move complete: moved 3, skipped 2. 2 files were already at the target.',
      )
    })

    it('mixed with single skipped file uses singular "was"', () => {
      expect(composeTransferCompleteToast({ operationType: 'move', filesProcessed: 4, filesSkipped: 1 })).toBe(
        'Move complete: moved 3, skipped 1. 1 file was already at the target.',
      )
    })
  })

  describe('trash and delete (no skip concept, no split)', () => {
    it('trash uses historic short wording', () => {
      expect(composeTransferCompleteToast({ operationType: 'trash', filesProcessed: 3, filesSkipped: 0 })).toBe(
        'Moved 3 files to trash',
      )
    })

    it('trash, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'trash', filesProcessed: 1, filesSkipped: 0 })).toBe(
        'Moved 1 file to trash',
      )
    })

    it('trash ignores selection counts (stays file-level, honest)', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'trash',
          filesProcessed: 3,
          filesSkipped: 0,
          fileCount: 1,
          folderCount: 1,
        }),
      ).toBe('Moved 3 files to trash')
    })

    it('delete uses historic short wording', () => {
      expect(composeTransferCompleteToast({ operationType: 'delete', filesProcessed: 3, filesSkipped: 0 })).toBe(
        'Delete complete: 3 files',
      )
    })

    it('delete, single file', () => {
      expect(composeTransferCompleteToast({ operationType: 'delete', filesProcessed: 1, filesSkipped: 0 })).toBe(
        'Delete complete: 1 file',
      )
    })
  })

  describe('what a move left behind in the source', () => {
    // A cross-filesystem move never copies what turned up in the source after
    // the scan, so it leaves those files alone. The toast has to say so: the
    // move succeeded, and part of what the user was looking at is still there.
    it('names the folder and counts what stayed', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 4,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 1,
          appearedDuringMove: { itemCount: 2, folderName: 'Work', folderCount: 1 },
        }),
      ).toBe('Moved 1 folder. 2 items appeared in Work during the move and stay there.')
    })

    it('reads naturally for a single item', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 4,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 1,
          appearedDuringMove: { itemCount: 1, folderName: 'Work', folderCount: 1 },
        }),
      ).toBe('Moved 1 folder. 1 item appeared in Work during the move and stays there.')
    })

    it('names only the first folder when several kept something', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 9,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 3,
          appearedDuringMove: { itemCount: 5, folderName: 'Work', folderCount: 2 },
        }),
      ).toBe('Moved 3 folders. 5 items appeared in Work and other source folders during the move and stay there.')
    })

    it('says nothing when the move took everything', () => {
      expect(
        composeTransferCompleteToast({
          operationType: 'move',
          filesProcessed: 4,
          filesSkipped: 0,
          fileCount: 0,
          folderCount: 1,
          appearedDuringMove: null,
        }),
      ).toBe('Moved 1 folder.')
    })
  })
})
