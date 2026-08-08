/**
 * The foreground-operation slot: which operation the progress modal currently
 * owns, so ambient surfaces can stay quiet about it.
 *
 * What matters here is the lifecycle, not the storage: the slot must empty on
 * every route out of the dialog, and a late clear from a dialog that has already
 * handed the slot on must not silence the new owner.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import {
  setForegroundOperationId,
  getForegroundOperationId,
  clearForegroundOperation,
} from './foreground-operation.svelte'

beforeEach(() => {
  setForegroundOperationId(null)
})

describe('foreground operation slot', () => {
  it('is empty when no progress dialog owns an operation', () => {
    expect(getForegroundOperationId()).toBeNull()
  })

  it('holds the operation the dialog took ownership of', () => {
    setForegroundOperationId('op-1')
    expect(getForegroundOperationId()).toBe('op-1')
  })

  it('empties on an explicit null', () => {
    setForegroundOperationId('op-1')
    setForegroundOperationId(null)
    expect(getForegroundOperationId()).toBeNull()
  })

  it('clears the slot for the operation that owns it', () => {
    setForegroundOperationId('op-1')
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBeNull()
  })

  it('ignores a clear from an operation that no longer owns the slot', () => {
    // A dialog tearing down after the next one already claimed the slot: its
    // late clear must not silence the operation now in the foreground.
    setForegroundOperationId('op-1')
    setForegroundOperationId('op-2')
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBe('op-2')
  })

  it('tolerates a clear with nothing in the slot', () => {
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBeNull()
  })
})
