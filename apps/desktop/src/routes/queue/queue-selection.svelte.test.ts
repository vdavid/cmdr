/**
 * The queue window's "Cancel selected" bookkeeping, driven through the real page.
 *
 * Selection is the one piece of queue state the frontend owns, and it has to
 * stay honest against a row list that outlives its rows: a retained failure
 * STAYS in the list with no checkbox, so a tick the user made while the
 * operation was still running has no way back out by hand. The page prunes it,
 * and these tests mount the page to prove the toolbar follows.
 *
 * The store is the real one; only its two Tauri streams are mocked, so the rows
 * arrive exactly as `operations-changed` delivers them.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'

const cancelOperations = vi.fn((_ids: string[]) => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelOperations: (ids: string[]) => cancelOperations(ids),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  dismissAllFailedOperations: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: vi.fn(() => Promise.resolve()),
  pauseAll: vi.fn(() => Promise.resolve()),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeAll: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  // The store's own imports: it subscribes on `init()` and seeds from the list.
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

// The window chrome the page sets up on mount: none of it is under test, and all
// of it would reach for a Tauri window that doesn't exist here.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setFocus: vi.fn(), close: vi.fn() }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('$lib/settings/window-settings', () => ({
  initWindowSettings: vi.fn(() => Promise.resolve()),
  initWindowLanguageSync: vi.fn(() => () => {}),
}))
vi.mock('$lib/accent-color', () => ({
  initAccentColor: vi.fn(() => Promise.resolve()),
  cleanupAccentColor: vi.fn(),
}))
vi.mock('$lib/reduce-transparency', () => ({
  initReduceTransparency: vi.fn(() => Promise.resolve()),
  cleanupReduceTransparency: vi.fn(),
}))
vi.mock('$lib/text-size.svelte', () => ({
  initTextSize: vi.fn(() => Promise.resolve()),
  cleanupTextSize: vi.fn(),
}))
vi.mock('$lib/window-positioning', () => ({ trackOwnRect: vi.fn(() => Promise.resolve(() => {})) }))
// `<Size>` inside the rows reads the reactive settings layer, which needs a live
// settings store the unit environment doesn't have.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ getFileSizeFormat: () => 'decimal' }))

/** The page builds its own store, so the module is wrapped to hand the test the
 *  same instance: snapshots then go in through the reducer the live
 *  `operations-changed` listener calls. */
let store: ReturnType<
  typeof import('$lib/file-operations/queue/operations-store.svelte').createOperationsStore
> | null = null
vi.mock('$lib/file-operations/queue/operations-store.svelte', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/file-operations/queue/operations-store.svelte')>()
  return {
    ...actual,
    createOperationsStore: () => {
      store = actual.createOperationsStore()
      return store
    },
  }
})

import QueuePage from './+page.svelte'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Naspolya/Backup',
    supportsRollback: true,
    error: null,
    ...over,
  }
}

/** The same operation once it stopped: what the backend retains, in place. */
function failed(operationId: string): OperationSnapshot {
  return snapshot({
    operationId,
    status: 'failed',
    supportsRollback: false,
    error: { type: 'io_error', path: '/Users/me/Documents/report.pdf', message: 'disk went away' },
  })
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

/** Mounts the page and waits out its async `onMount` so the list renders. */
async function renderPage(initial: OperationSnapshot[]): Promise<void> {
  target = document.createElement('div')
  document.body.appendChild(target)
  instance = mount(QueuePage, { target })
  // The list only renders once the page's async `onMount` chain has finished;
  // until then not even the empty state is there.
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('.empty-state')).not.toBeNull()
  })
  applySnapshot(initial)
}

/** Delivers a snapshot exactly as the `operations-changed` listener would. */
function applySnapshot(operations: OperationSnapshot[]): void {
  store?._testApplySnapshot(operations)
  flushSync()
}

function rowFor(operationId: string): HTMLElement | null {
  return target.querySelector(`[data-operation-id="${operationId}"]`)
}

function checkboxFor(operationId: string): HTMLInputElement | null {
  return rowFor(operationId)?.querySelector<HTMLInputElement>('input[type="checkbox"]') ?? null
}

function selectedCountText(): string | null {
  return target.querySelector('.selected-count')?.textContent ?? null
}

/** The toolbar's "Cancel selected", found by its label (it carries none of the
 *  row-level aria labels). */
function cancelSelectedButton(): HTMLButtonElement | null {
  return [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Cancel selected')) ?? null
}

beforeEach(() => {
  document.body.innerHTML = ''
  store = null
  instance = undefined
  cancelOperations.mockClear()
})

afterEach(() => {
  if (instance) void unmount(instance)
})

describe('queue window selection', () => {
  it('counts a checked row and enables "Cancel selected"', async () => {
    await renderPage([snapshot({ operationId: 'op-1' })])

    checkboxFor('op-1')?.click()
    flushSync()

    expect(selectedCountText()).toContain('1')
    expect(cancelSelectedButton()?.disabled).toBe(false)
  })

  it('drops the selection when the checked operation FAILS, so the toolbar stops lying', async () => {
    // The row stays (a retained failure is the whole point) but loses its
    // checkbox, so nothing but this prune can clear the tick. Left in place,
    // "1 selected" stays on screen and "Cancel selected" stays enabled while
    // doing nothing: the backend no-ops an id that isn't live any more.
    await renderPage([snapshot({ operationId: 'op-1' })])
    checkboxFor('op-1')?.click()
    flushSync()
    expect(selectedCountText()).toContain('1')

    applySnapshot([failed('op-1')])

    expect(rowFor('op-1')).not.toBeNull()
    expect(checkboxFor('op-1')).toBeNull()
    expect(selectedCountText()).toBeNull()
    expect(cancelSelectedButton()?.disabled).toBe(true)
  })

  it('keeps a live operation selected when a DIFFERENT one fails', async () => {
    await renderPage([snapshot({ operationId: 'op-1' }), snapshot({ operationId: 'op-2' })])
    checkboxFor('op-2')?.click()
    flushSync()

    applySnapshot([failed('op-1'), snapshot({ operationId: 'op-2' })])

    expect(selectedCountText()).toContain('1')
    expect(checkboxFor('op-2')?.checked).toBe(true)
    expect(cancelSelectedButton()?.disabled).toBe(false)
  })

  it('drops the selection when the operation leaves the list entirely', async () => {
    await renderPage([snapshot({ operationId: 'op-1' })])
    checkboxFor('op-1')?.click()
    flushSync()

    applySnapshot([])

    expect(selectedCountText()).toBeNull()
    expect(cancelSelectedButton()?.disabled).toBe(true)
  })

  it('cancels exactly the checked operations', async () => {
    await renderPage([snapshot({ operationId: 'op-1' }), snapshot({ operationId: 'op-2' })])
    checkboxFor('op-1')?.click()
    flushSync()

    cancelSelectedButton()?.click()

    expect(cancelOperations).toHaveBeenCalledWith(['op-1'])
  })
})
