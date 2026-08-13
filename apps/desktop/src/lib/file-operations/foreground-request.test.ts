/**
 * Resolving a Show request against the MAIN window's own snapshot: the id
 * crosses the window boundary, and everything the dialog renders comes from what
 * this window knows about that operation.
 */

import { describe, it, expect } from 'vitest'
import { adoptedOperationFor } from './foreground-request'
import type { OperationRow } from './queue/operations-store.svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'

function row(operationId: string, operationType: OperationSnapshot['operationType'] = 'copy'): OperationRow {
  return {
    snapshot: {
      operationId,
      operationType,
      status: 'running',
      source: '/Volumes/Card/DCIM',
      destination: '/Users/me/import',
      supportsRollback: true,
      error: null,
    },
    progress: null,
  }
}

describe('adoptedOperationFor', () => {
  it('describes the operation the id names, and nothing more', () => {
    const rows = [row('op-1'), row('op-2', 'move')]

    expect(adoptedOperationFor(rows, 'op-2')).toEqual({
      operationId: 'op-2',
      operationType: 'move',
      sourcePath: '/Volumes/Card/DCIM',
      destinationPath: '/Users/me/import',
    })
  })

  it('has nothing for an operation this window no longer lists', () => {
    // Ordinary, not defensive: an operation that ended between the click and the
    // delivery has left the snapshot, and its queue row went with it.
    expect(adoptedOperationFor([row('op-1')], 'op-gone')).toBeNull()
  })

  it('has nothing for an instant operation, which shows no progress at all', () => {
    expect(adoptedOperationFor([row('op-1', 'rename')], 'op-1')).toBeNull()
    expect(adoptedOperationFor([row('op-1', 'create_folder')], 'op-1')).toBeNull()
    expect(adoptedOperationFor([row('op-1', 'create_file')], 'op-1')).toBeNull()
  })
})
