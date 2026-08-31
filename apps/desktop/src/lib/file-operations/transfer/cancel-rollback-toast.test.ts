import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { readCancelRollback } from './cancel-rollback-toast'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { CancelRollback, SkipBreakdown, SkipReason } from '$lib/ipc/bindings'

// The readout resolves its wording through `t()` (catalog + ICU), which reads the
// active locale. Pin en-US so these are golden assertions on the shipped English.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function skip(reason: SkipReason, count: number, exampleName = 'invoice-2026.pdf'): SkipBreakdown {
  return { reason, count, exampleName }
}

function rollback(partial: Partial<CancelRollback>): CancelRollback {
  return { outcome: 'notRolledBack', reversed: 0, skips: [], ...partial }
}

describe('readCancelRollback', () => {
  describe('silence', () => {
    it('says nothing when no reversal ran', () => {
      // A plain Cancel keeps what was written, which is what the user asked for.
      expect(readCancelRollback(rollback({}), 'copy')).toBeNull()
    })

    it('says nothing when the reversal stopped before its first item', () => {
      expect(readCancelRollback(rollback({ outcome: 'notRolledBack', reversed: 0 }), 'move')).toBeNull()
    })

    it('says nothing when the transfer had written nothing to undo', () => {
      // A clean reversal over an empty ledger. "Removed 0 items" is noise.
      expect(readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 0 }), 'copy')).toBeNull()
    })
  })

  describe('a reversal that finished with nothing left behind', () => {
    it('names the deletion for a copy', () => {
      const readout = readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 1240 }), 'copy')
      expect(readout).toEqual({
        headline: 'Removed the 1,240 items Cmdr had written.',
        leftBehind: null,
        reasons: [],
        level: 'success',
      })
    })

    it('names the restore for a move, never a deletion', () => {
      // Undoing a move carries files home. Wording it as a delete would be a
      // data-safety lie in copy.
      const readout = readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 3 }), 'move')
      expect(readout?.headline).toBe('Put the 3 items back.')
    })

    it('reads as a success', () => {
      expect(readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 1 }), 'copy')?.level).toBe('success')
    })
  })

  describe('a reversal the user stopped partway', () => {
    // Told apart by its EMPTY skips: a full pass that skipped nothing lands
    // `rolledBack`, so `partiallyRolledBack` with no groups can only be a stop.
    it('says the rest are still there, for a copy', () => {
      const readout = readCancelRollback(rollback({ outcome: 'partiallyRolledBack', reversed: 12 }), 'copy')
      expect(readout).toEqual({
        headline: 'Stopped after removing 12 items. The rest are still there.',
        leftBehind: null,
        reasons: [],
        level: 'info',
      })
    })

    it('says where the rest stayed, for a move', () => {
      const readout = readCancelRollback(rollback({ outcome: 'partiallyRolledBack', reversed: 2 }), 'move')
      expect(readout?.headline).toBe('Stopped after putting 2 items back. The rest stayed where the move put them.')
    })

    it('is not a success, because something is still where the user cancelled it', () => {
      expect(readCancelRollback(rollback({ outcome: 'partiallyRolledBack', reversed: 2 }), 'copy')?.level).toBe('info')
    })
  })

  describe('a reversal that left things behind', () => {
    it('sets the expectation before it names a single reason', () => {
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 9, skips: [skip('drift', 1)] }),
        'copy',
      )
      expect(readout).toEqual({
        headline: 'Removed 9 items.',
        leftBehind: "Cmdr leaves alone anything it isn't sure about, so these stayed where they are:",
        reasons: ['Left invoice-2026.pdf alone: it changed after Cmdr put it there.'],
        level: 'info',
      })
    })

    it('never claims completeness the way the clean wording does', () => {
      // "the 9 items Cmdr had written" would say the destination is clear.
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 9, skips: [skip('drift', 1)] }),
        'copy',
      )
      expect(readout?.headline).not.toContain('had written')
    })

    it('opens on the explanation when the reversal undid nothing at all', () => {
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 0, skips: [skip('drift', 2)] }),
        'copy',
      )
      expect(readout?.headline).toBeNull()
      expect(readout?.reasons).toEqual(['Left 2 items alone: they changed after Cmdr put them there.'])
    })

    it('names the one file a reason applies to, and counts them when there are several', () => {
      const named = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip('drift', 1, 'notes.md')] }),
        'copy',
      )
      expect(named?.reasons).toEqual(['Left notes.md alone: it changed after Cmdr put it there.'])
      const counted = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip('drift', 1200)] }),
        'copy',
      )
      expect(counted?.reasons).toEqual(['Left 1,200 items alone: they changed after Cmdr put them there.'])
    })

    it('gives every reason its own line, in the order the backend grouped them', () => {
      const readout = readCancelRollback(
        rollback({
          outcome: 'partiallyRolledBack',
          reversed: 4,
          skips: [
            skip('drift', 1, 'notes.md'),
            skip('unverifiablePrecondition', 3),
            skip('restoreTargetOccupied', 1, 'photo.jpg'),
            skip('dirNotEmpty', 1, 'Scans'),
          ],
        }),
        'move',
      )
      expect(readout?.reasons).toEqual([
        'Left notes.md alone: it changed after Cmdr put it there.',
        "Left 3 items alone: Cmdr couldn't check whether they changed.",
        'Left photo.jpg where it is: something else now sits where it came from.',
        'Left the folder Scans alone: it has something in it now.',
      ])
    })

    it('counts folders as folders, not files', () => {
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip('dirNotEmpty', 3)] }),
        'copy',
      )
      expect(readout?.reasons).toEqual(['Left 3 folders alone: they have something in them now.'])
    })

    it('warns only when the DRIVE turned the undo down, never on a choice Cmdr made', () => {
      // Every other reason is Cmdr protecting something; `failed` is worth a
      // colour that says look at this, and may be worth retrying.
      const chosen = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip('drift', 1)] }),
        'copy',
      )
      expect(chosen?.level).toBe('info')
      const refused = readCancelRollback(
        rollback({
          outcome: 'partiallyRolledBack',
          reversed: 1,
          skips: [skip('drift', 1), skip('failed', 1, 'report.pdf')],
        }),
        'copy',
      )
      expect(refused?.level).toBe('warn')
      expect(refused?.reasons).toContain("Couldn't undo report.pdf. Its drive may be disconnected or read-only.")
    })

    it('never uses the words error or failed in front of a person', () => {
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip('failed', 7)] }),
        'copy',
      )
      const everyLine = [readout?.headline, readout?.leftBehind, ...(readout?.reasons ?? [])].join(' ').toLowerCase()
      expect(everyLine).not.toMatch(/\berror\b|\bfailed\b/)
    })

    it('keeps quiet about an item that was already gone, which counts as undone', () => {
      // `alreadyGone` is credited as reversed by the backend, so it should never
      // arrive as a group. If one ever does, it must not print a blank bullet.
      const readout = readCancelRollback(
        rollback({ outcome: 'partiallyRolledBack', reversed: 5, skips: [skip('alreadyGone', 2)] }),
        'copy',
      )
      expect(readout).toEqual({
        headline: 'Stopped after removing 5 items. The rest are still there.',
        leftBehind: null,
        reasons: [],
        level: 'info',
      })
    })

    it('has a line for every reason a reversal can report', () => {
      // The one gap that matters is a NEW reason arriving with no wording: the
      // count would still be dropped silently. `alreadyGone` is the only
      // deliberate omission, and it never reaches a report.
      const reasons: SkipReason[] = [
        'drift',
        'unverifiablePrecondition',
        'restoreTargetOccupied',
        'dirNotEmpty',
        'failed',
      ]
      for (const reason of reasons) {
        const readout = readCancelRollback(
          rollback({ outcome: 'partiallyRolledBack', reversed: 1, skips: [skip(reason, 1, 'thing.txt')] }),
          'move',
        )
        expect(readout?.reasons, `no line for ${reason}`).toHaveLength(1)
        expect(readout?.reasons[0], `${reason} should name the item`).toContain('thing.txt')
      }
    })
  })
})
