/**
 * Rollback-affordance matrix for `TransferProgressDialog.svelte`.
 *
 * Same-volume volume moves (one smb2 share / one MTP device, `volume.rename`
 * based) have NO backend rollback — the engine stops without reversing. So the
 * dialog DISABLES Rollback (with an explanatory tooltip) on that path, while a
 * plain Cancel stays reachable. Every other copy/move keeps a live Rollback.
 *
 * Two Rollback affordances exist:
 *   - the conflict-section footer (visible while a `write-conflict` is showing),
 *   - the main footer (visible during the normal progress phase).
 * Both must apply the disable+tooltip consistently. These tests drive both.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { WriteConflictEvent } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'
import TransferProgressDialog from './TransferProgressDialog.svelte'

const ROLLBACK_TOOLTIP = 'Rollback is not available for same-volume moves'

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
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'smb-share-1', name: 'NAS', path: '/Volumes/NAS', category: 'network', isEjectable: false },
  ],
}))

interface MountOptions {
  operationType: 'copy' | 'move'
  sourceVolumeId: string
  destVolumeId: string
}

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

/** Mounted targets, removed individually in `beforeEach` so the tooltip
 *  module's shared `<body>` container (created lazily on first hover) survives
 *  between tests instead of being orphaned by a blanket `innerHTML = ''`. */
const mountedTargets: HTMLElement[] = []

async function mountDialog(opts: MountOptions): Promise<HTMLDivElement> {
  conflictCb = null
  const target = document.createElement('div')
  document.body.appendChild(target)
  mountedTargets.push(target)
  mount(TransferProgressDialog, {
    target,
    props: {
      operationType: opts.operationType,
      sourcePaths: ['/Users/test/things'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      direction: 'right',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: opts.sourceVolumeId,
      destVolumeId: opts.destVolumeId,
      conflictResolution: 'stop',
      onComplete: () => {},
      onCancelled: () => {},
      onError: () => {},
    },
  })
  await flushMicrotasks()
  return target
}

/** Fires a synthetic file conflict so the conflict-section footer renders. */
async function fireConflict(): Promise<void> {
  const cb = conflictCb
  if (cb === null) throw new Error('conflict subscriber never registered')
  cb({
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
  await tick()
}

function buttonByText(target: HTMLElement, text: string): HTMLButtonElement | null {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('button'))
  return buttons.find((b) => b.textContent.trim() === text) ?? null
}

/**
 * Hovers a tooltip host and returns the rendered tooltip text. The tooltip
 * action shows after a 400 ms delay, so we drive a fake timer past it. Reads
 * the shared `.cmdr-tooltip` element the action appends to <body>.
 */
function readTooltipOnHover(host: Element): string {
  vi.useFakeTimers()
  try {
    host.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
    vi.advanceTimersByTime(500)
    const tip = document.querySelector('.cmdr-tooltip')
    const text = tip?.textContent ?? ''
    return text.trim()
  } finally {
    host.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }))
    vi.useRealTimers()
  }
}

beforeEach(() => {
  conflictCb = null
  // Remove only mounted dialog targets, not the tooltip module's shared
  // <body> container — wiping it orphans the module's cached reference and the
  // next hover appends the tooltip to a detached node (queryable as null).
  while (mountedTargets.length > 0) {
    mountedTargets.pop()?.remove()
  }
})

/* ------------------------------------------------------------------------- */
/* Main footer (progress phase)                                              */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — main footer', () => {
  it('disables Rollback for a same-volume volume move and shows the tooltip text', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback button present').toBeTruthy()
    expect(rollback?.disabled).toBe(true)
    // The disabled button is wrapped in a tooltip-host span (a disabled button
    // swallows its own pointer events, so the wrap is what fires the tooltip).
    const wrap = rollback?.closest('span')
    expect(wrap, 'tooltip host wrap present').toBeTruthy()
    if (wrap) expect(readTooltipOnHover(wrap)).toBe(ROLLBACK_TOOLTIP)
  })

  it('keeps Cancel reachable for a same-volume volume move', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'Cancel button present').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('keeps Rollback ENABLED for a cross-volume move', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.disabled).toBe(false)
  })

  it('keeps Rollback ENABLED for a local→local same-FS move (default volume has real rollback)', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.disabled).toBe(false)
  })

  it('keeps Rollback ENABLED for a same-volume COPY (only moves are affected)', async () => {
    const target = await mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.disabled).toBe(false)
  })

  it('has no a11y violations with the disabled Rollback', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await expectNoA11yViolations(target)
  })
})

/* ------------------------------------------------------------------------- */
/* Conflict-section footer                                                   */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — conflict-section footer', () => {
  it('disables Rollback and shows a reachable Cancel for a same-volume volume move', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback present in conflict footer').toBeTruthy()
    expect(rollback?.disabled).toBe(true)
    const wrap = rollback?.closest('.disabled-button-wrap')
    expect(wrap, 'tooltip host wrap present').toBeTruthy()
    if (wrap) expect(readTooltipOnHover(wrap)).toBe(ROLLBACK_TOOLTIP)
    // Plain Cancel must be available alongside it so the user can still back out.
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'Cancel present in conflict footer').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('keeps a live Rollback (no Cancel) for a cross-volume move conflict', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    await fireConflict()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.disabled).toBe(false)
    expect(buttonByText(target, 'Cancel')).toBeNull()
  })

  it('keeps a live Rollback for a same-volume COPY conflict', async () => {
    const target = await mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.disabled).toBe(false)
  })

  it('has no a11y violations with the disabled Rollback in the conflict footer', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    await expectNoA11yViolations(target)
  })
})
