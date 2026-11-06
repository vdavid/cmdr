/**
 * Headless tests for `createTransferScanState`: a scan start that something else overtakes
 * before it's under way.
 *
 * The scan start is async: four listener registrations, then the `startScanPreview` IPC. Two
 * things can overtake it partway through, and neither may leave a listener or an orphan
 * preview behind:
 *
 * - **The dialog closes.** An MCP `dialog_confirm` takes it down at once, and both its parents
 *   null their props object on close. The factory's getters read the dialog's props, which are
 *   live getters into that object, so a read after unmount throws: the `null is not an object
 *   (evaluating 't.transferDialogProps.sourcePaths')` rejection E2E runs logged.
 *   `getSourcePaths` here throws the same way once the dialog is gone.
 * - **The Copy/Move toggle lands on a same-volume move.** The toggle effect cancels the preview
 *   and resets the scan state, so the start it overtook must not write its result back over
 *   that reset, or over the scan a toggle back to Copy starts in its place.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync } from 'svelte'
import * as commands from '#lib/tauri-commands/index.js'
import { createTransferScanState } from './transfer-scan-state.svelte'

vi.mock('#lib/tauri-commands/index.js', () => ({
  startScanPreview: vi.fn(),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(),
  onScanPreviewComplete: vi.fn(),
  onScanPreviewError: vi.fn(),
  onScanPreviewCancelled: vi.fn(),
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 500),
}))

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
}

function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((settle) => {
    resolve = settle
  })
  return { promise, resolve }
}

/** Lets every pending continuation run, including ones nothing awaits anymore. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((done) => setTimeout(done, 0))
  }
}

describe('createTransferScanState', () => {
  let registrations: Deferred<undefined>
  let unlistens: Array<() => void>
  let destroyed: boolean
  const move = $state({ sameVolume: false })
  let scan: ReturnType<typeof createTransferScanState>
  let destroyRoot: () => void

  beforeEach(() => {
    vi.clearAllMocks()
    registrations = deferred()
    unlistens = []
    destroyed = false
    move.sameVolume = false
    vi.mocked(commands.startScanPreview).mockResolvedValue({ previewId: 'preview-1' })
    for (const register of [
      commands.onScanPreviewProgress,
      commands.onScanPreviewComplete,
      commands.onScanPreviewError,
      commands.onScanPreviewCancelled,
    ]) {
      vi.mocked(register).mockImplementation(async () => {
        await registrations.promise
        const unlisten = vi.fn()
        unlistens.push(unlisten)
        return unlisten
      })
    }
    destroyRoot = $effect.root(() => {
      scan = createTransferScanState({
        getSourcePaths: () => {
          // The dialog's prop getter, once its parent has nulled the props object.
          if (destroyed) throw new TypeError("null is not an object (evaluating 'transferDialogProps.sourcePaths')")
          return ['/src/a.txt']
        },
        getSourceVolumeId: () => 'root',
        getIsSameVolumeMove: () => move.sameVolume,
        getConfirmed: () => false,
        getDestroyed: () => destroyed,
        getScanPurpose: () => 'transfer',
        getMacOnlyFiles: () => 'include',
      })
    })
    // The toggle effect's first run is the one it skips; get it out of the way.
    flushSync()
  })

  afterEach(() => {
    destroyRoot()
  })

  /** What `TransferDialog`'s `onDestroy` does for a dialog that wasn't confirmed. */
  function close(): void {
    destroyed = true
    scan.freeAndCleanup()
  }

  /** Flips the Copy/Move toggle to or away from a same-volume move, and lets the effect react. */
  function setSameVolumeMove(sameVolume: boolean): void {
    move.sameVolume = sameVolume
    flushSync()
  }

  describe('closed before its scan preview is under way', () => {
    it('reads no props and keeps no listener when it closes while its listeners are still registering', async () => {
      scan.start()

      close()
      registrations.resolve(undefined)

      await expect(scan.scanStarted).resolves.toBeUndefined()
      expect(commands.startScanPreview).not.toHaveBeenCalled()
      expect(unlistens.length).toBeGreaterThan(0)
      for (const unlisten of unlistens) expect(unlisten).toHaveBeenCalledOnce()
    })

    it('frees a preview whose id only lands after the dialog closed', async () => {
      const preview = deferred<{ previewId: string }>()
      vi.mocked(commands.startScanPreview).mockReturnValue(preview.promise)
      registrations.resolve(undefined)
      scan.start()
      await vi.waitFor(() => {
        expect(commands.startScanPreview).toHaveBeenCalledWith(['/src/a.txt'], 'transfer', 500, 'root', 'include')
      })

      close()
      preview.resolve({ previewId: 'preview-late' })

      await expect(scan.scanStarted).resolves.toBeUndefined()
      expect(commands.cancelScanPreview).toHaveBeenCalledWith('preview-late')
      expect(commands.checkScanPreviewStatus).not.toHaveBeenCalled()
    })
  })

  describe('overtaken by the toggle to a same-volume move', () => {
    it('starts nothing and keeps no listener when the toggle lands while its listeners are still registering', async () => {
      scan.start()

      setSameVolumeMove(true)
      registrations.resolve(undefined)
      await settle()

      expect(commands.startScanPreview).not.toHaveBeenCalled()
      expect(unlistens.length).toBeGreaterThan(0)
      for (const unlisten of unlistens) expect(unlisten).toHaveBeenCalledOnce()
      expect(scan.isScanning).toBe(false)
      expect(scan.previewId).toBeNull()
    })

    it('frees its late preview and leaves the scan a toggle back to Copy started alone', async () => {
      const overtaken = deferred<{ previewId: string }>()
      vi.mocked(commands.startScanPreview)
        .mockReturnValueOnce(overtaken.promise)
        .mockResolvedValueOnce({ previewId: 'preview-current' })
      registrations.resolve(undefined)
      scan.start()
      await vi.waitFor(() => {
        expect(commands.startScanPreview).toHaveBeenCalledOnce()
      })

      setSameVolumeMove(true)
      setSameVolumeMove(false)
      await vi.waitFor(() => {
        expect(commands.startScanPreview).toHaveBeenCalledTimes(2)
      })
      await scan.scanStarted
      expect(scan.previewId).toBe('preview-current')

      overtaken.resolve({ previewId: 'preview-overtaken' })
      await settle()

      expect(commands.cancelScanPreview).toHaveBeenCalledWith('preview-overtaken')
      expect(commands.cancelScanPreview).not.toHaveBeenCalledWith('preview-current')
      expect(commands.checkScanPreviewStatus).not.toHaveBeenCalledWith('preview-overtaken')
      expect(scan.previewId).toBe('preview-current')
    })
  })
})
