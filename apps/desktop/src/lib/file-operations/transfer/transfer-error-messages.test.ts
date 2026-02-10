import { describe, expect, it } from 'vitest'
import { getUserFriendlyMessage, getTechnicalDetails } from './transfer-error-messages'
import type { WriteOperationError } from '$lib/file-explorer/types'

describe('getUserFriendlyMessage', () => {
    describe('copy operation (default)', () => {
        it('returns user-friendly message for source_not_found error', () => {
            const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe("Couldn't find the file")
            expect(result.message).toContain('copy')
            expect(result.message).toContain('no longer exists')
        })

        it('returns user-friendly message for destination_exists error', () => {
            const error: WriteOperationError = { type: 'destination_exists', path: '/dest/file.txt' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe('File already exists')
        })

        it('returns user-friendly message for permission_denied error', () => {
            const error: WriteOperationError = {
                type: 'permission_denied',
                path: '/protected/dir',
                message: 'Operation not permitted',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe("Couldn't access this location")
            expect(result.message).toContain('copy')
        })

        it('returns user-friendly message for insufficient_space error', () => {
            const error: WriteOperationError = {
                type: 'insufficient_space',
                required: 1073741824,
                available: 536870912,
                volumeName: 'Test Volume',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe('Not enough space')
            expect(result.message).toContain('1.0 GB')
            expect(result.message).toContain('512.0 MB')
        })

        it('returns user-friendly message for same_location error', () => {
            const error: WriteOperationError = { type: 'same_location', path: '/same/path' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe("Can't copy to the same location")
        })

        it('returns user-friendly message for destination_inside_source error', () => {
            const error: WriteOperationError = {
                type: 'destination_inside_source',
                source: '/folder',
                destination: '/folder/subfolder',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe("Can't copy a folder into itself")
        })

        it('returns user-friendly message for symlink_loop error', () => {
            const error: WriteOperationError = { type: 'symlink_loop', path: '/path/with/loop' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe('Link loop detected')
        })

        it('returns user-friendly message for cancelled error', () => {
            const error: WriteOperationError = { type: 'cancelled', message: 'User cancelled' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe('Copy cancelled')
            expect(result.message).toContain('copy')
        })

        it('returns "Copy failed" for io_error', () => {
            const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
            const result = getUserFriendlyMessage(error)

            expect(result.title).toBe('Copy failed')
        })
    })

    describe('move operation', () => {
        it('uses "move" in source_not_found message', () => {
            const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.message).toContain('move')
            expect(result.message).not.toContain('copy')
        })

        it('uses "move" in permission_denied message', () => {
            const error: WriteOperationError = {
                type: 'permission_denied',
                path: '/protected',
                message: 'denied',
            }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.message).toContain('move')
        })

        it('uses "Move" in same_location title', () => {
            const error: WriteOperationError = { type: 'same_location', path: '/same/path' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.title).toBe("Can't move to the same location")
        })

        it('uses "Move" in destination_inside_source title', () => {
            const error: WriteOperationError = {
                type: 'destination_inside_source',
                source: '/folder',
                destination: '/folder/sub',
            }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.title).toBe("Can't move a folder into itself")
            expect(result.suggestion).toContain('moving')
        })

        it('uses "Move cancelled" for cancelled error', () => {
            const error: WriteOperationError = { type: 'cancelled', message: 'User cancelled' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.title).toBe('Move cancelled')
            expect(result.message).toContain('move')
        })

        it('uses "Move failed" for io_error', () => {
            const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.title).toBe('Move failed')
        })

        it('uses "move" in generic io_error message', () => {
            const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Unknown XYZ' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.message).toBe("Couldn't move the file.")
        })

        it('uses "move" for device disconnection io_error', () => {
            const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Device disconnected' }
            const result = getUserFriendlyMessage(error, 'move')

            expect(result.message).toContain('disconnected during the move')
        })
    })

    describe('io_error messages', () => {
        it('detects device disconnection', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'Device disconnected during transfer',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain('disconnected')
            expect(result.suggestion).toContain('properly connected')
        })

        it('detects connection timeout', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'Connection timed out',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain('interrupted')
        })

        it('detects read errors', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'Read error on source file',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain("Couldn't read")
        })

        it('detects write errors', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'Write error on destination',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain("Couldn't write")
        })

        it('detects filename too long', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'File name too long for destination filesystem',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain('too long')
        })

        it('returns generic message for unknown IO errors', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '/path',
                message: 'Some unknown error XYZ123',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toBe("Couldn't copy the file.")
        })

        it('detects read-only device errors', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '',
                message: 'Error for mtp-35651584: This device is read-only. You can copy files from it, but not to it.',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).toContain('read-only')
            expect(result.suggestion).toContain('different destination')
        })

        it('does not misinterpret read-only as read error', () => {
            const error: WriteOperationError = {
                type: 'io_error',
                path: '',
                message: 'Error for mtp-35651584: This device is read-only.',
            }
            const result = getUserFriendlyMessage(error)

            expect(result.message).not.toContain("Couldn't read from the source")
            expect(result.message).toContain('read-only')
        })
    })
})

describe('getTechnicalDetails', () => {
    it('includes path for source_not_found error', () => {
        const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
        const result = getTechnicalDetails(error)

        expect(result).toContain('Path: /path/to/file.txt')
        expect(result).toContain('Error type: source_not_found')
    })

    it('includes path and message for permission_denied error', () => {
        const error: WriteOperationError = {
            type: 'permission_denied',
            path: '/protected/dir',
            message: 'Operation not permitted',
        }
        const result = getTechnicalDetails(error)

        expect(result).toContain('Path: /protected/dir')
        expect(result).toContain('Details: Operation not permitted')
    })

    it('includes space info for insufficient_space error', () => {
        const error: WriteOperationError = {
            type: 'insufficient_space',
            required: 1073741824,
            available: 536870912,
            volumeName: 'Test Volume',
        }
        const result = getTechnicalDetails(error)

        expect(result).toContain('Required: 1.0 GB')
        expect(result).toContain('Available: 512.0 MB')
        expect(result).toContain('Volume: Test Volume')
    })

    it('includes source and destination for destination_inside_source error', () => {
        const error: WriteOperationError = {
            type: 'destination_inside_source',
            source: '/folder',
            destination: '/folder/subfolder',
        }
        const result = getTechnicalDetails(error)

        expect(result).toContain('Source: /folder')
        expect(result).toContain('Destination: /folder/subfolder')
    })

    it('includes path and error message for io_error', () => {
        const error: WriteOperationError = {
            type: 'io_error',
            path: '/path/to/file',
            message: 'Device disconnected',
        }
        const result = getTechnicalDetails(error)

        expect(result).toContain('Path: /path/to/file')
        expect(result).toContain('Error: Device disconnected')
        expect(result).toContain('Error type: io_error')
    })
})

describe('error messages are volume-agnostic', () => {
    it('does not mention MTP in any error message', () => {
        const errors: WriteOperationError[] = [
            { type: 'source_not_found', path: '/mtp-device/file.txt' },
            { type: 'permission_denied', path: '/mtp-device/protected', message: 'MTP error' },
            { type: 'io_error', path: '/mtp', message: 'MTP transfer failed' },
        ]

        for (const error of errors) {
            const result = getUserFriendlyMessage(error)
            const allText = `${result.title} ${result.message} ${result.suggestion}`.toLowerCase()
            expect(allText).not.toContain('mtp')
        }
    })

    it('does not mention SMB in any error message', () => {
        const errors: WriteOperationError[] = [
            { type: 'source_not_found', path: '//server/share/file.txt' },
            { type: 'permission_denied', path: '//server/share', message: 'SMB error' },
            { type: 'io_error', path: '/smb', message: 'SMB connection failed' },
        ]

        for (const error of errors) {
            const result = getUserFriendlyMessage(error)
            const allText = `${result.title} ${result.message} ${result.suggestion}`.toLowerCase()
            expect(allText).not.toContain('smb')
        }
    })
})
