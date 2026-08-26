/**
 * Starting a file operation while the progress slot is already taken.
 *
 * The slot holds ONE operation. A second start used to walk in and overwrite the
 * first one's props: the mounted dialog got new props, never re-dispatched, and
 * the user saw nothing happen and heard nothing about it. The native menu and MCP
 * both reach this without passing any frontend modal gate, so the refusal has to
 * live here, at the start itself, and it has to SAY something:
 *
 * - a toast, for the person who picked File > Copy;
 * - an `mcp-response` failure naming the blocking dialog in a TYPED field, for the
 *   agent that has to decide what to close;
 * - and the operation already running is left completely alone.
 *
 * ❌ The gate covers commands that START an operation, never the ones that steer a
 * running one: cancel, queue, and a settled outcome all keep working while the
 * progress dialog is up, which is exactly when a user needs them.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import { createDialogState } from './dialog-state.svelte'
import type { TransferProgressPropsData } from './dialog-props'
import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'
import { addToast } from '$lib/ui/toast'

vi.mock('$lib/tauri-commands', () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword: vi.fn(() => Promise.resolve()),
  clearArchivePassword: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

const SOURCE_FOLDER = '/Users/me/photos'

function makePaneRef(currentPath = SOURCE_FOLDER) {
  return {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    getCurrentPath: vi.fn(() => currentPath),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  } as unknown as FilePaneAPI
}

function makeState() {
  const pane = makePaneRef()
  const dialogs = createDialogState({
    getLeftPaneRef: () => pane,
    getRightPaneRef: () => pane,
    getFocusedPaneRef: () => pane,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    onRefocus: vi.fn(),
    onOpenInEditor: vi.fn(),
  })
  return { dialogs }
}

function copyProps(overrides: Partial<TransferProgressPropsData> = {}): TransferProgressPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/first.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    sourcePaneSide: 'right',
    destinationPath: '/Users/me/backup',
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    duplicateFollowUp: 'nothing',
    ...overrides,
  }
}

/** The confirmation dialog's payload, the shape `handleTransferConfirm` reads. */
function transferConfirmationProps(mcpRequestId?: string): TransferDialogPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/second.jpg`],
    destinationPath: '/Users/me/elsewhere',
    direction: 'left',
    currentVolumeId: 'root',
    fileCount: 1,
    folderCount: 0,
    sourceFolderPath: SOURCE_FOLDER,
    sortColumn: 'name',
    sortOrder: 'ascending',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    mcpRequestId,
    duplicateFollowUp: 'nothing',
  }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('a second operation while the progress dialog is up', () => {
  it('refuses, naming the dialog in the way', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    const verdict = dialogs.startTransferProgress(copyProps({ sourcePaths: ['/Users/me/other.jpg'] }))

    expect(verdict).toEqual({ blockedBy: 'transfer-progress' })
  })

  it('leaves the running operation exactly as it was', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.startTransferProgress(copyProps({ sourcePaths: ['/Users/me/other.jpg'], destinationPath: '/tmp/wrong' }))

    // Pre-fix the second start overwrote these, so the mounted dialog re-rendered
    // against an operation it had never dispatched.
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual([`${SOURCE_FOLDER}/first.jpg`])
    expect(dialogs.transferProgressProps?.destinationPath).toBe('/Users/me/backup')
  })

  it('tells the person who asked', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.startTransferProgress(copyProps())

    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('names the password prompt when that is what holds the slot', () => {
    // Birth context alive with NOTHING on screen but the prompt. An agent told
    // "transfer-progress is open" here would try to close a dialog that isn't up.
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps({ sourcePaths: ['/Users/me/secret.zip/inner/report.pdf'] }))
    dialogs.handleTransferError({
      type: 'archive_needs_password',
      path: '/Users/me/secret.zip/inner/report.pdf',
      wrongAttempt: false,
    })
    expect(dialogs.showTransferProgressDialog).toBe(false)

    const verdict = dialogs.startTransferProgress(copyProps())

    expect(verdict).toEqual({ blockedBy: 'archive-password' })
  })
})

describe('the refusal an MCP agent receives', () => {
  it('fails the round-trip instead of letting it time out, and types the blocking dialog', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.showTransfer(transferConfirmationProps('req-7'))

    dialogs.handleTransferConfirm({
      destination: '/Users/me/elsewhere',
      volumeId: 'root',
      previewId: null,
      conflictResolution: 'skip',
      operationType: 'copy',
      preKnownConflicts: [],
    })

    expect(emit).toHaveBeenCalledWith(
      'mcp-response',
      expect.objectContaining({ requestId: 'req-7', ok: false, blockedBy: 'transfer-progress' }),
    )
  })

  it('carries a sentence the agent can act on', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.showTransfer(transferConfirmationProps('req-7'))

    dialogs.handleTransferConfirm({
      destination: '/Users/me/elsewhere',
      volumeId: 'root',
      previewId: null,
      conflictResolution: 'skip',
      operationType: 'copy',
      preKnownConflicts: [],
    })

    const payload = vi.mocked(emit).mock.calls.find(([name]) => name === 'mcp-response')?.[1] as {
      error?: string
    }
    expect(payload.error).toContain('transfer-progress')
  })

  it('stays quiet on the round-trip when a person started it', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.showTransfer(transferConfirmationProps())

    dialogs.handleTransferConfirm({
      destination: '/Users/me/elsewhere',
      volumeId: 'root',
      previewId: null,
      conflictResolution: 'skip',
      operationType: 'copy',
      preKnownConflicts: [],
    })

    expect(emit).not.toHaveBeenCalledWith('mcp-response', expect.anything())
  })
})

describe('steering the operation that IS running', () => {
  // The boundary the gate must not cross. Cancel, queue, and the settled outcomes
  // are what a user reaches for while the dialog is up; gating them would be worse
  // than the silent no-op this whole file exists to close.

  it('still backgrounds it', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferQueue()

    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
  })

  it('still cancels it', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferCancelled(3)

    expect(dialogs.showTransferProgressDialog).toBe(false)
  })

  it('frees the slot for the next operation once it settles', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferComplete({ filesProcessed: 1, filesSkipped: 0, bytesProcessed: 2048 })

    expect(dialogs.startTransferProgress(copyProps())).toBe('started')
  })

  it('lets a new operation past an ADOPTED one, which owns no slot', () => {
    // Watching an operation from the queue window isn't owning one. Refusing here
    // would break the birth-wins-over-adoption rule
    // (`dialog-state.foreground.svelte.test.ts`) and leave a user unable to start
    // anything while a queue row happened to be on screen.
    const { dialogs } = makeState()
    dialogs.foregroundOperation({
      operationId: 'op-42',
      operationType: 'copy',
      sourcePath: '/Volumes/Card/DCIM',
      destinationPath: '/Users/me/import',
    })

    expect(dialogs.startTransferProgress(copyProps())).toBe('started')
    expect(dialogs.adoptedProgressProps).toBeNull()
  })
})
