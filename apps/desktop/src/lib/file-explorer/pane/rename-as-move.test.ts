/**
 * What the Move dialog opens with when F2 hands it a rename that copies too much
 * to start unasked: one source, renamed in place on its own volume.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { blockedSpy } = vi.hoisted(() => ({ blockedSpy: vi.fn(() => false) }))
vi.mock('./operation-start-gate', () => ({ operationStartIsBlocked: blockedSpy }))

import { openRenameAsMove, renameAsMoveDialogProps } from './rename-as-move'
import type { PaneAccess } from './pane-access'

const SORT = { sortBy: 'name' as const, sortOrder: 'ascending' as const }

describe('renameAsMoveDialogProps', () => {
  const props = renameAsMoveDialogProps('left', 's3-acct', SORT, {
    sourcePath: '/bucket/photos',
    parentPath: '/bucket',
    newName: 'pictures',
    isDirectory: true,
  })

  it('moves the one source into the folder it already lives in, under the new name', () => {
    expect(props).toMatchObject({
      operationType: 'move',
      sourcePaths: ['/bucket/photos'],
      destinationPath: '/bucket',
      sourceFolderPath: '/bucket',
      newName: 'pictures',
      folderCount: 1,
      fileCount: 0,
    })
  })

  it('stays on the pane’s own volume on both sides', () => {
    expect(props).toMatchObject({ sourceVolumeId: 's3-acct', destVolumeId: 's3-acct', currentVolumeId: 's3-acct' })
  })

  it('names the other pane as the destination side, so confirm derives THIS pane as the source', () => {
    // `handleTransferConfirm` reads the source pane as the opposite of `direction`.
    expect(props.direction).toBe('right')
  })

  it('never ends in the duplicate rename editor', () => {
    expect(props.duplicateFollowUp).toBe('nothing')
  })
})

describe('openRenameAsMove', () => {
  const request = { sourcePath: '/bucket/big.iso', parentPath: '/bucket', newName: 'big-2.iso', isDirectory: false }
  const access = {
    getPaneVolumeId: () => 's3-acct',
    getPaneSort: () => ({ sortBy: 'size', sortOrder: 'descending' }),
  } as unknown as PaneAccess

  beforeEach(() => {
    blockedSpy.mockReturnValue(false)
  })

  it('opens the Move dialog with the pane’s volume and sort', () => {
    const showTransfer = vi.fn()

    openRenameAsMove(access, { showTransfer } as never, 'right', request)

    expect(showTransfer).toHaveBeenCalledWith(
      expect.objectContaining({
        sourceVolumeId: 's3-acct',
        sortColumn: 'size',
        sortOrder: 'descending',
        fileCount: 1,
        folderCount: 0,
        direction: 'left',
        newName: 'big-2.iso',
      }),
    )
  })

  it('opens nothing while another dialog is in the way', () => {
    blockedSpy.mockReturnValue(true)
    const showTransfer = vi.fn()

    openRenameAsMove(access, { showTransfer } as never, 'left', request)

    expect(showTransfer).not.toHaveBeenCalled()
  })
})
