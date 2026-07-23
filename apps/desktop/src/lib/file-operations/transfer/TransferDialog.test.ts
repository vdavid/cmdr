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

// Destination-existence probe behind the "this folder will be created" warning.
// Defaults to "exists" so most tests see no warning; a test overrides it.
const pathExistsCheckedMock = vi.fn<(path: string, volumeId?: string) => Promise<{ data: boolean; timedOut: boolean }>>(
  () => Promise.resolve({ data: true, timedOut: false }),
)

// Home dir resolution for the long-form display of a bare `~` destination.
vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

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
  pathExistsChecked: (...args: Parameters<typeof pathExistsCheckedMock>) => pathExistsCheckedMock(...args),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => (key === 'behavior.archiveCompressionLevel' ? 6 : 500)),
  // Compress mode renders `CompressLevelControl` → `SettingSlider`, which reads
  // its metadata and default through the barrel and writes via `setSetting`.
  setSetting: vi.fn(),
  getDefaultValue: vi.fn(() => 6),
  onSpecificSettingChange: vi.fn(() => () => {}),
  getSettingDefinition: vi.fn(() => ({
    label: 'Compression level',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  })),
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
    {
      id: 'smb://nas.local/public',
      name: 'NAS share',
      path: 'smb://nas.local/public',
      category: 'network',
      isEjectable: false,
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
  operationType?: 'copy' | 'move' | 'compress'
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

/** Reads the `data-scan-state` marker off the tallies element. */
function scanState(target: HTMLElement): string | null {
  return target.querySelector('.scan-stats')?.getAttribute('data-scan-state') ?? null
}

beforeEach(() => {
  scanCompleteCb = null
  scanVolumeForConflictsMock.mockReset()
  scanVolumeForConflictsMock.mockResolvedValue([])
  pathExistsCheckedMock.mockReset()
  pathExistsCheckedMock.mockResolvedValue({ data: true, timedOut: false })
  startScanPreviewMock.mockClear()
  startScanPreviewMock.mockResolvedValue({ previewId: 'preview-1' })
  cancelScanPreviewMock.mockClear()
  document.body.innerHTML = ''
})

/** Clicks the Copy/Move segmented toggle option by its label. */
function clickToggle(target: HTMLElement, label: 'Copy' | 'Move'): void {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('.operation-toggle .tg-item'))
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
/* Source-volume forwarding: a wrong source volume id makes the preview zero  */
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
/* Local→local move: the same-volume fast path must NOT apply, or the         */
/* Copy→Move toggle zeroes the dialog counters                                */
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
/* data-scan-state marker: race-free "counting done" signal for E2E           */
/* ------------------------------------------------------------------------- */

describe('TransferDialog data-scan-state marker', () => {
  it('reads "counting" while the deep scan is still running', async () => {
    // Hold the byte scan open (no scan-complete fired). The tallies element
    // must advertise that it's still counting so an E2E helper keeps polling.
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'root', currentVolumeId: 'root' })
    await flushMicrotasks()

    expect(scanCompleteCb, 'scan still in progress (complete not fired)').not.toBeNull()
    expect(scanState(target)).toBe('counting')
  })

  it('transitions to "done" once the scan-complete event arrives', async () => {
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'root', currentVolumeId: 'root' })
    await flushMicrotasks()
    expect(scanState(target)).toBe('counting')

    scanCompleteCb?.({
      previewId: 'preview-1',
      filesTotal: 1,
      dirsTotal: 0,
      bytesTotal: 3267,
      dedupBytesTotal: 3267,
    })
    await flushMicrotasks()

    expect(scanState(target)).toBe('done')
  })

  it('reads "skipped" for a same-volume move (no deep scan ever runs)', async () => {
    // A same-volume move renames server-side (zero bytes); the deep scan is
    // skipped, so the tallies legitimately stay at 0 and must say so.
    const target = mountDialog({ operationType: 'move', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()

    expect(startScanPreviewMock, 'same-volume move skips the scan').not.toHaveBeenCalled()
    expect(scanState(target)).toBe('skipped')
  })

  it('still reaches "done" for a same-volume COPY (copy scans even on one volume)', async () => {
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()
    expect(scanState(target)).toBe('counting')

    scanCompleteCb?.({
      previewId: 'preview-1',
      filesTotal: 2,
      dirsTotal: 1,
      bytesTotal: 4096,
      dedupBytesTotal: 4096,
    })
    await flushMicrotasks()

    expect(scanState(target)).toBe('done')
  })

  it('flips counting → skipped when toggling a same-volume copy to move', async () => {
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'ext', currentVolumeId: 'ext' })
    await flushMicrotasks()
    expect(scanState(target)).toBe('counting')

    clickToggle(target, 'Move')
    await flushMicrotasks()

    // Flipping to a same-volume move cancels the deep preview → skipped.
    expect(scanState(target)).toBe('skipped')
  })
})

/* ------------------------------------------------------------------------- */
/* Direction header label uses the volume display name at a volume root         */
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

/* ------------------------------------------------------------------------- */
/* Destination path: home long-form + "will be created" warning             */
/* ------------------------------------------------------------------------- */

function pathInput(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('.path-input')
  if (!input) throw new Error('path input not found')
  return input
}

/** Waits past the destination-existence debounce (300 ms) and flushes. */
async function settleExistsCheck(): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, 350))
  await flushMicrotasks()
}

describe('TransferDialog destination path', () => {
  it('shows the home dir as its absolute long form when the destination is exactly ~', async () => {
    const target = mountDialog({ destinationPath: '~', currentVolumeId: 'root' })
    await flushMicrotasks()

    // A bare `~` is replaced with the resolved absolute home, not left as `~`.
    expect(pathInput(target).value).toBe('/Users/test')
  })

  it('keeps a ~/sub destination in its short form', async () => {
    const target = mountDialog({ destinationPath: '~/Documents', currentVolumeId: 'root' })
    await flushMicrotasks()

    expect(pathInput(target).value).toBe('~/Documents')
  })

  it('warns that a non-existent destination folder will be created', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'copy', destinationPath: '/Users/test/brand-new' })
    await settleExistsCheck()

    const warning = target.querySelector('.path-warning')
    expect(warning).not.toBeNull()
    expect(warning?.textContent).toContain('will create it during the copy')
    // No red error alongside the yellow warning.
    expect(target.querySelector('.path-error')).toBeNull()
    expect(pathInput(target).classList.contains('has-warning')).toBe(true)
  })

  it('uses the move-specific copy for the create warning when moving', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'move', destinationPath: '/Users/test/brand-new' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')?.textContent).toContain('will create it during the move')
  })

  it('warns for a non-local (SMB) destination too, since the backend now creates it on every volume', async () => {
    // Pre-fix this warning was gated to local destinations (the backend's
    // recursive create was local-FS only). Now `create_directory_all` makes the
    // dest on every backend, so the yellow "will be created" warning shows
    // honestly for an SMB/MTP destination as well.
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({
      operationType: 'copy',
      currentVolumeId: 'smb://nas.local/public',
      destinationPath: 'smb://nas.local/public/brand-new',
    })
    await settleExistsCheck()

    const warning = target.querySelector('.path-warning')
    expect(warning).not.toBeNull()
    expect(warning?.textContent).toContain('will create it during the copy')
  })

  it('does not warn when the destination folder already exists', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: true, timedOut: false })
    const target = mountDialog({ destinationPath: '/Users/test/dest' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')).toBeNull()
    expect(pathInput(target).classList.contains('has-warning')).toBe(false)
  })

  it('stays quiet when the existence check times out (inconclusive)', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: true })
    const target = mountDialog({ destinationPath: '/Users/test/maybe' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')).toBeNull()
  })

  it('lets the red error win over the yellow warning for an invalid path', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ destinationPath: 'relative/path' })
    await settleExistsCheck()

    // Structurally invalid → red error shows, yellow warning suppressed.
    expect(target.querySelector('.path-error')).not.toBeNull()
    expect(target.querySelector('.path-warning')).toBeNull()
  })
})

/* ------------------------------------------------------------------------- */
/* Compress mode: third operation type                                       */
/* ------------------------------------------------------------------------- */

describe('TransferDialog compress mode', () => {
  function pathInput(target: HTMLElement): HTMLInputElement {
    const input = target.querySelector<HTMLInputElement>('.path-input')
    if (!input) throw new Error('path input not rendered')
    return input
  }

  it('suggests a `.zip` filename in the destination folder', async () => {
    // Two sources under /Users/test → the source-directory basename ("test") wins.
    const target = mountDialog({ operationType: 'compress', destinationPath: '/Users/test/dest' })
    await flushMicrotasks()
    expect(pathInput(target).value).toBe('/Users/test/dest/test.zip')
  })

  it('labels the confirm button "Compress"', async () => {
    const target = mountDialog({ operationType: 'compress' })
    await flushMicrotasks()
    const confirm = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Compress')
    expect(confirm).toBeTruthy()
  })

  it('does NOT run the multi-file conflict check (one new file has no dest conflicts)', async () => {
    const target = mountDialog({ operationType: 'compress' })
    await flushMicrotasks()
    expect(scanVolumeForConflictsMock).not.toHaveBeenCalled()
    expect(radioGroup(target)).toBeNull()
  })

  it('warns that an existing archive will be replaced', async () => {
    // The target zip already exists at the destination.
    pathExistsCheckedMock.mockResolvedValue({ data: true, timedOut: false })
    const target = mountDialog({ operationType: 'compress' })
    await settleExistsCheck()
    const warning = target.querySelector('.path-warning')
    expect(warning?.textContent).toContain('already here')
    // The copy/move "folder will be created" wording must NOT appear here.
    expect(warning?.textContent).not.toContain('create')
  })

  it('shows no overwrite warning when the target does not exist yet', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'compress' })
    await settleExistsCheck()
    expect(target.querySelector('.path-warning')).toBeNull()
  })

  it('auto-confirm does NOT overwrite an existing archive (surfaces the dialog instead)', async () => {
    // Auto-confirm (MCP) with the target zip already present: the dialog must NOT
    // dispatch — it stays open so the user decides.
    pathExistsCheckedMock.mockResolvedValue({ data: true, timedOut: false })
    const onConfirm = vi.fn()
    mountDialog({ operationType: 'compress', autoConfirm: true, onConfirm })
    await flushMicrotasks()
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('auto-confirm proceeds when the target archive does not exist', async () => {
    pathExistsCheckedMock.mockResolvedValue({ data: false, timedOut: false })
    const onConfirm = vi.fn()
    mountDialog({ operationType: 'compress', autoConfirm: true, onConfirm })
    await flushMicrotasks()
    expect(onConfirm).toHaveBeenCalledTimes(1)
    // Compress dispatches with an empty conflict list (no multi-file conflicts).
    expect(onConfirm.mock.calls[0][6]).toEqual([])
  })
})
