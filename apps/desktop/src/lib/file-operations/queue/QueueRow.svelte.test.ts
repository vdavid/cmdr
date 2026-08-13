import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, flushSync, type ComponentProps } from 'svelte'
import QueueRow from './QueueRow.svelte'
import { operationTypeIcon } from './operation-icon'
import type { OperationRow } from './operations-store.svelte'
import type { OperationSnapshot, WriteOperationError, WriteProgressEvent } from '$lib/ipc/bindings'
import { requestForegroundOperation } from '$lib/tauri-commands'

// The component reads reactive settings (file-size format) deep in `<Size>`. The
// real path needs the settings store; stub the format getter to keep the unit
// test isolated.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

// Show crosses to the main window over a Tauri event; the row's job is to ask
// for its own operation and nothing else.
vi.mock('$lib/tauri-commands', () => ({
  requestForegroundOperation: vi.fn(() => Promise.resolve()),
}))

function buildRow(
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
      destination: '/Volumes/Backup/report.pdf',
      supportsRollback,
      error: null,
    },
    progress,
  }
}

/** A retained failure as the backend hands it over: settled, unrollbackable,
 *  carrying the typed error that stopped it. */
function buildFailedRow(error: WriteOperationError, opType: OperationSnapshot['operationType'] = 'copy'): OperationRow {
  const failed = buildRow('failed', opType)
  return { ...failed, snapshot: { ...failed.snapshot, error } }
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

/** The row's callbacks default to no-ops, so each test names only the ones it
 *  asserts on. Pause / Cancel / Rollback aren't among them: they go to the
 *  operation's session, and what a click sends is asserted in
 *  `queue-row-session.svelte.test.ts`, which has a window registry. This file
 *  answers which control a given status offers. */
function render(props: Partial<ComponentProps<typeof QueueRow>> & { row: OperationRow }) {
  target = document.createElement('ul')
  document.body.appendChild(target)
  instance = mount(QueueRow, {
    target,
    props: {
      selected: false,
      onToggleSelect: () => {},
      onDismiss: () => {},
      ...props,
    },
  })
  flushSync()
}

/** The Rollback button, found by its label (it carries no aria-label). */
function rollbackButton(): HTMLButtonElement | null {
  return [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Rollback')) ?? null
}

beforeEach(() => {
  document.body.innerHTML = ''
  instance = undefined
})

describe('QueueRow', () => {
  it('shows Pause for a running op and Resume for a paused op', () => {
    render({ row: buildRow('running') })
    expect(target.querySelector('[aria-label="Pause this operation"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Resume this operation"]')).toBeNull()
    if (instance) void unmount(instance)

    render({ row: buildRow('paused') })
    expect(target.querySelector('[aria-label="Resume this operation"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Pause this operation"]')).toBeNull()
  })

  it('a queued op has Cancel but no Pause/Resume', () => {
    render({ row: buildRow('queued') })
    expect(target.querySelector('[aria-label="Cancel this operation"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Pause this operation"]')).toBeNull()
    expect(target.querySelector('[aria-label="Resume this operation"]')).toBeNull()
  })

  it('the select checkbox reflects `selected` and fires onToggleSelect', () => {
    const onToggleSelect = vi.fn()
    render({ row: buildRow('running'), selected: true, onToggleSelect })
    const checkbox = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(checkbox?.checked).toBe(true)
    checkbox?.click()
    expect(onToggleSelect).toHaveBeenCalledOnce()
  })

  it('renders a progress bar from a live write-progress event for a running op', () => {
    const progress: WriteProgressEvent = {
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'copying',
      currentFile: 'report.pdf',
      filesDone: 1,
      filesTotal: 4,
      bytesDone: 25,
      bytesTotal: 100,
    }
    render({ row: buildRow('running', 'copy', progress) })
    const bar = target.querySelector('[role="progressbar"]')
    expect(bar).not.toBeNull()
    expect(bar?.getAttribute('aria-valuenow')).toBe('25')
  })

  it('maps each instant op type to its own glyph, not the trash-2 fallback', () => {
    // Pure mapping: the snake_case wire values must hit their explicit arms.
    expect(operationTypeIcon('rename')).toBe('pencil')
    expect(operationTypeIcon('create_folder')).toBe('folder-plus')
    expect(operationTypeIcon('create_file')).toBe('file-plus')
    // The transfer/delete types keep their existing glyphs.
    expect(operationTypeIcon('copy')).toBe('copy')
    expect(operationTypeIcon('move')).toBe('folder-input')
    expect(operationTypeIcon('delete')).toBe('trash-2')
    expect(operationTypeIcon('trash')).toBe('trash-2')
    // A zip edit gets the dedicated archive glyph, not the move placeholder.
    expect(operationTypeIcon('archive_edit')).toBe('file-archive')
  })

  it('labels instant op rows with their action, not the "Working" fallback', () => {
    const cases: Array<[OperationSnapshot['operationType'], string]> = [
      ['rename', 'Renaming'],
      ['create_folder', 'Creating folder'],
      ['create_file', 'Creating file'],
      ['archive_edit', 'Editing archive'],
    ]
    for (const [opType, expected] of cases) {
      render({ row: buildRow('running', opType) })
      const label = target.querySelector('.op-label')?.textContent.trim()
      expect(label).toBe(expected)
      if (instance) void unmount(instance)
    }
  })

  it('offers Rollback only where the backend says the op can be reversed', () => {
    render({ row: buildRow('running', 'copy', null, true) })
    expect(rollbackButton()).not.toBeNull()

    // A same-volume move / delete / trash reports `supportsRollback: false`;
    // offering it would promise an undo the backend can't perform.
    if (instance) void unmount(instance)
    render({ row: buildRow('running', 'move', null, false) })
    expect(rollbackButton()).toBeNull()
  })

  it('a queued op has no Rollback: nothing has been written to undo', () => {
    render({ row: buildRow('queued', 'copy', null, true) })
    expect(target.querySelector('[aria-label="Cancel this operation"]')).not.toBeNull()
    expect(rollbackButton()).toBeNull()
  })

  it('a rolling-back op says so and drops Rollback, keeping Cancel to stop it', () => {
    // Rollback is an INTENT, not a lifecycle state: the snapshot still says
    // `running`, and only the live progress phase reveals it.
    const progress: WriteProgressEvent = {
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'rolling_back',
      currentFile: 'report.pdf',
      filesDone: 1,
      filesTotal: 4,
      bytesDone: 25,
      bytesTotal: 100,
    }
    render({ row: buildRow('running', 'copy', progress, true) })
    expect(target.querySelector('.status-text')?.textContent.trim()).toBe('Rolling back...')
    expect(rollbackButton()).toBeNull()
    expect(target.querySelector('[aria-label="Cancel this operation"]')).not.toBeNull()
  })

  it("a failed row says it couldn't finish and drops every live control", () => {
    render({ row: buildFailedRow({ type: 'read_only_device', path: '/Volumes/Stick', deviceName: 'Stick' }) })

    expect(target.querySelector('.status-text')?.textContent.trim()).toBe("Couldn't finish")
    expect(target.querySelector('[aria-label="Pause this operation"]')).toBeNull()
    expect(target.querySelector('[aria-label="Resume this operation"]')).toBeNull()
    expect(target.querySelector('[aria-label="Cancel this operation"]')).toBeNull()
    expect(rollbackButton()).toBeNull()
    // Nothing to cancel in bulk, so the row leaves the multi-select out.
    expect(target.querySelector('input[type="checkbox"]')).toBeNull()
  })

  it('a failed row renders the real reason, per error variant and per operation', () => {
    // Two variants and two operation types, so a broken variant-key selection
    // can't hide behind one lucky lookup.
    render({ row: buildFailedRow({ type: 'read_only_device', path: '/Volumes/Stick', deviceName: 'Stick' }) })
    let reason = target.querySelector('.reason-cell')?.textContent ?? ''
    expect(reason).toContain('Stick is read-only')
    expect(reason).toContain('Choose a different destination that supports writing.')

    if (instance) void unmount(instance)
    render({
      row: buildFailedRow({ type: 'permission_denied', path: '/protected', message: 'nope' }, 'delete'),
    })
    reason = target.querySelector('.reason-cell')?.textContent ?? ''
    expect(reason).toContain("You don't have permission to delete files here.")
  })

  it('clicking Dismiss fires onDismiss', () => {
    const onDismiss = vi.fn()
    render({ row: buildFailedRow({ type: 'source_not_found', path: '/gone.txt' }), onDismiss })
    target.querySelector<HTMLButtonElement>('[aria-label="Dismiss this operation"]')?.click()
    expect(onDismiss).toHaveBeenCalledOnce()
  })

  it('exposes the lifecycle status as a data attribute for E2E', () => {
    render({ row: buildRow('queued') })
    expect(target.querySelector('[data-status="queued"]')).not.toBeNull()
    expect(target.querySelector('[data-operation-id="op-1"]')).not.toBeNull()
  })
})

describe('QueueRow while the operation is still counting', () => {
  /** A scan-phase tick: the preview's counts, forwarded under the operation's
   *  id. Both totals stay 0 — finding them is what the scan is FOR. */
  function scanning(over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
    return {
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'scanning',
      currentFile: 'report.pdf',
      currentDir: '/Users/me/Documents',
      filesDone: 1_284,
      filesTotal: 0,
      bytesDone: 4_096,
      bytesTotal: 0,
      dirsDone: 37,
      ...over,
    }
  }

  it('renders live counts instead of a blank row, and no dual bar', () => {
    // The naive implementation leaves `showReadout` gated on totals that are
    // still 0, so the row draws nothing at all for the whole walk.
    render({ row: buildRow('running', 'copy', scanning()) })

    const text = target.textContent
    expect(text, 'the counts are real even though the totals are not').toContain('1,284')
    expect(text).toContain('37')
    expect(target.querySelectorAll('[role="progressbar"]').length, 'a bar measured against 0 is not progress').toBe(0)
  })

  it('renders those counts for a queued row too', () => {
    // On a busy lane this is the common case, not the edge: "Waiting" over a
    // bare row reads as a hung queue.
    render({ row: buildRow('queued', 'copy', scanning()) })

    expect(target.textContent).toContain('1,284')
  })

  it('offers no Pause: a scan has nothing to park', () => {
    render({ row: buildRow('running', 'copy', scanning()) })

    expect(target.querySelector('[aria-label="Pause this operation"]')).toBeNull()
    expect(
      target.querySelector('[aria-label="Cancel this operation"]'),
      'Cancel stays: it is the control that works during a scan',
    ).not.toBeNull()
  })

  it('offers no Rollback: nothing has been written to reverse', () => {
    render({ row: buildRow('running', 'copy', scanning(), true) })

    expect(rollbackButton()).toBeNull()
  })

  it('offers Rollback again once the write starts', () => {
    render({
      row: buildRow('running', 'copy', { ...scanning(), phase: 'copying', filesTotal: 4, bytesTotal: 100 }, true),
    })

    expect(rollbackButton()).not.toBeNull()
  })
})

describe('QueueRow: Show (back to the main window)', () => {
  const showButton = () => target.querySelector('[aria-label="Show this operation in the main window"]')

  /** A row mid-scan: still counting, no totals yet. */
  function scanningRow(): OperationRow {
    return buildRow('running', 'copy', {
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'scanning',
      currentFile: 'a.txt',
      filesDone: 12,
      filesTotal: 0,
      bytesDone: 400,
      bytesTotal: 0,
    })
  }

  it("offers Show on a running row, and asks for that row's operation", () => {
    render({ row: buildRow('running') })

    const button = showButton()
    expect(button).not.toBeNull()
    ;(button as HTMLButtonElement).click()

    expect(requestForegroundOperation).toHaveBeenCalledWith('op-1')
  })

  it('offers Show on a paused row and on one still counting', () => {
    render({ row: buildRow('paused') })
    expect(showButton()).not.toBeNull()
    if (instance) void unmount(instance)

    render({ row: scanningRow() })
    expect(showButton()).not.toBeNull()
  })

  it('offers none on a queued row: there is no progress to show yet', () => {
    // An operation waiting for a lane has nothing to fill the dialog's bars with,
    // so it waits its turn here, where the whole queue is visible.
    render({ row: buildRow('queued') })

    expect(showButton()).toBeNull()
  })

  it('offers none on a row that has already stopped', () => {
    render({ row: buildFailedRow({ type: 'io_error', path: '/x', message: 'boom' }) })

    expect(showButton()).toBeNull()
  })

  it('offers none for an instant operation: there is no progress to show', () => {
    render({ row: buildRow('running', 'rename') })

    expect(showButton()).toBeNull()
  })
})
