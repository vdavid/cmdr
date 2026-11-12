/**
 * The two backend answers the sheet reads, in the app's own vocabulary.
 *
 * ❗ Each refusal keeps a kind of its own, because the kind is what picks the
 * sentence AND the field it lands under (`connect-refusals.ts`). Folding one into
 * another puts a true sentence under the wrong field.
 */

import { describe, expect, it } from 'vitest'
import type { SavedServerOutcome } from '#lib/ipc/bindings.js'
import type { ConnectRefusalKind } from './connect-refusals'
import { needsAHuman, readConnectOutcome, readEjectAndSaveOutcome, readSavedServerOutcome } from './server-outcomes'

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
      [{ outcome: 'secret_not_moved' }, 'secret_not_moved'],
      [{ outcome: 'account_changed' }, 'account_changed'],
      [{ outcome: 'operation_running' }, 'operation_running'],
    ]
    for (const [outcome, refusal] of cases) {
      expect(readSavedServerOutcome(outcome)).toEqual({ kind: 'refused', refusal })
    }
  })

  it('names the saved server that already holds the address a move asked for', () => {
    expect(readSavedServerOutcome({ outcome: 'address_taken', name: 'Testnas' })).toEqual({
      kind: 'refused',
      refusal: 'address_taken',
      takenBy: 'Testnas',
    })
  })

  it('names the share still mounted from the old address an SMB move waits on', () => {
    expect(readSavedServerOutcome({ outcome: 'share_mounted', name: 'Photos' })).toEqual({
      kind: 'refused',
      refusal: 'share_mounted',
      share: 'Photos',
    })
  })
})

describe('readEjectAndSaveOutcome', () => {
  it('reads a settled run as the save it answered', () => {
    expect(readEjectAndSaveOutcome({ outcome: 'settled', answer: { outcome: 'saved' } })).toEqual({ kind: 'saved' })
    expect(readEjectAndSaveOutcome({ outcome: 'settled', answer: { outcome: 'operation_running' } })).toEqual({
      kind: 'refused',
      refusal: 'operation_running',
    })
  })

  it('names the share whose eject was refused, carrying why', () => {
    const error = { type: 'timedOut' } as const
    expect(readEjectAndSaveOutcome({ outcome: 'eject_refused', name: 'Photos', error })).toEqual({
      kind: 'refused',
      refusal: 'share_eject_refused',
      share: 'Photos',
      ejectError: error,
    })
  })

  it('reads a run the sheet called off as cancelled', () => {
    expect(readEjectAndSaveOutcome({ outcome: 'cancelled' })).toEqual({ kind: 'cancelled' })
  })
})

describe('readConnectOutcome', () => {
  it('reads a start folder outside the root as its own refusal, not as a bad address', () => {
    expect(readConnectOutcome({ outcome: 'start_folder_outside_root' })).toEqual({
      kind: 'refused',
      refusal: 'start_folder_outside_root',
    })
  })

  it('gives every S3 refusal its own kind', () => {
    const kinds = [
      'access_denied',
      'bucket_list_refused',
      'bucket_not_found',
      'clock_skewed',
      'not_an_s3_endpoint',
    ] as const
    for (const kind of kinds) {
      expect(readConnectOutcome({ outcome: kind })).toEqual({ kind: 'refused', refusal: kind })
    }
  })

  it('gives every NFS refusal its own kind', () => {
    for (const kind of ['export_not_found', 'export_refused', 'reserved_port_required', 'no_nfs_v3'] as const) {
      expect(readConnectOutcome({ outcome: kind })).toEqual({ kind: 'refused', refusal: kind })
      // None is a credential question: NFS has no secret a sheet could ask for.
      expect(needsAHuman(readConnectOutcome({ outcome: kind }))).toBe(false)
    }
  })

  it('carries the region a bucket lives in, and leaves it out when the server named none', () => {
    expect(readConnectOutcome({ outcome: 'region_mismatch', region: 'us-east-2' })).toEqual({
      kind: 'refused',
      refusal: 'region_mismatch',
      region: 'us-east-2',
    })
    expect(readConnectOutcome({ outcome: 'region_mismatch', region: null })).toEqual({
      kind: 'refused',
      refusal: 'region_mismatch',
    })
  })
})

describe('needsAHuman', () => {
  it('sends an S3 bucket that turned the key away to the sheet, since a wrong secret is one thing it means', () => {
    expect(needsAHuman({ kind: 'refused', refusal: 'access_denied' })).toBe(true)
  })

  it('keeps a key that can’t list buckets in the pane: no secret typed into a sheet opens the account root', () => {
    expect(needsAHuman({ kind: 'refused', refusal: 'bucket_list_refused' })).toBe(false)
  })
})
