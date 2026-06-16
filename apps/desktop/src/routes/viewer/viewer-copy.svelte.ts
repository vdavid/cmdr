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
  formatBytes,
  viewerCancelRead,
  viewerReadRange,
  viewerWriteRangeToFile,
  type RangeEnd,
  type ViewerError,
} from '$lib/tauri-commands'
import { save as showSavePanel } from '@tauri-apps/plugin-dialog'

import { addToast } from '$lib/ui/toast/toast-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'

import { selectCopyAction, type CopyAction } from './viewer-copy'

export type CopyOutcome =
  | { kind: 'silent'; text: string; bytes: number }
  | { kind: 'silent-error'; bytes: number; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }
  | { kind: 'confirm'; bytes: number; proceed: () => Promise<CopyResult> }
  | { kind: 'refuse'; bytes: number }
  | { kind: 'unknown-size'; proceed: () => Promise<CopyResult>; bytes: null }
  | { kind: 'empty' }
  | { kind: 'busy' }

export type CopyResult =
  | { ok: true; text: string }
  | { ok: false; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }

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

  async function performRead(): Promise<CopyResult> {
    const sessionId = deps.getSessionId()
    const ends = deps.getRangeEnds()
    if (!sessionId || ends === null) {
      return { ok: false, reason: 'other' }
    }
    const readId = allocateReadId()
    inFlightReadId = readId
    busy = true
    try {
      const res = await viewerReadRange(sessionId, readId, ends.anchor, ends.focus)
      if (res.ok) return { ok: true, text: res.text }
      if (res.error.kind === 'cancelled') return { ok: false, reason: 'cancelled', error: res.error }
      if (res.error.kind === 'timedOut') return { ok: false, reason: 'timedOut', error: res.error }
      return { ok: false, reason: 'other', error: res.error }
    } finally {
      busy = false
      inFlightReadId = null
    }
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
   * but the result is the typed write outcome rather than the read text. Uses the
   * same busy / cancel plumbing.
   */
  async function saveAs(destPath: string): Promise<CopyResult> {
    const sessionId = deps.getSessionId()
    const ends = deps.getRangeEnds()
    if (!sessionId || ends === null) {
      return { ok: false, reason: 'other' }
    }
    const readId = allocateReadId()
    inFlightReadId = readId
    busy = true
    try {
      const res = await viewerWriteRangeToFile(sessionId, readId, ends.anchor, ends.focus, destPath)
      if (res.ok) return { ok: true, text: '' }
      if (res.error.kind === 'cancelled') return { ok: false, reason: 'cancelled', error: res.error }
      if (res.error.kind === 'timedOut') return { ok: false, reason: 'timedOut', error: res.error }
      return { ok: false, reason: 'other', error: res.error }
    } finally {
      busy = false
      inFlightReadId = null
    }
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
      log.warn('Clipboard write rejected: {error}', { error: String(e) })
      return false
    }
  }

  async function handleSilentCopy(text: string, bytes: number): Promise<void> {
    const ok = await writeToClipboard(text)
    if (!ok) {
      addToast(tString('viewer.copy.clipboardUnreachable'), { level: 'warn' })
      return
    }
    addToast(tString('viewer.copy.onClipboard', { size: formatBytes(bytes) }), { level: 'info' })
  }

  async function handleCopy(): Promise<void> {
    const outcome = await copy.runCopy()
    switch (outcome.kind) {
      case 'empty':
      case 'busy':
        return
      case 'silent':
        await handleSilentCopy(outcome.text, outcome.bytes)
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
            await handleSilentCopy(res.text, outcome.bytes)
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
            const bytes = new TextEncoder().encode(res.text).length
            await handleSilentCopy(res.text, bytes)
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
   * streams the selection to the chosen path via `viewer_write_range_to_file`.
   * Dismisses the open copy dialog and shows a success toast on completion.
   */
  async function handleSaveAs(): Promise<void> {
    const fileName = deps.getFileName()
    const defaultName = `${fileName.replace(/\.[^.]*$/, '') || tString('viewer.saveAs.defaultName')}.selection.txt`
    let chosen: string | null
    try {
      chosen = await showSavePanel({ defaultPath: defaultName, title: tString('viewer.saveAs.title') })
    } catch (e) {
      log.warn('Save panel rejected: {error}', { error: String(e) })
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
      addToast(tString('viewer.saveAs.saved', { name: chosen.split('/').pop() ?? chosen }), { level: 'info' })
    } else if (res.reason === 'cancelled') {
      // No toast; the user pressed Escape.
    } else if (res.reason === 'timedOut') {
      addToast(tString('viewer.saveAs.tooLong'), { level: 'warn' })
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
