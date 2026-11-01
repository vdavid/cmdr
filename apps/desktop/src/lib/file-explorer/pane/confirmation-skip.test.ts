/**
 * Which confirmations the "Skip confirmation" setting may leave out, and the
 * answer a skipped one gives in the dialog's place.
 */

import { describe, expect, it, vi } from 'vitest'
import { skippedDeleteConfirmation, skippedTransferConfirmation } from './confirmation-skip'
import type { DeleteDialogPropsData } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { Initiator } from '#lib/tauri-commands/index.js'
import type { VolumeInfo } from '../types'

const VOLUMES: VolumeInfo[] = [
  { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
  { id: 'stick', name: 'Stick', path: '/Volumes/Stick', category: 'attached_volume', isEjectable: true },
  { id: 's3-bucket', name: 'Bucket', path: 's3://bucket', category: 'network', fsType: 's3', isEjectable: false },
] as VolumeInfo[]

vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => VOLUMES }))

function transferProps(overrides: Partial<TransferDialogPropsData> = {}): TransferDialogPropsData {
  return {
    operationType: 'copy',
    sourcePaths: ['/Users/me/photos/first.jpg'],
    destinationPath: '/Users/me/backup',
    direction: 'left',
    currentVolumeId: 'root',
    fileCount: 1,
    folderCount: 0,
    sourceFolderPath: '/Users/me/photos',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'openRenameEditor',
    initiator: 'user',
    ...overrides,
  }
}

function deleteProps(overrides: Partial<DeleteDialogPropsData> = {}): DeleteDialogPropsData {
  return {
    sourceItems: [{ name: 'first.jpg', size: 10, isDirectory: false, isSymlink: false }],
    sourcePaths: ['/Users/me/photos/first.jpg'],
    sourceFolderPath: '/Users/me/photos',
    isPermanent: false,
    supportsTrash: true,
    isFromCursor: true,
    sourceVolumeId: 'root',
    initiator: 'user',
    ...overrides,
  }
}

const NON_USER_INITIATORS: readonly Exclude<Initiator, 'user'>[] = ['agent', 'agentEdited', 'aiClient']

/** A payload that slipped past the types (an untyped dispatch) has to fail closed. */
function withoutInitiator<T extends { initiator: Initiator }>(props: T): T {
  return { ...props, initiator: undefined }
}

describe('skippedTransferConfirmation', () => {
  it('answers a copy with what the dialog preselects: the other pane, asking on conflicts', () => {
    expect(skippedTransferConfirmation(transferProps(), VOLUMES)).toEqual({
      destination: '/Users/me/backup',
      volumeId: 'root',
      previewId: null,
      conflictResolution: 'stop',
      operationType: 'copy',
      preKnownConflicts: [],
    })
  })

  it('answers a move the same way', () => {
    expect(skippedTransferConfirmation(transferProps({ operationType: 'move' }), VOLUMES)?.operationType).toBe('move')
  })

  it('sends the destination volume-relative, the way the dialog box holds it', () => {
    const props = transferProps({
      destinationPath: '/Volumes/Stick/backup',
      currentVolumeId: 'stick',
      destVolumeId: 'stick',
    })
    expect(skippedTransferConfirmation(props, VOLUMES)?.destination).toBe('/backup')
  })

  it('shows the dialog for a compress, which has a name to pick', () => {
    expect(skippedTransferConfirmation(transferProps({ operationType: 'compress' }), VOLUMES)).toBeNull()
  })

  it('shows the dialog in rename mode', () => {
    expect(skippedTransferConfirmation(transferProps({ operationType: 'move', newName: 'b' }), VOLUMES)).toBeNull()
  })

  it.each(NON_USER_INITIATORS)('shows the dialog when %s started it', (initiator) => {
    expect(skippedTransferConfirmation(transferProps({ initiator }), VOLUMES)).toBeNull()
  })

  it('skips for a person', () => {
    expect(skippedTransferConfirmation(transferProps({ initiator: 'user' }), VOLUMES)).not.toBeNull()
  })

  it('shows the dialog when nothing says who started it', () => {
    expect(skippedTransferConfirmation(withoutInitiator(transferProps()), VOLUMES)).toBeNull()
  })

  it('shows the dialog for an MCP round-trip, whoever it names', () => {
    expect(skippedTransferConfirmation(transferProps({ autoConfirm: true }), VOLUMES)).toBeNull()
    expect(skippedTransferConfirmation(transferProps({ mcpRequestId: 'req-1' }), VOLUMES)).toBeNull()
  })

  it('shows the dialog when the destination is inside a source, so its error gets seen', () => {
    const props = transferProps({
      sourcePaths: ['/Users/me/photos'],
      sourceFolderPath: '/Users/me',
      destinationPath: '/Users/me/photos/2026',
    })
    expect(skippedTransferConfirmation(props, VOLUMES)).toBeNull()
  })

  it('shows the dialog for a copy into its own subfolder on a drive under /Volumes', () => {
    const props = transferProps({
      sourcePaths: ['/Volumes/Stick/photos'],
      sourceFolderPath: '/Volumes/Stick',
      sourceVolumeId: 'stick',
      destinationPath: '/Volumes/Stick/photos/2026',
      currentVolumeId: 'stick',
      destVolumeId: 'stick',
    })
    expect(skippedTransferConfirmation(props, VOLUMES)).toBeNull()
  })

  it('skips a copy whose destination on another drive only repeats the source path', () => {
    const props = transferProps({
      sourcePaths: ['/photos'],
      sourceFolderPath: '/',
      destinationPath: '/Volumes/Stick/photos/2026',
      currentVolumeId: 'stick',
      destVolumeId: 'stick',
    })
    expect(skippedTransferConfirmation(props, VOLUMES)?.destination).toBe('/photos/2026')
  })

  it('shows the dialog for a move into the folder the items already sit in', () => {
    const props = transferProps({ operationType: 'move', destinationPath: '/Users/me/photos' })
    expect(skippedTransferConfirmation(props, VOLUMES)).toBeNull()
  })

  it('still skips a copy into the same folder, which duplicates', () => {
    expect(skippedTransferConfirmation(transferProps({ destinationPath: '/Users/me/photos' }), VOLUMES)).not.toBeNull()
  })

  it('shows the dialog when either side is S3, so the cost line gets seen', () => {
    expect(skippedTransferConfirmation(transferProps({ sourceVolumeId: 's3-bucket' }), VOLUMES)).toBeNull()
    expect(
      skippedTransferConfirmation(
        transferProps({ destVolumeId: 's3-bucket', currentVolumeId: 's3-bucket', destinationPath: '/backup' }),
        VOLUMES,
      ),
    ).toBeNull()
  })
})

describe('skippedDeleteConfirmation', () => {
  it('skips a move to the trash', () => {
    expect(skippedDeleteConfirmation(deleteProps())).toEqual({ previewId: null, isPermanent: false })
  })

  it('never skips a permanent delete', () => {
    expect(skippedDeleteConfirmation(deleteProps({ isPermanent: true }))).toBeNull()
  })

  it('never skips where the volume has no trash', () => {
    expect(skippedDeleteConfirmation(deleteProps({ supportsTrash: false }))).toBeNull()
  })

  it('never skips inside an archive', () => {
    expect(skippedDeleteConfirmation(deleteProps({ isArchive: true }))).toBeNull()
  })

  it('never skips when a cloud folder might turn the trash into a delete mid-scan', () => {
    expect(skippedDeleteConfirmation(deleteProps({ cloudFolderMayHoldOnlineOnly: true }))).toBeNull()
    expect(skippedDeleteConfirmation(deleteProps({ cloudOnlineOnly: 'mixed' }))).toBeNull()
  })

  it.each(NON_USER_INITIATORS)('never skips when %s started it', (initiator) => {
    expect(skippedDeleteConfirmation(deleteProps({ initiator }))).toBeNull()
  })

  it('skips for a person', () => {
    expect(skippedDeleteConfirmation(deleteProps({ initiator: 'user' }))).not.toBeNull()
  })

  it('never skips when nothing says who started it', () => {
    expect(skippedDeleteConfirmation(withoutInitiator(deleteProps()))).toBeNull()
  })

  it('never skips an MCP round-trip, whoever it names', () => {
    expect(skippedDeleteConfirmation(deleteProps({ autoConfirm: true }))).toBeNull()
    expect(skippedDeleteConfirmation(deleteProps({ mcpRequestId: 'req-1' }))).toBeNull()
  })
})
