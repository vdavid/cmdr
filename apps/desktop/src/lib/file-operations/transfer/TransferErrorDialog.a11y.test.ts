/**
 * Tier 3 a11y tests for `TransferErrorDialog.svelte`.
 *
 * `alertdialog` role with an error title, message, suggestion, and a
 * collapsible "Technical details" section. Tests cover multiple error
 * types (permission_denied, read_only_device, insufficient_space,
 * device_disconnected) and operation types.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferErrorDialog from './TransferErrorDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

describe('TransferErrorDialog a11y', () => {
  it('permission_denied (copy, close-only) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'copy',
        error: { type: 'permission_denied', path: '/Users/test/protected.txt', message: 'EACCES' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('insufficient_space (move) with retry has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'move',
        error: {
          type: 'insufficient_space',
          required: 1024 * 1024 * 500,
          available: 1024 * 1024 * 42,
          volumeName: 'External',
        },
        onClose: () => {},
        onRetry: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read_only_device (delete) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'delete',
        error: { type: 'read_only_device', path: '/Volumes/ReadOnly/file.txt', deviceName: 'ReadOnly' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('device_disconnected (trash) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'trash',
        error: { type: 'device_disconnected', path: '/Volumes/External/file.txt' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
