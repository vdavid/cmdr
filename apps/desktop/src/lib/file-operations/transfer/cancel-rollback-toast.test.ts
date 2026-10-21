import { describe, it, expect, beforeAll, beforeEach, afterAll, vi } from 'vitest'
import { raiseCancelRollbackToast, readCancelRollback } from './cancel-rollback-toast'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { addToast } from '#lib/ui/toast/index.js'
import type { CancelRollback, SkipBreakdown, SkipReason, StagedLeftovers } from '#lib/ipc/bindings.js'

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))

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
  return {
    outcome: 'notRolledBack',
    reversed: 0,
    skips: [],
    stagedLeftovers: [],
    originalsStillInPlace: null,
    recovered: [],
    ...partial,
  }
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
        headline: 'Removed all 1,240 items Cmdr had written.',
        leftBehind: null,
        reasons: [],
        recovered: [],
        staged: [],
        level: 'success',
      })
    })

    it('names the restore for a move, never a deletion', () => {
      // Undoing a move carries files home. Wording it as a delete would be a
      // data-safety lie in copy.
      const readout = readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 3 }), 'move')
      expect(readout?.headline).toBe('Put all 3 items back.')
    })

    it('drops the number when there was only one item, on both verbs', () => {
      // "Removed all 1 item" / "Put all 1 item back" is the shape the whole
      // sentence sits inside the plural to avoid.
      expect(readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 1 }), 'copy')?.headline).toBe(
        'Removed the item Cmdr had written.',
      )
      expect(readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 1 }), 'move')?.headline).toBe(
        'Put the item back.',
      )
    })

    it('reads as a success', () => {
      expect(readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 1 }), 'copy')?.level).toBe('success')
    })
  })

  describe('a reversal the user stopped partway', () => {
    // Told apart by its EMPTY skips: a full pass that skipped nothing lands
    // `rolledBack`, so `partiallyRolledBack` with no groups can only be a stop.
    it('says the rest stayed where Cmdr put them, for a copy', () => {
      const readout = readCancelRollback(rollback({ outcome: 'partiallyRolledBack', reversed: 12 }), 'copy')
      expect(readout).toEqual({
        headline: 'Stopped after removing 12 items. The rest stayed where Cmdr put them.',
        leftBehind: null,
        reasons: [],
        recovered: [],
        staged: [],
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
        leftBehind: 'Cmdr skips anything it isn’t sure about, so these stayed where they are:',
        reasons: ['Left invoice-2026.pdf alone: it changed after Cmdr put it there.'],
        recovered: [],
        staged: [],
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
        'Left 3 items alone: Cmdr couldn’t check whether they changed.',
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
      expect(refused?.reasons).toContain('Couldn’t undo report.pdf. Its drive may be disconnected or read-only.')
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
        headline: 'Stopped after removing 5 items. The rest stayed where Cmdr put them.',
        leftBehind: null,
        reasons: [],
        recovered: [],
        staged: [],
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

  describe('Cmdr’s own scratch the destination wouldn’t give up', () => {
    // An abandoned write keeps its handle open, the destination refuses the
    // delete for as long as that session lives, and the ledger reversal itself
    // can still be perfect. The readout is the only thing standing between that
    // and a user being told their NAS is clear while gigabytes sit on it.
    const refused = (count: number): StagedLeftovers => ({
      fate: 'refused',
      count,
      exampleName: 'holiday.jpg.cmdr-tmp-4d1f9c',
      volumeName: 'Naspolya',
    })
    const staged = [refused(1)]

    it('never lets a perfect ledger reversal read as a clean success', () => {
      const readout = readCancelRollback(
        rollback({ outcome: 'rolledBack', reversed: 4, stagedLeftovers: staged }),
        'copy',
      )
      expect(readout).toEqual({
        // `someDeleted`, never `doneDeleting`: "the items Cmdr had written" would
        // claim a completeness the destination just denied.
        headline: 'Removed 4 items.',
        leftBehind: null,
        reasons: [],
        recovered: [],
        staged: [
          'Couldn’t remove holiday.jpg.cmdr-tmp-4d1f9c, an unfinished copy left at the destination. ' +
            'It’s safe to delete, and Cmdr tries again the next time it connects there.',
        ],
        level: 'warn',
      })
    })

    it('keeps the clean wording and the success colour when the sweep got everything', () => {
      const readout = readCancelRollback(rollback({ outcome: 'rolledBack', reversed: 4, stagedLeftovers: [] }), 'copy')
      expect(readout?.headline).toBe('Removed all 4 items Cmdr had written.')
      expect(readout?.staged).toEqual([])
      expect(readout?.level).toBe('success')
    })

    it('speaks even when no reversal ran at all', () => {
      // A plain Stop, or a cross-volume move's cancel: there is no undo to
      // report, and the leftover is still the user's to know about.
      const readout = readCancelRollback(rollback({ stagedLeftovers: staged }), 'move')
      expect(readout?.headline).toBeNull()
      expect(readout?.staged[0]).toContain('holiday.jpg.cmdr-tmp-4d1f9c')
      expect(readout?.level).toBe('warn')
    })

    it('counts them instead of naming one when several stayed', () => {
      const readout = readCancelRollback(
        rollback({
          outcome: 'rolledBack',
          reversed: 0,
          stagedLeftovers: [refused(1240)],
        }),
        'copy',
      )
      expect(readout?.staged).toEqual([
        'Couldn’t remove 1,240 unfinished copies left at the destination. ' +
          'They’re safe to delete, and Cmdr tries again the next time it connects there.',
      ])
    })

    it('says scratch on a server that was out goes once the server is back', () => {
      const readout = readCancelRollback(
        rollback({
          stagedLeftovers: [
            { fate: 'removedWhenBack', count: 1, exampleName: 'holiday.jpg.cmdr-tmp-4d1f9c', volumeName: 'Naspolya' },
          ],
        }),
        'copy',
      )
      expect(readout?.staged).toEqual([
        'holiday.jpg.cmdr-tmp-4d1f9c, an unfinished copy, stays on Naspolya for now. ' +
          'Cmdr removes it as soon as Naspolya is back.',
      ])
    })

    it('words a complete file waiting for its name as the user’s file, never as scratch', () => {
      const readout = readCancelRollback(
        rollback({
          stagedLeftovers: [{ fate: 'landsWhenBack', count: 1, exampleName: 'holiday.jpg', volumeName: 'Naspolya' }],
        }),
        'copy',
      )
      expect(readout?.staged).toEqual([
        'holiday.jpg is complete on Naspolya under a temporary name, ' +
          'and gets its own name the next time Cmdr connects to Naspolya.',
      ])
      expect(readout?.staged[0]).not.toMatch(/remove|delete|unfinished/i)
    })

    it('names a file saved under a recovered name as the user’s file, never as scratch', () => {
      const readout = readCancelRollback(
        rollback({
          stagedLeftovers: [
            { fate: 'rescued', count: 1, exampleName: 'holiday (recovered).jpg', volumeName: 'Naspolya' },
            { fate: 'rescued', count: 2, exampleName: 'a (recovered).mov', volumeName: 'Naspolya' },
          ],
        }),
        'copy',
      )
      expect(readout?.staged).toEqual([
        'Cmdr saved a complete new file on Naspolya as holiday (recovered).jpg, because it couldn’t take its own ' +
          'name. Check it, then give it the name you want.',
        'Cmdr saved 2 complete new files on Naspolya under “(recovered)” names, because they couldn’t take their ' +
          'own. Check them, then give them the names you want.',
      ])
      expect(readout?.staged.join(' ')).not.toMatch(/remove|delete|unfinished/i)
    })

    it('gives each fate its own line, in the order the backend grouped them', () => {
      const readout = readCancelRollback(
        rollback({
          stagedLeftovers: [
            refused(1),
            { fate: 'removedWhenBack', count: 3, exampleName: 'a.mov.cmdr-tmp-1', volumeName: 'Naspolya' },
            { fate: 'landsWhenBack', count: 2, exampleName: 'b.mov', volumeName: 'Naspolya' },
          ],
        }),
        'copy',
      )
      expect(readout?.staged).toEqual([
        'Couldn’t remove holiday.jpg.cmdr-tmp-4d1f9c, an unfinished copy left at the destination. ' +
          'It’s safe to delete, and Cmdr tries again the next time it connects there.',
        '3 unfinished copies stay on Naspolya for now. Cmdr removes them as soon as Naspolya is back.',
        '2 complete files wait on Naspolya under temporary names. ' +
          'Each gets its own name the next time Cmdr connects to Naspolya.',
      ])
    })

    it('rides alongside the reasons rather than among them', () => {
      // The reasons sit under "Cmdr skips anything it isn't sure about", which is
      // Cmdr protecting the user's files. This is Cmdr's own leftover, so it
      // must not borrow that framing.
      const readout = readCancelRollback(
        rollback({
          outcome: 'partiallyRolledBack',
          reversed: 2,
          skips: [skip('drift', 1)],
          stagedLeftovers: staged,
        }),
        'copy',
      )
      expect(readout?.reasons).toHaveLength(1)
      expect(readout?.reasons[0]).not.toContain('cmdr-tmp')
      expect(readout?.staged[0]).toContain('cmdr-tmp')
      expect(readout?.level).toBe('warn')
    })
  })

  describe('an original kept under a recovered name', () => {
    it('breaks a plain Stop’s silence and names both exact paths', () => {
      const readout = readCancelRollback(
        rollback({
          recovered: [
            {
              path: '/Volumes/Photos/Scans',
              keptAt: '/Volumes/Photos/Scans (recovered)',
            },
          ],
        }),
        'copy',
      )

      expect(readout).toEqual({
        headline: null,
        leftBehind: null,
        reasons: [],
        recovered: ['Cmdr kept the original from /Volumes/Photos/Scans at /Volumes/Photos/Scans (recovered).'],
        staged: [],
        level: 'info',
      })
    })

    it('prevents a completed rollback from reading as a clean success', () => {
      const readout = readCancelRollback(
        rollback({
          outcome: 'rolledBack',
          reversed: 4,
          recovered: [{ path: '/old', keptAt: '/old (recovered)' }],
        }),
        'copy',
      )

      expect(readout?.headline).toBe('Removed 4 items.')
      expect(readout?.level).toBe('info')
      expect(readout?.recovered).toHaveLength(1)
    })
  })

  describe('a cross-filesystem move stopped while it was clearing the originals', () => {
    // The whole copy is at the destination and durable by the time this phase
    // starts, so there is no undo to run and none to report. Staying silent
    // would leave a user who pressed Rollback thinking nothing had happened,
    // while some of their originals were already gone for good.
    const landed = { count: 200 }

    it('says what stopped, where the files are, and how many originals stayed', () => {
      const readout = readCancelRollback(rollback({ originalsStillInPlace: landed }), 'move')
      expect(readout).toEqual({
        headline:
          'Cmdr stopped while removing the originals, but everything it moved is already at the destination ' +
          'and stays there. 200 originals are still where they were.',
        leftBehind: null,
        reasons: [],
        recovered: [],
        staged: [],
        level: 'info',
      })
    })

    it('speaks of a single original in the singular, with no number', () => {
      const readout = readCancelRollback(rollback({ originalsStillInPlace: { count: 1 } }), 'move')
      expect(readout?.headline).toBe(
        'Cmdr stopped while removing the originals, but everything it moved is already at the destination ' +
          'and stays there. One original is still where it was.',
      )
    })

    it('says plainly that a removal was under way, so the user knows some originals went', () => {
      // The count alone would leave that to inference. The user needs to read
      // it, without the line turning into a warning about something wrong.
      const readout = readCancelRollback(rollback({ originalsStillInPlace: landed }), 'move')
      expect(readout?.headline).toContain('removing the originals')
      expect(readout?.level).toBe('info')
    })

    it('groups the thousands, like every other count the toast prints', () => {
      const readout = readCancelRollback(rollback({ originalsStillInPlace: { count: 1240 } }), 'move')
      expect(readout?.headline).toContain('1,240 originals')
    })

    it('stays calm: nothing went wrong, and it never says error or failed', () => {
      const readout = readCancelRollback(rollback({ originalsStillInPlace: landed }), 'move')
      expect(readout?.level).toBe('info')
      expect(readout?.headline?.toLowerCase()).not.toMatch(/\berror\b|\bfailed\b/)
    })

    it('claims nothing about the originals it did remove', () => {
      // Reversing those would be a second full transfer across the boundary,
      // which Cmdr does not do. The line must not imply it will.
      const readout = readCancelRollback(rollback({ originalsStillInPlace: landed }), 'move')
      expect(readout?.headline?.toLowerCase()).not.toMatch(/put .* back|bring .* back|undo/)
    })

    it('still reports scratch the destination kept, and takes the warning colour', () => {
      const readout = readCancelRollback(
        rollback({
          originalsStillInPlace: landed,
          stagedLeftovers: [
            { fate: 'refused', count: 1, exampleName: 'holiday.jpg.cmdr-tmp-4d1f9c', volumeName: 'Naspolya' },
          ],
        }),
        'move',
      )
      expect(readout?.headline).toContain('200 originals')
      expect(readout?.staged[0]).toContain('cmdr-tmp')
      expect(readout?.level).toBe('warn')
    })

    it('leaves every other cancelled operation silent, since only this one sets it', () => {
      expect(readCancelRollback(rollback({}), 'move')).toBeNull()
      expect(readCancelRollback(rollback({}), 'copy')).toBeNull()
    })
  })
})

describe('raiseCancelRollbackToast', () => {
  beforeEach(() => {
    vi.mocked(addToast).mockClear()
  })

  function timeoutOf(partial: Partial<CancelRollback>): number | undefined {
    raiseCancelRollbackToast(rollback(partial), 'copy')
    return vi.mocked(addToast).mock.calls[0]?.[1]?.timeoutMs
  }

  it('holds a one-line summary only as long as the app’s other result toasts', () => {
    // Regression: an always-true `staged !== null` once gave every summary the long hold.
    expect(timeoutOf({ outcome: 'rolledBack', reversed: 3 })).toBe(7000)
  })

  it('holds longer when there are reasons to read', () => {
    expect(timeoutOf({ outcome: 'partiallyRolledBack', reversed: 3, skips: [skip('drift', 2)] })).toBe(12000)
  })

  it('holds longer when Cmdr’s own scratch stayed behind', () => {
    const stagedLeftovers: StagedLeftovers[] = [
      { fate: 'refused', count: 1, exampleName: 'a.cmdr-tmp-1', volumeName: 'Naspolya' },
    ]
    expect(timeoutOf({ outcome: 'rolledBack', reversed: 3, stagedLeftovers })).toBe(12000)
  })
})
