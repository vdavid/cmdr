/**
 * Which backend command a confirmed transfer routes to, and what it carries.
 *
 * Birth is a pure function of the config now, so these cases need no dialog, no
 * session, and no event plumbing: call `dispatchTransferOperation` and look at
 * which mocked command it reached for. The routing that isn't obvious from the
 * operation type is the archive one — a move into or out of a `.zip` must leave
 * the local fast path even when both sides share a volume id.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
  compressFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'copy' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'move' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'delete' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1', operationType: 'trash' })),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  // Key-aware so the archive compression level is distinguishable from the
  // progress-interval / max-conflicts settings (all others resolve to 200).
  getSetting: vi.fn((key: string) => (key === 'behavior.archiveCompressionLevel' ? 6 : 200)),
}))

import { dispatchTransferOperation, type TransferDispatchConfig } from './transfer-dispatch'
import {
  copyBetweenVolumes,
  moveBetweenVolumes,
  compressFiles,
  moveFiles,
  deleteFiles,
  trashFiles,
} from '$lib/tauri-commands'

function makeConfig(over: Partial<TransferDispatchConfig> = {}): TransferDispatchConfig {
  return {
    operationType: 'copy',
    sourcePaths: ['/src/file.txt'],
    destinationPath: '/dst',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    conflictResolution: 'stop',
    preKnownConflicts: [],
    itemSizes: [],
    ...over,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('dispatchTransferOperation: routing', () => {
  it('dispatches a local copy through copyBetweenVolumes', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'copy' }))
    expect(copyBetweenVolumes).toHaveBeenCalledTimes(1)
  })

  it('dispatches a local move through moveFiles', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' }))
    expect(moveFiles).toHaveBeenCalledTimes(1)
    expect(moveBetweenVolumes).not.toHaveBeenCalled()
  })

  it('dispatches a cross-volume move through moveBetweenVolumes', async () => {
    await dispatchTransferOperation(
      makeConfig({ operationType: 'move', sourceVolumeId: 'mtp-1', destVolumeId: 'root' }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move INTO a zip through moveBetweenVolumes, not the local fast-path', async () => {
    // Source and dest share the parent drive's `root` id (the zip lives on it), so
    // the volume-id comparison alone would pick `moveFiles`. The dest PATH inside a
    // `.zip` forces the cross-volume route (backend runs the archive-edit flow).
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/file.txt'],
        destinationPath: '/left/foo.zip/inner',
      }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move OUT of a zip through moveBetweenVolumes, not the local fast-path', async () => {
    // Extract-out move: the SOURCE path is inside a `.zip` while both ids are `root`.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/foo.zip/inner.txt'],
        destinationPath: '/right',
      }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('dispatches delete through deleteFiles', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'delete' }))
    expect(deleteFiles).toHaveBeenCalledTimes(1)
  })

  it('dispatches trash through trashFiles', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'trash' }))
    expect(trashFiles).toHaveBeenCalledTimes(1)
  })
})

describe('dispatchTransferOperation: compression-level threading', () => {
  // The FE reads `behavior.archiveCompressionLevel` once at dispatch (mocked to 6)
  // and passes it in the operation config for every zip-writing path, so the
  // backend applies the chosen deflate level. Non-archive copies simply ignore it.
  it('passes the compression level to compressFiles', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'compress' }))
    expect(compressFiles).toHaveBeenCalledWith(
      'root',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
      undefined,
    )
  })

  it('passes the compression level to copyBetweenVolumes (copy INTO an archive uses the same level)', async () => {
    await dispatchTransferOperation(makeConfig({ operationType: 'copy' }))
    expect(copyBetweenVolumes).toHaveBeenCalledWith(
      'root',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
      undefined,
    )
  })

  it('passes the compression level to moveBetweenVolumes (move INTO an archive uses the same level)', async () => {
    await dispatchTransferOperation(
      makeConfig({ operationType: 'move', sourceVolumeId: 'mtp-1', destVolumeId: 'root' }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledWith(
      'mtp-1',
      ['/src/file.txt'],
      'root',
      '/dst',
      expect.objectContaining({ compressionLevel: 6 }),
      undefined,
    )
  })
})
