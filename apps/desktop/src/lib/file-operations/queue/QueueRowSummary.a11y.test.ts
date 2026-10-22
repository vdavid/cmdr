/** Tier-3 a11y: a queue row's summary line, in each of its shapes. */

import { describe, it, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import QueueRowSummary from './QueueRowSummary.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { OperationItems, OperationSnapshot } from '#lib/ipc/bindings.js'
import type { RollbackConfirmVariant } from '../reversal-wording'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Dropbox/report.pdf',
    destination: '/Volumes/Backup/report.pdf',
    rollbackSupport: 'archiveRewrite',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    startedAt: null,
    items: null,
    ...over,
  }
}

const items: OperationItems = {
  count: 3,
  kinds: { files: 1, folders: 2 },
  names: ['a', 'b', 'c'],
  from: '~/Dropbox',
  to: 'cmdr-s3-test/target/folder',
}

beforeEach(() => {
  document.body.innerHTML = ''
})

// The summary is a `gridcell`, so it's mounted in a `row` inside a `grid` the way
// the queue page does, to keep the structure valid for axe.
async function mountSummary(
  snap: OperationSnapshot,
  reversalVariant: RollbackConfirmVariant | null = null,
): Promise<HTMLElement> {
  const grid = document.createElement('div')
  grid.setAttribute('role', 'grid')
  grid.setAttribute('aria-label', 'Operations')
  const row = document.createElement('div')
  row.setAttribute('role', 'row')
  grid.appendChild(row)
  document.body.appendChild(grid)
  mount(QueueRowSummary, { target: row, props: { id: 'summary-1', label: 'Copying', snapshot: snap, reversalVariant } })
  await tick()
  return grid
}

describe('QueueRowSummary a11y', () => {
  it('a copy with named items has no a11y violations', async () => {
    await expectNoA11yViolations(await mountSummary(snapshot({ items })))
  })

  it('a delete with named items (no destination) has no a11y violations', async () => {
    const snap = snapshot({ operationType: 'delete', destination: null, items: { ...items, to: null } })
    await expectNoA11yViolations(await mountSummary(snap))
  })

  it('a row without items falls back to basenames with no a11y violations', async () => {
    await expectNoA11yViolations(await mountSummary(snapshot()))
  })

  it('a removal reversal has no a11y violations', async () => {
    const snap = snapshot({ operationType: 'delete', destination: null, source: '/Users/me/Dropbox' })
    await expectNoA11yViolations(await mountSummary(snap, 'undoByDeleting'))
  })
})
