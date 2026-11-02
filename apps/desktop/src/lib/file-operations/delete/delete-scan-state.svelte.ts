/**
 * Reactive scan-preview orchestration lifted out of `DeleteDialog.svelte`, the
 * way `transfer-scan-state.svelte.ts` serves `TransferDialog`.
 *
 * Owns the walk that feeds the dialog's tallies: the live counters and
 * throughput, the four scan-progress Tauri listeners, the `start()` lifecycle,
 * and the online-only answer a cloud folder's walk gives (`onlineOnlyFoundByWalk`
 * plus the promise confirm waits on). The dialog keeps the confirm flow, the
 * Shift-hold wiring, and the decision of when to free the preview (it alone
 * knows whether the user confirmed).
 *
 * Inputs come through getter callbacks, matching `createTransferScanState`, so
 * the start can read the dialog's props before its first await (see `startScan`).
 */

import {
  startScanPreview,
  cancelScanPreview,
  onScanPreviewProgress,
  onScanPreviewComplete,
  onScanPreviewError,
  onScanPreviewCancelled,
  type UnlistenFn,
} from '#lib/tauri-commands/index.js'
import { getSetting } from '#lib/settings/index.js'
import { formatFilesPerSecond } from '#lib/units/index.js'
import { withTimeout } from '#lib/utils/timing.js'
import { ScanThroughput } from '../scan-throughput'
import { holdOperationStartReply } from '../transfer/transfer-dispatch'
import type { CloudOnlineOnlyExtent } from './delete-dialog-utils'

/** ❗ The safe default when no answer ever lands is the TRASH, which is
 *  today's behavior. The scan watchdog already bounds the walk by inactivity,
 *  so this only covers a scan whose events never reach us at all; it's
 *  generous because a real walk of a big tree legitimately takes a while, and
 *  giving up early would run exactly the download we're trying to avoid. */
const ONLINE_ONLY_ANSWER_TIMEOUT_MS = 90_000

export interface DeleteScanStateDeps {
  getSourcePaths: () => string[]
  /** Routes the walk through the Volume trait for non-local volumes (MTP, SMB). */
  getSourceVolumeId: () => string
  /** Whether the dialog has unmounted. The start can outlive it. */
  getDestroyed: () => boolean
}

export function createDeleteScanState(deps: DeleteScanStateDeps) {
  /** The scan walk met an online-only file inside a selected folder. Flips the
   *  dialog live: the banner appears, the trash switch goes, and the confirm
   *  button becomes the permanent delete, all before anything is pressed.
   *
   *  ❗ Always `'mixed'`, never `'all'`. The walk reports one boolean for the
   *  whole folder, so a hit says "something in here is online-only" and says
   *  nothing about the rest — and the folder holding one evicted file beside a
   *  hundred ordinary ones is the ordinary case. The mixed wording is true
   *  either way; the other one would tell someone every file they picked is
   *  online-only on the strength of a single hit. */
  let onlineOnlyFoundByWalk = $state<CloudOnlineOnlyExtent | null>(null)

  let previewId = $state<string | null>(null)
  /** Resolves once `startScanPreview` has answered and `previewId` is set.
   *  Confirm awaits it, for the same reason `TransferDialog` awaits
   *  `scan.scanStarted`: dispatching with a null `previewId` leaves the
   *  operation nothing to claim, so it re-walks the tree CONCURRENTLY with
   *  the preview `startScan` already began, and that orphaned walk has no
   *  owner and nothing to cancel it (teardown's cleanup is gated on
   *  `!confirmed`, and confirming sets that before the id ever arrives). */
  let scanStarted: Promise<void> = Promise.resolve()
  /** Resolves once the walk has said whether anything in the selection is
   *  online-only: on the first progress tick reporting one (a hit is the whole
   *  answer, so a huge folder needn't finish counting), on the completion that
   *  reports none, or on any terminal scan event. Awaited ONLY when
   *  `cloudFolderMayHoldOnlineOnly`; everywhere else confirm stays instant. */
  let settleOnlineOnlyAnswer: () => void = () => {}
  const onlineOnlyAnswer = new Promise<void>((resolve) => {
    settleOnlineOnlyAnswer = resolve
  })
  let filesFound = $state(0)
  let dirsFound = $state(0)
  let bytesFound = $state(0)
  let isScanning = $state(false)
  let scanComplete = $state(false)
  let currentDir = $state<string | null>(null)
  const throughput = new ScanThroughput()
  let filesPerSec = $state<number | null>(null)
  let bytesPerSec = $state<number | null>(null)

  /** The walk's speed, through the one files-per-second policy the transfer
   *  bars use, so a slow scan reads "0.4 files/s" instead of the "0 files/s" a
   *  bare `Math.round` produced. `null` once it rounds to nothing, which is
   *  also what hides the line. */
  const scanRate = $derived(filesPerSec === null ? null : formatFilesPerSecond(filesPerSec))
  let unlisteners: UnlistenFn[] = []

  /** Accepts the event if it belongs to our scan, filtering stale events from previous scans. */
  function isOurScanEvent(eventPreviewId: string): boolean {
    if (!previewId) previewId = eventPreviewId
    return eventPreviewId === previewId
  }

  /** Keeps a scan listener for `cleanup`, or hands it straight back when the dialog closed while it registered. */
  function keepListener(unlisten: UnlistenFn): void {
    if (deps.getDestroyed()) unlisten()
    else unlisteners.push(unlisten)
  }

  /**
   * Starts the scan preview to count files/dirs/bytes.
   *
   * It can outlive the dialog: a quick Escape or an MCP close unmounts it while the listeners
   * are still registering. So it reads its inputs BEFORE the first await, because the getters
   * read the dialog's props, both parents (`DialogManager`, `DialogGallery`) null their props
   * object on close, and a prop read after that throws. Past each await, a closed dialog keeps
   * nothing: late listeners go straight back, no scan starts, and a preview whose id lands
   * after teardown is freed here, since teardown had no id to free.
   */
  async function startScan() {
    const request = {
      sourcePaths: deps.getSourcePaths(),
      sourceVolumeId: deps.getSourceVolumeId(),
    }
    // Subscribe to events BEFORE starting scan (avoid missing fast completions)
    keepListener(
      await onScanPreviewProgress((event) => {
        if (!isOurScanEvent(event.previewId)) return
        filesFound = event.filesFound
        dirsFound = event.dirsFound
        bytesFound = event.bytesFound
        currentDir = event.currentDir ?? null
        if (event.onlineOnlyFound) {
          onlineOnlyFoundByWalk = 'mixed'
          settleOnlineOnlyAnswer()
        }
        const r = throughput.push({
          timestampMs: Date.now(),
          files: event.filesFound,
          bytes: event.bytesFound,
        })
        filesPerSec = r.filesPerSecond
        bytesPerSec = r.bytesPerSecond
      }),
    )
    keepListener(
      await onScanPreviewComplete((event) => {
        if (!isOurScanEvent(event.previewId)) return
        filesFound = event.filesTotal
        dirsFound = event.dirsTotal
        bytesFound = event.bytesTotal
        if (event.onlineOnlyFound) onlineOnlyFoundByWalk = 'mixed'
        isScanning = false
        scanComplete = true
        // A finished walk is the definitive answer either way.
        settleOnlineOnlyAnswer()
      }),
    )
    keepListener(
      await onScanPreviewError((event) => {
        if (!isOurScanEvent(event.previewId)) return
        isScanning = false
        // Keep showing whatever stats we have. A walk that stopped can't
        // answer, so confirm falls back to the trash.
        settleOnlineOnlyAnswer()
      }),
    )
    keepListener(
      await onScanPreviewCancelled((event) => {
        if (!isOurScanEvent(event.previewId)) return
        isScanning = false
        settleOnlineOnlyAnswer()
      }),
    )

    if (deps.getDestroyed()) return

    // Start the scan
    isScanning = true
    const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
    const result = await startScanPreview(request.sourcePaths, 'delete', progressIntervalMs, request.sourceVolumeId)
    if ('refusal' in result) {
      // No volume answers for the source (an unplugged phone): nothing was
      // walked and no preview exists. Same as a walk that stopped; the
      // delete itself is refused typed if it's confirmed.
      isScanning = false
      settleOnlineOnlyAnswer()
      return
    }
    if (deps.getDestroyed()) {
      void cancelScanPreview(result.previewId)
      return
    }
    previewId = result.previewId
  }

  /** Starts the walk. Called from the dialog's `onMount`; confirm awaits `scanStarted`. */
  function start() {
    scanStarted = startScan()
  }

  /** Waits for the walk to say whether anything is online-only. An agent waiting
   *  on this press for its operation id (a `dialog_confirm`'s `mcpRequestId`)
   *  would time out on its short start budget meanwhile,
   *  so it's told to hold for as long as this may wait. */
  async function awaitOnlineOnlyAnswer(waitingAgent: string | undefined): Promise<void> {
    if (waitingAgent !== undefined) holdOperationStartReply(waitingAgent, ONLINE_ONLY_ANSWER_TIMEOUT_MS)
    await withTimeout(onlineOnlyAnswer, ONLINE_ONLY_ANSWER_TIMEOUT_MS, undefined)
  }

  /** Frees the scan preview: cancels an in-flight walk and evicts any cached
   *  result, so a dismiss AFTER the scan completed doesn't leak the cache until
   *  quit. The caller skips it once the user confirmed, since the op consumes it. */
  function freePreview() {
    if (previewId) {
      void cancelScanPreview(previewId)
    }
  }

  /** Drops the scan listeners WITHOUT freeing the preview. */
  function cleanup() {
    for (const unlisten of unlisteners) {
      unlisten()
    }
    unlisteners = []
  }

  return {
    start,
    awaitOnlineOnlyAnswer,
    freePreview,
    cleanup,
    /** Releases a confirm waiting on the online-only answer (teardown: nothing may await it). */
    settleOnlineOnlyAnswer: () => {
      settleOnlineOnlyAnswer()
    },
    get scanStarted() {
      return scanStarted
    },
    get previewId() {
      return previewId
    },
    /** `'mixed'` once the walk met an online-only file, else `null`. */
    get onlineOnlyFoundByWalk() {
      return onlineOnlyFoundByWalk
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
    get isScanning() {
      return isScanning
    },
    get scanComplete() {
      return scanComplete
    },
    get currentDir() {
      return currentDir
    },
    get scanRate() {
      return scanRate
    },
    get bytesPerSec() {
      return bytesPerSec
    },
  }
}
