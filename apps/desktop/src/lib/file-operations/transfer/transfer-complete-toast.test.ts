import { describe, it, expect } from 'vitest'
import { composeTransferCompleteToast } from './transfer-complete-toast'

describe('composeTransferCompleteToast', () => {
  describe('copy', () => {
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

  describe('move', () => {
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
      // Move skip semantics differ: the source file stays at source, the target file
      // (which won the skip) was already there. Don't claim "now at target" the way
      // copy does — that would lie about the moved subset and ignore the source-still-has-it.
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

  describe('trash and delete (no skip concept)', () => {
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
})
