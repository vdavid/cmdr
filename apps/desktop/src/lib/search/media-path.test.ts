import { describe, expect, it } from 'vitest'
import { resolveMediaHitPath } from './media-path'

describe('resolveMediaHitPath', () => {
    it('passes an absolute local path through unchanged when the mount root is /', () => {
        expect(resolveMediaHitPath('/', '/Users/dave/Pictures/x.jpg')).toBe('/Users/dave/Pictures/x.jpg')
    })

    it('passes through unchanged for an empty mount root', () => {
        expect(resolveMediaHitPath('', '/Users/dave/x.jpg')).toBe('/Users/dave/x.jpg')
    })

    it('prepends the SMB mount root to a mount-relative hit path', () => {
        expect(resolveMediaHitPath('/Volumes/naspi', '/DCIM/x.jpg')).toBe('/Volumes/naspi/DCIM/x.jpg')
    })

    it('collapses a trailing slash on the mount root to a single separator', () => {
        expect(resolveMediaHitPath('/Volumes/naspi/', '/DCIM/x.jpg')).toBe('/Volumes/naspi/DCIM/x.jpg')
    })

    it('adds a separator when the relative path lacks a leading slash', () => {
        expect(resolveMediaHitPath('/Volumes/naspi', 'DCIM/x.jpg')).toBe('/Volumes/naspi/DCIM/x.jpg')
    })
})
