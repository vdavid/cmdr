/**
 * Tier-3 a11y for the row's reversal controls, mounted on their own.
 *
 * Their accessible names are the words on them plus the `aria-describedby` that
 * ties each to its history row, so the element the description points at has to
 * exist here too — a dangling reference is exactly what this catches.
 *
 * The behavior (which control shows for which lifecycle status, and what each
 * press sends) lives in `RollbackControls.svelte.test.ts`, mounted through the
 * dialog.
 */

import { describe, it, beforeEach, afterEach, expect, vi } from 'vitest'
import { mount, unmount, tick, flushSync } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve(liveOperations)),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import RollbackControls from './RollbackControls.svelte'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'

let liveOperations: OperationSnapshot[] = []
/** Torn down between cases: a live component keeps an effect that re-acquires
 *  its session whenever a registry appears, and the fan-out allows one session
 *  per operation, so a leftover mount would steal the next case's claim. */
let view: ReturnType<typeof mount> | null = null

function reversal(status: OperationSnapshot['status']): OperationSnapshot {
  return {
    operationId: 'inv-1',
    operationType: 'delete',
    status,
    source: '/Volumes/Backup',
    destination: null,
    supportsRollback: false,
    reverses: 'copy',
    error: null,
  }
}

/** The row these controls belong to, reduced to what their description points
 *  at: axe fails an `aria-describedby` with no target. */
async function mountControls(status: OperationSnapshot['status']): Promise<HTMLElement> {
  liveOperations = [reversal(status)]
  getOperationSessions()?._testEmit({ kind: 'snapshot', operations: liveOperations })

  const row = document.createElement('div')
  row.innerHTML = '<button type="button" id="op-head-op-copy">Copied 3 items</button>'
  document.body.appendChild(row)
  view = mount(RollbackControls, {
    target: row,
    props: { inverseOpId: 'inv-1', describedBy: 'op-head-op-copy' },
  })
  await tick()
  flushSync()
  return row
}

beforeEach(async () => {
  document.body.innerHTML = ''
  liveOperations = []
  await initOperationSessions()
})

afterEach(() => {
  if (view) void unmount(view)
  view = null
  destroyOperationSessions()
})

describe('RollbackControls a11y', () => {
  it('a running reversal has no a11y violations', async () => {
    const row = await mountControls('running')
    expect(row.textContent).toContain('Pause')
    await expectNoA11yViolations(row)
  })

  it('a paused reversal has no a11y violations', async () => {
    const row = await mountControls('paused')
    expect(row.textContent).toContain('Resume')
    await expectNoA11yViolations(row)
  })

  it('a reversal still waiting its turn has no a11y violations', async () => {
    const row = await mountControls('queued')
    expect(row.textContent).toContain('Cancel')
    await expectNoA11yViolations(row)
  })
})
