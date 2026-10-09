import { describe, it, expect } from 'vitest'
import { formatOperationPath } from './operation-path'

describe('formatOperationPath', () => {
  it('leaves a path that names a place on this Mac alone', () => {
    expect(formatOperationPath('/Users/me/photos/a.raw', null)).toBe('/Users/me/photos/a.raw')
  })

  it('puts the volume name in front of a path relative to that volume', () => {
    expect(formatOperationPath('/DCIM/x', 'Pixel 8')).toBe('Pixel 8 › /DCIM/x')
  })

  it('treats an empty volume name as no volume', () => {
    expect(formatOperationPath('/DCIM/x', '')).toBe('/DCIM/x')
  })
})
