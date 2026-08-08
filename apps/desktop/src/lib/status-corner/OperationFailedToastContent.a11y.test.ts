/**
 * Tier 3 a11y tests for `OperationFailedToastContent.svelte`. Its explanation is
 * markup injected by the error pipeline, so it's worth an axe pass of its own.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

import OperationFailedToastContent from './OperationFailedToastContent.svelte'

const snapshot: OperationSnapshot = {
  operationId: 'op-1',
  operationType: 'copy',
  status: 'failed',
  source: '/Users/me/Documents/report.pdf',
  destination: '/Volumes/Backup',
  supportsRollback: false,
  error: { type: 'insufficient_space', required: 1073741824, available: 1024, volumeName: 'Backup' },
}

describe('OperationFailedToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationFailedToastContent, { target, props: { toastId: 'toast-1', snapshot } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
