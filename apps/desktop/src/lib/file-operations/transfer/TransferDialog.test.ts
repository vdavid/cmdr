/**
 * Component tests for `TransferDialog.svelte`'s upfront conflict UX.
 *
 * Covers the M2 behavior: the top-level conflict check runs in parallel with
 * the (potentially slow) scan preview, dir-vs-dir collisions classify as merge
 * info rather than conflicts, the file-policy radios show for merges too, the
 * cross-type "Overwrite all" guardrail, and the auto-confirm (MCP) payload
 * wiring. The volume store, Tauri IPC, and settings are stubbed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'

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
      operationType: 'copy',
      sourcePaths: ['/Users/test/photos', '/Users/test/notes.txt'],
      destinationPath: '/Users/test/dest',
      direction: 'right',
      currentVolumeId: 'root',
      fileCount: 1,
      folderCount: 1,
      sourceFolderPath: '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
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
  document.body.innerHTML = ''
})

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
