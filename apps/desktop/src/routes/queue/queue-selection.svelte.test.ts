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
import type {
  OperationSnapshot,
  QueueMoveOutcome,
  QueueRowMenuAction,
  QueueRowMenuEntry,
  WriteProgressEvent,
} from '#lib/ipc/bindings.js'
import { oneCopyingFile } from '#lib/file-operations/transfer-tree/fixtures/one-copying-file.js'

const cancelOperations = vi.fn((_ids: string[]) => Promise.resolve())
const cancelOperation = vi.fn((_id: string) => Promise.resolve())
const dismissFailedOperation = vi.fn((_id: string) => Promise.resolve())
const requestForegroundOperation = vi.fn((_id: string) => Promise.resolve())
const pauseOperation = vi.fn((_id: string) => Promise.resolve())
const resumeOperation = vi.fn((_id: string) => Promise.resolve())
const cancelWriteOperation = vi.fn((_id: string, _rollback: boolean) => Promise.resolve())
const moveQueuedOperation = vi.fn((_id: string, _direction: string): Promise<QueueMoveOutcome> =>
  Promise.resolve({ kind: 'not_queued' }),
)
/** The ⌘A menu road's listener, as the page registers it. */
let selectAllFromMenu: (() => void) | null = null
const showQueueRowContextMenu = vi.fn((_entries: QueueRowMenuEntry[], _shortcuts: Record<string, string>) =>
  Promise.resolve(),
)
/** The row menu's pick listener, as the page registers it. */
let pickFromRowMenu: ((action: QueueRowMenuAction) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  cancelOperation: (id: string) => cancelOperation(id),
  cancelOperations: (ids: string[]) => cancelOperations(ids),
  cancelWriteOperation: (id: string, rollback: boolean) => cancelWriteOperation(id, rollback),
  dismissAllFailedOperations: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: (id: string) => dismissFailedOperation(id),
  moveQueuedOperation: (id: string, direction: string) => moveQueuedOperation(id, direction),
  onQueueSelectAll: (callback: () => void) => {
    selectAllFromMenu = callback
    return Promise.resolve(() => {})
  },
  requestForegroundOperation: (id: string) => requestForegroundOperation(id),
  showQueueRowContextMenu: (entries: QueueRowMenuEntry[], shortcuts: Record<string, string>) =>
    showQueueRowContextMenu(entries, shortcuts),
  onQueueRowContextAction: (callback: (action: QueueRowMenuAction) => void) => {
    pickFromRowMenu = callback
    return Promise.resolve(() => {})
  },
  // B raises the row's rollback confirmation, a `ModalDialog`.
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  pauseAll: vi.fn(() => Promise.resolve()),
  pauseOperation: (id: string) => pauseOperation(id),
  resumeAll: vi.fn(() => Promise.resolve()),
  resumeOperation: (id: string) => resumeOperation(id),
  // The store's own imports: it subscribes on `init()` and seeds from the list.
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  // A running copy's row embeds its live tree.
  getTransferTreeView: vi.fn(() => Promise.resolve(oneCopyingFile)),
}))

// The window chrome the page sets up on mount: none of it is under test, and all
// of it would reach for a Tauri window that doesn't exist here.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setFocus: vi.fn(), close: vi.fn() }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('#lib/settings/window-settings.js', () => ({
  initWindowSettings: vi.fn(() => Promise.resolve()),
  initWindowLanguageSync: vi.fn(() => () => {}),
}))
vi.mock('#lib/accent-color.js', () => ({
  initAccentColor: vi.fn(() => Promise.resolve()),
  cleanupAccentColor: vi.fn(),
}))
vi.mock('#lib/glass-material.js', () => ({
  initGlassMaterial: vi.fn(() => Promise.resolve()),
  cleanupGlassMaterial: vi.fn(),
}))
vi.mock('#lib/display-accessibility.js', () => ({
  initDisplayAccessibility: vi.fn(() => Promise.resolve()),
  cleanupDisplayAccessibility: vi.fn(),
}))
vi.mock('#lib/text-size.svelte.js', () => ({
  initTextSize: vi.fn(() => Promise.resolve()),
  cleanupTextSize: vi.fn(),
}))
vi.mock('#lib/window-positioning.js', () => ({ trackOwnRect: vi.fn(() => Promise.resolve(() => {})) }))
// `<Size>` inside the rows reads the reactive settings layer, which needs a live
// settings store the unit environment doesn't have.
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'decimal' }))

/** The page builds its own store, so the module is wrapped to hand the test the
 *  same instance: snapshots then go in through the reducer the live
 *  `operations-changed` listener calls. */
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

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Naspolya/Backup',
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    startedAt: null,
    items: null,
    ...over,
  }
}

/** The same operation once it stopped: what the backend retains, in place. */
function failed(operationId: string): OperationSnapshot {
  return snapshot({
    operationId,
    status: 'failed',
    rollbackSupport: 'settled',
    reverses: null,
    error: { type: 'io_error', path: '/Users/me/Documents/report.pdf', message: 'disk went away' },
    canRetry: false,
    conflictPolicy: null,
    startedAt: null,
    items: null,
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
  cancelOperation.mockClear()
  dismissFailedOperation.mockClear()
  requestForegroundOperation.mockClear()
  pauseOperation.mockClear()
  resumeOperation.mockClear()
  cancelWriteOperation.mockClear()
  moveQueuedOperation.mockClear()
  selectAllFromMenu = null
  showQueueRowContextMenu.mockClear()
  pickFromRowMenu = null
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

// ── Keyboard (DETAILS § Keyboard) ────────────────────────────────────────────

/** Presses a key where focus is (the bare page when nothing is focused), as the
 *  window's keydown listener receives it. */
function press(init: KeyboardEventInit): void {
  const where = document.activeElement ?? document.body
  where.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
  flushSync()
}

const DOWN = { key: 'ArrowDown', code: 'ArrowDown' }
const UP = { key: 'ArrowUp', code: 'ArrowUp' }

function cursorRowId(): string | null {
  return target.querySelector('.queue-row.is-under-cursor')?.getAttribute('data-operation-id') ?? null
}

function focusedRowId(): string | null {
  return document.activeElement?.closest('.queue-row')?.getAttribute('data-operation-id') ?? null
}

const three = [
  snapshot({ operationId: 'op-1' }),
  snapshot({ operationId: 'op-2', status: 'queued' }),
  snapshot({ operationId: 'op-3', status: 'queued' }),
]

describe('queue window keyboard', () => {
  it('is one Tab stop: a single tabbable row, and every row button out of the Tab order', async () => {
    await renderPage(three)

    expect(target.querySelectorAll('[role="row"][tabindex="0"]')).toHaveLength(1)
    const buttons = [...target.querySelectorAll('[role="row"] button')]
    expect(buttons.length).toBeGreaterThan(0)
    expect(buttons.every((b) => b.getAttribute('tabindex') === '-1')).toBe(true)
  })

  it('↓ from the bare page lands the cursor on the first row and focuses it', async () => {
    await renderPage(three)

    press(DOWN)

    expect(cursorRowId()).toBe('op-1')
    expect(focusedRowId()).toBe('op-1')
  })

  it('↑ / ↓ step, Home / End jump, and the Tab stop follows the cursor', async () => {
    await renderPage(three)
    press(DOWN)
    press(DOWN)
    expect(cursorRowId()).toBe('op-2')
    press({ key: 'End', code: 'End' })
    expect(cursorRowId()).toBe('op-3')
    press(UP)
    expect(cursorRowId()).toBe('op-2')
    press({ key: 'Home', code: 'Home' })
    expect(focusedRowId()).toBe('op-1')
    expect(rowFor('op-1')?.getAttribute('tabindex')).toBe('0')
    expect(rowFor('op-2')?.getAttribute('tabindex')).toBe('-1')
  })

  it('moves the cursor to a row clicked anywhere', async () => {
    await renderPage(three)

    rowFor('op-3')?.querySelector<HTMLElement>('.summary-row')?.click()
    flushSync()

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('Space checks the cursor row for "Cancel selected", and the row says it is selected', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: ' ', code: 'Space' })

    expect(rowFor('op-1')?.getAttribute('aria-selected')).toBe('true')
    expect(checkboxFor('op-1')?.checked).toBe(true)
    expect(selectedCountText()).toContain('1')
  })

  it('⇧↓ extends the selection as the cursor moves', async () => {
    await renderPage(three)
    press(DOWN)

    press({ ...DOWN, shiftKey: true })
    press({ ...DOWN, shiftKey: true })

    expect(selectedCountText()).toContain('3')
    expect(cursorRowId()).toBe('op-3')
  })

  it('⌘A (the menu road) checks every row that can still be canceled', async () => {
    await renderPage([...three, failed('op-4')])

    selectAllFromMenu?.()
    flushSync()

    expect(selectedCountText()).toContain('3')
    expect(rowFor('op-4')?.hasAttribute('aria-selected')).toBe(false)
  })

  it('⌫ cancels the cursor row with no confirmation, like its Cancel button', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: 'Backspace', code: 'Backspace' })

    await vi.waitFor(() => {
      expect(cancelOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('⌫ dismisses a row that couldn’t finish', async () => {
    await renderPage([failed('op-1')])
    press(DOWN)

    press({ key: 'Delete', code: 'Delete' })

    expect(dismissFailedOperation).toHaveBeenCalledWith('op-1')
  })

  it('Enter shows a running row in the main window, and does nothing on a waiting one', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'Enter', code: 'Enter' })
    expect(requestForegroundOperation).toHaveBeenCalledWith('op-1')

    requestForegroundOperation.mockClear()
    press(DOWN)
    press({ key: 'Enter', code: 'Enter' })
    expect(requestForegroundOperation).not.toHaveBeenCalled()
  })

  // The row's buttons take no Tab stop, so each has a letter that acts on the cursor
  // row, read like the decision prompts' (`decision-keys.ts`). DETAILS § Keyboard.
  it('S shows the cursor row in the main window, like Enter', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 's', code: 'KeyS' })
    expect(requestForegroundOperation).toHaveBeenCalledWith('op-1')
  })

  it('P pauses the cursor row', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'p', code: 'KeyP' })
    await vi.waitFor(() => {
      expect(pauseOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('C cancels the cursor row with no confirmation, like its Cancel button and ⌫', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'c', code: 'KeyC' })
    await vi.waitFor(() => {
      expect(cancelOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('B asks before rolling the cursor row back, like its Rollback button', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'b', code: 'KeyB' })
    expect(target.querySelector('[data-dialog-id="rollback-confirmation"]')).not.toBeNull()
    expect(cancelWriteOperation).not.toHaveBeenCalled()
    expect(cancelOperation).not.toHaveBeenCalled()
  })

  it('reaches the letters from a focused row button, and through the physical key on a non-Latin layout', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'ArrowRight', code: 'ArrowRight' })
    expect(document.activeElement?.tagName).toBe('BUTTON')
    // A Russian layout's C key types "с".
    press({ key: 'с', code: 'KeyC' })
    await vi.waitFor(() => {
      expect(cancelOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('ignores a held letter, and a letter with a modifier', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'c', code: 'KeyC', repeat: true })
    press({ key: 'c', code: 'KeyC', metaKey: true })
    press({ key: 'C', code: 'KeyC', shiftKey: true })
    press({ key: 'p', code: 'KeyP', repeat: true })
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(pauseOperation).not.toHaveBeenCalled()
  })

  it('⌥↑ asks the backend to move the waiting row, and announces where it landed', async () => {
    moveQueuedOperation.mockResolvedValueOnce({ kind: 'moved', position: 1, waiting: 2 })
    await renderPage(three)
    press({ key: 'End', code: 'End' })

    press({ ...UP, altKey: true })

    expect(moveQueuedOperation).toHaveBeenCalledWith('op-3', 'earlier')
    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('[role="status"]')?.textContent).toBe('Moved up. Now 1 of 2 in line.')
    })
  })

  it('keeps the cursor and focus on a reordered row', async () => {
    await renderPage(three)
    press({ key: 'End', code: 'End' })

    applySnapshot([three[0], three[2], three[1]])

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('hands the cursor and focus to the row that slid into place when the cursor row finishes', async () => {
    await renderPage(three)
    press(DOWN)
    press(DOWN)

    applySnapshot([three[0], three[2]])

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('→ walks into the row’s buttons and ← walks back out to the row', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: 'ArrowRight', code: 'ArrowRight' })
    expect(document.activeElement?.tagName).toBe('BUTTON')
    expect(focusedRowId()).toBe('op-1')

    press({ key: 'ArrowLeft', code: 'ArrowLeft' })
    expect(document.activeElement).toBe(rowFor('op-1'))
  })
})

// ── The inline tree (`QueueRow`, DETAILS § "The inline tree") ─────────────────

/** A tick past the scan, so a running copy's row shows its live tree. */
function copying(operationId: string): WriteProgressEvent {
  return {
    operationId,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'holiday.mov',
    filesDone: 1,
    filesTotal: 4,
    bytesDone: 1_000,
    bytesTotal: 16_000,
  }
}

async function renderWithTrees(running: string[], waiting: string[] = []): Promise<void> {
  await renderPage([
    ...running.map((operationId) => snapshot({ operationId })),
    ...waiting.map((operationId) => snapshot({ operationId, status: 'queued' })),
  ])
  for (const operationId of running) store?._testApplyProgress(copying(operationId))
  flushSync()
  await vi.waitFor(() => {
    for (const operationId of running) expect(rowFor(operationId)?.querySelector('[role="treeitem"]')).not.toBeNull()
  })
}

describe('queue window: a row’s right-click menu', () => {
  function rightClick(operationId: string): void {
    rowFor(operationId)?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    flushSync()
  }

  /** The menu the page asked for, as `action` / `action!` (greyed) / `-`. */
  function openedMenu(): string[] {
    const entries = showQueueRowContextMenu.mock.calls.at(-1)?.[0] ?? []
    return entries.map((e) => {
      if (e.kind === 'separator') return '-'
      return `${e.action}${e.kind === 'check' && e.checked ? '✓' : ''}${e.enabled ? '' : '!'}`
    })
  }

  it('puts the cursor on the row and opens its menu, labelled with the window’s keys', async () => {
    await renderPage(three)
    rightClick('op-2')
    expect(cursorRowId()).toBe('op-2')
    expect(openedMenu()).toEqual(['moveEarlier!', 'moveLater', '-', 'select', '-', 'cancel'])
    expect(showQueueRowContextMenu.mock.calls[0][1]).toMatchObject({
      'queue.cancel': 'C',
      'queue.toggleSelected': 'Space',
    })
  })

  it('offers a running row its own items', async () => {
    await renderPage(three)
    rightClick('op-1')
    expect(openedMenu()).toEqual([
      'show',
      'pause',
      '-',
      'moveEarlier!',
      'moveLater!',
      '-',
      'select',
      '-',
      'cancel',
      'rollback',
    ])
  })

  it('runs a pick through the same action as its key, on the row it opened over', async () => {
    await renderPage(three)
    rightClick('op-3')
    pickFromRowMenu?.('moveEarlier')
    expect(moveQueuedOperation).toHaveBeenCalledWith('op-3', 'earlier')

    rightClick('op-1')
    pickFromRowMenu?.('cancel')
    await vi.waitFor(() => {
      expect(cancelOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('checks the row for "Cancel selected" on Select', async () => {
    await renderPage(three)
    rightClick('op-2')
    pickFromRowMenu?.('select')
    flushSync()
    expect(selectedCountText()).toBe('1 selected')
    rightClick('op-2')
    expect(openedMenu()).toContain('select✓')
  })

  it('offers Dismiss on a failure, and dismisses it like its button and ⌫', async () => {
    await renderPage([three[0], failed('op-9')])
    rightClick('op-9')
    expect(openedMenu()).toEqual(['moveEarlier!', 'moveLater!', '-', 'select!', '-', 'dismiss'])
    pickFromRowMenu?.('dismiss')
    await vi.waitFor(() => {
      expect(dismissFailedOperation).toHaveBeenCalledWith('op-9')
    })
  })

  it('drops a pick the row no longer offers by the time it lands', async () => {
    await renderPage(three)
    rightClick('op-1')
    applySnapshot([snapshot({ operationId: 'op-1', status: 'paused' }), three[1], three[2]])
    expect(pickFromRowMenu).not.toBeNull()
    pickFromRowMenu?.('pause')
    await Promise.resolve()
    // Toggling would RESUME the row the user meant to pause.
    expect(resumeOperation).not.toHaveBeenCalled()
    expect(pauseOperation).not.toHaveBeenCalled()
  })
})

describe('queue window: a row’s live tree', () => {
  it('keeps the grid’s keys out of the tree: ⌫ there cancels nothing, and ↓ stays in it', async () => {
    await renderWithTrees(['op-1'], ['op-2'])
    press(DOWN)
    rowFor('op-1')?.querySelector<HTMLElement>('[role="treeitem"]')?.focus()

    press({ key: 'Backspace', code: 'Backspace' })
    press({ key: 'c', code: 'KeyC' })
    press({ key: 'p', code: 'KeyP' })
    press(DOWN)
    press({ key: ' ', code: 'Space' })

    expect(cancelOperation).not.toHaveBeenCalled()
    expect(pauseOperation).not.toHaveBeenCalled()
    expect(cursorRowId()).toBe('op-1')
    expect(selectedCountText()).toBeNull()
  })

  it('Escape in the tree hands focus back to its row, where the grid keys work again', async () => {
    await renderWithTrees(['op-1'], ['op-2'])
    press(DOWN)
    rowFor('op-1')?.querySelector<HTMLElement>('[role="treeitem"]')?.focus()

    press({ key: 'Escape', code: 'Escape' })
    expect(document.activeElement).toBe(rowFor('op-1'))

    press(DOWN)
    expect(cursorRowId()).toBe('op-2')
  })

  it('a click in another row’s tree moves the cursor to that row', async () => {
    await renderWithTrees(['op-1', 'op-2'])
    press(DOWN)

    rowFor('op-2')?.querySelector<HTMLElement>('[role="treeitem"]')?.focus()
    flushSync()

    expect(cursorRowId()).toBe('op-2')
    // Only the cursor row's tree is a Tab stop, so the grid stays one stop plus that tree.
    expect(rowFor('op-2')?.querySelector('[role="treeitem"]')?.getAttribute('tabindex')).toBe('0')
    expect(rowFor('op-1')?.querySelectorAll('[data-queue-tree] [tabindex="0"]')).toHaveLength(0)
  })

  it('gives each tree a collapse toggle once two rows show one, and none to a lone tree', async () => {
    await renderWithTrees(['op-1'], ['op-2'])
    expect(target.querySelectorAll('[aria-label="Files in this operation"]')).toHaveLength(0)
    if (instance) void unmount(instance)

    await renderWithTrees(['op-1', 'op-2'])
    expect(target.querySelectorAll('[aria-label="Files in this operation"]')).toHaveLength(2)
  })
})
