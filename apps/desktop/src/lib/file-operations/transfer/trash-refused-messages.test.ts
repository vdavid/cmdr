/**
 * What a refused trash says, and when it offers Full Disk Access as the next step.
 *
 * The offer is the delicate part: too eager and Cmdr blames a permission for every
 * failure until people stop reading it; too shy and the one user who needed the hint
 * gets "try again" under a folded disclosure, which is the report that started this.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import type { TrashRefusalKind } from '#lib/ipc/bindings.js'

// `vi.hoisted`, because the mock factories below are hoisted above this file's imports
// and run while another module is still loading: a plain `const` is still in its TDZ then.
const { isMacOS, fdaIsMissing } = vi.hoisted(() => ({
  isMacOS: vi.fn<() => boolean>(() => true),
  fdaIsMissing: vi.fn<() => boolean>(() => false),
}))

vi.mock('#lib/shortcuts/key-capture.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/shortcuts/key-capture.js')>()),
  isMacOS: () => isMacOS(),
}))
vi.mock('#lib/onboarding/fda-status.svelte.js', () => ({
  fdaIsMissing: () => fdaIsMissing(),
}))

import { getUserFriendlyMessage, mayBeAPermissionGrantAway } from './transfer-error-messages'

const ALL_REASONS: TrashRefusalKind[] = [
  'notPermitted',
  'locked',
  'folderNotWritable',
  'ownedByAnotherUser',
  'trashNotWritable',
  'noTrashForVolume',
  'other',
]

function refused(reason: TrashRefusalKind, itemCount = 7, onlineOnly = false) {
  return getUserFriendlyMessage({ type: 'trash_refused', itemCount, reason, message: 'os words', onlineOnly }, 'trash')
}

describe('a refused trash', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
    isMacOS.mockReturnValue(true)
    fdaIsMissing.mockReturnValue(false)
  })

  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('words every reason distinctly, so the dialog never repeats itself', () => {
    const messages = ALL_REASONS.map((r) => refused(r).message)
    expect(new Set(messages).size).toBe(ALL_REASONS.length)
    const suggestions = ALL_REASONS.map((r) => refused(r).suggestion)
    expect(new Set(suggestions).size).toBe(ALL_REASONS.length)
  })

  it('names how many items it could not take', () => {
    expect(refused('notPermitted', 7).message).toContain('7')
  })

  // Retrying a permission refusal produces the identical refusal. The old wording said
  // "Try again. If the problem persists, check the technical details below."
  it('never tells the user to try again', () => {
    for (const reason of ALL_REASONS) {
      expect(refused(reason).suggestion.toLowerCase()).not.toContain('try again')
    }
  })

  // A refusing trash folder blocks only the trash: a permanent delete never goes
  // near it, so it's the one way through Cmdr still has (cmdr-reports#104).
  it('offers a permanent delete only where the trash, not the item, is what refused', () => {
    expect(refused('noTrashForVolume').suggestion).toContain('Shift+F8')
    expect(refused('trashNotWritable').suggestion).toContain('Shift+F8')
  })

  // A permanent delete needs the same permission the trash was refused, so offering it
  // there sends the user into a second refusal. Regression anchor: ERR-JQK62.
  it('never offers a permanent delete for a permission refusal', () => {
    for (const reason of ['notPermitted', 'locked', 'folderNotWritable', 'ownedByAnotherUser'] as const) {
      expect(refused(reason).suggestion).not.toContain('Shift+F8')
    }
  })

  it('tells the user how to unlock a Locked item', () => {
    expect(refused('locked').suggestion).toContain('Get Info')
  })

  // The reporter of ERR-KKQHK had Full Disk Access and an ordinary file, and Finder
  // trashed it. With nothing more specific to say, pointing at Finder is the next step.
  it('points an unexplained permission refusal at Finder', () => {
    expect(refused('notPermitted').suggestion).toContain('Finder')
    expect(refused('ownedByAnotherUser').suggestion).toContain('Finder')
  })

  describe('the Full Disk Access line', () => {
    it('stays away while the grant is there', () => {
      fdaIsMissing.mockReturnValue(false)
      for (const reason of ALL_REASONS) {
        expect(refused(reason).suggestion).not.toContain('full disk access')
      }
    })

    it('joins a permission-shaped refusal when the grant is missing', () => {
      fdaIsMissing.mockReturnValue(true)
      expect(refused('notPermitted').suggestion).toContain('full disk access')
      expect(refused('noTrashForVolume').suggestion).toContain('full disk access')
    })

    // A refusal Cmdr couldn't classify is not evidence about permissions. Offering the
    // grant there is how the hint becomes noise people learn to skip.
    it('stays away from an unclassified refusal even when the grant is missing', () => {
      fdaIsMissing.mockReturnValue(true)
      expect(refused('other').suggestion).not.toContain('full disk access')
    })

    it('stays away off macOS, where the permission does not exist', () => {
      fdaIsMissing.mockReturnValue(true)
      isMacOS.mockReturnValue(false)
      expect(refused('notPermitted').suggestion).not.toContain('full disk access')
    })

    // Walks every variant, so a reason added later can't silently default into the offer.
    it('is decided per reason, with the unclassified one opted out', () => {
      expect(ALL_REASONS.filter(mayBeAPermissionGrantAway)).toEqual(['notPermitted', 'noTrashForVolume'])
    })

    // An evicted cloud file refuses as 513 (`notPermitted`) as readily as 3328, so the
    // reason alone can't tell the two apart. Sending someone to System Settings when
    // their real problem is a file that lives on Dropbox's servers is a wrong answer
    // they would act on. The backend flags the refusal; see `delete/cloud_trash.rs`.
    it('stays away from an online-only refusal, whichever reason the OS gave it', () => {
      fdaIsMissing.mockReturnValue(true)
      expect(refused('notPermitted', 7, true).suggestion).not.toContain('full disk access')
      expect(refused('noTrashForVolume', 7, true).suggestion).not.toContain('full disk access')
    })
  })
})
