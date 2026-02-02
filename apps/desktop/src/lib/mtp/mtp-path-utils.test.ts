/**
 * Tests for MTP path utility functions
 */
import { describe, it, expect } from 'vitest'
import {
    parseMtpPath,
    constructMtpPath,
    isMtpVolumeId,
    getMtpParentPath,
    joinMtpPath,
    getMtpDisplayPath,
} from './mtp-path-utils'

describe('parseMtpPath', () => {
    it('parses a valid MTP path with device and storage IDs', () => {
        const result = parseMtpPath('mtp://0-5/65537')
        expect(result).toEqual({
            deviceId: '0-5',
            storageId: 65537,
            path: '',
        })
    })

    it('parses a valid MTP path with nested path', () => {
        const result = parseMtpPath('mtp://0-5/65537/DCIM/Camera')
        expect(result).toEqual({
            deviceId: '0-5',
            storageId: 65537,
            path: 'DCIM/Camera',
        })
    })

    it('parses a valid MTP path with single folder', () => {
        const result = parseMtpPath('mtp://device-123/1/Documents')
        expect(result).toEqual({
            deviceId: 'device-123',
            storageId: 1,
            path: 'Documents',
        })
    })

    it('returns null for non-MTP paths', () => {
        expect(parseMtpPath('/Users/test')).toBeNull()
        expect(parseMtpPath('file://path')).toBeNull()
        expect(parseMtpPath('')).toBeNull()
    })

    it('returns null for invalid MTP paths', () => {
        expect(parseMtpPath('mtp://')).toBeNull()
        expect(parseMtpPath('mtp://device')).toBeNull()
        expect(parseMtpPath('mtp://device/notanumber')).toBeNull()
    })
})

describe('constructMtpPath', () => {
    it('constructs a base MTP path without inner path', () => {
        expect(constructMtpPath('0-5', 65537)).toBe('mtp://0-5/65537')
    })

    it('constructs a path with empty string inner path', () => {
        expect(constructMtpPath('0-5', 65537, '')).toBe('mtp://0-5/65537')
    })

    it('constructs a path with "/" inner path', () => {
        expect(constructMtpPath('0-5', 65537, '/')).toBe('mtp://0-5/65537')
    })

    it('constructs a path with nested inner path', () => {
        expect(constructMtpPath('0-5', 65537, 'DCIM/Camera')).toBe('mtp://0-5/65537/DCIM/Camera')
    })

    it('normalizes inner path with leading slash', () => {
        expect(constructMtpPath('0-5', 65537, '/DCIM/Camera')).toBe('mtp://0-5/65537/DCIM/Camera')
    })

    it('handles single folder path', () => {
        expect(constructMtpPath('device', 1, 'Downloads')).toBe('mtp://device/1/Downloads')
    })
})

describe('isMtpVolumeId', () => {
    it('returns true for volume ID with colon format', () => {
        expect(isMtpVolumeId('0-5:65537')).toBe(true)
        expect(isMtpVolumeId('device-123:1')).toBe(true)
    })

    it('returns true for volume ID with mtp- prefix', () => {
        expect(isMtpVolumeId('mtp-336592896')).toBe(true)
        expect(isMtpVolumeId('mtp-336592896:65537')).toBe(true)
    })

    it('returns false for local volume IDs', () => {
        expect(isMtpVolumeId('local')).toBe(false)
        expect(isMtpVolumeId('/')).toBe(false)
        expect(isMtpVolumeId('Macintosh HD')).toBe(false)
    })
})

describe('getMtpParentPath', () => {
    it('returns null for storage root path', () => {
        expect(getMtpParentPath('mtp://0-5/65537')).toBeNull()
        expect(getMtpParentPath('mtp://0-5/65537/')).toBeNull() // Edge case: parseMtpPath returns empty path
    })

    it('returns storage root for single-level path', () => {
        expect(getMtpParentPath('mtp://0-5/65537/DCIM')).toBe('mtp://0-5/65537')
    })

    it('returns parent folder for nested path', () => {
        expect(getMtpParentPath('mtp://0-5/65537/DCIM/Camera')).toBe('mtp://0-5/65537/DCIM')
    })

    it('returns parent for deeply nested path', () => {
        expect(getMtpParentPath('mtp://0-5/65537/a/b/c/d')).toBe('mtp://0-5/65537/a/b/c')
    })

    it('returns null for non-MTP paths', () => {
        expect(getMtpParentPath('/Users/test')).toBeNull()
    })
})

describe('joinMtpPath', () => {
    it('joins child to storage root', () => {
        expect(joinMtpPath('mtp://0-5/65537', 'DCIM')).toBe('mtp://0-5/65537/DCIM')
    })

    it('joins child to nested path', () => {
        expect(joinMtpPath('mtp://0-5/65537/DCIM', 'Camera')).toBe('mtp://0-5/65537/DCIM/Camera')
    })

    it('returns original for non-MTP paths', () => {
        expect(joinMtpPath('/Users/test', 'Documents')).toBe('/Users/test')
    })
})

describe('getMtpDisplayPath', () => {
    it('returns "/" for storage root', () => {
        expect(getMtpDisplayPath('mtp://0-5/65537')).toBe('/')
    })

    it('returns display path for nested path', () => {
        expect(getMtpDisplayPath('mtp://0-5/65537/DCIM/Camera')).toBe('/DCIM/Camera')
    })

    it('returns display path for single folder', () => {
        expect(getMtpDisplayPath('mtp://0-5/65537/Documents')).toBe('/Documents')
    })

    it('returns original path for non-MTP paths', () => {
        expect(getMtpDisplayPath('/Users/test')).toBe('/Users/test')
    })
})
