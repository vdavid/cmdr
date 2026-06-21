import { describe, it, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QueueRow from './QueueRow.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { OperationRow } from './operations-store.svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

function row(
  status: OperationSnapshot['status'],
  opType: OperationSnapshot['operationType'] = 'copy',
  progress: WriteProgressEvent | null = null,
): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: opType,
      status,
      source: '/Users/me/Documents/report.pdf',
      destination: opType === 'delete' || opType === 'trash' ? null : '/Volumes/Backup/report.pdf',
    },
    progress,
  }
}

const runningProgress: WriteProgressEvent = {
  operationId: 'op-1',
  operationType: 'copy',
  phase: 'copying',
  currentFile: 'report.pdf',
  filesDone: 1,
  filesTotal: 4,
  bytesDone: 25,
  bytesTotal: 100,
  etaSeconds: 42,
}

beforeEach(() => {
  document.body.innerHTML = ''
})

// QueueRow is an <li>, so it's mounted into a <ul> to keep the list semantics
// valid for axe (a bare <li> is a structure violation).
async function mountRow(r: OperationRow, selected = false): Promise<HTMLElement> {
  const list = document.createElement('ul')
  document.body.appendChild(list)
  mount(QueueRow, {
    target: list,
    props: { row: r, selected, onToggleSelect: () => {}, onPauseResume: () => {}, onCancel: () => {} },
  })
  await tick()
  return list
}

describe('QueueRow a11y', () => {
  it('a running copy row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress))
    await expectNoA11yViolations(list)
  })

  it('a paused row has no a11y violations', async () => {
    const list = await mountRow(row('paused', 'copy', runningProgress))
    await expectNoA11yViolations(list)
  })

  it('a queued move row has no a11y violations', async () => {
    const list = await mountRow(row('queued', 'move'))
    await expectNoA11yViolations(list)
  })

  it('a selected delete row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'delete', runningProgress), true)
    await expectNoA11yViolations(list)
  })
})
