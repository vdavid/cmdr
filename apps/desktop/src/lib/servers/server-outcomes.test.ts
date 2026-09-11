/**
 * The two backend answers the sheet reads, in the app's own vocabulary.
 *
 * ❗ Each refusal keeps a kind of its own, because the kind is what picks the
 * sentence AND the field it lands under (`connect-refusals.ts`). Folding one into
 * another puts a true sentence under the wrong field.
 */

import { describe, expect, it } from 'vitest'
import type { SavedServerOutcome } from '$lib/ipc/bindings'
import type { ConnectRefusalKind } from './connect-refusals'
import { readConnectOutcome, readSavedServerOutcome } from './server-outcomes'

describe('readSavedServerOutcome', () => {
  it('reads a save that landed as saved', () => {
    expect(readSavedServerOutcome({ outcome: 'saved' })).toEqual({ kind: 'saved' })
  })

  it('gives every refusal its own kind', () => {
    const cases: [SavedServerOutcome, ConnectRefusalKind][] = [
      [{ outcome: 'start_folder_outside_root' }, 'start_folder_outside_root'],
      [{ outcome: 'root_not_found' }, 'root_not_found'],
      [{ outcome: 'start_folder_not_found' }, 'start_folder_not_found'],
      // ❗ Not the connect path's `unreachable`: nothing was saved, and the
      // address a dial refusal points at is locked in edit mode.
      [{ outcome: 'unreachable' }, 'save_unconfirmed'],
    ]
    for (const [outcome, refusal] of cases) {
      expect(readSavedServerOutcome(outcome)).toEqual({ kind: 'refused', refusal })
    }
  })
})

describe('readConnectOutcome', () => {
  it('reads a start folder outside the root as its own refusal, not as a bad address', () => {
    expect(readConnectOutcome({ outcome: 'start_folder_outside_root' })).toEqual({
      kind: 'refused',
      refusal: 'start_folder_outside_root',
    })
  })
})
