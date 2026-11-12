import { describe, expect, it } from 'vitest'
import { resolveMediaHitPath } from './media-path'

describe('resolveMediaHitPath', () => {
  it('passes an absolute local path through unchanged when the mount root is /', () => {
    expect(resolveMediaHitPath('/', '/Users/test_user_10/Pictures/x.jpg')).toBe('/Users/test_user_10/Pictures/x.jpg')
  })

  it('passes through unchanged for an empty mount root', () => {
    expect(resolveMediaHitPath('', '/Users/test_user_10/x.jpg')).toBe('/Users/test_user_10/x.jpg')
  })

  it('prepends the SMB mount root to a mount-relative hit path', () => {
    expect(resolveMediaHitPath('/Volumes/testshare', '/DCIM/x.jpg')).toBe('/Volumes/testshare/DCIM/x.jpg')
  })

  it('collapses a trailing slash on the mount root to a single separator', () => {
    expect(resolveMediaHitPath('/Volumes/testshare/', '/DCIM/x.jpg')).toBe('/Volumes/testshare/DCIM/x.jpg')
  })

  it('adds a separator when the relative path lacks a leading slash', () => {
    expect(resolveMediaHitPath('/Volumes/testshare', 'DCIM/x.jpg')).toBe('/Volumes/testshare/DCIM/x.jpg')
  })
})
