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
import type { WriteOperationError } from '$lib/file-explorer/types'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

function mountDialog(props: {
  error: WriteOperationError
  onRetry?: () => void
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
    },
  })
  return target
}

describe('TransferErrorDialog: typed-error rendering', () => {
  it('renders the FE-derived title for the typed error', async () => {
    const target = mountDialog({ error: { type: 'source_not_found', path: '/p' } })
    await tick()
    expect(target.textContent).toContain("Couldn't find the file")
  })

  it('renders message and suggestion from the typed error', async () => {
    const target = mountDialog({ error: { type: 'destination_exists', path: '/dest/file.txt' } })
    await tick()
    expect(target.textContent).toContain("There's already a file with this name at the destination.")
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
    const target = mountDialog({ error: { type: 'read_only_device', path: '/p', deviceName: null } })
    await tick()
    const icon = target.querySelector('.error-icon')
    expect(icon?.className).toContain('icon-info')
  })

  it('renders Retry when the category is transient (even without retryHint)', async () => {
    const target = mountDialog({ error: { type: 'connection_interrupted', path: '/p' }, onRetry: () => {} })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).toContain('Retry')
  })

  it('renders Retry when retryHint is true on a non-transient category', async () => {
    // io_error → serious, retryHint=true
    const target = mountDialog({ error: { type: 'io_error', path: '/p', message: 'm' }, onRetry: () => {} })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).toContain('Retry')
  })

  it('hides Retry when category is needs_action and retryHint is false', async () => {
    // read_only_device → needs_action, retryHint=false
    const target = mountDialog({ error: { type: 'read_only_device', path: '/p', deviceName: null }, onRetry: () => {} })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
    expect(buttons).not.toContain('Retry')
  })

  it('shows the typed error in the technical-details textarea', async () => {
    const target = mountDialog({ error: { type: 'io_error', path: '/some/path', message: 'boom' } })
    await tick()

    const toggle = target.querySelector<HTMLButtonElement>('.details-toggle')
    toggle?.click()
    await tick()

    const textarea = target.querySelector<HTMLTextAreaElement>('.details-text')
    expect(textarea?.value).toContain('Path: /some/path')
    expect(textarea?.value).toContain('Error: boom')
    expect(textarea?.value).toContain('Error type: io_error')
  })
})
