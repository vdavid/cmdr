import { afterEach, describe, expect, it, vi } from 'vitest'
import { getUserFriendlyMessage, getTechnicalDetails } from './transfer-error-messages'
import type { WriteOperationError } from '$lib/file-explorer/types'

// Mock navigator to control isMacOS() behavior
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')

function setMacOS(isMac: boolean) {
  navigatorSpy.mockReturnValue({
    userAgent: isMac ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' : 'Mozilla/5.0 (X11; Linux x86_64)',
  } as Navigator)
}

afterEach(() => {
  navigatorSpy.mockReset()
})

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

    it('uses "move" for device disconnection', () => {
      const error: WriteOperationError = { type: 'device_disconnected', path: '/path' }
      const result = getUserFriendlyMessage(error, 'move')

      expect(result.message).toContain('disconnected during the move')
    })
  })

  describe('structured error variants', () => {
    it('handles device_disconnected', () => {
      const error: WriteOperationError = { type: 'device_disconnected', path: '/path' }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('Device disconnected')
      expect(result.message).toContain('disconnected')
      expect(result.suggestion).toContain('properly connected')
    })

    it('handles connection_interrupted', () => {
      const error: WriteOperationError = { type: 'connection_interrupted', path: '/path' }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain('interrupted')
    })

    it('handles read_error', () => {
      const error: WriteOperationError = {
        type: 'read_error',
        path: '/path',
        message: 'Failed to read from source',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain("Couldn't read")
    })

    it('handles write_error', () => {
      const error: WriteOperationError = {
        type: 'write_error',
        path: '/path',
        message: 'Failed to write to destination',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain("Couldn't write")
    })

    it('handles name_too_long', () => {
      const error: WriteOperationError = { type: 'name_too_long', path: '/path/very-long-name' }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('Name too long')
      expect(result.message).toContain('too long')
    })

    it('handles read_only_device', () => {
      const error: WriteOperationError = { type: 'read_only_device', path: '/path', deviceName: 'My Phone' }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain('My Phone')
      expect(result.message).toContain('read-only')
      expect(result.suggestion).toContain('different destination')
    })

    it('handles read_only_device without device name', () => {
      const error: WriteOperationError = { type: 'read_only_device', path: '/path', deviceName: null }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain('The target device')
      expect(result.message).toContain('read-only')
    })

    it('handles invalid_name', () => {
      const error: WriteOperationError = {
        type: 'invalid_name',
        path: '/path/bad:name',
        message: 'Colon not allowed',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('Invalid file name')
      expect(result.message).toContain('characters not allowed')
    })

    it('returns generic message for unknown io_error', () => {
      const error: WriteOperationError = {
        type: 'io_error',
        path: '/path',
        message: 'Some unknown error XYZ123',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toBe("Couldn't copy the file.")
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
      message: 'Unexpected error',
    }
    const result = getTechnicalDetails(error)

    expect(result).toContain('Path: /path/to/file')
    expect(result).toContain('Error: Unexpected error')
    expect(result).toContain('Error type: io_error')
  })

  it('includes path for device_disconnected', () => {
    const error: WriteOperationError = { type: 'device_disconnected', path: '/mtp/device' }
    const result = getTechnicalDetails(error)

    expect(result).toContain('Path: /mtp/device')
    expect(result).toContain('Error type: device_disconnected')
  })

  it('includes device name for read_only_device', () => {
    const error: WriteOperationError = { type: 'read_only_device', path: '/path', deviceName: 'Pixel 8' }
    const result = getTechnicalDetails(error)

    expect(result).toContain('Path: /path')
    expect(result).toContain('Device: Pixel 8')
    expect(result).toContain('Error type: read_only_device')
  })

  it('includes path and message for read_error', () => {
    const error: WriteOperationError = {
      type: 'read_error',
      path: '/source/file',
      message: 'Failed to read: I/O error',
    }
    const result = getTechnicalDetails(error)

    expect(result).toContain('Path: /source/file')
    expect(result).toContain('Error: Failed to read: I/O error')
  })
})

describe('getUserFriendlyMessage — delete operation', () => {
  it('uses "delete" in source_not_found message', () => {
    const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.message).toContain('delete')
  })

  it('uses "Delete failed" for io_error', () => {
    const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.title).toBe('Delete failed')
  })

  it('gives macOS-specific suggestion for permission_denied on delete', () => {
    setMacOS(true)
    const error: WriteOperationError = {
      type: 'permission_denied',
      path: '/protected',
      message: 'denied',
    }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.suggestion).toContain('Finder')
    expect(result.suggestion).toContain('Get Info')
  })

  it('gives Linux-specific suggestion for permission_denied on delete', () => {
    setMacOS(false)
    const error: WriteOperationError = {
      type: 'permission_denied',
      path: '/protected',
      message: 'denied',
    }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.suggestion).toContain('chmod')
    expect(result.suggestion).not.toContain('Finder')
  })

  it('gives macOS-specific suggestion for file_locked on delete', () => {
    setMacOS(true)
    const error: WriteOperationError = { type: 'file_locked', path: '/path/to/locked.txt' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.message).toContain('locked')
    expect(result.suggestion).toContain('Finder')
  })

  it('gives Linux-specific suggestion for file_locked on delete', () => {
    setMacOS(false)
    const error: WriteOperationError = { type: 'file_locked', path: '/path/to/locked.txt' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.message).toContain('locked')
    expect(result.suggestion).toContain('chmod')
    expect(result.suggestion).not.toContain('Finder')
  })
})

describe('getUserFriendlyMessage — trash operation', () => {
  it('uses "move to trash" in source_not_found message', () => {
    const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.message).toContain('move to trash')
  })

  it('uses "Move to trash failed" for io_error', () => {
    const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.title).toBe('Move to trash failed')
  })

  it('gives macOS-specific suggestion for permission_denied on trash', () => {
    setMacOS(true)
    const error: WriteOperationError = {
      type: 'permission_denied',
      path: '/protected',
      message: 'denied',
    }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.suggestion).toContain('Finder')
    expect(result.suggestion).toContain('locked')
  })

  it('gives Linux-specific suggestion for permission_denied on trash', () => {
    setMacOS(false)
    const error: WriteOperationError = {
      type: 'permission_denied',
      path: '/protected',
      message: 'denied',
    }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.suggestion).toContain('chmod')
    expect(result.suggestion).not.toContain('Finder')
  })

  it('handles trash_not_supported variant', () => {
    const error: WriteOperationError = { type: 'trash_not_supported', path: '/Volumes/USB/file.txt' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.message).toContain("doesn't support trash")
    expect(result.suggestion).toContain('Shift+F8')
  })

  it('uses "Move to trash cancelled" for cancelled error', () => {
    const error: WriteOperationError = { type: 'cancelled', message: 'User cancelled' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.title).toBe('Move to trash cancelled')
  })
})

describe('error messages are volume-agnostic', () => {
  it('does not mention MTP in any error message', () => {
    const errors: WriteOperationError[] = [
      { type: 'source_not_found', path: '/mtp-device/file.txt' },
      { type: 'permission_denied', path: '/mtp-device/protected', message: 'MTP error' },
      { type: 'device_disconnected', path: '/mtp-device/file.txt' },
      { type: 'read_only_device', path: '/mtp-device', deviceName: null },
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
      { type: 'connection_interrupted', path: '//server/share/file.txt' },
    ]

    for (const error of errors) {
      const result = getUserFriendlyMessage(error)
      const allText = `${result.title} ${result.message} ${result.suggestion}`.toLowerCase()
      expect(allText).not.toContain('smb')
    }
  })
})
