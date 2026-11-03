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
 *
 * A reload from the toast that went through runs `onReloaded` (the page refetches); one
 * that didn't re-raises the same toast as a warning saying why, with a retry.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { addToast, dismissToast } from '#lib/ui/toast/toast-store.svelte.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import ViewerReloadToastContent from './ViewerReloadToastContent.svelte'

export type FileChangedKind = 'grew' | 'rotated'

export interface FileChangedPayload {
  kind: FileChangedKind
  newSize?: number | null
}

export interface CreateViewerTailDeps {
  getSessionId: () => string
  getTailMode: () => boolean
  onAppendDetected: (newSize: number | null) => void
  /** A reload from the toast went through: drop the stale rows and refetch, like an append in tail mode. */
  onReloaded: () => void
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

export function createViewerTail(deps: CreateViewerTailDeps): ViewerTail {
  let unlisten: UnlistenFn | undefined

  /**
   * Raises (or re-raises, by id) the reload toast. `failureKey` is why the last reload didn't go
   * through: the toast stays up as a warning that says so and offers a retry.
   */
  function showReloadToast(sessionId: string, kind: FileChangedKind, failureKey: MessageKey | null = null): void {
    const id = toastIdFor(sessionId, kind)
    // On rotation, supersede any open "grew" toast: the older message is no
    // longer accurate because the file's been replaced.
    if (kind === 'rotated') {
      dismissToast(toastIdFor(sessionId, 'grew'))
    }
    addToast(ViewerReloadToastContent, {
      id,
      level: failureKey === null ? 'info' : 'warn',
      dismissal: 'persistent',
      closeTooltip: tString('viewer.reloadToast.dismissTooltip'),
      props: {
        sessionId,
        kind,
        failureKey,
        onReloaded: deps.onReloaded,
        onReloadRefused: (key: MessageKey) => {
          showReloadToast(sessionId, kind, key)
        },
      },
    })
  }

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
