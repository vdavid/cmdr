/**
 * Starting an operation straight in the background (#381), and watching one
 * that went there.
 *
 * The start is the progress dialog's birth with the dialog taken out: the same
 * dispatch, but no foreground claim, so the conflict host owns any clash, and no
 * slot, so the next F5 opens at once. Afterwards the job is ambient, with one
 * exception the window still owes: an archive-password stop, which the backend
 * doesn't retain and nobody else would ask.
 *
 * Driven headlessly through the real session registry and the progress
 * harness's mocked event streams, so an event reaches the watch down the path a
 * live one takes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { WriteOperationError } from '$lib/file-explorer/types'

vi.mock('$lib/tauri-commands', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).tauriCommandsMock(),
)
vi.mock('$lib/file-operations/queue/queue-window', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).queueWindowMock(),
)
vi.mock('$lib/ui/toast', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).toastMock(),
)
vi.mock('$lib/settings', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).settingsMock(),
)
vi.mock('$lib/intl/messages.svelte', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).messagesMock(),
)
vi.mock('$lib/logging/logger', async () =>
  (await import('$lib/file-operations/transfer/test-transfer-progress-harness.svelte')).loggerMock(),
)

import { createBackgroundOperations, type BackgroundOperationsDeps } from './background-operations.svelte'
import { listeners, settle, snapshot } from '$lib/file-operations/transfer/test-transfer-progress-harness.svelte'
import { copyBetweenVolumes, listOperations, trashFiles, deleteFiles } from '$lib/tauri-commands'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { addToast } from '$lib/ui/toast'
import { getForegroundOperationId, isForegroundClaimPending } from '$lib/file-operations/foreground-operation.svelte'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import { conflictOwner } from '$lib/file-operations/operation-conflict-rules'
import type { TransferProgressPropsData } from './dialog-props'
import type { FilePaneAPI } from './types'

const SOURCE_FOLDER = '/Users/me/photos'

function copyProps(over: Partial<TransferProgressPropsData> = {}): TransferProgressPropsData {
  return {
    operationType: 'copy',
    sourcePaths: [`${SOURCE_FOLDER}/a.jpg`],
    sourceFolderPath: SOURCE_FOLDER,
    sourcePaneSide: 'left',
    destinationPath: '/Users/me/backup',
    direction: 'right',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    conflictResolution: 'stop',
    preKnownConflicts: [],
    duplicateFollowUp: 'nothing',
    startInBackground: true,
    ...over,
  }
}

function makePane(currentPath = SOURCE_FOLDER) {
  const spies = {
    clearSelection: vi.fn(),
    clearOperationSnapshot: vi.fn(() => null),
    getCurrentPath: vi.fn(() => currentPath),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

function makeBackground() {
  const left = makePane()
  const right = makePane('/Users/me/backup')
  const deps = {
    getLeftPaneRef: () => left.ref,
    getRightPaneRef: () => right.ref,
    onStartRefused: vi.fn(),
    onNeedsPassword: vi.fn(),
    onCompleted: vi.fn(),
  } satisfies BackgroundOperationsDeps
  return { background: createBackgroundOperations(deps), deps, left }
}

const needsPassword: WriteOperationError = {
  type: 'archive_needs_password',
  path: '/Users/me/secret.zip',
  wrongAttempt: false,
}

beforeEach(async () => {
  vi.clearAllMocks()
  listeners.error = null
  listeners.complete = null
  vi.mocked(listOperations).mockResolvedValue([snapshot('op-1', 'running')])
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

describe('starting in the background', () => {
  it('dispatches exactly what the progress dialog would, from the same props', async () => {
    const { background } = makeBackground()

    await background.start(copyProps())

    expect(copyBetweenVolumes).toHaveBeenCalledWith(
      'root',
      [`${SOURCE_FOLDER}/a.jpg`],
      'root',
      '/Users/me/backup',
      expect.objectContaining({ conflictResolution: 'stop', previewId: 'preview-1', preKnownConflicts: [] }),
      undefined,
    )
  })

  it('claims nothing in the foreground, so a clash in the job is the conflict host’s to ask', async () => {
    let claimDuringDispatch: boolean | null = null
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => {
      claimDuringDispatch = isForegroundClaimPending()
      return Promise.resolve({ operationId: 'op-1', operationType: 'copy' })
    })
    const { background } = makeBackground()

    await background.start(copyProps())

    expect(claimDuringDispatch).toBe(false)
    expect(getForegroundOperationId()).toBeNull()
    // So the conflict host takes the clash, during the dispatch and after it:
    // F2 never means "overwrite silently".
    const foreground = { foregroundOperationId: getForegroundOperationId(), claimPending: isForegroundClaimPending() }
    expect(conflictOwner('op-1', foreground)).toBe('here')
  })

  it('shows the queue without taking focus, says where the job went, and drops the source selection', async () => {
    const { background, left } = makeBackground()

    await background.start(copyProps())

    expect(openQueueWindow).toHaveBeenCalledWith({ focus: false })
    expect(addToast).toHaveBeenCalledWith('fileOperations.backgroundStart.startedToast', {
      level: 'info',
      toastGroup: 'transfer-queue',
    })
    expect(left.spies.clearSelection).toHaveBeenCalledOnce()
  })

  it('starts a trash as a trash, never as a delete', async () => {
    const { background } = makeBackground()

    await background.start(copyProps({ operationType: 'trash', destinationPath: undefined, itemSizes: [10] }))

    expect(trashFiles).toHaveBeenCalledOnce()
    expect(deleteFiles).not.toHaveBeenCalled()
  })

  it('hands a refused start back with its typed error, and touches nothing else', async () => {
    const refusal = Object.assign(new Error('inside'), { type: 'destination_inside_source', path: '/src' })
    vi.mocked(copyBetweenVolumes).mockImplementationOnce(() => Promise.reject(refusal))
    const { background, deps, left } = makeBackground()
    const props = copyProps()

    await background.start(props)

    expect(deps.onStartRefused).toHaveBeenCalledWith(props, refusal)
    expect(openQueueWindow).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
    expect(left.spies.clearSelection).not.toHaveBeenCalled()
  })
})

describe('watching a background job', () => {
  it('passes an archive-password stop back with the props it was born from', async () => {
    const { background, deps } = makeBackground()
    const props = copyProps()
    await background.start(props)
    await settle()

    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })
    await settle()

    expect(deps.onNeedsPassword).toHaveBeenCalledWith(props, needsPassword, 'op-1')
  })

  it('catches a password stop that landed before the watch began', async () => {
    const { background, deps } = makeBackground()
    const props = copyProps()
    // Delivered while nothing has claimed op-1: the fan-out holds it.
    listeners.error?.({ operationId: 'op-1', operationType: 'copy', error: needsPassword, progressAtStop: null })

    background.watch(props, 'op-1')
    await settle()

    expect(deps.onNeedsPassword).toHaveBeenCalledWith(props, needsPassword, 'op-1')
  })

  it('leaves every other ending to its ambient surface', async () => {
    const { background, deps } = makeBackground()
    await background.start(copyProps())
    await settle()

    listeners.error?.({
      operationId: 'op-1',
      operationType: 'copy',
      error: { type: 'io_error', path: '/x', message: 'boom' },
      progressAtStop: null,
    })
    await settle()

    expect(deps.onNeedsPassword).not.toHaveBeenCalled()
    expect(deps.onStartRefused).not.toHaveBeenCalled()
  })
})
