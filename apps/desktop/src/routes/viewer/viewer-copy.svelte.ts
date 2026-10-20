/**
 * Selection-to-clipboard composable for the viewer.
 *
 * Owns the three-band copy flow (silent / confirm / refuse), the busy flag that
 * prevents overlapping copies, and the in-flight `read_id` used to cancel a long
 * read via Escape.
 *
 * The page component wires this in: it provides `getSessionId`, `getSelectionBytes`,
 * and `getRangeEnds` (the current selection's anchor/focus expressed as `RangeEnd`s
 * for the IPC layer). On a copy gesture, the page calls `runCopy()` which picks the
 * right band and returns a `CopyOutcome`. Toast/dialog presentation lives in the page
 * to keep this module independent of UI primitives.
 */
import {
  viewerCancelRead,
  viewerReadRange,
  viewerWriteRangeToFile,
  type RangeEnd,
  type SavedAs,
  type ViewerError,
} from '#lib/tauri-commands/index.js'
import { save as showSavePanel } from '@tauri-apps/plugin-dialog'
import { addToast } from '#lib/ui/toast/toast-store.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { selectCopyAction, type CopyAction } from './viewer-copy'
import { formatByteSize } from '#lib/units/index.js'

export type CopyOutcome =
  | { kind: 'silent'; text: string; bytes: number }
  | { kind: 'silent-error'; bytes: number; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }
  | { kind: 'confirm'; bytes: number; proceed: () => Promise<CopyResult> }
  | { kind: 'refuse'; bytes: number }
  | { kind: 'unknown-size'; proceed: () => Promise<CopyResult>; bytes: null }
  | { kind: 'empty' }
  | { kind: 'busy' }

/** A range IPC that didn't finish, with the typed reason the caller words. */
export type RangeFailure = { ok: false; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }

export type CopyResult = { ok: true; text: string } | RangeFailure

/** A finished save says how it wrote the file: UTF-8, or the source's own encoding. */
export type SaveResult = { ok: true; savedAs: SavedAs } | RangeFailure

/** The resolved arguments every range IPC takes (one payload, since the two ends share a type). */
interface RangeCall {
  sessionId: string
  readId: number
  anchor: RangeEnd
  focus: RangeEnd
}

interface CopyDeps {
  getSessionId: () => string
  /**
   * Returns the byte estimate of the current selection, or `null` if the size can't be
   * estimated (ByteSeek-no-index in a range we haven't fetched). The composable treats
   * `null` as "ask before paying for the read"; the page can refuse outright if the
   * caller prefers.
   */
  getSelectionBytes: () => number | null
  /**
   * Returns the current selection's endpoints as `RangeEnd`s for the IPC layer, or
   * `null` if the selection is empty. The composable doesn't reach into the selection
   * model directly; the page maps `Selection | null` -> `(anchor, focus): RangeEnd`.
   */
  getRangeEnds: () => { anchor: RangeEnd; focus: RangeEnd } | null
}

export function createViewerCopy(deps: CopyDeps) {
  let busy = $state(false)
  let inFlightReadId = $state<number | null>(null)

  // Monotonic per-composable counter; only needs uniqueness within a session.
  let nextReadId = 0

  /** Picks the next `read_id` to send with `viewerReadRange`. */
  function allocateReadId(): number {
    const id = nextReadId
    nextReadId += 1
    return id
  }

  /**
   * Shared plumbing for the two range IPCs: resolves the session and selection, holds
   * the busy flag and in-flight `read_id` for the duration, and maps the typed
   * `ViewerError` to a `CopyResult` reason.
   */
  async function runRangeIpc<Done extends { ok: true }>(
    call: (range: RangeCall) => Promise<Done | { ok: false; error: ViewerError }>,
  ): Promise<Done | RangeFailure> {
    const sessionId = deps.getSessionId()
    const ends = deps.getRangeEnds()
    if (!sessionId || ends === null) {
      return { ok: false, reason: 'other' }
    }
    const readId = allocateReadId()
    inFlightReadId = readId
    busy = true
    try {
      const res = await call({ sessionId, readId, anchor: ends.anchor, focus: ends.focus })
      if (res.ok) return res
      if (res.error.kind === 'cancelled') return { ok: false, reason: 'cancelled', error: res.error }
      if (res.error.kind === 'timedOut') return { ok: false, reason: 'timedOut', error: res.error }
      return { ok: false, reason: 'other', error: res.error }
    } finally {
      busy = false
      inFlightReadId = null
    }
  }

  function performRead(): Promise<CopyResult> {
    return runRangeIpc(({ sessionId, readId, anchor, focus }) => viewerReadRange(sessionId, readId, anchor, focus))
  }

  /**
   * Picks the right copy band and either reads immediately (silent) or returns a
   * `proceed()` for the caller to drive after user confirmation.
   *
   * Empty selections return `{ kind: 'empty' }` (caller should no-op). A second copy
   * gesture while one is in flight returns `{ kind: 'busy' }` (caller should no-op).
   */
  async function runCopy(): Promise<CopyOutcome> {
    if (busy) return { kind: 'busy' }
    const ends = deps.getRangeEnds()
    if (ends === null) return { kind: 'empty' }

    const bytes = deps.getSelectionBytes()
    if (bytes === null) {
      // Unknown size: ask before paying. Caller decides how to render the confirm.
      return { kind: 'unknown-size', bytes: null, proceed: performRead }
    }

    const action: CopyAction = selectCopyAction(bytes)
    if (action === 'silent') {
      const result = await performRead()
      if (result.ok) return { kind: 'silent', text: result.text, bytes }
      // Surface the failure to the caller. Per design-principles top-5 §3
      // ("communicate what's actually happening"), a silent-band IO error gets a
      // brief toast instead of pretending nothing happened. The caller suppresses
      // the toast for `cancelled` (the user pressed Escape, intentional).
      return { kind: 'silent-error', bytes, reason: result.reason, error: result.error }
    }
    if (action === 'refuse') return { kind: 'refuse', bytes }
    return { kind: 'confirm', bytes, proceed: performRead }
  }

  /**
   * Cancels the in-flight read, if any. Safe to call when no read is running (no-op).
   * Returns immediately; the actual cancel propagates through the cancel flag on the
   * backend.
   */
  async function cancelInFlight(): Promise<void> {
    const sessionId = deps.getSessionId()
    const readId = inFlightReadId
    if (!sessionId || readId === null) return
    await viewerCancelRead(sessionId, readId)
  }

  /**
   * Writes the selection to a file at `destPath`. Same band-agnostic API as `runCopy`,
   * but the result is the typed write outcome (including how the file was encoded)
   * rather than the read text. Uses the same busy / cancel plumbing.
   */
  function saveAs(destPath: string): Promise<SaveResult> {
    return runRangeIpc(({ sessionId, readId, anchor, focus }) =>
      viewerWriteRangeToFile(sessionId, readId, anchor, focus, destPath),
    )
  }

  return {
    get busy() {
      return busy
    },
    get inFlightReadId() {
      return inFlightReadId
    },
    runCopy,
    cancelInFlight,
    saveAs,
  }
}

type ViewerCopy = ReturnType<typeof createViewerCopy>

interface CopyOrchestratorDeps {
  /** The lower-level read/write copy composable (`createViewerCopy`). */
  copy: ViewerCopy
  /** The open file's name, used to build the "save as" default file name. */
  getFileName: () => string
}

/**
 * Copy-flow orchestration for the viewer: turns a copy gesture into the right
 * clipboard write, confirm/refuse dialog, or "save as" panel, and surfaces toasts.
 *
 * Owns the dialog state the page binds to `ViewerCopyDialogs` (`confirmBytes`,
 * `refuseBytes`) plus the deferred `proceed` callback for a confirmed read. Wraps the
 * lower-level `createViewerCopy` read/write composable; the page calls `handleCopy()`
 * on ⌘C / context-menu Copy and `handleSaveAs()` from the dialog's "Save as" action.
 *
 * Lives in `.svelte.ts` because `confirmBytes` / `refuseBytes` are `$state`.
 */
export function createViewerCopyOrchestrator(deps: CopyOrchestratorDeps) {
  const log = getAppLogger('viewer')
  const { copy } = deps

  /** Whether a copy confirm dialog (10 to 100 MiB band) is showing. */
  let confirmBytes = $state<number | null>(null)
  let confirmProceed: (() => Promise<void>) | null = null
  /** Whether the > 100 MiB refuse dialog is showing. */
  let refuseBytes = $state<number | null>(null)

  async function writeToClipboard(text: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(text)
      return true
    } catch (e) {
      log.warn('Clipboard write rejected: {error}', { error: e })
      return false
    }
  }

  /**
   * Writes `text` to the clipboard and says how much landed there.
   *
   * ❗ The size is MEASURED off the text, never taken from the estimate that picked the
   * band: the estimate prorates a partial line by its UTF-16 fraction and short-circuits
   * a whole-file selection to the file size, so the two differ by design. A number in
   * front of the user is a claim about their data (spec invariant I3), and this is the
   * only place that holds the real text.
   */
  async function handleSilentCopy(text: string): Promise<void> {
    const ok = await writeToClipboard(text)
    if (!ok) {
      addToast(tString('viewer.copy.clipboardUnreachable'), { level: 'warn' })
      return
    }
    const bytes = new TextEncoder().encode(text).length
    addToast(tString('viewer.copy.onClipboard', { size: formatByteSize(bytes) }), { level: 'info' })
  }

  async function handleCopy(): Promise<void> {
    const outcome = await copy.runCopy()
    switch (outcome.kind) {
      case 'empty':
      case 'busy':
        return
      case 'silent':
        await handleSilentCopy(outcome.text)
        return
      case 'silent-error':
        if (outcome.reason === 'cancelled') return // user pressed Escape, intentional
        log.warn('Silent-band copy read failed: reason={reason}, error={error}', {
          reason: outcome.reason,
          error: outcome.error ? JSON.stringify(outcome.error) : 'none',
        })
        if (outcome.reason === 'timedOut') {
          addToast(tString('viewer.copy.readTooLong'), { level: 'warn' })
        } else {
          addToast(tString('viewer.copy.copyFailed'), { level: 'warn' })
        }
        return
      case 'confirm':
        confirmBytes = outcome.bytes
        confirmProceed = async () => {
          confirmBytes = null
          const res = await outcome.proceed()
          if (res.ok) {
            await handleSilentCopy(res.text)
          } else if (res.reason === 'cancelled') {
            // User pressed Escape; no toast.
          } else if (res.reason === 'timedOut') {
            addToast(tString('viewer.copy.readTooLong'), { level: 'warn' })
          } else {
            addToast(tString('viewer.copy.readFailed'), { level: 'warn' })
          }
        }
        return
      case 'unknown-size':
        // ByteSeek-no-index range we never scrolled through. Same UX as confirm,
        // but with a hint that we don't know the size yet.
        confirmBytes = -1
        confirmProceed = async () => {
          confirmBytes = null
          const res = await outcome.proceed()
          if (res.ok) {
            await handleSilentCopy(res.text)
          }
        }
        return
      case 'refuse':
        refuseBytes = outcome.bytes
        return
    }
  }

  function cancelConfirm(): void {
    confirmBytes = null
    confirmProceed = null
  }

  function dismissRefuse(): void {
    refuseBytes = null
  }

  /** Runs the deferred read for a confirmed copy, if a confirm dialog is open. */
  function proceedConfirm(): void {
    if (confirmProceed) void confirmProceed()
  }

  /**
   * Save as file flow: opens the native macOS save panel via the Tauri dialog plugin
   * with a sensible default name (the open file's stem + ".selection.txt"), then
   * streams the selection to the chosen path via `viewer_write_range_to_file`. The
   * backend reads and writes it in bounded chunks, so this stays the way out of the
   * clipboard's size refusal however large the selection is.
   * Dismisses the open copy dialog and shows a success toast on completion.
   */
  async function handleSaveAs(): Promise<void> {
    const fileName = deps.getFileName()
    const defaultName = `${fileName.replace(/\.[^.]*$/, '') || tString('viewer.saveAs.defaultName')}.selection.txt`
    let chosen: string | null
    try {
      chosen = await showSavePanel({ defaultPath: defaultName, title: tString('viewer.saveAs.title') })
    } catch (e) {
      log.warn('Save panel rejected: {error}', { error: e })
      addToast(tString('viewer.saveAs.panelFailed'), { level: 'warn' })
      return
    }
    if (chosen === null) return // user cancelled

    // Close the open copy dialog so the user can see progress.
    confirmBytes = null
    confirmProceed = null
    refuseBytes = null

    const res = await copy.saveAs(chosen)
    if (res.ok) {
      const name = chosen.split('/').pop() ?? chosen
      // A save in the source's own encoding isn't the UTF-8 text the user may expect, so say why.
      const key = res.savedAs.kind === 'sourceEncoding' ? 'viewer.saveAs.savedInSourceEncoding' : 'viewer.saveAs.saved'
      addToast(tString(key, { name }), { level: 'info' })
    } else if (res.reason === 'cancelled') {
      // No toast; the user pressed Escape.
    } else if (res.reason === 'timedOut') {
      addToast(tString('viewer.saveAs.tooLong'), { level: 'warn' })
    } else if (res.error?.kind === 'destinationIsReadOnly') {
      // A path a route serves (inside a `.zip`, or inside a repo's `.git`
      // history) has no folder on disk to land in. Naming that is the whole
      // difference between a user who moves on and one who retries forever.
      addToast(tString('viewer.saveAs.destinationReadOnly'), { level: 'warn' })
    } else {
      addToast(tString('viewer.saveAs.saveFailed'), { level: 'warn' })
    }
  }

  return {
    get confirmBytes() {
      return confirmBytes
    },
    get refuseBytes() {
      return refuseBytes
    },
    /** Whether a copy confirm dialog (10 to 100 MiB band) is currently showing. */
    get isConfirmOpen() {
      return confirmBytes !== null
    },
    /** Whether the > 100 MiB refuse dialog is currently showing. */
    get isRefuseOpen() {
      return refuseBytes !== null
    },
    handleCopy,
    handleSaveAs,
    cancelConfirm,
    dismissRefuse,
    proceedConfirm,
  }
}
