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
      reverses: null,
      error: null,
    },
    progress,
  }
}

/** A row for an operation that IS the reversal of a finished one. The backend
 *  registers it as its INVERSE type (undoing a move runs as a move, undoing a
 *  copy as a delete) and hangs the original kind off `reverses`, which is the
 *  only thing telling the row it's an undo rather than the real thing. */
function buildReversalRow(
  reverses: NonNullable<OperationSnapshot['reverses']>,
  opType: OperationSnapshot['operationType'],
  status: OperationSnapshot['status'] = 'running',
): OperationRow {
  const progress: WriteProgressEvent = {
    operationId: 'op-1',
    operationType: opType,
    phase: 'rolling_back',
    currentFile: 'report.pdf',
    filesDone: 12,
    filesTotal: 1240,
    bytesDone: 25,
    bytesTotal: 100,
  }
  const base = buildRow(status, opType, progress, false)
  return { ...base, snapshot: { ...base.snapshot, reverses } }
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

  it('names a reversal by what it does to the files, not by the op type it runs as', () => {
    // Undoing a move is journaled AS a move, so the plain action word would say
    // "Moving" over an operation the person asked to undo.
    render({ row: buildReversalRow('move', 'move') })
    expect(target.querySelector('.op-label')?.textContent.trim()).toBe('Putting files back')
  })

  it('never tells someone their undo is deleting when it is putting files back', () => {
    for (const kind of ['move', 'trash'] as const) {
      render({ row: buildReversalRow(kind, 'move') })
      expect(target.querySelector('.op-label')?.textContent).not.toContain('Deleting')
    }
  })

  it('says plainly that undoing a copy deletes, because it does', () => {
    // The inverse of a copy runs as a delete, and here that IS the honest word:
    // under-warning on this one would be the mirror-image lie.
    render({ row: buildReversalRow('copy', 'delete') })
    expect(target.querySelector('.op-label')?.textContent.trim()).toBe('Deleting what it created')
  })

  it('words a reversal of a rename as putting the old names back', () => {
    render({ row: buildReversalRow('rename', 'rename') })
    expect(target.querySelector('.op-label')?.textContent.trim()).toBe('Putting the old names back')
  })

  it('flies the undo glyph on a reversal instead of the op type’s own', () => {
    // Both rows are `operationType: 'delete'`; only one of them is an undo. If
    // the row ever goes back to reading `operationType` for its glyph, these two
    // become the same drawing and a reversal of a copy wears a trash can.
    render({ row: buildReversalRow('copy', 'delete') })
    const reversalGlyph = target.querySelector('.type-cell svg')?.innerHTML
    render({ row: buildRow('running', 'delete') })
    const deleteGlyph = target.querySelector('.type-cell svg')?.innerHTML

    expect(reversalGlyph).toBeTruthy()
    expect(deleteGlyph).toBeTruthy()
    expect(reversalGlyph).not.toBe(deleteGlyph)
  })

  it('lets a paused reversal say Paused, which the in-flight wording hides', () => {
    // The status cell is free here because the LABEL already says it's a
    // reversal. On the in-flight path it isn't, which is why that one still
    // spends the cell on "Rolling back...".
    render({ row: buildReversalRow('move', 'move', 'paused') })
    expect(target.querySelector('.status-text')?.textContent.trim()).toBe('Paused')
  })

  it('names the folder a removal reversal is clearing, so it cannot read as the folder going away', () => {
    // A removal reversal has no destination, so no arrow renders and the one path
    // it does show sits against "Deleting what it created". Bare, that reads as
    // "delete /Volumes/Backup" — the exact misread this preposition removes.
    const base = buildReversalRow('copy', 'delete')
    render({
      row: {
        ...base,
        snapshot: { ...base.snapshot, source: '/Volumes/Backup', destination: null },
      },
    })
    expect(target.querySelector('.summary-row')?.textContent.replace(/\s+/g, ' ').trim()).toBe(
      'Deleting what it created in Backup',
    )
  })

  it('leaves a restoring reversal its arrow, which already says which way the files go', () => {
    render({ row: buildReversalRow('move', 'move') })
    const summary = target.querySelector('.summary-row')?.textContent.replace(/\s+/g, ' ').trim() ?? ''
    expect(summary).toContain('report.pdf')
    // Only the destination-less shape needs the preposition; adding it here would
    // read as "putting files back in report.pdf".
    expect(summary).not.toContain('in report.pdf')
  })

  it('offers no Rollback on a reversal: there is nothing left to undo', () => {
    render({ row: buildReversalRow('move', 'move') })
    expect(rollbackButton()).toBeNull()
  })

  it("a failed row says it couldn't finish and drops every live control", () => {
    render({
      row: buildFailedRow({
        type: 'read_only_device',
        path: '/Volumes/Stick',
        deviceName: 'Stick',
        side: 'destination',
      }),
    })

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
    render({
      row: buildFailedRow({
        type: 'read_only_device',
        path: '/Volumes/Stick',
        deviceName: 'Stick',
        side: 'destination',
      }),
    })
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

  it('offers Pause during the scan: the walk parks on the same gate the write does', () => {
    render({ row: buildRow('running', 'copy', scanning()) })

    expect(target.querySelector('[aria-label="Pause this operation"]')).not.toBeNull()
    expect(
      target.querySelector('[aria-label="Cancel this operation"]'),
      'Cancel stays: it is the control that works during a scan',
    ).not.toBeNull()
  })

  it('offers Resume on a paused scan, and stops claiming it is counting', () => {
    // The row a "Pause all" leaves behind. Without Resume its only way back is
    // Cancel, which throws the scan away.
    render({ row: buildRow('paused', 'copy', scanning()) })

    expect(target.querySelector('[aria-label="Resume this operation"]')).not.toBeNull()
    expect(
      target.querySelector('[aria-label="Scanning\u2026"]'),
      'the spinner claims the walk is moving, and it is parked',
    ).toBeNull()
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

describe('QueueRow: a move on its last stage', () => {
  /** A move between two filesystems in its source-deletion phase: every file has
   *  landed at the destination and Cmdr is removing the originals. The strategy
   *  reports `supportsRollback: true` (it really can reverse, up to here), so the
   *  phase is the only thing that says the moment has passed. */
  function sweeping(phase: 'copying' | 'deleting'): OperationRow {
    return buildRow(
      'running',
      'move',
      {
        operationId: 'op-1',
        operationType: 'move',
        phase,
        currentFile: 'report.pdf',
        filesDone: 1,
        filesTotal: 4,
        bytesDone: 25,
        bytesTotal: 100,
      },
      true,
    )
  }

  it('offers no Rollback while the originals are going: nothing can be carried back', () => {
    // Pre-fix the row offered a button whose click only stopped the sweep, the
    // same thing Cancel beside it does, while the confirmation promised the files
    // would travel home.
    render({ row: sweeping('deleting') })

    expect(rollbackButton()).toBeNull()
    expect(
      target.querySelector('[aria-label="Cancel this operation"]'),
      'Cancel stays: it still spares the originals the sweep has not reached',
    ).not.toBeNull()
  })

  it('offers Rollback during the copy stage of the same move', () => {
    render({ row: sweeping('copying') })

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
