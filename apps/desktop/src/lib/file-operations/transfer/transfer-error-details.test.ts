/**
 * The "Technical details" block per variant: the exact lines a user copies into a
 * bug report. The common path-only and path-plus-message shapes are pinned in
 * `transfer-error-messages.test.ts`; this file covers the variants with their own
 * lines, whole, so a dropped or reordered fact shows up as a diff.
 */
import { describe, it, expect } from 'vitest'
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { getTechnicalDetails } from './transfer-error-details'

describe('getTechnicalDetails: volume-gone variants', () => {
  // The volume list has already dropped the drive, so this block is the only
  // place its name and id survive.
  it('names the vanished drive, its id, and which side it was', () => {
    const error: WriteOperationError = {
      type: 'device_disconnected',
      path: '/Volumes/Stick/a.txt',
      side: { role: 'destination', volumeId: 'vol-42', volumeName: 'Stick', counterpartName: 'Macintosh HD' },
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /Volumes/Stick/a.txt\n' +
        'Volume: Stick (vol-42)\n' +
        'Side: destination\n' +
        'Error type: device_disconnected',
    )
  })

  it('states the server and how long Cmdr waited before giving up', () => {
    const error: WriteOperationError = {
      type: 'server_connection_lost',
      path: '/Volumes/naspi/photos/b.jpg',
      volumeName: 'naspi',
      gaveUpAfterSecs: 120,
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /Volumes/naspi/photos/b.jpg\nServer: naspi\nWaited: 120 s\nError type: server_connection_lost',
    )
  })

  // A sign-in prompt or a link that kept dropping ends it without the wait.
  it('says "no" when Cmdr did not wait for the server', () => {
    const error: WriteOperationError = {
      type: 'server_connection_lost',
      path: '/Volumes/naspi/a',
      volumeName: 'naspi',
      gaveUpAfterSecs: null,
    }

    expect(getTechnicalDetails(error).split('\n')).toContain('Waited: no')
  })

  it('names the item and the volume when it changed while the server was away', () => {
    const error: WriteOperationError = {
      type: 'changed_while_disconnected',
      path: '/Volumes/naspi/old/c.txt',
      volumeName: 'naspi',
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /Volumes/naspi/old/c.txt\nVolume: naspi\nError type: changed_while_disconnected',
    )
  })

  it('carries the errno and volume of an unconfirmed move', () => {
    const error: WriteOperationError = {
      type: 'move_not_confirmed',
      path: '/Volumes/Stick/d',
      errno: 5,
      volumeName: 'Stick',
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /Volumes/Stick/d\nErrno: 5\nVolume: Stick\nError type: move_not_confirmed',
    )
  })

  it('leaves out the errno and volume of an unconfirmed move when neither is known', () => {
    const error: WriteOperationError = { type: 'move_not_confirmed', path: '/x/d', errno: null, volumeName: null }

    expect(getTechnicalDetails(error)).toBe('Path: /x/d\nError type: move_not_confirmed')
  })
})

describe('getTechnicalDetails: variants with their own lines', () => {
  it('shows only the path for a read-only device with no name', () => {
    const error: WriteOperationError = {
      type: 'read_only_device',
      path: '/mtp/x',
      deviceName: null,
      side: 'destination',
    }

    expect(getTechnicalDetails(error)).toBe('Path: /mtp/x\nError type: read_only_device')
  })

  it('leaves out the volume line when the short volume has no name', () => {
    const error: WriteOperationError = {
      type: 'insufficient_space',
      required: 2_000_000,
      available: 1_000_000,
      volumeName: null,
    }
    const lines = getTechnicalDetails(error).split('\n')

    expect(lines).toHaveLength(3)
    expect(lines.some((line) => line.startsWith('Volume:'))).toBe(false)
  })

  it('names the mount point a delete refused to reach into', () => {
    const error: WriteOperationError = {
      type: 'mounted_volume_inside',
      mountPoint: '/Users/me/mnt/nas',
      action: 'delete',
    }

    expect(getTechnicalDetails(error)).toBe('Mount point: /Users/me/mnt/nas\nError type: mounted_volume_inside')
  })

  it('names both sources that share a name', () => {
    const error: WriteOperationError = {
      type: 'duplicate_source_names',
      name: 'report.pdf',
      first: '/a/report.pdf',
      second: '/b/report.pdf',
    }

    expect(getTechnicalDetails(error)).toBe(
      'Name: report.pdf\nFirst: /a/report.pdf\nSecond: /b/report.pdf\nError type: duplicate_source_names',
    )
  })

  it('lists every oversized file under the filesystem limit', () => {
    const error: WriteOperationError = {
      type: 'files_too_large_for_filesystem',
      filesystem: 'fat32',
      maxSize: 4_294_967_295,
      files: [
        { name: 'movie.mkv', size: 5_000_000_000 },
        { name: 'disk.img', size: 8_000_000_000 },
      ],
      totalCount: 2,
    }
    const lines = getTechnicalDetails(error).split('\n')

    expect(lines[0]).toBe('Filesystem: fat32')
    expect(lines[1]).toMatch(/^Max file size: /)
    expect(lines[2]).toBe('Files over the limit: 2')
    expect(lines[3]).toMatch(/^ {2}movie\.mkv \(.+\)$/)
    expect(lines[4]).toMatch(/^ {2}disk\.img \(.+\)$/)
    expect(lines[5]).toBe('Error type: files_too_large_for_filesystem')
  })

  // The new data exists ONLY at `keptAt`, so the block must carry both paths.
  it('names both the target and where the only copy of the new data is', () => {
    const error: WriteOperationError = {
      type: 'new_data_kept_at',
      path: '/dst/a.txt',
      keptAt: '/dst/a (recovered).txt',
      message: 'rename refused',
    }

    expect(getTechnicalDetails(error)).toBe(
      'Path: /dst/a.txt\nNew data kept at: /dst/a (recovered).txt\nError: rename refused\nError type: new_data_kept_at',
    )
  })

  // Every recovered file, then the cause's own lines, so the report still says
  // what stopped the copy.
  it('lists every original kept aside, then the cause that stopped the copy', () => {
    const error: WriteOperationError = {
      type: 'originals_kept_aside',
      recovered: [
        { path: '/dst/x', keptAt: '/dst/x (recovered)' },
        { path: '/dst/y', keptAt: '/dst/y (recovered)' },
      ],
      cause: { type: 'write_error', path: '/dst/x/inner', message: 'disk said no' },
    }

    expect(getTechnicalDetails(error)).toBe(
      'Kept /dst/x at: /dst/x (recovered)\n' +
        'Kept /dst/y at: /dst/y (recovered)\n' +
        'Path: /dst/x/inner\n' +
        'Error: disk said no\n' +
        'Error type: write_error\n' +
        'Error type: originals_kept_aside',
    )
  })

  it('lists every group of kept new files, then the cause that stopped the transfer', () => {
    const error: WriteOperationError = {
      type: 'new_files_kept',
      kept: [
        { fate: 'landsWhenBack', count: 2, exampleName: 'a.jpg', volumeName: 'NAS' },
        { fate: 'rescued', count: 1, exampleName: 'b (recovered).jpg', volumeName: 'NAS' },
      ],
      cause: { type: 'write_error', path: '/dst/c.jpg', message: 'disk said no' },
    }

    expect(getTechnicalDetails(error)).toBe(
      'Kept (landsWhenBack) on NAS: 2, for example a.jpg\n' +
        'Kept (rescued) on NAS: 1, for example b (recovered).jpg\n' +
        'Path: /dst/c.jpg\n' +
        'Error: disk said no\n' +
        'Error type: write_error\n' +
        'Error type: new_files_kept',
    )
  })

  it('names both ends of a move whose original stayed, then what refused the delete', () => {
    const error: WriteOperationError = {
      type: 'source_not_removed',
      path: '/src/a',
      landedAt: '/dst/a',
      cause: { type: 'file_locked', path: '/src/a' },
    }

    expect(getTechnicalDetails(error)).toBe(
      'Original kept at: /src/a\n' +
        'Copy landed at: /dst/a\n' +
        'Path: /src/a\n' +
        'Error type: file_locked\n' +
        'Error type: source_not_removed',
    )
  })

  it('carries a cancel reason when there is one, and nothing else when there isn’t', () => {
    expect(getTechnicalDetails({ type: 'cancelled', message: 'Stopped by user' })).toBe(
      'Details: Stopped by user\nError type: cancelled',
    )
    expect(getTechnicalDetails({ type: 'cancelled', message: '' })).toBe('Error type: cancelled')
  })

  // A replace that couldn't record its landing left `path` untouched: the path is
  // the one fact the report needs.
  it('names the path a replace left untouched when it couldn’t record its landing', () => {
    const error: WriteOperationError = { type: 'landing_not_recorded', path: '/dst/keep.txt' }

    expect(getTechnicalDetails(error)).toBe('Path: /dst/keep.txt\nError type: landing_not_recorded')
  })

  it('names the path of a source in cold storage or one that changed under the copy', () => {
    expect(getTechnicalDetails({ type: 'source_in_cold_storage', path: 's3://b/k' })).toBe(
      'Path: s3://b/k\nError type: source_in_cold_storage',
    )
    expect(getTechnicalDetails({ type: 'source_changed', path: '/src/live.log' })).toBe(
      'Path: /src/live.log\nError type: source_changed',
    )
  })

  it('falls back to the type alone for a variant with no path', () => {
    const error: WriteOperationError = {
      type: 'trash_refused',
      itemCount: 3,
      reason: 'notPermitted',
      message: 'macOS said no',
    }

    expect(getTechnicalDetails(error)).toBe('Error type: trash_refused')
  })
})
