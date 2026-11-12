/**
 * Tests for `TransferErrorDialog.svelte`'s typed-error rendering path.
 *
 * The dialog renders entirely from the typed `WriteOperationError`:
 *   - title / message / suggestion via `getUserFriendlyMessage`
 *   - category (icon + container tint) and Retry visibility via `getErrorDisplayMeta`
 *   - technical details via `getTechnicalDetails`
 *
 * No backend prose crosses IPC; the FE owns all the words and the classification.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferErrorDialog from './TransferErrorDialog.svelte'
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { buttonLabel } from '../test-button-label'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

const openErrorReportFor = vi.hoisted(() => vi.fn())
vi.mock('#lib/error-reporter/error-report-flow.svelte.js', () => ({ openErrorReportFor }))

function mountDialog(props: {
  error: WriteOperationError
  onRetry?: () => void
  onCopyAnyway?: () => void
  operationType?: 'copy' | 'move' | 'delete' | 'trash'
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferErrorDialog, {
    target,
    props: {
      operationType: props.operationType ?? 'move',
      error: props.error,
      onClose: () => {},
      ...(props.onRetry ? { onRetry: props.onRetry } : {}),
      ...(props.onCopyAnyway ? { onCopyAnyway: props.onCopyAnyway } : {}),
    },
  })
  return target
}

describe('TransferErrorDialog: typed-error rendering', () => {
  it('renders the FE-derived title for the typed error', async () => {
    const target = mountDialog({ error: { type: 'source_not_found', path: '/p' } })
    await tick()
    expect(target.textContent).toContain('Couldn’t find the file')
  })

  it('renders message and suggestion from the typed error', async () => {
    const target = mountDialog({ error: { type: 'destination_exists', path: '/dest/file.txt' } })
    await tick()
    expect(target.textContent).toContain('There’s already a file with this name at the destination.')
    expect(target.textContent).toContain('Choose a different name')
  })

  it('uses error styling (CircleAlert icon) for a serious category', async () => {
    const target = mountDialog({ error: { type: 'io_error', path: '/p', message: 'm' } })
    await tick()
    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-error')
  })

  it('uses warning styling (TriangleAlert icon) for a transient category', async () => {
    const target = mountDialog({ error: { type: 'connection_interrupted', path: '/p' } })
    await tick()
    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-warning')
  })

  it('uses neutral styling (Info icon) for a needs_action category', async () => {
    const target = mountDialog({
      error: { type: 'read_only_device', path: '/p', deviceName: null, side: 'destination' },
    })
    await tick()
    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-info')
  })

  it('renders Retry when the category is transient (even without retryHint)', async () => {
    const target = mountDialog({ error: { type: 'connection_interrupted', path: '/p' }, onRetry: () => {} })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map(buttonLabel)
    expect(buttons).toContain('Retry')
  })

  it('renders Retry when retryHint is true on a non-transient category', async () => {
    // io_error → serious, retryHint=true
    const target = mountDialog({ error: { type: 'io_error', path: '/p', message: 'm' }, onRetry: () => {} })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map(buttonLabel)
    expect(buttons).toContain('Retry')
  })

  it('hides Retry when category is needs_action and retryHint is false', async () => {
    // read_only_device → needs_action, retryHint=false
    const target = mountDialog({
      error: { type: 'read_only_device', path: '/p', deviceName: null, side: 'destination' },
      onRetry: () => {},
    })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map(buttonLabel)
    expect(buttons).not.toContain('Retry')
  })

  // The connection to a server ended the copy: the dialog leads with the server, words
  // the wait, and offers a Retry for once it's back.
  it('renders a lost server connection with the server as the cause, and a Retry', async () => {
    const target = mountDialog({
      error: { type: 'server_connection_lost', path: '/nas/a.jpg', volumeName: 'Testnas', gaveUpAfterSecs: null },
      operationType: 'copy',
      onRetry: () => {},
    })
    await tick()
    expect(target.textContent).toContain('Lost the connection to Testnas')
    expect(target.textContent).toContain('The connection to Testnas dropped while Cmdr was copying a.jpg.')
    expect(target.querySelector('.error-icon')?.className).toContain('icon-info')
    expect(Array.from(target.querySelectorAll('button')).map(buttonLabel)).toContain('Retry')
  })

  describe('a space shortfall (#351)', () => {
    const shortfall: WriteOperationError = {
      type: 'insufficient_space',
      required: 2_000_000_000,
      available: 500_000_000,
      volumeName: 'Backup',
    }

    function buttonLabels(target: HTMLElement): string[] {
      return Array.from(target.querySelectorAll('button')).map(buttonLabel)
    }

    it('offers Copy anyway on a copy, which starts it again', async () => {
      const onCopyAnyway = vi.fn()
      const target = mountDialog({ error: shortfall, operationType: 'copy', onCopyAnyway })
      await tick()
      const button = Array.from(target.querySelectorAll('button')).find((b) => buttonLabel(b) === 'Copy anyway')
      expect(button).toBeDefined()
      button?.click()
      expect(onCopyAnyway).toHaveBeenCalledTimes(1)
    })

    it('offers no Copy anyway when nothing can start the copy again', async () => {
      const target = mountDialog({ error: shortfall, operationType: 'copy' })
      await tick()
      expect(buttonLabels(target)).not.toContain('Copy anyway')
    })

    it('offers no Copy anyway for another error', async () => {
      const target = mountDialog({
        error: { type: 'read_only_device', path: '/p', deviceName: null, side: 'destination' },
        operationType: 'copy',
        onCopyAnyway: () => {},
      })
      await tick()
      expect(buttonLabels(target)).not.toContain('Copy anyway')
    })
  })

  it('shows the typed error in the technical-details textarea', async () => {
    const target = mountDialog({ error: { type: 'io_error', path: '/some/path', message: 'boom' } })
    await tick()

    const toggle = target.querySelector<HTMLButtonElement>('.details-toggle')
    toggle?.click()
    await tick()

    const textarea = target.querySelector<HTMLTextAreaElement>('textarea')
    expect(textarea?.value).toContain('Path: /some/path')
    expect(textarea?.value).toContain('Error: boom')
    expect(textarea?.value).toContain('Error type: io_error')
  })
})

describe('TransferErrorDialog: Send error report…', () => {
  it('offers a report naming the operation, the typed error kind, and the title it shows', async () => {
    const target = mountDialog({ error: { type: 'source_not_found', path: '/p' }, operationType: 'copy' })
    await tick()
    const send = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Send error report…')
    expect(send).toBeDefined()
    send?.click()
    expect(openErrorReportFor).toHaveBeenCalledWith({
      kind: 'operationFailed',
      from: 'transferError',
      operation: 'copy',
      error: 'source_not_found',
      cause: null,
      title: 'Couldn’t find the file',
    })
  })
})
