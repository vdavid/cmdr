/** The shared leaves in `types.ts` that carry logic of their own. */

import { describe, expect, it } from 'vitest'
import { formatBytes } from './types'

describe('formatBytes', () => {
  it('formats bytes across units', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1536)).toBe('1.50 KB')
    expect(formatBytes(10 * 1024)).toBe('10.0 KB')
    expect(formatBytes(1_048_576)).toBe('1.00 MB')
    expect(formatBytes(2 * 1024 ** 3)).toBe('2.00 GB')
  })
})
