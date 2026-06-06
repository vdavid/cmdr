/**
 * Reactive scan-preview orchestration lifted out of `TransferDialog.svelte`.
 *
 * Owns the deep recursive scan preview that feeds the dialog's Size bar and
 * file/dir tallies: the live counters, the four scan-progress Tauri listeners,
 * the `startScan()` / `cancelPreview()` lifecycle, and the Copy/Move toggle
 * `$effect` that (re)starts or cancels the preview when the user flips to or
 * away from a same-volume move.
 *
 * The factory takes its reactive inputs (source paths, sort info, source volume
 * id, the `isSameVolumeMove` gate, and the dialog's `confirmed` / `destroyed`
 * flags) via getter callbacks, matching the codebase's established factory
 * pattern (`createTypeToJumpState`, `createDragDropController`). State is exposed
 * through getters; the dialog reads them in its markup and derivations.
 *
 * ## Why the same-volume gating lives here
 *
 * A same-volume move is a server-side rename (zero bytes), so the deep byte scan
 * is pure waste — on a NAS it used to cost 30–40 s of "Verifying before move…".
 * `start()` runs the initial scan only when NOT a same-volume move; the toggle
 * `$effect` handles LATER flips. The `$effect` is created synchronously in the
 * factory body (the `initListingDiffSync` pattern, landmine L3) so it lands in
 * the component's effect-tracking context — it must NOT move into `start()`.
 *
 * The `DEFAULT_VOLUME_ID` exclusion that defines `isSameVolumeMove` is owned by
 * the dialog (it's a derivation over props), not this factory; the factory only
 * reacts to the boolean.
 */

import {
  startScanPreview,
  cancelScanPreview,
  checkScanPreviewStatus,
  onScanPreviewProgress,
  onScanPreviewComplete,
  onScanPreviewError,
  onScanPreviewCancelled,
  type UnlistenFn,
} from '$lib/tauri-commands'
import type { SortColumn, SortOrder } from '$lib/file-explorer/types'
import { getSetting } from '$lib/settings'

export interface TransferScanStateDeps {
  /** Source paths to scan (static prop). */
  getSourcePaths: () => string[]
  /** Current sort column on the source pane (for scan-preview ordering). */
  getSortColumn: () => SortColumn
  /** Current sort order on the source pane. */
  getSortOrder: () => SortOrder
  /** Real source volume id, forwarded so the scan stats the right volume. */
  getSourceVolumeId: () => string
  /**
   * Whether the active operation is a same-volume move (server-side rename, zero
   * bytes). Reactive: read inside the toggle `$effect` so a Copy/Move flip
   * (re)starts or cancels the preview. The `DEFAULT_VOLUME_ID` exclusion is
   * folded into this boolean by the dialog.
   */
  getIsSameVolumeMove: () => boolean
  /** Whether the user has confirmed (so the toggle effect doesn't restart a scan mid-confirm). */
  getConfirmed: () => boolean
  /** Whether the dialog is being destroyed (so the toggle effect doesn't restart a scan on teardown). */
  getDestroyed: () => boolean
}

export function createTransferScanState(deps: TransferScanStateDeps) {
  // Scan preview state
  let previewId = $state<string | null>(null)
  let filesFound = $state(0)
  let dirsFound = $state(0)
  // `bytesFound` is the write footprint (what the copy writes). `dedupBytesFound`
  // is the `du`-equivalent source size; the two differ only when the source has
  // hardlinks (cargo `target/`, Time Machine, deduped backups), in which case the
  // dialog shows a one-line note clarifying the gap.
  let bytesFound = $state(0)
  let dedupBytesFound = $state(0)
  let isScanning = $state(false)
  let scanComplete = $state(false)
  let unlisteners: UnlistenFn[] = []

  // Promise that resolves once startScanPreview IPC has returned and previewId is set.
  // handleConfirm awaits this to guarantee previewId is non-null when passed to
  // TransferProgressDialog, otherwise a fast confirm races with IPC and leaves the
  // progress dialog stuck in "Scanning 0 files" forever. Kept outside $state — it's a
  // handle the confirm path awaits, not a rendered value.
  let scanStarted: Promise<void> = Promise.resolve()

  /** Cleans up event listeners for scan preview. */
  function cleanup() {
    for (const unlisten of unlisteners) {
      unlisten()
    }
    unlisteners = []
  }

  /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
  function isOurScanEvent(eventPreviewId: string): boolean {
    // Don't accept events until we know our previewId from the IPC return.
    // This prevents adopting stale events from previous orphaned scans.
    if (!previewId) return false
    return eventPreviewId === previewId
  }

  /** Starts the scan preview to count files/dirs/bytes. */
  async function startScan() {
    // Subscribe to events BEFORE starting scan (avoid missing fast completions)
    unlisteners.push(
      await onScanPreviewProgress((event) => {
        if (!isOurScanEvent(event.previewId)) return
        filesFound = event.filesFound
        dirsFound = event.dirsFound
        bytesFound = event.bytesFound
      }),
    )
    unlisteners.push(
      await onScanPreviewComplete((event) => {
        if (!isOurScanEvent(event.previewId)) return
        filesFound = event.filesTotal
        dirsFound = event.dirsTotal
        bytesFound = event.bytesTotal
        dedupBytesFound = event.dedupBytesTotal
        isScanning = false
        scanComplete = true
      }),
    )
    unlisteners.push(
      await onScanPreviewError((event) => {
        if (!isOurScanEvent(event.previewId)) return
        isScanning = false
        // Keep showing whatever stats we have
      }),
    )
    unlisteners.push(
      await onScanPreviewCancelled((event) => {
        if (!isOurScanEvent(event.previewId)) return
        isScanning = false
      }),
    )

    // Start the scan
    isScanning = true
    const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
    const result = await startScanPreview(
      deps.getSourcePaths(),
      deps.getSortColumn(),
      deps.getSortOrder(),
      progressIntervalMs,
      deps.getSourceVolumeId(),
    )
    previewId = result.previewId

    // Check if the scan already completed while we were awaiting the IPC return.
    // Events that arrived before previewId was set were dropped (isOurScanEvent returned false),
    // so we need to read the backend's cached totals and hydrate the dialog from them.
    // Without this, M2a's watcher-backed oracle (a ~5 ms scan) lands its events
    // before we register listeners and the dialog shows "✓ 0 files" forever.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- may have changed during await
    if (isScanning) {
      const totals = await checkScanPreviewStatus(previewId)
      if (totals) {
        filesFound = totals.filesTotal
        dirsFound = totals.dirsTotal
        bytesFound = totals.bytesTotal
        dedupBytesFound = totals.dedupBytesTotal
        isScanning = false
        scanComplete = true
      }
    }
  }

  /** Cancels the in-flight deep scan preview and resets its state, without
   *  touching the (independent) conflict check. Used when the user flips to a
   *  same-volume Move, where the deep byte scan is waste — the move is a
   *  rename. Idempotent: a no-op when no preview is running. */
  function cancelPreview() {
    if (previewId) {
      void cancelScanPreview(previewId)
    }
    cleanup() // drop the scan-preview listeners
    previewId = null
    isScanning = false
    scanComplete = false
    filesFound = 0
    dirsFound = 0
    bytesFound = 0
    dedupBytesFound = 0
    scanStarted = Promise.resolve()
  }

  /** Starts the initial scan unless this is a same-volume move (server-side
   *  rename, zero bytes — the deep recursive byte scan is pure waste). Called
   *  from the dialog's `onMount`. Tracks the promise via `scanStarted` so the
   *  confirm path can await it (ensures `previewId` is set before `onConfirm`). */
  function start() {
    if (!deps.getIsSameVolumeMove()) {
      scanStarted = startScan()
    }
  }

  /** Frees the scan preview (cancels an in-flight scan and evicts any cached
   *  `CachedScanResult`) and drops the listeners. Called on plain cancel/dismiss.
   *  Regardless of `isScanning`, so a dismiss after the scan completed doesn't
   *  leak the cache. When the user CONFIRMED, the caller uses `cleanup()` instead
   *  so the progress dialog can consume the still-needed preview. */
  function freeAndCleanup() {
    if (previewId) {
      void cancelScanPreview(previewId)
    }
    cleanup()
  }

  // Copy/Move toggle gating for same-volume moves. `startScan()` runs once via
  // `start()` for the initial operation; this effect handles LATER toggles:
  //  - flip to a same-volume Move → cancel the deep recursive preview (a
  //    rename moves zero bytes, so there's nothing for the Size bar to show).
  //  - flip away (to Copy, or to a cross-volume Move) → (re)start the preview,
  //    because Copy genuinely needs byte totals for its Size bar.
  // The conflict check is independent, so it's unaffected. Created synchronously
  // in the factory body (L3) so it lands in the component's effect-tracking
  // context — NOT inside start()/onMount.
  let toggleEffectInitialized = false
  $effect(() => {
    // Track the reactive inputs.
    const sameVolumeMove = deps.getIsSameVolumeMove()
    if (!toggleEffectInitialized) {
      // Skip the first run: `start()` owns the initial scan/skip decision.
      toggleEffectInitialized = true
      return
    }
    if (sameVolumeMove) {
      cancelPreview()
    } else if (!previewId && !deps.getConfirmed() && !deps.getDestroyed()) {
      // No preview running and we're back on a path that needs one: start it.
      scanStarted = startScan()
    }
  })

  return {
    start,
    cancelPreview,
    freeAndCleanup,
    /** Drops the scan-preview listeners WITHOUT cancelling the preview. Used on
     *  destroy after the user confirmed: the progress dialog consumes the scan. */
    cleanup,
    /** Awaitable handle: resolves once `startScanPreview` IPC has returned and `previewId` is set. */
    get scanStarted() {
      return scanStarted
    },
    get previewId() {
      return previewId
    },
    get filesFound() {
      return filesFound
    },
    get dirsFound() {
      return dirsFound
    },
    get bytesFound() {
      return bytesFound
    },
    get dedupBytesFound() {
      return dedupBytesFound
    },
    get isScanning() {
      return isScanning
    },
    get scanComplete() {
      return scanComplete
    },
  }
}
