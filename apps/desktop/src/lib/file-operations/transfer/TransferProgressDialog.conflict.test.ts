/**
 * Conflict-dialog layout: when `WriteConflictEvent.sourceIsDirectory` and
 * `destinationIsDirectory` disagree, the dialog renders the type-mismatch
 * variant (two cards + danger-styled "Replace folder/file" buttons) and
 * disables the conditional bulk variants (Overwrite smaller / older).
 * When they agree, it falls back to the standard size-and-date comparison
 * with the secondary-styled "Overwrite" buttons.
 *
 * We drive the dialog by capturing the conflict callback and synthesising
 * events with each shape. Then we assert what's in the DOM and which
 * buttons are disabled. We also run axe-core against each variant so the
 * tier-3 a11y contract holds.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { WriteConflictEvent } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'
import TransferProgressDialogStatic from './TransferProgressDialog.svelte'

let conflictCb: ((e: WriteConflictEvent) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(() => {
      conflictCb = null
    })
  }),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  formatDuration: vi.fn((s: number) => `${String(s)}s`),
  formatFilesPerSecond: vi.fn((r: number) => `${String(r)} files/s`),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
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

async function mountDialogWithConflict(event: WriteConflictEvent): Promise<HTMLDivElement> {
  conflictCb = null
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferProgressDialogStatic, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/things'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      direction: 'right',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      conflictResolution: 'stop',
      onComplete: () => {},
      onCancelled: () => {},
      onError: () => {},
    },
  })
  await flushPromises()
  // Capture into a const so TS narrowing doesn't get widened back to nullable
  // across the async helper's let-binding closure (a real risk: another await
  // could in theory reassign `conflictCb`, even though we control the mock).
  const cb = conflictCb as ((e: WriteConflictEvent) => void) | null
  if (cb === null) throw new Error('conflict subscriber never registered')
  cb(event)
  await tick()
  return target
}

function buttonByText(target: HTMLElement, text: string): HTMLButtonElement | null {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('button'))
  return buttons.find((b) => b.textContent.trim() === text) ?? null
}

describe('TransferProgressDialog conflict layout', () => {
  it('file-vs-file shows the size/date comparison and a plain "Overwrite" button', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/report.pdf',
      destinationPath: '/Users/test/dest/report.pdf',
      sourceSize: 2048,
      destinationSize: 1024,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: -1024,
      sourceIsDirectory: false,
      destinationIsDirectory: false,
    })

    expect(target.textContent).toContain('File already exists')
    expect(target.textContent).toContain('report.pdf')
    // No type-mismatch lede.
    expect(target.querySelector('.conflict-lede')).toBeNull()
    expect(target.querySelector('.mismatch-cards')).toBeNull()
    // Plain "Overwrite" buttons (secondary variant).
    const overwrite = buttonByText(target, 'Overwrite')
    expect(overwrite, '"Overwrite" button exists').toBeTruthy()
    expect(overwrite?.classList.contains('btn-secondary'), 'secondary variant').toBe(true)
    expect(overwrite?.disabled).toBe(false)
    // Conditional bulk variants enabled.
    expect(buttonByText(target, 'Overwrite all smaller')?.disabled).toBe(false)
    expect(buttonByText(target, 'Overwrite all older')?.disabled).toBe(false)
  })

  it('file replacing folder shows "Replace folder" as the destructive primary', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/notes',
      destinationPath: '/Users/test/dest/notes',
      sourceSize: 512,
      destinationSize: 0,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: 0,
      // Source is a file, destination is a folder: clobbering the folder.
      sourceIsDirectory: false,
      destinationIsDirectory: true,
    })

    expect(target.textContent).toContain('Replace this folder with a file?')
    // Mismatch lede appears and explains the destructive nature.
    expect(target.querySelector('.conflict-lede')).not.toBeNull()
    expect(target.textContent).toContain('whole folder')
    // Two cards present.
    expect(target.querySelectorAll('.mismatch-card').length).toBe(2)
    expect(target.querySelector('.mismatch-card--at-risk')).not.toBeNull()
    // Primary button reads "Replace folder" and uses the danger variant.
    const replace = buttonByText(target, 'Replace folder')
    expect(replace, '"Replace folder" button exists').toBeTruthy()
    expect(replace?.classList.contains('btn-danger'), 'danger variant').toBe(true)
    // Conditional bulk variants disabled with tooltip hint.
    expect(buttonByText(target, 'Overwrite all smaller')?.disabled).toBe(true)
    expect(buttonByText(target, 'Overwrite all older')?.disabled).toBe(true)
    // No file-vs-file size/date row.
    expect(target.querySelector('.conflict-comparison')).toBeNull()
  })

  it('folder replacing file shows "Replace file" as the destructive primary', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/archive',
      destinationPath: '/Users/test/dest/archive',
      sourceSize: 0,
      destinationSize: 4096,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: 0,
      // Source is a folder, destination is a file: clobbering the file.
      sourceIsDirectory: true,
      destinationIsDirectory: false,
    })

    expect(target.textContent).toContain('Replace this file with a folder?')
    expect(target.textContent).toContain('would be replaced by a folder')
    expect(buttonByText(target, 'Replace file')?.classList.contains('btn-danger')).toBe(true)
    expect(buttonByText(target, 'Overwrite all smaller')?.disabled).toBe(true)
  })

  it('file-vs-file conflict has no a11y violations', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/report.pdf',
      destinationPath: '/Users/test/dest/report.pdf',
      sourceSize: 2048,
      destinationSize: 1024,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: -1024,
      sourceIsDirectory: false,
      destinationIsDirectory: false,
    })
    await expectNoA11yViolations(target)
  })

  it('file-replacing-folder conflict has no a11y violations', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/notes',
      destinationPath: '/Users/test/dest/notes',
      sourceSize: 512,
      destinationSize: 0,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: 0,
      sourceIsDirectory: false,
      destinationIsDirectory: true,
    })
    await expectNoA11yViolations(target)
  })

  it('folder-replacing-file conflict has no a11y violations', async () => {
    const target = await mountDialogWithConflict({
      operationId: 'op-1',
      sourcePath: '/Users/test/things/archive',
      destinationPath: '/Users/test/dest/archive',
      sourceSize: 0,
      destinationSize: 4096,
      sourceModified: 1_710_000_000,
      destinationModified: 1_700_000_000,
      destinationIsNewer: false,
      sizeDifference: 0,
      sourceIsDirectory: true,
      destinationIsDirectory: false,
    })
    await expectNoA11yViolations(target)
  })
})
