/**
 * The failed row's reason, mapped from the wire snapshot onto the error
 * pipeline. The pipeline itself is covered in
 * `../transfer/transfer-error-messages.test.ts`; these tests are about the
 * mapping, which is the part a new operation type can silently break.
 */

import { describe, it, expect } from 'vitest'
import type { OperationSnapshot, WriteOperationError } from '$lib/ipc/bindings'
import { failureReasonFor } from './failure-reason'

function snapshot(error: WriteOperationError | null, over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: error === null ? 'running' : 'failed',
    source: '/Users/me/Documents/report.pdf',
    destination: '/Volumes/Backup',
    supportsRollback: false,
    error,
    ...over,
  }
}

describe('failureReasonFor', () => {
  it('says nothing about a row that carries no error', () => {
    expect(failureReasonFor(snapshot(null))).toBeNull()
  })

  it('picks the per-operation wording from the operation type', () => {
    const error: WriteOperationError = { type: 'permission_denied', path: '/protected', message: 'nope' }

    const copying = failureReasonFor(snapshot(error))
    const deleting = failureReasonFor(snapshot(error, { operationType: 'delete' }))

    expect(copying?.message).toBe("You don't have permission to copy files here.")
    expect(deleting?.message).toBe("You don't have permission to delete files here.")
  })

  it("carries the variant's own facts, not a generic sentence", () => {
    const error: WriteOperationError = {
      type: 'insufficient_space',
      required: 1073741824,
      available: 536870912,
      volumeName: 'Backup',
    }
    const reason = failureReasonFor(snapshot(error))
    expect(reason?.title).toBe('Not enough space')
    expect(reason?.message).toContain('1.00 GB')
    expect(reason?.message).toContain('512.00 MB')
  })

  it('borrows the copy wording for the operation types the catalog has no arm for', () => {
    // `archive_edit`, `rename`, `create_folder`, and `create_file` reach the
    // queue as wire types the `errors.write.*` catalog never phrases; falling
    // back keeps them readable instead of resolving a missing key.
    const error: WriteOperationError = { type: 'source_not_found', path: '/gone.zip' }
    for (const operationType of ['archive_edit', 'rename', 'create_folder', 'create_file'] as const) {
      expect(failureReasonFor(snapshot(error, { operationType }))?.message).toBe(
        'The file or folder you tried to copy no longer exists.',
      )
    }
  })
})
