/**
 * Tests for `DialogManager.svelte`'s error boundary and its single progress
 * dialog.
 *
 * A dialog that throws while rendering used to leave the app wedged: nothing
 * reached the screen, but the `show*` flag was already true, so
 * `isConfirmationDialogOpen()` kept suppressing the pane's keyboard with no
 * dialog to escape from. The boundary has to catch the throw, hand it to the
 * recovery callback (which dismisses every dialog and refocuses the pane), and
 * leave the rest of the app mounted.
 *
 * `AlertDialog` is mocked with a fixture that throws from its instance script,
 * so the real `DialogManager` is what's under test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, type ComponentProps } from 'svelte'

vi.mock('$lib/ui/AlertDialog.svelte', async () => ({
  default: (await import('../../../../test/fixtures/dialog-throw-fixture.svelte')).default,
}))

// A marker stands in for the progress dialog: the real one dispatches a backend
// operation on mount, and what's under test here is how many of it render.
vi.mock('../../file-operations/transfer/TransferProgressDialog.svelte', async () => ({
  default: (await import('../../../../test/fixtures/dialog-marker-fixture.svelte')).default,
}))

// The error dialog is REAL in the Retry suite below, and its `ModalDialog` tells
// the backend it opened. There's no backend here.
vi.mock('$lib/tauri-commands', async (orig) => ({
  ...(await orig<typeof import('$lib/tauri-commands')>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import DialogManager from './DialogManager.svelte'
import type { AdoptedOperationData, TransferProgressPropsData } from './dialog-props'

type DialogManagerProps = ComponentProps<typeof DialogManager>

/** Every prop `DialogManager` needs, with nothing open and no-op callbacks. */
function baseProps(onDialogRenderError: (error: unknown) => void): DialogManagerProps {
  const noop = (): void => {}
  return {
    onDialogRenderError,
    showTransferDialog: false,
    transferDialogProps: null,
    showTransferProgressDialog: false,
    transferProgressProps: null,
    adoptedProgressProps: null,
    showNewFolderDialog: false,
    newFolderDialogProps: null,
    showNewFileDialog: false,
    newFileDialogProps: null,
    showAlertDialog: false,
    alertDialogProps: null,
    showTransferErrorDialog: false,
    transferErrorProps: null,
    showArchivePasswordDialog: false,
    archivePasswordProps: null,
    showDeleteDialog: false,
    deleteDialogProps: null,
    onTransferConfirm: noop,
    registerTransferConfirmer: () => noop,
    registerDeleteConfirmer: () => noop,
    onTransferCancel: noop,
    onTransferComplete: noop,
    onTransferCancelled: noop,
    onTransferError: noop,
    onTransferQueue: noop,
    onAdoptedComplete: noop,
    onAdoptedCancelled: noop,
    onAdoptedError: noop,
    onAdoptedQueue: noop,
    onTransferErrorClose: noop,
    onArchivePasswordSubmit: noop,
    onArchivePasswordCancel: noop,
    onNewFolderCreated: noop,
    onNewFolderCancel: noop,
    onNewFileCreated: noop,
    onNewFileCancel: noop,
    onAlertClose: noop,
    onDeleteConfirm: noop,
    onDeleteCancel: noop,
  }
}

/** The props for an open alert dialog, which the mock makes throw on render. */
function openAlertProps(onDialogRenderError: (error: unknown) => void): DialogManagerProps {
  return {
    ...baseProps(onDialogRenderError),
    showAlertDialog: true,
    alertDialogProps: { title: 'Heads up', message: 'Something to say' },
  }
}

describe('DialogManager error boundary', () => {
  let host: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    host = document.createElement('div')
    document.body.appendChild(host)
  })

  afterEach(() => {
    if (component) {
      void unmount(component)
      component = null
    }
    host.remove()
  })

  it('hands a dialog that throws during render to the recovery callback instead of propagating', () => {
    const onDialogRenderError = vi.fn()
    const props = openAlertProps(onDialogRenderError)

    // Mounting must NOT throw: the boundary is what stands between a broken
    // dialog and a webview with a suppressed keyboard and a blank screen.
    expect(() => {
      component = mount(DialogManager, { target: host, props }) as Record<string, unknown>
      flushSync()
    }).not.toThrow()

    expect(onDialogRenderError).toHaveBeenCalledTimes(1)
    expect(onDialogRenderError.mock.calls[0][0]).toBeInstanceOf(Error)
    expect((onDialogRenderError.mock.calls[0][0] as Error).message).toContain('blew up while rendering')
  })

  it('renders nothing after the failure, so no half-built dialog is left on screen', () => {
    const props = openAlertProps(vi.fn())

    component = mount(DialogManager, { target: host, props }) as Record<string, unknown>
    flushSync()

    expect(host.querySelector('[role="dialog"], [role="alertdialog"]')).toBeNull()
    expect(host.textContent.trim()).toBe('')
  })

  it('stays quiet and mounts normally when no dialog is open', () => {
    const onDialogRenderError = vi.fn()

    component = mount(DialogManager, { target: host, props: baseProps(onDialogRenderError) }) as Record<string, unknown>
    flushSync()

    expect(onDialogRenderError).not.toHaveBeenCalled()
  })
})

describe('DialogManager progress dialog', () => {
  let host: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    host = document.createElement('div')
    document.body.appendChild(host)
  })

  afterEach(() => {
    if (component) {
      void unmount(component)
      component = null
    }
    host.remove()
  })

  const adopted: AdoptedOperationData = {
    operationId: 'op-1',
    operationType: 'copy',
    sourcePath: '/src',
    destinationPath: '/dst',
    reverses: null,
  }

  const dispatching: TransferProgressPropsData = {
    operationType: 'copy',
    sourcePaths: ['/src/a.txt'],
    sourceFolderPath: '/src',
    sourcePaneSide: 'left',
    destinationPath: '/dst',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'local',
    duplicateFollowUp: 'nothing',
  }

  function markers(): NodeListOf<Element> {
    return host.querySelectorAll('[data-testid="progress-dialog"]')
  }

  function render(props: Partial<DialogManagerProps>) {
    component = mount(DialogManager, {
      target: host,
      props: { ...baseProps(vi.fn()), ...props },
    }) as Record<string, unknown>
    flushSync()
  }

  it('shows the adopted view when the queue handed an operation over', () => {
    render({ showTransferProgressDialog: true, adoptedProgressProps: adopted })

    expect(markers()).toHaveLength(1)
    expect(markers()[0].getAttribute('data-adopted')).toBe('op-1')
  })

  it('shows the dispatching view when this window started the operation', () => {
    render({ showTransferProgressDialog: true, transferProgressProps: dispatching })

    expect(markers()).toHaveLength(1)
    expect(markers()[0].getAttribute('data-adopted')).toBe('')
  })

  it('never stacks two progress dialogs, even with both slots filled', () => {
    // `foregroundOperation` refuses an occupied slot, so this state can't occur
    // upstream today. The markup is a chain so that stays true for free: one
    // careless edit in `dialog-state` must not put two modals over a transfer.
    render({
      showTransferProgressDialog: true,
      adoptedProgressProps: adopted,
      transferProgressProps: dispatching,
    })

    expect(markers()).toHaveLength(1)
    expect(markers()[0].getAttribute('data-adopted')).toBe('op-1')
  })
})

/**
 * The transfer error dialog's Retry button.
 *
 * cmdr-reports#17: a `delete_pending` error told the user to try again and offered
 * no Retry. `errorDisplayMetaMap` has classified nine variants as retryable since
 * it was written, and the dialog gates its button on an `onRetry` that production
 * never passed, so the table promised a button nobody ever saw.
 */
describe('DialogManager transfer error Retry', () => {
  let host: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    host = document.createElement('div')
    document.body.appendChild(host)
  })

  afterEach(() => {
    if (component) {
      void unmount(component)
      component = null
    }
    host.remove()
  })

  const failedMove: TransferProgressPropsData = {
    operationType: 'move',
    sourcePaths: ['/Users/me/a.jpg'],
    sourceFolderPath: '/Users/me',
    sourcePaneSide: 'right',
    destinationPath: '/Volumes/naspi/photos',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
    duplicateFollowUp: 'nothing',
  }

  function render(props: Partial<DialogManagerProps>) {
    component = mount(DialogManager, {
      target: host,
      props: { ...baseProps(vi.fn()), ...props },
    }) as Record<string, unknown>
    flushSync()
  }

  function retryButton(): HTMLButtonElement | undefined {
    return [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Retry')
  }

  it('offers Retry for a delete_pending error and hands the click to the retry handler', () => {
    const onTransferErrorRetry = vi.fn()
    render({
      showTransferErrorDialog: true,
      transferErrorProps: {
        operationType: 'move',
        error: { type: 'delete_pending', path: '/Volumes/naspi/photos/a.jpg' },
        progressAtStop: null,
        retry: failedMove,
      },
      onTransferErrorRetry,
    })

    const button = retryButton()
    expect(button, 'a retryable error with something to retry shows Retry').toBeDefined()
    button?.click()
    expect(onTransferErrorRetry).toHaveBeenCalledTimes(1)
  })

  it('offers no Retry when there is nothing this window could start again', () => {
    render({
      showTransferErrorDialog: true,
      transferErrorProps: {
        operationType: 'move',
        error: { type: 'delete_pending', path: '/Volumes/naspi/photos/a.jpg' },
        progressAtStop: null,
        retry: null,
      },
      onTransferErrorRetry: vi.fn(),
    })

    expect(retryButton()).toBeUndefined()
  })

  it('offers Copy anyway for a copy refused for space and hands the click to its handler', () => {
    const onTransferErrorCopyAnyway = vi.fn()
    render({
      showTransferErrorDialog: true,
      transferErrorProps: {
        operationType: 'copy',
        error: { type: 'insufficient_space', required: 2_000, available: 500, volumeName: null },
        progressAtStop: null,
        retry: { ...failedMove, operationType: 'copy' },
      },
      onTransferErrorCopyAnyway,
    })

    const button = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Copy anyway')
    expect(button).toBeDefined()
    button?.click()
    expect(onTransferErrorCopyAnyway).toHaveBeenCalledTimes(1)
  })
})
