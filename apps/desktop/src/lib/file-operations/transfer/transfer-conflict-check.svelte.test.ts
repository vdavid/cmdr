/**
 * Headless tests for `createTransferConflictCheck`, focused on the one thing this
 * side of the duplicate feature owns: what it hands the backend.
 *
 * A same-folder copy is a duplicate, not a conflict, and the backend is what
 * decides that (`commands/file_system/volume_copy.rs`). It can only decide it
 * when the check forwards `sourceVolumeId` AND `sourcePaths` — the names alone
 * are matched against the destination listing, where every source of a
 * same-folder copy appears as its own clash. So the `scanVolumeForConflicts`
 * mock here behaves like that backend rather than answering a canned list: it
 * applies the self-collision filter when it's given source paths, and can't when
 * it isn't. Drop the forwarding and the dialog silently grows a conflict count
 * and the overwrite/skip/rename radios again, with the bulk-skip names to match.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { SourceItemInput, VolumeConflictInfo } from '$lib/tauri-commands'
import { createTransferConflictCheck } from './transfer-conflict-check.svelte'

/** Stands in for the destination listing: every name the destination holds. */
let namesAtDestination: string[] = []

const scanVolumeForConflictsMock = vi.fn(
  (
    _volumeId: string,
    sourceItems: SourceItemInput[],
    destPath: string,
    _sourceVolumeId?: string,
    sourcePaths?: string[],
  ): Promise<VolumeConflictInfo[]> => {
    // What the per-backend check does: match by NAME against one dest listing.
    const byName = sourceItems
      .filter((item) => namesAtDestination.includes(item.name))
      .map((item) => ({
        sourcePath: item.name,
        destPath: `${destPath}/${item.name}`,
        sourceIsDirectory: false,
        destIsDirectory: false,
        sourceSize: 0,
        destSize: 0,
        sourceModified: null,
        destModified: null,
      })) as unknown as VolumeConflictInfo[]
    // What the layer above it does: drop the collisions that name the source
    // itself. Impossible without the source paths.
    if (!sourcePaths?.length) return Promise.resolve(byName)
    return Promise.resolve(byName.filter((conflict) => !sourcePaths.includes(`${destPath}/${conflict.sourcePath}`)))
  },
)

vi.mock('$lib/tauri-commands', () => ({
  scanVolumeForConflicts: (
    volumeId: string,
    sourceItems: SourceItemInput[],
    destPath: string,
    sourceVolumeId?: string,
    sourcePaths?: string[],
  ) => scanVolumeForConflictsMock(volumeId, sourceItems, destPath, sourceVolumeId, sourcePaths),
}))

const log = {
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  debug: vi.fn(),
  trace: vi.fn(),
}

function makeCheck(sourcePaths: string[], destPath: string) {
  return createTransferConflictCheck({
    getSelectedVolumeId: () => 'volume-1',
    getSourcePaths: () => sourcePaths,
    getEditedPath: () => destPath,
    getSourceVolumeId: () => 'volume-1',
    getDestroyed: () => false,
    log: log as never,
  })
}

beforeEach(() => {
  scanVolumeForConflictsMock.mockClear()
  namesAtDestination = ['photo.jpg', 'notes.txt']
})

describe('createTransferConflictCheck', () => {
  it('reports no conflicts for a copy into the folder the sources already live in', async () => {
    const check = makeCheck(['/photos/photo.jpg', '/photos/notes.txt'], '/photos')

    await check.check()

    expect(check.totalConflictCount).toBe(0)
    expect(check.mergeFolderCount).toBe(0)
    expect(check.conflictNames).toEqual([])
    expect(check.conflictCheckComplete).toBe(true)
  })

  it('forwards the source volume and the source paths, which is what lets the backend answer that', async () => {
    const check = makeCheck(['/photos/photo.jpg'], '/photos')

    await check.check()

    expect(scanVolumeForConflictsMock).toHaveBeenCalledTimes(1)
    const [volumeId, sourceItems, destPath, sourceVolumeId, sourcePaths] = scanVolumeForConflictsMock.mock.calls[0]
    expect(volumeId).toBe('volume-1')
    expect(sourceItems.map((item) => item.name)).toEqual(['photo.jpg'])
    expect(destPath).toBe('/photos')
    expect(sourceVolumeId).toBe('volume-1')
    expect(sourcePaths).toEqual(['/photos/photo.jpg'])
  })

  it('still reports a genuine clash from another folder, names and all', async () => {
    const check = makeCheck(['/backup/photo.jpg'], '/photos')

    await check.check()

    expect(check.totalConflictCount).toBe(1)
    expect(check.conflictNames).toEqual(['photo.jpg'])
    expect(check.conflictCheckComplete).toBe(true)
  })
})
