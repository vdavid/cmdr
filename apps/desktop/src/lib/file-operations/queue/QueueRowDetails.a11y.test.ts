import { describe, it, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QueueRowDetails from './QueueRowDetails.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { OperationDetails, OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { getOperationDetails } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  getOperationDetails: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatDateTime: () => '2026-10-09 14:05',
  getFileSizeFormat: () => 'decimal',
}))

const fetchDetails = vi.mocked(getOperationDetails)

function snapshotWith(status: OperationSnapshot['status']): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status,
    source: 'a.raw (300 items)',
    destination: 'backup',
    supportsRollback: true,
    reverses: null,
    error: null,
  }
}

const capped: OperationDetails = {
  operationId: 'op-1',
  sourcePaths: Array.from(
    { length: 200 },
    (_, i) => `/Users/me/photos/2026/a-very-long-folder-name/img-${String(i)}.raw`,
  ),
  sourceCount: 300,
  destinationPath: '/Volumes/Naspolya/backup',
  sourceVolumeName: null,
  destinationVolumeName: null,
  queuedAt: 1_700_000_000,
  startedAt: 1_700_000_005,
}

const copying: WriteProgressEvent = {
  operationId: 'op-1',
  operationType: 'copy',
  phase: 'copying',
  currentFile: 'img-7.raw',
  filesDone: 7,
  filesTotal: 300,
  bytesDone: 10,
  bytesTotal: 50,
}

beforeEach(() => {
  document.body.innerHTML = ''
  fetchDetails.mockReset()
})

async function mountPanel(
  status: OperationSnapshot['status'],
  progress: WriteProgressEvent | null,
): Promise<HTMLElement> {
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(QueueRowDetails, { target: host, props: { snapshot: snapshotWith(status), progress, id: 'details-op-1' } })
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
  return host
}

describe('QueueRowDetails a11y', () => {
  // The busiest panel: a capped, scrolling source list with its "and N more"
  // tail, plus every fact line.
  it('a running operation with a long selection has no a11y violations', async () => {
    fetchDetails.mockResolvedValue(capped)
    await expectNoA11yViolations(await mountPanel('running', copying))
  })

  it('a waiting operation has no a11y violations', async () => {
    fetchDetails.mockResolvedValue({ ...capped, startedAt: null })
    await expectNoA11yViolations(await mountPanel('queued', null))
  })

  it('the unavailable note has no a11y violations', async () => {
    fetchDetails.mockRejectedValue(new Error('not allowed'))
    await expectNoA11yViolations(await mountPanel('running', copying))
  })
})
