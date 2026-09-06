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

  it('routes a move into the zip ROOT through moveBetweenVolumes (the F6-into-an-open-zip case)', async () => {
    // The destination shape the other cases missed: the pane sits AT `/left/foo.zip`,
    // which is exactly where Enter on a zip lands you, so it's the ordinary way to
    // F6 into an archive. Both ids are the parent drive's `root`, so only the
    // destination path can catch this — and it has to be asked the WIDE question,
    // because a destination names a container to write INTO. The narrow one says
    // "the `.zip` file itself isn't inside an archive", the local `moveFiles` fast
    // path runs, and the backend stats a regular file and refuses with
    // "Destination must be a directory".
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/right/file-a.txt'],
        destinationPath: '/left/foo.zip',
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

  it('keeps a move of the ARCHIVE FILE itself on the local fast-path', async () => {
    // The other half of the boundary check, and the one a wide predicate gets
    // wrong: `/left/foo.zip` names a real file on disk, so moving it is an
    // ordinary same-drive move — no archive-edit flow, no cross-volume route.
    // The backend agrees (its routing asks `path_is_inside_archive`, which is
    // false for the `.zip` itself), so routing it cross-volume would be the FE
    // inventing work the backend never asked for.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/foo.zip'],
        destinationPath: '/right',
      }),
    )
    expect(moveFiles).toHaveBeenCalledTimes(1)
    expect(moveBetweenVolumes).not.toHaveBeenCalled()
  })

  it('keeps a move of an Office document on the local fast-path', async () => {
    // A `.docx` is a browsable zip container, so the boundary check sees it. It's
    // still a plain file: moving one is an ordinary same-drive move, and the
    // narrow check is what keeps every Office-document move off the slow route.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/report.docx', '/left/sheet.xlsx'],
        destinationPath: '/right',
      }),
    )
    expect(moveFiles).toHaveBeenCalledTimes(1)
    expect(moveBetweenVolumes).not.toHaveBeenCalled()
  })

  it('asks the destination a WIDER question than the source, in one move', async () => {
    // The asymmetry in a single call, so neither side can be quietly changed to
    // match the other. Source `/left/foo.zip` is an archive FILE being moved (an
    // ordinary file: narrow says no); destination `/left/bar.zip` is an archive
    // being written INTO (wide says yes). One `true` is enough to route, and it
    // has to come from the destination.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/foo.zip'],
        destinationPath: '/left/bar.zip',
      }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move into a DOCUMENT container root cross-volume, where the backend refuses it', async () => {
    // Read-only is the backend's call, not the dispatcher's. Routing it correctly
    // is what gets the user a typed `ReadOnlyDevice` refusal from
    // `ensure_zip_writable` instead of a confusing "Destination must be a
    // directory" from a fast path that should never have run.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/right/file-a.txt'],
        destinationPath: '/left/report.docx',
      }),
    )
    expect(moveBetweenVolumes).toHaveBeenCalledTimes(1)
    expect(moveFiles).not.toHaveBeenCalled()
  })

  it('routes a move out of a DOCUMENT container cross-volume, where the backend refuses it', async () => {
    // Read-only is the backend's to enforce, not the dispatcher's: this reaches
    // `moveBetweenVolumes`, where `ensure_zip_writable` returns `ReadOnlyDevice`
    // because a `.docx` is `ArchiveFormat::Ooxml`, never `Zip`.
    await dispatchTransferOperation(
      makeConfig({
        operationType: 'move',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        sourcePaths: ['/left/report.docx/word/document.xml'],
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
