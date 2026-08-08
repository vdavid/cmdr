/**
 * Tier 3 a11y tests for `OperationFailuresToastContent.svelte`, the summary a
 * burst of failures collapses into.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { OperationRow } from '$lib/file-operations/queue/operations-store.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: (): OperationRow[] => [],
}))

import OperationFailuresToastContent from './OperationFailuresToastContent.svelte'

describe('OperationFailuresToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationFailuresToastContent, { target, props: { toastId: 'toast-1' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
