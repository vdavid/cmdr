/**
 * Component tests for `TransferDialog.svelte`'s upfront conflict UX.
 *
 * Covers the decoupled conflict UX: the top-level conflict check runs in parallel with
 * the (potentially slow) scan preview, dir-vs-dir collisions classify as merge
 * info rather than conflicts, the file-policy radios show for merges too, the
 * cross-type "Overwrite all" guardrail, and the auto-confirm (MCP) payload
 * wiring. The volume store, Tauri IPC, and settings are stubbed.
 */

import { describe, it, expect, vi } from 'vitest'
import { tick } from 'svelte'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'
import {
  estimateOperationCostMock,
  startScanPreviewMock,
  cancelScanPreviewMock,
  scanCompleteCb,
  scanVolumeForConflictsMock,
  destinationExistsMock,
  destinationWriteAccessMock,
  destinationRootEchoMock,
  makeConflict,
  flushMicrotasks,
  mountDialog,
  radioGroup,
  scanState,
  deferred,
  confirmButton,
  cancelButton,
  closeButton,
  clickToggle,
  pathInput,
  type ConfirmFn,
} from './test-transfer-dialog-harness'
it('shows a conflict spinner beside the file count only after 100 ms, without a checking row', async () => {
  vi.useFakeTimers()
  try {
    const pending = deferred<VolumeConflictInfo[]>()
    scanVolumeForConflictsMock.mockReturnValueOnce(pending.promise)
    const target = mountDialog()
    await vi.advanceTimersByTimeAsync(0)
    const slot = target.querySelector('.conflict-check-status')
    expect(slot).not.toBeNull()
    expect(target.querySelector('.conflicts-checking')).toBeNull()
    expect(slot?.querySelector('.spinner')).toBeNull()
    await vi.advanceTimersByTimeAsync(99)
    expect(slot?.querySelector('.spinner')).toBeNull()
    await vi.advanceTimersByTimeAsync(1)
    expect(slot?.querySelector('[role="status"]')).not.toBeNull()
    pending.resolve([])
    await vi.advanceTimersByTimeAsync(0)
    expect(slot?.querySelector('.spinner')).toBeNull()
    const input = pathInput(target)
    input.value = '/Users/test/other'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await vi.advanceTimersByTimeAsync(400)
    expect(slot?.querySelector('.spinner')).toBeNull()
  } finally {
    vi.useRealTimers()
  }
})

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
    const [call] = scanVolumeForConflictsMock.mock.calls[0]
    expect(call.sourceVolumeId).toBe('root')
    expect(call.sourcePaths).toEqual(['/Users/test/photos', '/Users/test/notes.txt'])
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
    expect(summary?.textContent.replace(/\s+/g, ' ').trim()).toBe(
      '1 file already exists. What do you want to do with it?',
    )
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
    const onConfirm: ConfirmFn = ({ preKnownConflicts }) => {
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
/* Auto-confirm (MCP) payload wiring                                         */
/* ------------------------------------------------------------------------- */

describe('TransferDialog auto-confirm payload', () => {
  it('maps the MCP onConflict string onto the dispatched resolution', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'notes.txt', sourceIsDirectory: false, destIsDirectory: false }),
    ])

    const captured: { preKnown: string[] | null; resolution: ConflictResolution | null } = {
      preKnown: null,
      resolution: null,
    }
    const onConfirm: ConfirmFn = ({ conflictResolution, preKnownConflicts }) => {
      captured.preKnown = preKnownConflicts
      captured.resolution = conflictResolution
    }

    mountDialog({ autoConfirm: true, autoConfirmOnConflict: 'overwrite_all', onConfirm })
    await flushMicrotasks()

    // `overwrite` does NOT wait for the conflict check (only `skip` does — see
    // "confirm without waiting for the conflict check" below). The names ride along
    // here because the check has already resolved by dispatch time, and this pins
    // that we don't blank them just because the policy isn't `skip`. What is NOT
    // asserted, because it isn't guaranteed: that a still-pending check delays this
    // dispatch. Under a slow dest listing it dispatches with `[]`, by design.
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

  it('dispatches immediately with previewId=null for a same-volume move', async () => {
    const captured: { previewId: string | null; op: string | null } = {
      previewId: 'unset',
      op: null,
    }
    const onConfirm: ConfirmFn = ({ previewId, operationType }) => {
      captured.previewId = previewId
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
    const captured: { previewId: string | null; op: string | null } = {
      previewId: 'unset',
      op: null,
    }
    const onConfirm: ConfirmFn = ({ previewId, operationType }) => {
      captured.previewId = previewId
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

  it('scans a same-volume move where renames copy (S3), so the counts and the cost line appear', async () => {
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({
      operationType: 'move',
      sourceVolumeId: 's3-photos',
      currentVolumeId: 's3-photos',
      onConfirm,
    })
    await flushMicrotasks()

    expect(startScanPreviewMock, 'a move that copies on the server scans').toHaveBeenCalled()
    expect(scanState(target)).toBe('counting')

    scanCompleteCb?.({ previewId: 'preview-1', filesTotal: 3, dirsTotal: 1, bytesTotal: 30, dedupBytesTotal: 30 })
    await flushMicrotasks()
    expect(scanState(target)).toBe('done')
    expect(estimateOperationCostMock).toHaveBeenCalledWith(
      expect.objectContaining({ operation: 'move', previewId: 'preview-1', sourceVolumeId: 's3-photos' }),
    )

    confirmButton(target).click()
    await flushMicrotasks()
    expect(onConfirm, 'the move hands its scan to the backend').toHaveBeenCalledWith(
      expect.objectContaining({ previewId: 'preview-1' }),
    )
  })

  it('prices the overwrites the chosen policy makes, from the conflict check', async () => {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'notes.txt', sourceSize: 10, destSize: 5, sourceModified: 200, destModified: 100 }),
    ])
    const target = mountDialog({ operationType: 'copy', sourceVolumeId: 'root', currentVolumeId: 's3-photos' })
    await flushMicrotasks()
    scanCompleteCb?.({ previewId: 'preview-1', filesTotal: 2, dirsTotal: 0, bytesTotal: 30, dedupBytesTotal: 30 })
    await flushMicrotasks()

    target.querySelector<HTMLInputElement>('input[type="radio"][value="overwrite"]')?.click()
    await flushMicrotasks()

    expect(estimateOperationCostMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        clashes: {
          resolution: 'overwrite',
          clashes: [{ sourceSize: 10, destSize: 5, sourceModified: 200, destModified: 100 }],
        },
      }),
    )
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
/* Destination path: home long-form + "will be created" warning             */
/* ------------------------------------------------------------------------- */

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
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'copy', destinationPath: '/Users/test/brand-new' })
    await settleExistsCheck()

    const warning = target.querySelector('.path-warning')
    expect(warning).not.toBeNull()
    expect(warning?.textContent).toContain('will create it during the copy')
    // No red error alongside the yellow warning.
    expect(target.querySelector('.path-error')).toBeNull()
    expect(pathInput(target).closest('.text-field')?.classList.contains('text-field-warning')).toBe(true)
  })

  it('uses the move-specific copy for the create warning when moving', async () => {
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'move', destinationPath: '/Users/test/brand-new' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')?.textContent).toContain('will create it during the move')
  })

  it('warns for a non-local (SMB) destination too, since the backend now creates it on every volume', async () => {
    // Pre-fix this warning was gated to local destinations (the backend's
    // recursive create was local-FS only). Now `create_directory_all` makes the
    // dest on every backend, so the yellow "will be created" warning shows
    // honestly for an SMB/MTP destination as well.
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
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
    destinationExistsMock.mockResolvedValue({ data: true, timedOut: false })
    const target = mountDialog({ destinationPath: '/Users/test/dest' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')).toBeNull()
    expect(pathInput(target).closest('.text-field')?.classList.contains('text-field-warning')).toBe(false)
  })

  it('stays quiet when the existence check times out (inconclusive)', async () => {
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: true })
    const target = mountDialog({ destinationPath: '/Users/test/maybe' })
    await settleExistsCheck()

    expect(target.querySelector('.path-warning')).toBeNull()
  })

  it('lets the red error win over the yellow warning for an invalid path', async () => {
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ destinationPath: 'relative/path' })
    await settleExistsCheck()

    // Structurally invalid → red error shows, yellow warning suppressed.
    expect(target.querySelector('.path-error')).not.toBeNull()
    expect(target.querySelector('.path-warning')).toBeNull()
  })

  describe('a path repeating the place’s own root folder (#164)', () => {
    const ECHO = { rootFolder: '/srv/data', resolved: '/srv/data/srv/data/photos', stripped: '/photos' }

    function rootEchoWarning(target: HTMLElement): HTMLElement | null {
      return target.querySelector('.root-echo')
    }

    function stripButton(target: HTMLElement): HTMLButtonElement {
      const btn = rootEchoWarning(target)?.querySelector<HTMLButtonElement>('button')
      if (!btn) throw new Error('strip button not rendered')
      return btn
    }

    it('names where the path goes and offers the stripped reading, prefilled or typed', async () => {
      // A prefilled path is ambiguous the same way a typed one is: a place rooted
      // at `/home/bob` can really hold `/home/bob/home/bob/folder`.
      destinationRootEchoMock.mockResolvedValue(ECHO)
      const target = mountDialog({ destinationPath: '/srv/data/photos' })
      await settleExistsCheck()

      expect(destinationRootEchoMock).toHaveBeenCalledWith({ volumeId: 'root', path: '/srv/data/photos' })
      const warning = rootEchoWarning(target)
      expect(warning?.textContent).toContain('/srv/data/srv/data/photos')
      expect(warning?.textContent).toContain('/photos')
      // Nothing rewrites the field on its own.
      expect(pathInput(target).value).toBe('/srv/data/photos')
    })

    it('rewrites the field only when the button is pressed', async () => {
      destinationRootEchoMock.mockImplementation(({ path }) =>
        Promise.resolve(path === '/srv/data/photos' ? ECHO : null),
      )
      const target = mountDialog({ destinationPath: '/srv/data/photos' })
      await settleExistsCheck()

      stripButton(target).click()
      await settleExistsCheck()

      expect(pathInput(target).value).toBe('/photos')
      expect(rootEchoWarning(target)).toBeNull()
    })

    it('lets Enter send the path exactly as typed', async () => {
      destinationRootEchoMock.mockResolvedValue(ECHO)
      const onConfirm = vi.fn<ConfirmFn>()
      const target = mountDialog({ destinationPath: '/srv/data/photos', onConfirm })
      await settleExistsCheck()

      pathInput(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await flushMicrotasks()

      expect(onConfirm).toHaveBeenCalledTimes(1)
      expect(onConfirm.mock.calls[0][0].destination).toBe('/srv/data/photos')
    })

    it('shows nothing for a path that reads one way', async () => {
      const target = mountDialog({ destinationPath: '/Users/test/dest' })
      await settleExistsCheck()

      expect(rootEchoWarning(target)).toBeNull()
    })
  })

  it('says plainly when the destination folder takes no writes, instead of promising to create it', async () => {
    // ❗ The Pixel case: copying onto a phone's `/` only surfaced after confirm, as
    // "Not enough space". The folder doesn't exist yet AND takes no writes, so
    // "Cmdr will create it" would be a promise the copy can't keep.
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'unexplained' })
    const target = mountDialog({ operationType: 'copy', destinationPath: '/system/New', currentVolumeId: 'root' })
    await settleExistsCheck()

    expect(destinationWriteAccessMock).toHaveBeenCalledWith({ volumeId: 'root', path: '/system/New' })
    expect(target.querySelector('.path-error')?.textContent).toContain('doesn’t accept new files')
    expect(target.querySelector('.path-warning')).toBeNull()
  })

  it('names the reason when the backend can tell read-only from a missing permission', async () => {
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'readOnlyFilesystem' })
    const readOnly = mountDialog({ destinationPath: '/Volumes/Installer' })
    await settleExistsCheck()
    expect(readOnly.querySelector('.path-error')?.textContent).toContain('read-only')

    document.body.innerHTML = ''
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'noPermission' })
    const locked = mountDialog({ destinationPath: '/Users/other/private' })
    await settleExistsCheck()
    expect(locked.querySelector('.path-error')?.textContent).toContain('permission')
  })

  it('disables Confirm while the folder refuses writes, since the transfer would only refuse it again', async () => {
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'readOnlyFilesystem' })
    const target = mountDialog({ destinationPath: '/Volumes/Installer' })
    await settleExistsCheck()

    expect(target.querySelector('.path-error')?.textContent).toContain('read-only')
    expect(confirmButton(target).disabled).toBe(true)
  })

  it('ignores Enter while the folder refuses writes, the same as the disabled button', async () => {
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unwritable', reason: 'noPermission' })
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ destinationPath: '/Users/other/private', onConfirm })
    await settleExistsCheck()

    pathInput(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    const dialog = target.querySelector<HTMLElement>('[role="dialog"], dialog') ?? target
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushMicrotasks()

    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('stays quiet when the backend can’t tell whether the folder takes writes', async () => {
    destinationWriteAccessMock.mockResolvedValue({ kind: 'unknown' })
    const target = mountDialog({ destinationPath: '/Volumes/naspi/share' })
    await settleExistsCheck()

    expect(target.querySelector('.path-error')).toBeNull()
    expect(confirmButton(target).disabled).toBe(false)
  })
})

/* ------------------------------------------------------------------------- */
/* Compress mode: third operation type                                       */
/* ------------------------------------------------------------------------- */

describe('TransferDialog compress mode', () => {
  function pathInput(target: HTMLElement): HTMLInputElement {
    const input = target.querySelector<HTMLInputElement>('input[aria-label="Destination path"]')
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
    destinationExistsMock.mockResolvedValue({ data: true, timedOut: false })
    const target = mountDialog({ operationType: 'compress' })
    await settleExistsCheck()
    const warning = target.querySelector('.path-warning')
    expect(warning?.textContent).toContain('already here')
    // The copy/move "folder will be created" wording must NOT appear here.
    expect(warning?.textContent).not.toContain('create')
  })

  it('shows no overwrite warning when the target does not exist yet', async () => {
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    const target = mountDialog({ operationType: 'compress' })
    await settleExistsCheck()
    expect(target.querySelector('.path-warning')).toBeNull()
  })

  it('auto-confirm does NOT overwrite an existing archive (surfaces the dialog instead)', async () => {
    // Auto-confirm (MCP) with the target zip already present: the dialog must NOT
    // dispatch — it stays open so the user decides.
    destinationExistsMock.mockResolvedValue({ data: true, timedOut: false })
    const onConfirm = vi.fn<ConfirmFn>()
    mountDialog({ operationType: 'compress', autoConfirm: true, onConfirm })
    await flushMicrotasks()
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('auto-confirm proceeds when the target archive does not exist', async () => {
    destinationExistsMock.mockResolvedValue({ data: false, timedOut: false })
    const onConfirm = vi.fn<ConfirmFn>()
    mountDialog({ operationType: 'compress', autoConfirm: true, onConfirm })
    await flushMicrotasks()
    expect(onConfirm).toHaveBeenCalledTimes(1)
    // Compress dispatches with an empty conflict list (no multi-file conflicts).
    expect(onConfirm.mock.calls[0][0].preKnownConflicts).toEqual([])
  })
})

/* ------------------------------------------------------------------------- */
/* Confirm acts at once: no silent wait on the conflict check                */
/* ------------------------------------------------------------------------- */

describe('TransferDialog confirm without waiting for the conflict check', () => {
  it('dispatches while the conflict check is still pending, under the default stop policy', async () => {
    // The dest listing never comes back (the slow-SMB case): the click must still
    // reach `onConfirm` instead of sitting on the promise with a live-looking button.
    const pendingCheck = deferred<VolumeConflictInfo[]>()
    scanVolumeForConflictsMock.mockReturnValue(pendingCheck.promise)

    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ onConfirm })
    await flushMicrotasks()

    confirmButton(target).click()
    await flushMicrotasks()

    expect(onConfirm, 'the confirm must not wait on the dest listing').toHaveBeenCalledTimes(1)
    // `stop` asks per clash at runtime, and the backend ignores `pre_known_conflicts`
    // outside `Skip`, so dispatching with an empty list loses information, not safety.
    expect(onConfirm.mock.calls[0][0].conflictResolution).toBe('stop')
    expect(onConfirm.mock.calls[0][0].preKnownConflicts).toEqual([])
  })

  it('dispatches a same-volume move while the conflict check is still pending', async () => {
    const pendingCheck = deferred<VolumeConflictInfo[]>()
    scanVolumeForConflictsMock.mockReturnValue(pendingCheck.promise)

    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({
      operationType: 'move',
      sourceVolumeId: 'ext',
      currentVolumeId: 'ext',
      onConfirm,
    })
    await flushMicrotasks()

    confirmButton(target).click()
    await flushMicrotasks()

    expect(onConfirm, 'the rename fast path must not wait either').toHaveBeenCalledTimes(1)
    expect(onConfirm.mock.calls[0][0].preKnownConflicts).toEqual([])
  })

  it('still waits for the conflict names under the Skip policy (the bulk-skip perf win)', async () => {
    // `Skip` is the ONE resolution the backend reads `pre_known_conflicts` for, and
    // only the MCP path can select it before the check lands (the radios don't
    // render while it runs), so no human is waiting on this await.
    const pendingCheck = deferred<VolumeConflictInfo[]>()
    scanVolumeForConflictsMock.mockReturnValue(pendingCheck.promise)

    const onConfirm = vi.fn<ConfirmFn>()
    mountDialog({ autoConfirm: true, autoConfirmOnConflict: 'skip_all', onConfirm })
    await flushMicrotasks()

    expect(onConfirm, 'Skip needs the names upfront, so it waits').not.toHaveBeenCalled()

    pendingCheck.resolve([makeConflict({ sourcePath: 'notes.txt' })])
    await flushMicrotasks()

    expect(onConfirm).toHaveBeenCalledTimes(1)
    expect(onConfirm.mock.calls[0][0].preKnownConflicts).toEqual(['notes.txt'])
  })

  it('disables the confirm button and shows a spinner while a confirm is genuinely pending', async () => {
    // Hold `startScanPreview` open. The confirm path still awaits it (the progress
    // dialog needs a non-null `previewId`), so this is the window where the button
    // must read as busy rather than inviting a second click.
    const pendingScan = deferred<{ previewId: string }>()
    startScanPreviewMock.mockReturnValue(pendingScan.promise)

    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ onConfirm })
    await flushMicrotasks()

    confirmButton(target).click()
    await flushMicrotasks()

    expect(onConfirm, 'still waiting on the preview id').not.toHaveBeenCalled()
    expect(confirmButton(target).disabled, 'a pending confirm must not look clickable').toBe(true)
    expect(confirmButton(target).querySelector('.spinner'), 'a pending confirm shows a spinner').not.toBeNull()

    pendingScan.resolve({ previewId: 'preview-1' })
    await flushMicrotasks()

    expect(onConfirm).toHaveBeenCalledTimes(1)
  })

  it('Cancel during a pending confirm leaves the scan preview alone', async () => {
    // Skip is the policy that still waits, so it gives us a real in-flight confirm
    // to fire Cancel against.
    const pendingCheck = deferred<VolumeConflictInfo[]>()
    scanVolumeForConflictsMock.mockReturnValue(pendingCheck.promise)

    const onConfirm = vi.fn<ConfirmFn>()
    const onCancel = vi.fn()
    const target = mountDialog({
      autoConfirm: true,
      autoConfirmOnConflict: 'skip_all',
      onConfirm,
      onCancel,
    })
    await flushMicrotasks()
    expect(onConfirm).not.toHaveBeenCalled()

    // The footer's Cancel goes inert alongside the confirm button, so its click alone
    // would prove nothing.
    expect(cancelButton(target).disabled, 'Cancel reads as unavailable while a confirm is pending').toBe(true)
    cancelButton(target).click()

    // The × stays live whatever the footer does (Escape lands on the same handler), so
    // it's the route the `confirmed` guard in `handleCancel` actually has to hold.
    closeButton(target).click()
    await flushMicrotasks()

    // Freeing here pulls the preview out from under the pending dispatch, and the
    // progress dialog then opens onto a cancelled preview.
    expect(cancelScanPreviewMock, 'a confirmed dialog must not free its preview').not.toHaveBeenCalled()
    expect(onCancel, 'the confirm already committed; Cancel is a no-op').not.toHaveBeenCalled()

    pendingCheck.resolve([makeConflict({ sourcePath: 'notes.txt' })])
    await flushMicrotasks()

    expect(onConfirm).toHaveBeenCalledTimes(1)
  })
})
