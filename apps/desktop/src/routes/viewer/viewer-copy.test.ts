import { describe, it, expect } from 'vitest'

import { COPY_CONFIRM_BYTES, COPY_REFUSE_BYTES, selectCopyAction } from './viewer-copy'

describe('selectCopyAction', () => {
  it('zero and tiny selections land in silent', () => {
    expect(selectCopyAction(0)).toBe('silent')
    expect(selectCopyAction(1)).toBe('silent')
    expect(selectCopyAction(1024)).toBe('silent')
  })

  it('boundary: just under 10 MiB is silent', () => {
    expect(selectCopyAction(COPY_CONFIRM_BYTES - 1)).toBe('silent')
  })

  it('boundary: exactly 10 MiB is confirm', () => {
    expect(selectCopyAction(COPY_CONFIRM_BYTES)).toBe('confirm')
  })

  it('between confirm and refuse is confirm', () => {
    expect(selectCopyAction(50 * 1024 * 1024)).toBe('confirm')
  })

  it('boundary: just under 100 MiB is confirm', () => {
    expect(selectCopyAction(COPY_REFUSE_BYTES - 1)).toBe('confirm')
  })

  it('boundary: exactly 100 MiB is refuse', () => {
    expect(selectCopyAction(COPY_REFUSE_BYTES)).toBe('refuse')
  })

  it('large selections are refuse', () => {
    expect(selectCopyAction(500 * 1024 * 1024)).toBe('refuse')
    expect(selectCopyAction(4 * 1024 * 1024 * 1024)).toBe('refuse')
  })

  it('negative input defends to silent', () => {
    expect(selectCopyAction(-1)).toBe('silent')
    expect(selectCopyAction(-1000)).toBe('silent')
  })
})
