/**
 * Tier 3 a11y tests for the operation-queue window, mounted as the real page: the empty state,
 * and a grid holding a running, a waiting, and a failed operation.
 *
 * The rows have their own tier-3 file (`QueueRow.a11y.test.ts`); this one audits what only the
 * page puts together: the title, the toolbar, the `grid` the rows sit in, and the live region.
 * Only the Tauri edges are stubbed, so the rows arrive through the real store's reducer.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot } from '#lib/ipc/bindings.js'
import { oneCopyingFile } from '#lib/file-operations/transfer-tree/fixtures/one-copying-file.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

// Hoisted with the `vi.mock` factories below, which run before any plain top-level `const`.
const { resolved, noUnlisten } = vi.hoisted(() => ({
  resolved: () => Promise.resolve(),
  noUnlisten: () => Promise.resolve(() => {}),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  cancelOperation: vi.fn(resolved),
  cancelOperations: vi.fn(resolved),
  cancelWriteOperation: vi.fn(resolved),
  dismissAllFailedOperations: vi.fn(resolved),
  dismissFailedOperation: vi.fn(resolved),
  moveQueuedOperation: vi.fn(() => Promise.resolve({ kind: 'not_queued' })),
  notifyDialogOpened: vi.fn(resolved),
  notifyDialogClosed: vi.fn(resolved),
  onQueueSelectAll: vi.fn(noUnlisten),
  onQueueRowContextAction: vi.fn(noUnlisten),
  pauseAll: vi.fn(resolved),
  pauseOperation: vi.fn(resolved),
  requestForegroundOperation: vi.fn(resolved),
  resumeAll: vi.fn(resolved),
  resumeOperation: vi.fn(resolved),
  showQueueRowContextMenu: vi.fn(resolved),
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(noUnlisten),
  onWriteProgress: vi.fn(noUnlisten),
  getTransferTreeView: vi.fn(() => Promise.resolve(oneCopyingFile)),
}))

// The window chrome the page sets up on mount reaches for a Tauri window that doesn't exist here.
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ setFocus: vi.fn(), close: vi.fn() }) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(noUnlisten) }))
vi.mock('#lib/settings/window-settings.js', () => ({
  initWindowSettings: vi.fn(resolved),
  initWindowLanguageSync: vi.fn(() => () => {}),
}))
vi.mock('#lib/accent-color.js', () => ({ initAccentColor: vi.fn(resolved), cleanupAccentColor: vi.fn() }))
vi.mock('#lib/glass-material.js', () => ({ initGlassMaterial: vi.fn(resolved), cleanupGlassMaterial: vi.fn() }))
vi.mock('#lib/display-accessibility.js', () => ({
  initDisplayAccessibility: vi.fn(resolved),
  cleanupDisplayAccessibility: vi.fn(),
}))
vi.mock('#lib/text-size.svelte.js', () => ({ initTextSize: vi.fn(resolved), cleanupTextSize: vi.fn() }))
vi.mock('#lib/window-positioning.js', () => ({ trackOwnRect: vi.fn(noUnlisten) }))
// `<Size>` inside the rows reads the reactive settings layer.
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'decimal' }))

/** The page builds its own store; this hands the test the same instance, so snapshots go in
 *  through the reducer the live `operations-changed` listener calls. */
let store: ReturnType<
  typeof import('#lib/file-operations/queue/operations-store.svelte.js').createOperationsStore
> | null = null
vi.mock('#lib/file-operations/queue/operations-store.svelte.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/file-operations/queue/operations-store.svelte.js')>()
  return {
    ...actual,
    createOperationsStore: () => {
      store = actual.createOperationsStore()
      return store
    },
  }
})

import QueuePage from './+page.svelte'

function op(operationId: string, over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId,
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Backup',
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    items: null,
    ...over,
  }
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

/** Mounts the page, waits out its async `onMount`, then delivers `operations`. */
async function renderPage(operations: OperationSnapshot[]): Promise<void> {
  target = document.createElement('div')
  document.body.appendChild(target)
  instance = mount(QueuePage, { target })
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('.empty-state')).not.toBeNull()
  })
  store?._testApplySnapshot(operations)
  flushSync()
}

beforeEach(() => {
  document.body.innerHTML = ''
  store = null
  instance = undefined
})

afterEach(() => {
  if (instance) void unmount(instance)
})

describe('queue window a11y', () => {
  it('empty state has no a11y violations', async () => {
    await renderPage([])
    await expectNoA11yViolations(target)
  })

  it('a running, a waiting, and a failed operation have no a11y violations', async () => {
    await renderPage([
      op('op-1'),
      op('op-2', { status: 'queued' }),
      op('op-3', {
        status: 'failed',
        rollbackSupport: 'settled',
        error: { type: 'io_error', path: '/Users/me/Documents/report.pdf', message: 'disk went away' },
      }),
    ])
    expect(target.querySelector('[role="grid"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
