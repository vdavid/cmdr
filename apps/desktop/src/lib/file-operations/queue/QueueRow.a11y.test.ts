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
  supportsRollback = false,
): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: opType,
      status,
      source: '/Users/me/Documents/report.pdf',
      destination: opType === 'delete' || opType === 'trash' ? null : '/Volumes/Backup/report.pdf',
      supportsRollback,
      error: null,
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
    props: {
      row: r,
      selected,
      onToggleSelect: () => {},
      onPauseResume: () => {},
      onCancel: () => {},
      onRollback: () => {},
      onDismiss: () => {},
    },
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

  // The Rollback button is the row's one danger-styled control, and the only
  // one whose accessible name comes from its label rather than an aria-label.
  it('a rollbackable copy row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress, true))
    await expectNoA11yViolations(list)
  })

  // The failed row is the only one carrying prose, and its explanation is
  // injected as markup by the error pipeline.
  it('a failed row with its reason has no a11y violations', async () => {
    const failed = row('failed', 'copy')
    const list = await mountRow({
      ...failed,
      snapshot: {
        ...failed.snapshot,
        error: { type: 'insufficient_space', required: 1073741824, available: 1024, volumeName: 'Backup' },
      },
    })
    await expectNoA11yViolations(list)
  })
})
