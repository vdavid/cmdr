import { afterEach, describe, expect, it, vi } from 'vitest'
import { getUserFriendlyMessage, getTechnicalDetails, getErrorDisplayMeta } from './transfer-error-messages'
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
      expect(result.message).toContain('1.00 GB')
      expect(result.message).toContain('512.00 MB')
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

      expect(result.title).toBe('Copy canceled')
      expect(result.message).toContain('copy')
    })

    it('returns "Couldn\'t copy" for io_error', () => {
      const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe("Couldn't copy")
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

    it('uses "Move canceled" for cancelled error', () => {
      const error: WriteOperationError = { type: 'cancelled', message: 'User cancelled' }
      const result = getUserFriendlyMessage(error, 'move')

      expect(result.title).toBe('Move canceled')
      expect(result.message).toContain('move')
    })

    it('uses "Couldn\'t move" for io_error', () => {
      const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
      const result = getUserFriendlyMessage(error, 'move')

      expect(result.title).toBe("Couldn't move")
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
      const error: WriteOperationError = {
        type: 'read_only_device',
        path: '/path',
        deviceName: 'My Phone',
        side: 'destination',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain('My Phone')
      expect(result.message).toContain('read-only')
      expect(result.suggestion).toContain('different destination')
    })

    it('handles read_only_device without device name', () => {
      const error: WriteOperationError = {
        type: 'read_only_device',
        path: '/path',
        deviceName: null,
        side: 'destination',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.message).toContain('The target device')
      expect(result.message).toContain('read-only')
    })

    // The two sides are different sentences, not two wordings of one. A move OFF
    // a read-only source (a repo's `.git` history, a tar) is refused because the
    // source can never delete the original, so pointing that user at the
    // destination names the half that was fine.
    it('words a read-only SOURCE as a move it cannot do, not a destination to change', () => {
      const error: WriteOperationError = {
        type: 'read_only_device',
        path: '/repo/.git',
        deviceName: '.git',
        side: 'source',
      }
      const result = getUserFriendlyMessage(error, 'move')

      expect(result.title).toBe('Read-only source')
      expect(result.message).toContain('.git')
      expect(result.message).toContain('copy files out of it')
      expect(result.message).toContain('not move them out')
      expect(result.suggestion).toContain('Copy the files instead')
      // ❌ Never the destination sentence: that half was fine.
      expect(result.suggestion).not.toContain('different destination')
    })

    it('falls back to naming the source when a read-only source has no name', () => {
      const error: WriteOperationError = {
        type: 'read_only_device',
        path: '/x/bundle.tar',
        deviceName: null,
        side: 'source',
      }
      const result = getUserFriendlyMessage(error, 'move')

      expect(result.message).toContain('The source')
      expect(result.message).not.toContain('The target device')
    })

    it('handles invalid_name', () => {
      const error: WriteOperationError = {
        type: 'invalid_name',
        path: '/path/bad:name',
        message: 'Colon not allowed',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('Invalid file name')
      // The message's job is naming the file to rename, so the path has to reach it.
      expect(result.message).toContain('/path/bad:name')
    })

    it('handles delete_pending with a dedicated message', () => {
      const error: WriteOperationError = { type: 'delete_pending', path: '/Volumes/share/photo.jpg' }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('File is being removed')
      expect(result.message).toContain('marked it for deletion')
      expect(result.suggestion).toContain('Wait a moment')
    })

    it('handles read_only_device with a dedicated message (no fallthrough)', () => {
      const error: WriteOperationError = {
        type: 'read_only_device',
        path: '/path',
        deviceName: 'My Phone',
        side: 'destination',
      }
      const result = getUserFriendlyMessage(error)

      expect(result.title).toBe('Read-only device')
      expect(result.message).toContain('My Phone')
      expect(result.suggestion).toContain('different destination')
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

  it('shows the real file and the named NTSTATUS for invalid_name', () => {
    // The whole panel, pinned: this is what the user copies into a bug report.
    // The failing SMB copy used to render `Protocol error: 0xC0000033 during
    // Create` under `Error type: io_error` on the enclosing FOLDER's path; smb2
    // has the status in its table now and the typed variant carries the item
    // that actually failed. The friendly prose says what to do about it, so
    // these lines stay purely diagnostic and don't restate it.
    const error: WriteOperationError = {
      type: 'invalid_name',
      path: '/Volumes/naspi/export/how_are_you_feeling.json',
      message: 'Protocol error: STATUS_OBJECT_NAME_INVALID during Create',
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /Volumes/naspi/export/how_are_you_feeling.json\n' +
        'Error: Protocol error: STATUS_OBJECT_NAME_INVALID during Create\n' +
        'Error type: invalid_name',
    )
  })

  it('includes space info for insufficient_space error', () => {
    const error: WriteOperationError = {
      type: 'insufficient_space',
      required: 1073741824,
      available: 536870912,
      volumeName: 'Test Volume',
    }
    const result = getTechnicalDetails(error)

    expect(result).toContain('Required: 1.00 GB')
    expect(result).toContain('Available: 512.00 MB')
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
    const error: WriteOperationError = {
      type: 'read_only_device',
      path: '/path',
      deviceName: 'Pixel 8',
      side: 'destination',
    }
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

describe('getUserFriendlyMessage: delete operation', () => {
  it('uses "delete" in source_not_found message', () => {
    const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.message).toContain('delete')
  })

  it('uses "Couldn\'t delete" for io_error', () => {
    const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
    const result = getUserFriendlyMessage(error, 'delete')

    expect(result.title).toBe("Couldn't delete")
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

describe('getUserFriendlyMessage: trash operation', () => {
  it('uses "move to trash" in source_not_found message', () => {
    const error: WriteOperationError = { type: 'source_not_found', path: '/path/to/file.txt' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.message).toContain('move to trash')
  })

  it('uses "Couldn\'t move to trash" for io_error', () => {
    const error: WriteOperationError = { type: 'io_error', path: '/path', message: 'Something broke' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.title).toBe("Couldn't move to trash")
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

  it('uses "Move to trash canceled" for cancelled error', () => {
    const error: WriteOperationError = { type: 'cancelled', message: 'User cancelled' }
    const result = getUserFriendlyMessage(error, 'trash')

    expect(result.title).toBe('Move to trash canceled')
  })
})

describe('error messages are volume-agnostic', () => {
  it('does not mention MTP in any error message', () => {
    const errors: WriteOperationError[] = [
      { type: 'source_not_found', path: '/mtp-device/file.txt' },
      { type: 'permission_denied', path: '/mtp-device/protected', message: 'MTP error' },
      { type: 'device_disconnected', path: '/mtp-device/file.txt' },
      { type: 'read_only_device', path: '/mtp-device', deviceName: null, side: 'destination' },
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

describe('getErrorDisplayMeta', () => {
  // Mirrors the category + retryHint the Rust `friendly_from_write_error` mapper
  // assigned per `WriteOperationError` variant (now derived on the FE).
  const cases: Array<{ error: WriteOperationError; category: string; retryHint: boolean }> = [
    { error: { type: 'source_not_found', path: '/p' }, category: 'needs_action', retryHint: false },
    { error: { type: 'destination_exists', path: '/p' }, category: 'needs_action', retryHint: false },
    { error: { type: 'permission_denied', path: '/p', message: 'm' }, category: 'needs_action', retryHint: false },
    { error: { type: 'cancelled', message: 'm' }, category: 'transient', retryHint: true },
    { error: { type: 'device_disconnected', path: '/p' }, category: 'needs_action', retryHint: true },
    { error: { type: 'connection_interrupted', path: '/p' }, category: 'transient', retryHint: true },
    {
      error: { type: 'insufficient_space', required: 1, available: 0, volumeName: null },
      category: 'needs_action',
      retryHint: false,
    },
    {
      error: { type: 'destination_inside_source', source: '/a', destination: '/a/b' },
      category: 'needs_action',
      retryHint: false,
    },
    { error: { type: 'symlink_loop', path: '/p' }, category: 'serious', retryHint: false },
    {
      error: { type: 'read_only_device', path: '/p', deviceName: null, side: 'destination' },
      category: 'needs_action',
      retryHint: false,
    },
    { error: { type: 'file_locked', path: '/p' }, category: 'needs_action', retryHint: false },
    { error: { type: 'trash_not_supported', path: '/p' }, category: 'needs_action', retryHint: false },
    { error: { type: 'read_error', path: '/p', message: 'm' }, category: 'serious', retryHint: true },
    { error: { type: 'write_error', path: '/p', message: 'm' }, category: 'serious', retryHint: true },
    { error: { type: 'name_too_long', path: '/p' }, category: 'needs_action', retryHint: false },
    { error: { type: 'invalid_name', path: '/p', message: 'm' }, category: 'needs_action', retryHint: false },
    { error: { type: 'delete_pending', path: '/p' }, category: 'transient', retryHint: true },
    { error: { type: 'io_error', path: '/p', message: 'm' }, category: 'serious', retryHint: true },
  ]

  for (const { error, category, retryHint } of cases) {
    it(`maps ${error.type} → ${category}, retryHint=${String(retryHint)}`, () => {
      const meta = getErrorDisplayMeta(error)
      expect(meta.category).toBe(category)
      expect(meta.retryHint).toBe(retryHint)
    })
  }
})
