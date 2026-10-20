/**
 * Tests for `handleTagsSkipped`: the warning raised when a tag toggle leaves files
 * whose existing tags couldn't be read untouched.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { addToastMock } = vi.hoisted(() => ({
  addToastMock: vi.fn(() => 'toast-id'),
}))

vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: addToastMock,
}))

import { handleTagsSkipped, TAGS_SKIPPED_TOAST_ID } from './tags-skipped-toast'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('handleTagsSkipped', () => {
  it('raises one deduped warning naming the count', () => {
    handleTagsSkipped(3)

    expect(addToastMock).toHaveBeenCalledOnce()
    const [message, options] = addToastMock.mock.calls[0] as unknown as [string, unknown]
    expect(message).toContain('3 items')
    expect(options).toEqual({ id: TAGS_SKIPPED_TOAST_ID, level: 'warn' })
  })

  it('uses the singular form for one file', () => {
    handleTagsSkipped(1)

    const [message] = addToastMock.mock.calls[0] as unknown as [string]
    expect(message).toContain('one item')
  })
})
