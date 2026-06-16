/**
 * SPIKE parity net (Milestone 0): the ICU-backed `composeTransferCompleteToastIcu`
 * must produce output BYTE-IDENTICAL to the live `composeTransferCompleteToast`
 * across every branch. This is the acid test that ICU `select` + `plural` can
 * express the hardest existing toast wording at en parity (exit criterion (a)).
 *
 * If any case diverges, ICU cannot cleanly express that branch — STOP and report
 * which branch and why (it reshapes Decision 2), don't paper over it here.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { composeTransferCompleteToast, type TransferCompleteToastInput } from './transfer-complete-toast'
import { composeTransferCompleteToastIcu } from './transfer-complete-toast-icu'
import { _setLocaleForTests } from '$lib/intl/locale'

beforeAll(() => _setLocaleForTests('en-US'))
afterAll(() => _setLocaleForTests(null))

/**
 * Every branch of the live composer, as explicit cases (mirrors
 * `transfer-complete-toast.test.ts`) plus a small generated sweep so plural/
 * was-were boundaries (0/1/2/many) are all hit on both sides.
 */
const explicitCases: TransferCompleteToastInput[] = [
  // copy with split, all copied
  { operationType: 'copy', filesProcessed: 2, filesSkipped: 0, fileCount: 2, folderCount: 0 },
  { operationType: 'copy', filesProcessed: 1, filesSkipped: 0, fileCount: 1, folderCount: 0 },
  { operationType: 'copy', filesProcessed: 10, filesSkipped: 0, fileCount: 0, folderCount: 3 },
  { operationType: 'copy', filesProcessed: 4, filesSkipped: 0, fileCount: 0, folderCount: 1 },
  { operationType: 'copy', filesProcessed: 7, filesSkipped: 0, fileCount: 2, folderCount: 1 },
  { operationType: 'copy', filesProcessed: 99, filesSkipped: 0, fileCount: 1, folderCount: 3 },
  { operationType: 'copy', filesProcessed: 5, filesSkipped: 0, fileCount: 0, folderCount: 3 },
  // move with split
  { operationType: 'move', filesProcessed: 50, filesSkipped: 0, fileCount: 1, folderCount: 3 },
  { operationType: 'move', filesProcessed: 9, filesSkipped: 0, fileCount: 2, folderCount: 1 },
  // split with skipped files
  { operationType: 'move', filesProcessed: 8, filesSkipped: 1, fileCount: 2, folderCount: 1 },
  { operationType: 'move', filesProcessed: 6, filesSkipped: 2, fileCount: 2, folderCount: 1 },
  { operationType: 'copy', filesProcessed: 3, filesSkipped: 3, fileCount: 3, folderCount: 0 },
  { operationType: 'copy', filesProcessed: 1, filesSkipped: 1, fileCount: 1, folderCount: 0 },
  { operationType: 'copy', filesProcessed: 5, filesSkipped: 1, fileCount: 2, folderCount: 1 },
  // copy fallback (no selection counts)
  { operationType: 'copy', filesProcessed: 5, filesSkipped: 0 },
  { operationType: 'copy', filesProcessed: 1, filesSkipped: 0 },
  { operationType: 'copy', filesProcessed: 5, filesSkipped: 5 },
  { operationType: 'copy', filesProcessed: 1, filesSkipped: 1 },
  { operationType: 'copy', filesProcessed: 5, filesSkipped: 2 },
  // move fallback
  { operationType: 'move', filesProcessed: 5, filesSkipped: 0 },
  { operationType: 'move', filesProcessed: 1, filesSkipped: 0 },
  { operationType: 'move', filesProcessed: 5, filesSkipped: 5 },
  { operationType: 'move', filesProcessed: 1, filesSkipped: 1 },
  { operationType: 'move', filesProcessed: 5, filesSkipped: 2 },
  { operationType: 'move', filesProcessed: 4, filesSkipped: 1 },
  // trash and delete
  { operationType: 'trash', filesProcessed: 3, filesSkipped: 0 },
  { operationType: 'trash', filesProcessed: 1, filesSkipped: 0 },
  { operationType: 'trash', filesProcessed: 3, filesSkipped: 0, fileCount: 1, folderCount: 1 },
  { operationType: 'delete', filesProcessed: 3, filesSkipped: 0 },
  { operationType: 'delete', filesProcessed: 1, filesSkipped: 0 },
]

/** A generated sweep over the plural/was-were boundaries on every operation type. */
function generatedCases(): TransferCompleteToastInput[] {
  const cases: TransferCompleteToastInput[] = []
  const ops: TransferOpForSweep[] = ['copy', 'move', 'trash', 'delete']
  const counts = [0, 1, 2, 3, 1234]
  for (const operationType of ops) {
    for (const fileCount of counts) {
      for (const folderCount of counts) {
        for (const filesSkipped of [0, 1, 2]) {
          // filesProcessed must be >= filesSkipped to stay a valid input.
          const filesProcessed = Math.max(fileCount, filesSkipped, 1)
          if (filesSkipped > filesProcessed) continue
          cases.push({ operationType, filesProcessed, filesSkipped, fileCount, folderCount })
          // Also the fallback shape (no selection split).
          cases.push({ operationType, filesProcessed, filesSkipped })
        }
      }
    }
  }
  return cases
}

type TransferOpForSweep = TransferCompleteToastInput['operationType']

describe('transfer-complete-toast ICU parity', () => {
  it('matches the live composer on every explicit branch case', () => {
    for (const input of explicitCases) {
      expect(composeTransferCompleteToastIcu(input), JSON.stringify(input)).toBe(composeTransferCompleteToast(input))
    }
  })

  it('matches the live composer across the generated plural/was-were sweep', () => {
    for (const input of generatedCases()) {
      expect(composeTransferCompleteToastIcu(input), JSON.stringify(input)).toBe(composeTransferCompleteToast(input))
    }
  })
})
