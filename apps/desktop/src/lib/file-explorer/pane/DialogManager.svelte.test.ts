/**
 * Tests for `DialogManager.svelte`'s error boundary.
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

import DialogManager from './DialogManager.svelte'

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
