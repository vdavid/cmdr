/**
 * Component tests for `S3CostLine.svelte`: when it asks the backend, what it
 * shows, and that the rollback question never carries it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, type ComponentProps } from 'svelte'
import type { CostEstimate, CostEstimateRequest } from '$lib/tauri-commands'

const { estimateOperationCost } = vi.hoisted(() => ({
  estimateOperationCost: vi.fn<(request: CostEstimateRequest) => Promise<CostEstimate[]>>(),
}))
// `ModalDialog` (the rollback guard) reports its open state through the same module.
vi.mock('$lib/tauri-commands', () => ({
  estimateOperationCost,
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import { _setLocaleForTests } from '$lib/intl/locale'
import S3CostLine from './S3CostLine.svelte'
import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'

const request: CostEstimateRequest = {
  operation: 'copy',
  previewId: 'preview-1',
  sourceVolumeId: 'local',
  destinationVolumeId: 's3-bucket',
}

let target: HTMLElement
async function render(props: ComponentProps<typeof S3CostLine>) {
  target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(S3CostLine, { target, props })
  flushSync()
  // Let the IPC promise settle and the answer render.
  await tick()
  await Promise.resolve()
  flushSync()
  return component
}

beforeEach(() => {
  document.body.innerHTML = ''
  estimateOperationCost.mockReset()
  _setLocaleForTests('en-US')
})
afterEach(() => {
  _setLocaleForTests(null)
})

describe('S3CostLine', () => {
  it('doesn’t ask or render anything while the request is null', async () => {
    await render({ request: null })
    expect(estimateOperationCost).not.toHaveBeenCalled()
    expect(target.querySelector('.s3-cost')).toBeNull()
  })

  it('renders nothing when every estimate rounds to zero', async () => {
    estimateOperationCost.mockResolvedValue([{ amount: 0.004, currency: 'USD', providerLabel: 'AWS' }])
    await render({ request })
    expect(estimateOperationCost).toHaveBeenCalledWith(request)
    expect(target.querySelector('.s3-cost')).toBeNull()
  })

  it('renders one line per shown estimate, then the info tip', async () => {
    estimateOperationCost.mockResolvedValue([
      { amount: 0.02, currency: 'USD', providerLabel: 'AWS' },
      { amount: 1.5, currency: 'EUR', providerLabel: 'Hetzner' },
    ])
    await render({ request })
    const lines = [...target.querySelectorAll('.s3-cost-amount')].map((el) => el.textContent)
    expect(lines).toEqual(['About $0.02 at AWS list prices', 'About €1.50 at Hetzner list prices'])
    expect(target.querySelectorAll('button[aria-label="About this estimate"]')).toHaveLength(1)
  })

  it('drops a stale answer when the request changed while it was in flight', async () => {
    let answerFirst: (estimates: CostEstimate[]) => void = () => {}
    estimateOperationCost.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          answerFirst = resolve
        }),
    )
    estimateOperationCost.mockResolvedValueOnce([])
    const props = $state<ComponentProps<typeof S3CostLine>>({ request })
    target = document.createElement('div')
    document.body.appendChild(target)
    mount(S3CostLine, { target, props })
    flushSync()
    props.request = { ...request, previewId: 'preview-2' }
    flushSync()
    await tick()
    answerFirst([{ amount: 3, currency: 'USD', providerLabel: 'AWS' }])
    await tick()
    await Promise.resolve()
    flushSync()
    expect(estimateOperationCost).toHaveBeenCalledTimes(2)
    expect(target.querySelector('.s3-cost')).toBeNull()
  })

  it('renders nothing when the backend refuses', async () => {
    estimateOperationCost.mockRejectedValue(new Error('no preview'))
    await render({ request })
    expect(target.querySelector('.s3-cost')).toBeNull()
  })
})

describe('RollbackConfirmDialog', () => {
  it('carries no cost line: a rollback just runs', async () => {
    estimateOperationCost.mockResolvedValue([{ amount: 5, currency: 'USD', providerLabel: 'AWS' }])
    target = document.createElement('div')
    document.body.appendChild(target)
    mount(RollbackConfirmDialog, {
      target,
      props: { variant: 'stopAndDelete', onConfirm: () => {}, onCancel: () => {} },
    })
    flushSync()
    await tick()
    flushSync()
    expect(estimateOperationCost).not.toHaveBeenCalled()
    expect(document.body.querySelector('.s3-cost')).toBeNull()
    expect(document.body.textContent).not.toContain('list prices')
  })
})
