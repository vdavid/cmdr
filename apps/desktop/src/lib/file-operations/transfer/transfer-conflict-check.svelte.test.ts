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
  log.warn.mockClear()
  log.error.mockClear()
  namesAtDestination = ['photo.jpg', 'notes.txt']
})

describe('createTransferConflictCheck', () => {
  it("logs a check the volume couldn't answer at warn, since the dialog already says so", async () => {
    // The dialog renders "couldn't check" for `unknown`; at error level every slow or
    // disconnected volume filed an error report (ERR-J9BKB, ERR-F7N2B, ERR-YKADZ).
    scanVolumeForConflictsMock.mockRejectedValueOnce(new Error('timedOut'))
    const check = makeCheck(['/photos/photo.jpg'], '/elsewhere')

    await check.check()

    expect(check.conflictCheckUnknown).toBe(true)
    expect(log.error).not.toHaveBeenCalled()
    expect(log.warn).toHaveBeenCalledTimes(1)
  })

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

  it('keeps the sizes and dates of the file clashes, for pricing an overwrite', async () => {
    scanVolumeForConflictsMock.mockResolvedValueOnce([
      {
        sourcePath: 'photo.jpg',
        destPath: '/photos/photo.jpg',
        sourceIsDirectory: false,
        destIsDirectory: false,
        sourceSize: 10,
        destSize: 5,
        sourceModified: 200,
        destModified: 100,
      },
      {
        sourcePath: 'album',
        destPath: '/photos/album',
        sourceIsDirectory: true,
        destIsDirectory: false,
        sourceSize: 0,
        destSize: 7,
        sourceModified: null,
        destModified: null,
      },
    ])
    const check = makeCheck(['/backup/photo.jpg', '/backup/album'], '/photos')

    await check.check()

    // A folder replacing a file is a clash too, but it isn't an object overwritten by bytes.
    expect(check.fileClashes).toEqual([{ sourceSize: 10, destSize: 5, sourceModified: 200, destModified: 100 }])
  })
})

it('probes the requested filename but keeps the original source name as the bulk-skip key', async () => {
  namesAtDestination = ['backup.txt']
  const check = createTransferConflictCheck({
    getSelectedVolumeId: () => 'volume-1',
    getSourcePaths: () => ['/photos/notes.txt'],
    getEditedPath: () => '/photos',
    getDestinationName: () => 'backup.txt',
    getSourceVolumeId: () => 'volume-1',
    getDestroyed: () => false,
    log: log as never,
  })
  await check.check()
  expect(check.totalConflictCount).toBe(1)
  expect(check.conflictNames).toEqual(['notes.txt'])
  expect(scanVolumeForConflictsMock.mock.calls[0][1][0].name).toBe('backup.txt')
})

it('discards a slow conflict answer after the destination changes', async () => {
  let resolveOld!: (value: VolumeConflictInfo[]) => void
  scanVolumeForConflictsMock.mockImplementationOnce(
    () =>
      new Promise<VolumeConflictInfo[]>((resolve) => {
        resolveOld = resolve
      }),
  )
  const check = makeCheck(['/backup/photo.jpg'], '/photos')
  const oldCheck = check.check()
  check.reset()
  namesAtDestination = []
  await check.check()
  resolveOld([
    {
      sourcePath: 'photo.jpg',
      destPath: '/photos/photo.jpg',
      sourceIsDirectory: false,
      destIsDirectory: false,
      sourceSize: 0,
      destSize: 0,
      sourceModified: null,
      destModified: null,
    },
  ])
  await oldCheck
  expect(check.conflictCheckComplete).toBe(true)
  expect(check.totalConflictCount).toBe(0)
  expect(check.conflictNames).toEqual([])
})
