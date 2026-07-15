import { describe, expect, it } from 'vitest'
import type { CoveredCount } from '$lib/tauri-commands'
import { shouldRepollPreview } from './media-index-preview-poll'

function covered(partial: Partial<CoveredCount>): CoveredCount {
  return { folders: 0, images: 0, pending: false, ...partial }
}

describe('shouldRepollPreview', () => {
  it('re-polls before the first result lands (null)', () => {
    expect(shouldRepollPreview(null)).toBe(true)
  })

  it('re-polls while the backend still reports pending', () => {
    // A drive is still scanning, or importance hasn''t scored the volume yet.
    expect(shouldRepollPreview(covered({ images: 0, pending: true }))).toBe(true)
    expect(shouldRepollPreview(covered({ images: 1200, pending: true }))).toBe(true)
  })

  it('stops once the count is resolved (not pending)', () => {
    expect(shouldRepollPreview(covered({ images: 1200, folders: 8, pending: false }))).toBe(false)
    // Zero-but-resolved is still resolved: nothing matches at this level, no re-poll.
    expect(shouldRepollPreview(covered({ images: 0, folders: 0, pending: false }))).toBe(false)
  })
})
