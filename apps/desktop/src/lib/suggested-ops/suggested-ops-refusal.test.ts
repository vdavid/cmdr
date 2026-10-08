/**
 * The reason under a suggestion whose approval didn't start. The write-error pipeline itself is
 * covered in `../file-operations/transfer/transfer-error-messages.test.ts`; these pin that a
 * suggestion reads exactly as the clicked operation would, verb for verb.
 */

import { describe, it, expect } from 'vitest'
import type { WriteOperationError } from '$lib/ipc/bindings'
import { getUserFriendlyMessage } from '$lib/file-operations/transfer/transfer-error-messages'
import { refusalReason } from './suggested-ops-refusal'

const sourceGone: WriteOperationError = { type: 'source_not_found', path: '/Users/someone/Downloads/one.dmg' }

describe('refusalReason', () => {
  it('words an engine refusal exactly as the clicked operation would', () => {
    const reason = refusalReason({ kind: 'refused', error: sourceGone }, 'trash')
    const clicked = getUserFriendlyMessage(sourceGone, 'trash')

    expect(reason).toEqual({ message: clicked.message, suggestion: clicked.suggestion })
  })

  it('reads a rename as the move it runs as, and an extract as the copy it is', () => {
    expect(refusalReason({ kind: 'refused', error: sourceGone }, 'rename').message).toBe(
      getUserFriendlyMessage(sourceGone, 'move').message,
    )
    expect(refusalReason({ kind: 'refused', error: sourceGone }, 'extract').message).toBe(
      getUserFriendlyMessage(sourceGone, 'copy').message,
    )
  })

  it('says a phone or server nobody connected is not connected yet, never that it left', () => {
    const reason = refusalReason({ kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM' } }, 'move')

    expect(reason.message).toContain('hasn’t connected')
  })

  it('has its own words for the two refusals only suggestions have', () => {
    expect(refusalReason({ kind: 'nothingToRun' }, 'trash').message).toContain('nothing ran')
    expect(refusalReason({ kind: 'couldNotStart' }, 'move').message).toContain('nothing ran')
  })
})
