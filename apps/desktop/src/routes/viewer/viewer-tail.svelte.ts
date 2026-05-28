/**
 * Viewer tail composable: listens for `viewer:file-changed:<sessionId>` Tauri
 * events and dispatches them to either:
 *
 * - A persistent reload toast (when tail mode is off, or always on a rotation).
 * - A `reloadTail()` side effect (when tail mode is on and the event is `grew`).
 *
 * Toasts are deduped by id so a flurry of file-changed events collapses into a
 * single visible toast. A `rotated` event that arrives after a `grew` toast
 * supersedes the grew toast: the file is gone, so the older "reload to catch
 * up" message is no longer accurate.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { addToast, dismissToast } from '$lib/ui/toast/toast-store.svelte'
import ViewerReloadToast, { setReloadToastContext as setReloadToastContextRaw } from './ViewerReloadToast.svelte'

// `setReloadToastContext` is exported from a `.svelte` module block; the
// ESLint+TS pipeline can't always resolve the type across that boundary, so
// we re-type the symbol locally. The shape matches the module export at
// `ViewerReloadToast.svelte:19`.
const setReloadToastContext = setReloadToastContextRaw as (next: {
  sessionId: string
  toastId: string
  kind: FileChangedKind
}) => void

export type FileChangedKind = 'grew' | 'rotated'

export interface FileChangedPayload {
  kind: FileChangedKind
  newSize?: number | null
}

export interface CreateViewerTailDeps {
  getSessionId: () => string
  getTailMode: () => boolean
  onAppendDetected: (newSize: number | null) => void
}

export interface ViewerTail {
  init: () => Promise<void>
  destroy: () => void
  /** Test-only: hands a fake event to the dispatcher. */
  testOnlyDispatch: (payload: FileChangedPayload) => void
}

function toastIdFor(sessionId: string, kind: FileChangedKind): string {
  return `viewer-file-changed-${sessionId}-${kind}`
}

function showReloadToast(sessionId: string, kind: FileChangedKind): void {
  const id = toastIdFor(sessionId, kind)
  // On rotation, supersede any open "grew" toast: the older message is no
  // longer accurate because the file's been replaced.
  if (kind === 'rotated') {
    dismissToast(toastIdFor(sessionId, 'grew'))
  }
  setReloadToastContext({ sessionId, toastId: id, kind })
  addToast(ViewerReloadToast, {
    id,
    level: 'info',
    dismissal: 'persistent',
    closeTooltip: 'Dismiss without reloading',
  })
}

export function createViewerTail(deps: CreateViewerTailDeps): ViewerTail {
  let unlisten: UnlistenFn | undefined

  function dispatch(payload: FileChangedPayload): void {
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    if (payload.kind === 'rotated') {
      showReloadToast(sessionId, 'rotated')
      return
    }
    if (deps.getTailMode()) {
      deps.onAppendDetected(payload.newSize ?? null)
      return
    }
    showReloadToast(sessionId, 'grew')
  }

  return {
    init: async (): Promise<void> => {
      const sessionId = deps.getSessionId()
      if (!sessionId) return
      unlisten = await listen<FileChangedPayload>(`viewer:file-changed:${sessionId}`, (e) => {
        dispatch(e.payload)
      })
    },
    destroy: (): void => {
      unlisten?.()
      unlisten = undefined
    },
    testOnlyDispatch: dispatch,
  }
}
