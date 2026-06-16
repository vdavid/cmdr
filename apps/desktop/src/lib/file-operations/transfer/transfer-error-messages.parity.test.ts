/**
 * Byte-for-byte parity net for the write-error copy (the GAP-1 i18n move).
 *
 * `getUserFriendlyMessage` now pulls its title/message/suggestion from the
 * `errors.write.*` catalog (via `getMessage`, raw lookup, no ICU) instead of
 * inline literals. This test pins the FULL rendered output — every variant ×
 * every operation, plus the platform/op branches — to the exact pre-migration
 * English, so the catalog move stays behavior-preserving. `TransferErrorDialog`
 * / `FallbackErrorContent` render exactly this output, so pinning it here pins
 * the dialog.
 *
 * The expected strings below are the pre-migration English, written out
 * independently of the catalog; if a catalog edit drifts the rendered copy, this
 * fails. (`transfer-error-messages.test.ts` covers the same factory with
 * partial / structural assertions; this is the exhaustive full-string net.)
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { getUserFriendlyMessage } from './transfer-error-messages'
import type { FriendlyErrorMessage } from './transfer-error-messages'
import type { WriteOperationError, TransferOperationType } from '$lib/file-explorer/types'
import { formatBytes } from '$lib/tauri-commands'
import { colorizeSizeString } from '$lib/file-explorer/selection/selection-info-utils'

// The insufficient_space message interpolates colorized, formatted sizes. Those
// helpers are NOT part of the migrated copy (only the template moved), so derive
// the expected interpolations from them rather than hardcoding their HTML.
const REQUIRED = 1073741824
const AVAILABLE = 536870912
const requiredSize = colorizeSizeString(formatBytes(REQUIRED))
const availableSize = colorizeSizeString(formatBytes(AVAILABLE))

// `trash_not_supported` interpolates the live `file.deletePermanently` binding.
// Pin it so the suggestion is deterministic across platforms.
vi.mock('$lib/shortcuts', async (orig) => {
  const actual = await orig<typeof import('$lib/shortcuts')>()
  return { ...actual, getEffectiveShortcuts: (id: string) => (id === 'file.deletePermanently' ? ['⇧F8'] : []) }
})

const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
function setMacOS(isMac: boolean) {
  navigatorSpy.mockReturnValue({
    userAgent: isMac ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' : 'Mozilla/5.0 (X11; Linux x86_64)',
  } as Navigator)
}
afterEach(() => navigatorSpy.mockReset())

interface Case {
  name: string
  error: WriteOperationError
  op?: TransferOperationType
  mac?: boolean
  expected: FriendlyErrorMessage
}

const cases: Case[] = [
  {
    name: 'source_not_found (copy)',
    error: { type: 'source_not_found', path: '/p' },
    expected: {
      title: "Couldn't find the file",
      message: 'The file or folder you tried to copy no longer exists.',
      suggestion: 'It may have been moved, renamed, or deleted. Try refreshing the file list.',
    },
  },
  {
    name: 'source_not_found (trash)',
    error: { type: 'source_not_found', path: '/p' },
    op: 'trash',
    expected: {
      title: "Couldn't find the file",
      message: 'The file or folder you tried to move to trash no longer exists.',
      suggestion: 'It may have been moved, renamed, or deleted. Try refreshing the file list.',
    },
  },
  {
    name: 'destination_exists',
    error: { type: 'destination_exists', path: '/p' },
    expected: {
      title: 'File already exists',
      message: "There's already a file with this name at the destination.",
      suggestion: 'Choose a different name or location, or delete the existing file first.',
    },
  },
  {
    name: 'same_location (move)',
    error: { type: 'same_location', path: '/p' },
    op: 'move',
    expected: {
      title: "Can't move to the same location",
      message: 'The source and destination are the same.',
      suggestion: 'Choose a different destination folder.',
    },
  },
  {
    name: 'destination_inside_source (move)',
    error: { type: 'destination_inside_source', source: '/a', destination: '/a/b' },
    op: 'move',
    expected: {
      title: "Can't move a folder into itself",
      message: "You're trying to move a folder into one of its own subfolders.",
      suggestion: 'Choose a destination outside of the folder you are moving.',
    },
  },
  {
    name: 'symlink_loop',
    error: { type: 'symlink_loop', path: '/p' },
    expected: {
      title: 'Link loop detected',
      message: 'This folder contains symbolic links that create an infinite loop.',
      suggestion: 'The folder structure contains circular references. You may need to remove some symbolic links.',
    },
  },
  {
    name: 'cancelled (copy)',
    error: { type: 'cancelled', message: 'm' },
    expected: {
      title: 'Copy cancelled',
      message: 'The copy operation was cancelled.',
      suggestion: 'You can try again when ready.',
    },
  },
  {
    name: 'cancelled (trash)',
    error: { type: 'cancelled', message: 'm' },
    op: 'trash',
    expected: {
      title: 'Move to trash cancelled',
      message: 'The move to trash operation was cancelled.',
      suggestion: 'You can try again when ready.',
    },
  },
  {
    name: 'device_disconnected (move)',
    error: { type: 'device_disconnected', path: '/p' },
    op: 'move',
    expected: {
      title: 'Device disconnected',
      message: 'The device was disconnected during the move.',
      suggestion: 'Make sure the device is properly connected and try again.',
    },
  },
  {
    name: 'trash_not_supported',
    error: { type: 'trash_not_supported', path: '/p' },
    op: 'trash',
    expected: {
      title: 'Trash not supported',
      message: "This volume doesn't support trash.",
      suggestion: 'Use ⇧F8 to delete permanently instead.',
    },
  },
  {
    name: 'connection_interrupted',
    error: { type: 'connection_interrupted', path: '/p' },
    expected: {
      title: 'Connection interrupted',
      message: 'The connection was interrupted.',
      suggestion:
        'Check your connection and try again. If copying to a network location, ensure the server is reachable.',
    },
  },
  {
    name: 'read_error (move)',
    error: { type: 'read_error', path: '/p', message: 'm' },
    op: 'move',
    expected: {
      title: 'Move failed',
      message: "Couldn't read from the source.",
      suggestion: 'Try again. If the problem persists, check the technical details below.',
    },
  },
  {
    name: 'write_error (copy)',
    error: { type: 'write_error', path: '/p', message: 'm' },
    expected: {
      title: 'Copy failed',
      message: "Couldn't write to the destination.",
      suggestion: 'Try again. If the problem persists, check the technical details below.',
    },
  },
  {
    name: 'name_too_long',
    error: { type: 'name_too_long', path: '/p' },
    expected: {
      title: 'Name too long',
      message: 'The file name is too long for the destination.',
      suggestion: 'Try renaming the file to use a shorter name.',
    },
  },
  {
    name: 'invalid_name',
    error: { type: 'invalid_name', path: '/p', message: 'm' },
    expected: {
      title: 'Invalid file name',
      message: 'The file name contains characters not allowed at the destination.',
      suggestion: 'Try renaming the file to remove special characters.',
    },
  },
  {
    name: 'io_error (delete)',
    error: { type: 'io_error', path: '/p', message: 'm' },
    op: 'delete',
    expected: {
      title: 'Delete failed',
      message: "Couldn't delete the file.",
      suggestion: 'Try again. If the problem persists, check the technical details below.',
    },
  },
  {
    name: 'permission_denied (copy → default suggestion)',
    error: { type: 'permission_denied', path: '/p', message: 'm' },
    expected: {
      title: "Couldn't access this location",
      message: "You don't have permission to copy files here.",
      suggestion:
        'Check that you have write access to the destination folder. You may need to unlock the device or change folder permissions.',
    },
  },
  {
    name: 'permission_denied (delete, macOS)',
    error: { type: 'permission_denied', path: '/p', message: 'm' },
    op: 'delete',
    mac: true,
    expected: {
      title: "Couldn't access this location",
      message: "You don't have permission to delete files here.",
      suggestion:
        'Check that you have write access to the parent folder. The file may be locked. Unlock it in Finder (Get Info > uncheck Locked) and try again.',
    },
  },
  {
    name: 'permission_denied (delete, Linux)',
    error: { type: 'permission_denied', path: '/p', message: 'm' },
    op: 'delete',
    mac: false,
    expected: {
      title: "Couldn't access this location",
      message: "You don't have permission to delete files here.",
      suggestion:
        'Check that you have write access to the parent folder. The file may be protected. Check its permissions (e.g. via chmod or your file manager) and try again.',
    },
  },
  {
    name: 'insufficient_space',
    error: { type: 'insufficient_space', required: REQUIRED, available: AVAILABLE, volumeName: null },
    expected: {
      title: 'Not enough space',
      // The size HTML comes from colorizeSizeString(formatBytes(...)); the
      // template text around it is the migrated copy this pins.
      message: `The destination needs ${requiredSize} but only has ${availableSize} available.`,
      suggestion:
        'Free up some space on the destination by deleting unnecessary files, or choose a different location.',
    },
  },
  {
    name: 'read_only_device (named)',
    error: { type: 'read_only_device', path: '/p', deviceName: 'My Phone' },
    expected: {
      title: 'Read-only device',
      message: 'My Phone is read-only. You can copy files from it, but not to it.',
      suggestion: 'Choose a different destination that supports writing.',
    },
  },
  {
    name: 'read_only_device (no name → fallback)',
    error: { type: 'read_only_device', path: '/p', deviceName: null },
    expected: {
      title: 'Read-only device',
      message: 'The target device is read-only. You can copy files from it, but not to it.',
      suggestion: 'Choose a different destination that supports writing.',
    },
  },
  {
    name: 'file_locked (macOS)',
    error: { type: 'file_locked', path: '/p' },
    op: 'delete',
    mac: true,
    expected: {
      title: 'File is locked',
      message: "The file is locked and can't be deleted.",
      suggestion: 'Unlock it in Finder (Get Info > uncheck Locked) and try again.',
    },
  },
  {
    name: 'file_locked (Linux)',
    error: { type: 'file_locked', path: '/p' },
    op: 'delete',
    mac: false,
    expected: {
      title: 'File is locked',
      message: "The file is locked and can't be deleted.",
      suggestion:
        'The file may be protected. Check its permissions (e.g. via chmod or your file manager) and try again.',
    },
  },
  {
    name: 'delete_pending',
    error: { type: 'delete_pending', path: '/p' },
    expected: {
      title: 'File is being removed',
      message:
        'This file is on its way out. The server marked it for deletion, but another open handle is keeping it around until that handle closes.',
      suggestion:
        'Wait a moment and try again. Once the last handle closes, the file disappears. If it sticks around, close any other apps that might have it open.',
    },
  },
]

describe('write-error copy parity (getUserFriendlyMessage reproduces the pre-migration English byte-for-byte)', () => {
  for (const c of cases) {
    it(c.name, () => {
      if (c.mac !== undefined) setMacOS(c.mac)
      const actual = getUserFriendlyMessage(c.error, c.op ?? 'copy')
      expect(actual.title, `${c.name} title`).toBe(c.expected.title)
      expect(actual.message, `${c.name} message`).toBe(c.expected.message)
      expect(actual.suggestion, `${c.name} suggestion`).toBe(c.expected.suggestion)
    })
  }
})
