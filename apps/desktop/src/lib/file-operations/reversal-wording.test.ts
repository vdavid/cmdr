/**
 * Unit tests for the reversal wording: which reversal an `OpKind` earns, and the
 * catalog key each surface names it with.
 *
 * The point of the table below is that a HUMAN can read it. Every row puts the
 * question the user answers next to the two things they read while it runs, in
 * resolved English, so a copy edit that makes the progress bar contradict the
 * confirmation is visible in the diff rather than only on someone's screen.
 */

import { describe, it, expect } from 'vitest'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { OpKind } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'
import {
  type RollbackConfirmVariant,
  rollbackConfirmVariant,
  reversalLabelKey,
  reversalTitleKey,
} from './reversal-wording'

/** The confirmation body each variant raises, mirroring `RollbackConfirmDialog`'s
 *  own `wording()` map. Duplicated here on purpose: if the dialog's map is edited
 *  away from this one, the agreement test below is what notices. */
const CONFIRM_BODY: Record<Exclude<RollbackConfirmVariant, 'stopAndDelete'>, MessageKey> = {
  undoByDeleting: 'fileOperations.rollbackConfirm.bodyUndoByDeleting',
  undoByMovingBack: 'fileOperations.rollbackConfirm.bodyUndoByMovingBack',
  undoByRenamingBack: 'fileOperations.rollbackConfirm.bodyUndoByRenamingBack',
}

describe('rollbackConfirmVariant', () => {
  it('picks the wording by what the inverse DOES, mirroring the backend’s inverse_kind', () => {
    const cases: Record<OpKind, RollbackConfirmVariant> = {
      copy: 'undoByDeleting',
      createFolder: 'undoByDeleting',
      createFile: 'undoByDeleting',
      archiveEdit: 'undoByDeleting',
      // Never reachable: a permanent delete is gated as not-rollbackable, so no
      // button ever appears on one. Mapped anyway so a new kind is a compile error.
      delete: 'undoByDeleting',
      move: 'undoByMovingBack',
      trash: 'undoByMovingBack',
      rename: 'undoByRenamingBack',
    }
    for (const [kind, variant] of Object.entries(cases)) {
      expect(rollbackConfirmVariant(kind as OpKind)).toBe(variant)
    }
  })

  it('never words a move or a rename as a delete', () => {
    // The one mistake this mapping exists to prevent: reusing the copy wording on
    // an operation whose reversal takes nothing away.
    expect(rollbackConfirmVariant('move')).not.toBe('undoByDeleting')
    expect(rollbackConfirmVariant('rename')).not.toBe('undoByDeleting')
  })
})

describe('the reversal wording, one kind at a time', () => {
  // Undoing a MOVE and undoing a TRASH both carry files home.
  it.each(['move', 'trash'] as const)('words undoing a %s as putting files back', (kind) => {
    const variant = rollbackConfirmVariant(kind)
    expect(tString(reversalLabelKey(variant))).toBe('Putting files back')
    expect(tString(reversalTitleKey(variant), { count: 1240, countText: formatInteger(1240) })).toBe(
      'Putting 1,240 files back...',
    )
  })

  // Undoing a copy, a new folder, a new file, or a compress removes what got made.
  it.each(['copy', 'createFolder', 'createFile', 'archiveEdit'] as const)(
    'words undoing a %s as deleting what it created',
    (kind) => {
      const variant = rollbackConfirmVariant(kind)
      expect(tString(reversalLabelKey(variant))).toBe('Deleting what it created')
      expect(tString(reversalTitleKey(variant), { count: 1240, countText: formatInteger(1240) })).toBe(
        'Deleting the 1,240 files it created...',
      )
    },
  )

  it('words undoing a rename as putting the old names back', () => {
    const variant = rollbackConfirmVariant('rename')
    expect(tString(reversalLabelKey(variant))).toBe('Putting the old names back')
    expect(tString(reversalTitleKey(variant), { count: 12, countText: formatInteger(12) })).toBe(
      'Putting 12 old names back...',
    )
  })

  it('says what it will do before the journal count lands, rather than naming zero files', () => {
    // The first frames of a reversal, and every reversal whose rollback units are
    // all directories. A title reading "Putting 0 files back" would be a lie the
    // next frame corrects.
    for (const kind of ['move', 'copy', 'rename'] as const) {
      const title = tString(reversalTitleKey(rollbackConfirmVariant(kind)), {
        count: 0,
        countText: formatInteger(0),
      })
      expect(title).not.toContain('0')
      expect(title.length).toBeGreaterThan(0)
    }
  })

  it('groups a big count for the locale rather than printing raw digits', () => {
    expect(tString(reversalTitleKey('undoByMovingBack'), { count: 1240, countText: formatInteger(1240) })).toContain(
      '1,240',
    )
  })

  it('says one file without a number, which is how a person would say it', () => {
    expect(tString(reversalTitleKey('undoByMovingBack'), { count: 1, countText: formatInteger(1) })).toBe(
      'Putting the file back...',
    )
  })
})

describe('the running bar agrees with the question that raised it', () => {
  // The defect this whole module exists to prevent: the confirmation promises a
  // restore and two seconds later the progress bar announces a delete. Both read
  // the same variant, so the only way they can disagree is a copy edit to one of
  // them — which is what these assertions catch.
  const OPERATION_KINDS: OpKind[] = [
    'copy',
    'move',
    'delete',
    'trash',
    'rename',
    'createFolder',
    'createFile',
    'archiveEdit',
  ]

  it.each(OPERATION_KINDS)('never promises a restore for %s and then deletes, or the reverse', (kind) => {
    const variant = rollbackConfirmVariant(kind)
    // `stopAndDelete` is the in-flight variant; no finished operation maps to it.
    expect(variant).not.toBe('stopAndDelete')
    const body = tString(CONFIRM_BODY[variant as Exclude<RollbackConfirmVariant, 'stopAndDelete'>])
    const label = tString(reversalLabelKey(variant))

    // "Deletes" in the question means "deleting" on the bar, and neither of the
    // two restoring variants may say it.
    const questionDeletes = body.includes('deletes')
    const barDeletes = label.includes('Deleting')
    expect(barDeletes).toBe(questionDeletes)
  })

  it('gives each of the three reversals its own words, so two of them can’t read alike', () => {
    const labels = (['undoByMovingBack', 'undoByDeleting', 'undoByRenamingBack'] as const).map((v) =>
      tString(reversalLabelKey(v)),
    )
    expect(new Set(labels).size).toBe(3)
  })
})
