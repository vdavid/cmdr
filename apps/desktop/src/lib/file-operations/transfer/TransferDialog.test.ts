/**
 * Component tests for `TransferDialog.svelte`'s upfront conflict UX.
 *
 * Covers the decoupled conflict UX: the top-level conflict check runs in parallel with
 * the (potentially slow) scan preview, dir-vs-dir collisions classify as merge
 * info rather than conflicts, the file-policy radios show for merges too, the
 * cross-type "Overwrite all" guardrail, and the auto-confirm (MCP) payload
 * wiring. The volume store, Tauri IPC, and settings are stubbed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import * as commands from '$lib/tauri-commands'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'

const startScanPreviewMock = vi.mocked(commands.startScanPreview)
const cancelScanPreviewMock = vi.mocked(commands.cancelScanPreview)

/* ------------------------------------------------------------------------- */
/* Mock harness                                                              */
/* ------------------------------------------------------------------------- */

// Captured scan-preview-complete callback, so a test can decide WHEN the
// (slow) byte scan finishes relative to the conflict check.
let scanCompleteCb: ((e: ScanCompleteEvent) => void) | null = null

interface ScanCompleteEvent {
  previewId: string
  filesTotal: number
  dirsTotal: number
  bytesTotal: number
  dedupBytesTotal: number
}

const scanVolumeForConflictsMock = vi.fn<
  (
    volumeId: string,
    sourceItems: unknown[],
    destPath: string,
    sourceVolumeId?: string,
    sourcePaths?: string[],
  ) => Promise<VolumeConflictInfo[]>
>(() => Promise.resolve([]))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() =>
    Promise.resolve({ data: { totalBytes: 1024 * 1024 * 1024, availableBytes: 1024 * 1024 * 500 } }),
  ),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  // Returns null so the dialog keeps waiting on the (captured) complete event
  // instead of hydrating from cached totals — lets a test hold the byte scan
  // open while the conflict check resolves.
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn((cb: (e: ScanCompleteEvent) => void) => {
    scanCompleteCb = cb
    return Promise.resolve(() => {
      scanCompleteCb = null
    })
  }),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: (...args: Parameters<typeof scanVolumeForConflictsMock>) =>
    scanVolumeForConflictsMock(...args),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
    {
      id: 'mtp-336592896:65538',
      name: 'Virtual Pixel 9 - SD Card',
      path: '/mtp-20-5/65538',
      category: 'mobile_device',
      isEjectable: true,
    },
  ],
}))

/* ------------------------------------------------------------------------- */
/* Helpers                                                                    */
/* ------------------------------------------------------------------------- */

function makeConflict(overrides: Partial<VolumeConflictInfo>): VolumeConflictInfo {
  return {
    sourcePath: 'item',
    destPath: 'item',
    sourceSize: 0,
    destSize: 0,
    sourceModified: null,
    destModified: null,
    sourceIsDirectory: false,
    destIsDirectory: false,
    ...overrides,
  }
}

async function flushMicrotasks(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

interface MountOpts {
  autoConfirm?: boolean
  autoConfirmOnConflict?: string
  onConfirm?: ConfirmFn
  operationType?: 'copy' | 'move'
  sourceVolumeId?: string
  /** The destination volume the dialog starts on (= `selectedVolumeId`). */
  currentVolumeId?: string
  sourceFolderPath?: string
  destinationPath?: string
  direction?: 'left' | 'right'
}

type ConfirmFn = (
  destination: string,
  volumeId: string,
  previewId: string | null,
  conflictResolution: ConflictResolution,
  operationType: string,
  scanInProgress: boolean,
  preKnownConflicts: string[],
) => void

function mountDialog(opts: MountOpts = {}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferDialog, {
    target,
    props: {
      operationType: opts.operationType ?? 'copy',
      sourcePaths: ['/Users/test/photos', '/Users/test/notes.txt'],
      destinationPath: opts.destinationPath ?? '/Users/test/dest',
      direction: opts.direction ?? 'right',
      currentVolumeId: opts.currentVolumeId ?? 'root',
      fileCount: 1,
      folderCount: 1,
      sourceFolderPath: opts.sourceFolderPath ?? '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: opts.sourceVolumeId ?? 'root',
      destVolumeId: opts.currentVolumeId ?? 'root',
      autoConfirm: opts.autoConfirm ?? false,
      autoConfirmOnConflict: opts.autoConfirmOnConflict,
      onConfirm: opts.onConfirm ?? (() => {}),
      onCancel: () => {},
    },
  })
  return target
}

function radioGroup(target: HTMLElement): HTMLElement | null {
  return target.querySelector('.conflict-policy')
}

beforeEach(() => {
  scanCompleteCb = null
  scanVolumeForConflictsMock.mockReset()
  scanVolumeForConflictsMock.mockResolvedValue([])
  startScanPreviewMock.mockClear()
  startScanPreviewMock.mockResolvedValue({ previewId: 'preview-1' })
  cancelScanPreviewMock.mockClear()
  document.body.innerHTML = ''
})

/** Clicks the Copy/Move segmented toggle option by its label. */
function clickToggle(target: HTMLElement, label: 'Copy' | 'Move'): void {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('.toggle-option'))
  const btn = buttons.find((b) => b.textContent.trim() === label)
  if (!btn) throw new Error(`toggle option "${label}" not found`)
  btn.click()
}

/* ------------------------------------------------------------------------- */
/* Decoupling: conflict info appears while the byte scan is still running    */
/* ------------------------------------------------------------------------- */

describe('TransferDialog upfront conflict check decoupling', () => {
  it('renders conflict info while the scan preview is still running', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const target = mountDialog()
    // Deliberately DO NOT fire the scan-complete event — the byte scan is held
    // open. The conflict check still resolves and renders.
    await flushMicrotasks()

    expect(scanCompleteCb, 'scan still in progress (complete not fired)').not.toBeNull()
    expect(target.textContent).toContain('file already exists')
    expect(radioGroup(target), 'radios visible during scan').not.toBeNull()
  })

  it('passes the source volume id and paths so the backend resolves real types', async () => {
    mountDialog()
    await flushMicrotasks()

    expect(scanVolumeForConflictsMock).toHaveBeenCalled()
    const call = scanVolumeForConflictsMock.mock.calls[0]
    // args: (volumeId, sourceItems, destPath, sourceVolumeId, sourcePaths)
    expect(call[3]).toBe('root')
    expect(call[4]).toEqual(['/Users/test/photos', '/Users/test/notes.txt'])
  })
})

/* ------------------------------------------------------------------------- */
/* Source-volume forwarding (field bug 4): the scan stats the right volume    */
/* ------------------------------------------------------------------------- */

describe('TransferDialog source-volume forwarding', () => {
  it('passes the source volume id to startScanPreview (MTP source → local dest)', async () => {
    // A drag of MTP-shaped paths onto a local destination resolves the real MTP
    // source volume id, which the dialog must forward to the byte scan. With the
    // old `sourceVolumeId = destVolumeId` placeholder this was the local dest id,
    // so the scan stat'd MTP paths as local and reported 0 bytes / 0 files.
    mountDialog({ operationType: 'copy', sourceVolumeId: 'mtp-dev:65538', currentVolumeId: 'root' })
    await flushMicrotasks()

    expect(startScanPreviewMock).toHaveBeenCalledTimes(1)
    // args: (sourcePaths, sortColumn, sortOrder, progressIntervalMs, sourceVolumeId)
    expect(startScanPreviewMock.mock.calls[0][4]).toBe('mtp-dev:65538')
  })
})

/* ------------------------------------------------------------------------- */
/* Dir-vs-dir classifies as merge info, not a conflict                       */
/* ------------------------------------------------------------------------- */

describe('TransferDialog folder-merge classification', () => {
  it('shows the merge info line and no conflict summary for a dir-dir collision', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'photos', sourceIsDirectory: true, destIsDirectory: true }),
    ])

    const target = mountDialog()
    await flushMicrotasks()

    expect(target.textContent).toContain('1 folder will merge with an existing folder')
    expect(target.textContent).not.toContain('file already exists')
    // Radios still show: a merge can surface file clashes mid-op.
    expect(radioGroup(target), 'radios show for a merge').not.toBeNull()
  })

  it('pluralizes the merge info line for multiple dir-dir collisions', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'photos', sourceIsDirectory: true, destIsDirectory: true }),
      makeConflict({ sourcePath: 'music', sourceIsDirectory: true, destIsDirectory: true }),
    ])

    const target = mountDialog()
    await flushMicrotasks()

    expect(target.textContent).toContain('2 folders will merge with existing folders')
  })

  it('counts only real conflicts toward the file-exists summary, excluding merges', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'photos', sourceIsDirectory: true, destIsDirectory: true }),
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const target = mountDialog()
    await flushMicrotasks()

    // The summary renders the count and the noun in adjacent elements, so
    // assert on the singular noun + the normalized text rather than the exact
    // whitespace between them.
    const summary = target.querySelector('.conflicts-summary')
    expect(summary?.textContent.replace(/\s+/g, ' ').trim()).toBe('1 file already exists')
    expect(target.textContent).toContain('1 folder will merge with an existing folder')
  })
})

/* ------------------------------------------------------------------------- */
/* Bulk-skip names exclude dir-dir merges                                    */
/* ------------------------------------------------------------------------- */

describe('TransferDialog bulk-skip name forwarding', () => {
  it('forwards only real-conflict names, never dir-dir merge names', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'photos', sourceIsDirectory: true, destIsDirectory: true }),
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const captured: { preKnown: string[] | null } = { preKnown: null }
    const onConfirm: ConfirmFn = (_d, _v, _p, _r, _o, _s, preKnownConflicts) => {
      captured.preKnown = preKnownConflicts
    }

    mountDialog({ autoConfirm: true, autoConfirmOnConflict: 'skip_all', onConfirm })
    await flushMicrotasks()

    expect(captured.preKnown).toEqual(['notes.txt'])
  })
})

/* ------------------------------------------------------------------------- */
/* Cross-type guardrail: "Overwrite all" warning                             */
/* ------------------------------------------------------------------------- */

describe('TransferDialog cross-type overwrite guardrail', () => {
  it('shows the red warning when a type mismatch exists and Overwrite all is selected', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'photos', sourceIsDirectory: false, destIsDirectory: true }),
    ])

    const target = mountDialog()
    await flushMicrotasks()

    // No warning until the user picks Overwrite (default policy is "stop").
    expect(target.querySelector('.conflict-warning')).toBeNull()

    const overwrite = target.querySelector<HTMLInputElement>('input[type="radio"][value="overwrite"]')
    expect(overwrite, 'overwrite radio present').not.toBeNull()
    overwrite?.click()
    await tick()

    const warning = target.querySelector('.conflict-warning')
    expect(warning, 'red warning shown on Overwrite all + type mismatch').not.toBeNull()
    expect(warning?.getAttribute('role')).toBe('alert')
    expect(warning?.textContent).toContain('different type')
  })

  it('shows no warning for a pure file conflict even with Overwrite all', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const target = mountDialog()
    await flushMicrotasks()

    const overwrite = target.querySelector<HTMLInputElement>('input[type="radio"][value="overwrite"]')
    overwrite?.click()
    await tick()

    expect(target.querySelector('.conflict-warning')).toBeNull()
  })
})

/* ------------------------------------------------------------------------- */
/* Auto-confirm (MCP) dispatches with conflictNames populated                */
/* ------------------------------------------------------------------------- */

describe('TransferDialog auto-confirm payload', () => {
  it('dispatches with conflictNames populated on the MCP fast path', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const captured: { preKnown: string[] | null; resolution: ConflictResolution | null } = {
      preKnown: null,
      resolution: null,
    }
    const onConfirm: ConfirmFn = (_d, _v, _p, conflictResolution, _o, _s, preKnownConflicts) => {
      captured.preKnown = preKnownConflicts
      captured.resolution = conflictResolution
    }

    mountDialog({ autoConfirm: true, autoConfirmOnConflict: 'overwrite_all', onConfirm })
    await flushMicrotasks()

    expect(captured.preKnown).toEqual(['notes.txt'])
    expect(captured.resolution).toBe('overwrite')
  })
})

/* ------------------------------------------------------------------------- */
/* Same-volume move: skip the deep scan, dispatch immediately                */
/* ------------------------------------------------------------------------- */

describe('TransferDialog same-volume move scan gating', () => {
  it('does NOT start the deep scan preview for a same-volume move', async () => {
    mountDialog({ operationType: 'move', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()

    expect(startScanPreviewMock, 'a same-volume move must skip the deep byte scan').not.toHaveBeenCalled()
  })

  it('still starts the deep scan for a same-volume COPY (copy needs byte totals)', async () => {
    mountDialog({ operationType: 'copy', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()

    expect(startScanPreviewMock, 'a copy always needs the byte scan').toHaveBeenCalledTimes(1)
  })

  it('dispatches immediately with previewId=null and scanInProgress=false for a same-volume move', async () => {
    const captured: { previewId: string | null; scanInProgress: boolean | null; op: string | null } = {
      previewId: 'unset',
      scanInProgress: null,
      op: null,
    }
    const onConfirm: ConfirmFn = (_d, _v, previewId, _r, operationType, scanInProgress) => {
      captured.previewId = previewId
      captured.scanInProgress = scanInProgress
      captured.op = operationType
    }

    const target = mountDialog({
      operationType: 'move',
      sourceVolumeId: 'ext',
      currentVolumeId: 'ext',
      onConfirm,
    })
    await flushMicrotasks()
    clickToggle(target, 'Move') // already Move; the confirm path is what matters
    // Trigger confirm via Enter on the dialog.
    const dialog = target.querySelector<HTMLElement>('[role="dialog"], dialog') ?? target
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushMicrotasks()

    expect(captured.op).toBe('move')
    expect(captured.previewId, 'no cached preview to consume on the fast path').toBeNull()
    expect(captured.scanInProgress, 'dispatch must not gate on a scan').toBe(false)
  })

  it('cancels the running preview when flipping a cross-volume copy to a same-volume move', async () => {
    // Start as a Move from `ext` but landing on `root` (cross-volume) → the deep
    // preview runs. Flipping the destination is awkward in a unit test, so start
    // as a Copy on the same volume (preview runs), then flip to Move (same
    // volume) and assert the preview is cancelled.
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()
    expect(startScanPreviewMock).toHaveBeenCalledTimes(1)

    clickToggle(target, 'Move')
    await flushMicrotasks()

    expect(cancelScanPreviewMock, 'flipping to a same-volume move cancels the deep preview').toHaveBeenCalled()
  })

  it('restarts the preview when flipping back from a same-volume move to copy', async () => {
    const target = mountDialog({ operationType: 'move', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()
    // Move on same volume → no scan yet.
    expect(startScanPreviewMock).not.toHaveBeenCalled()

    clickToggle(target, 'Copy')
    await flushMicrotasks()

    // Copy needs byte totals → the preview starts.
    expect(startScanPreviewMock, 'flip to copy (re)starts the byte scan').toHaveBeenCalledTimes(1)
  })
})

/* ------------------------------------------------------------------------- */
/* Local→local move: the same-volume fast path must NOT apply (BUG 1)        */
/* ------------------------------------------------------------------------- */

describe('TransferDialog local→local move scan gating', () => {
  it('starts the deep scan for a local→local move (default volume is NOT a same-volume move)', async () => {
    // Both source and dest are the default local volume (root → root). The
    // backend has a real local move path that consumes the preview cache, so the
    // deep scan MUST run — the same-volume rename fast path is only for
    // non-default volumes (one SMB share / one MTP device).
    mountDialog({ operationType: 'move', sourceVolumeId: 'root', currentVolumeId: 'root' })
    await flushMicrotasks()

    expect(startScanPreviewMock, 'a local→local move must run the deep byte scan').toHaveBeenCalledTimes(1)
  })

  it('does NOT cancel the scan or zero the tallies when toggling Copy→Move locally', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([])
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'root', currentVolumeId: 'root' })
    await flushMicrotasks()
    expect(startScanPreviewMock).toHaveBeenCalledTimes(1)

    // Feed scan-complete totals so the tallies are populated, like the field repro.
    scanCompleteCb?.({
      previewId: 'preview-1',
      filesTotal: 1,
      dirsTotal: 0,
      bytesTotal: 3267,
      dedupBytesTotal: 3267,
    })
    await flushMicrotasks()

    const statsBefore = target.querySelector('.scan-stats')?.textContent ?? ''
    expect(statsBefore).toContain('1')
    expect(statsBefore).toContain('file')

    clickToggle(target, 'Move')
    await flushMicrotasks()

    // The fast-path cancel must NOT fire for a local→local move.
    expect(cancelScanPreviewMock, 'local→local toggle must not cancel the preview').not.toHaveBeenCalled()
    // The tallies must be preserved (not reset to 0).
    const statsAfter = target.querySelector('.scan-stats')?.textContent ?? ''
    expect(statsAfter).toContain('1')
    expect(statsAfter).toContain('file')
  })

  it('dispatches a local→local move WITH a previewId (the backend consumes the cache)', async () => {
    const captured: { previewId: string | null; scanInProgress: boolean | null; op: string | null } = {
      previewId: 'unset',
      scanInProgress: null,
      op: null,
    }
    const onConfirm: ConfirmFn = (_d, _v, previewId, _r, operationType, scanInProgress) => {
      captured.previewId = previewId
      captured.scanInProgress = scanInProgress
      captured.op = operationType
    }

    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'root', currentVolumeId: 'root', onConfirm })
    await flushMicrotasks()
    scanCompleteCb?.({
      previewId: 'preview-1',
      filesTotal: 1,
      dirsTotal: 0,
      bytesTotal: 3267,
      dedupBytesTotal: 3267,
    })
    await flushMicrotasks()

    clickToggle(target, 'Move')
    await flushMicrotasks()

    const dialog = target.querySelector<HTMLElement>('[role="dialog"], dialog') ?? target
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushMicrotasks()

    expect(captured.op).toBe('move')
    expect(captured.previewId, 'local→local move must carry the previewId so the BE consumes the cache').toBe(
      'preview-1',
    )
  })
})

/* ------------------------------------------------------------------------- */
/* Direction header label uses the volume display name at a volume root (BUG 3) */
/* ------------------------------------------------------------------------- */

describe('TransferDialog direction-header label', () => {
  it('renders the volume display name (not the storage id) for an MTP storage root source', async () => {
    // Source pane is at the MTP "SD Card" storage root, whose basename is the
    // raw storage id "65538" (0x10002). F5/F6 must show the volume name instead.
    const target = mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'mtp-336592896:65538',
      sourceFolderPath: '/mtp-20-5/65538',
      currentVolumeId: 'root',
      destinationPath: '/Users/test/dest',
      direction: 'left',
    })
    await flushMicrotasks()

    const source = target.querySelector('.folder-name.source')?.textContent.trim() ?? ''
    expect(source).toBe('Virtual Pixel 9 - SD Card')
    expect(source).not.toContain('65538')
  })

  it('still renders the folder basename for a normal subfolder source', async () => {
    const target = mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'root',
      sourceFolderPath: '/Users/test/photos',
      currentVolumeId: 'root',
      destinationPath: '/Users/test/dest',
      direction: 'left',
    })
    await flushMicrotasks()

    const source = target.querySelector('.folder-name.source')?.textContent.trim() ?? ''
    expect(source).toBe('photos')
  })
})
