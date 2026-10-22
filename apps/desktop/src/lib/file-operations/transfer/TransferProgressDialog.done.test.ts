/**
 * The progress dialog's done state, rendered (#484): with "Keep the progress
 * window open when a file operation finishes" on, a completed copy stays up with
 * a past-tense title, full bars, a summary, Close instead of Cancel, and every
 * way out (Close, Enter, Escape, ×) closing it. Undo waits for `write-settled`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type {
  OperationSnapshot,
  WriteCompleteEvent,
  WriteSettledEvent,
  WriteProgressEvent,
} from '#lib/tauri-commands/index.js'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'
import TransferProgressDialog from './TransferProgressDialog.svelte'

const hooks = vi.hoisted(() => ({
  complete: null as ((e: WriteCompleteEvent) => void) | null,
  settled: null as ((e: WriteSettledEvent) => void) | null,
  progress: null as ((e: WriteProgressEvent) => void) | null,
  keepOpen: true,
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  copyFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn((cb: (e: WriteProgressEvent) => void) => {
    hooks.progress = cb
    return Promise.resolve(() => {})
  }),
  onWriteComplete: vi.fn((cb: (e: WriteCompleteEvent) => void) => {
    hooks.complete = cb
    return Promise.resolve(() => {})
  }),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn((cb: (e: WriteSettledEvent) => void) => {
    hooks.settled = cb
    return Promise.resolve(() => {})
  }),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  rollbackOperation: vi.fn(() => Promise.resolve({ inverseOpId: 'op-2' })),
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: 'op-1',
        operationType: 'copy',
        status: 'running',
        source: '/Users/test',
        destination: '/Users/dest',
        rollbackSupport: 'supported',
        reverses: null,
        error: null,
        canRetry: false,
        conflictPolicy: 'skip',
        startedAt: null,
        items: null,
      },
    ]),
  ),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn((key: string) => (key === 'fileOperations.keepProgressOpenWhenDone' ? hooks.keepOpen : 500)),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

beforeEach(async () => {
  hooks.complete = null
  hooks.settled = null
  hooks.progress = null
  hooks.keepOpen = true
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountFinished() {
  const onComplete = vi.fn()
  const onDoneClosed = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/a.txt'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/dest',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      conflictResolution: 'skip',
      onComplete,
      onDoneClosed,
      onCancelled: vi.fn(),
      onError: vi.fn(),
    },
  })
  await flushPromises()
  if (!hooks.progress || !hooks.complete) throw new Error('subscribers never registered')
  hooks.progress({
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'a.txt',
    filesDone: 3,
    filesTotal: 5,
    bytesDone: 300,
    bytesTotal: 500,
  })
  hooks.complete({
    operationId: 'op-1',
    operationType: 'copy',
    filesProcessed: 5,
    filesSkipped: 1,
    bytesProcessed: 500,
  })
  await flushPromises()
  return { component, target, onComplete, onDoneClosed }
}

function title(target: HTMLElement): string {
  return target.querySelector('.dialog-title-bar h2')?.textContent.trim() ?? ''
}

function buttons(target: HTMLElement): string[] {
  return [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
}

function button(target: HTMLElement, label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent.trim() === label)
  if (!found) throw new Error(`no ${label} button`)
  return found
}

describe('TransferProgressDialog, done state', () => {
  it('stays up, says what it did, and trades Cancel for Close', async () => {
    const { component, target, onComplete, onDoneClosed } = await mountFinished()

    expect(onComplete).toHaveBeenCalledWith(expect.objectContaining({ filesProcessed: 5 }), { keptOpen: true })
    expect(onDoneClosed).not.toHaveBeenCalled()
    expect(title(target)).toBe('Copied 4 files')
    expect(target.textContent).toContain('Skipped 1 file that already existed')
    expect(target.textContent).toContain('If a file already exists: Skip')
    const labels = buttons(target)
    expect(labels).toContain('Close')
    expect(labels).not.toContain('Cancel')
    expect(labels).not.toContain('Pause')
    expect(labels).not.toContain('Rollback')

    void unmount(component)
  })

  it('fills the bars', async () => {
    const { component, target } = await mountFinished()

    const bars = [...target.querySelectorAll('[role="progressbar"]')]
    expect(bars.length).toBeGreaterThan(0)
    for (const bar of bars) expect(bar.getAttribute('aria-valuenow')).toBe(bar.getAttribute('aria-valuemax'))

    void unmount(component)
  })

  it('closes on Close', async () => {
    const { component, target, onDoneClosed } = await mountFinished()

    button(target, 'Close').click()

    expect(onDoneClosed).toHaveBeenCalledTimes(1)
    void unmount(component)
  })

  it('closes on Escape and on Enter', async () => {
    const first = await mountFinished()
    const overlay = first.target.querySelector('.modal-overlay') ?? first.target
    overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(first.onDoneClosed).toHaveBeenCalledTimes(1)
    void unmount(first.component)

    const second = await mountFinished()
    const overlay2 = second.target.querySelector('.modal-overlay') ?? second.target
    overlay2.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(second.onDoneClosed).toHaveBeenCalledTimes(1)
    void unmount(second.component)
  })

  it('offers Undo once the journal can answer, and asks before it acts', async () => {
    const { getOperationLogDetail, rollbackOperation } = await import('#lib/tauri-commands/index.js')
    vi.mocked(getOperationLogDetail).mockResolvedValue({
      operation: { opId: 'op-1', kind: 'copy', rollbackState: 'rollbackable', notRollbackableReason: null },
      items: [],
      totalItems: 0,
    } as unknown as Awaited<ReturnType<typeof getOperationLogDetail>>)
    const { component, target, onDoneClosed } = await mountFinished()

    // Before `write-settled`, the journal row isn't there to read yet.
    expect(button(target, 'Undo').getAttribute('aria-disabled')).toBe('true')
    button(target, 'Undo').click()
    await flushPromises()
    expect(document.querySelector('.rollback-confirm-body')).toBeNull()

    if (!hooks.settled) throw new Error('settled subscriber never registered')
    hooks.settled({ operationId: 'op-1', operationType: 'copy', volumeId: null })
    await flushPromises()
    expect(button(target, 'Undo').getAttribute('aria-disabled')).toBeNull()

    button(target, 'Undo').click()
    await flushPromises()
    expect(rollbackOperation).not.toHaveBeenCalled()
    expect(document.querySelector('.rollback-confirm-body'), 'the rollback question is up').not.toBeNull()

    void unmount(component)
    expect(onDoneClosed).not.toHaveBeenCalled()
  })

  it('without the setting, closes as before', async () => {
    hooks.keepOpen = false
    const { component, onComplete, onDoneClosed } = await mountFinished()
    // The anti-flicker floor before an ordinary close.
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 450)
    })

    expect(onComplete).toHaveBeenCalledWith(expect.objectContaining({ filesProcessed: 5 }), { keptOpen: false })
    expect(onDoneClosed).not.toHaveBeenCalled()

    void unmount(component)
  })
})
