/**
 * Drag-out completion event bridge.
 *
 * Subscribes ONCE to the backend's two drag-out session events and turns them
 * into ONE toast per drag SESSION (not one per downloaded file):
 *
 * - **`drag-out-session-started`** — the first fulfillment began. Finder shows
 *   nothing while a promise downloads, and a phone/NAS drag is slow (MTP is
 *   serial USB), so we raise a signs-of-life in-progress toast ("Copying N
 *   items…") keyed by the session. This is the signs-of-life affordance: visible
 *   feedback within ~1 s for big drags, with no Cancel button (v1 stays
 *   no-user-cancel — Finder owns the gesture).
 * - **`drag-out-session-complete`** — the session drained. We REPLACE the
 *   in-progress toast (same id) with a completion toast: success counts via the
 *   shared transfer-toast composer, or a failure toast naming the file(s).
 *   Finder also shows its own NSError alert per failed item, so our toast
 *   complements rather than duplicates it.
 *
 * Mounted from `routes/(main)/+layout.svelte`; the unsubscribe is returned so
 * the layout can clean up on destroy. Mirrors the downloads event bridge shape.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import { onDragOutSessionComplete, onDragOutSessionStarted } from '$lib/tauri-commands'
import { composeDragOutCompleteToast, type DragOutSessionComplete } from './drag-out-toast'

const log = getAppLogger('drag-out')

const TOAST_GROUP = 'drag-out'

interface DragOutSessionStarted {
  sessionKey: number
  totalItems: number
}

/** Stable per-session toast id so the completion toast replaces the in-progress one. */
function toastIdFor(sessionKey: number): string {
  return `drag-out:${String(sessionKey)}`
}

/**
 * Mount both listeners. Returns a single unsubscribe that detaches both — call
 * it from the layout's `onDestroy`.
 */
export async function startDragOutEventBridge(): Promise<UnlistenFn> {
  const unlistenStarted = await onDragOutSessionStarted((payload) => {
    handleSessionStarted(payload)
  })
  const unlistenComplete = await onDragOutSessionComplete((payload) => {
    handleSessionComplete(payload)
  })
  log.debug('Drag-out event bridge mounted')
  return () => {
    unlistenStarted()
    unlistenComplete()
  }
}

function handleSessionStarted(payload: DragOutSessionStarted): void {
  const { sessionKey, totalItems } = payload
  const itemsLabel = pluralize(totalItems, 'item')
  log.debug('Drag-out session {sessionKey} started ({totalItems} {itemsLabel})', {
    sessionKey,
    totalItems,
    itemsLabel,
  })

  // A neutral in-progress toast (default level, persistent until the completion
  // toast replaces it). "Downloading" covers both source kinds (phone, NAS).
  // No Cancel button — v1 has no user-cancel (Finder owns the gesture). The
  // completion event replaces this same id.
  const message = `Downloading ${String(totalItems)} ${itemsLabel}…`
  addToast(message, {
    id: toastIdFor(sessionKey),
    level: 'default',
    dismissal: 'persistent',
    toastGroup: TOAST_GROUP,
  })
}

function handleSessionComplete(payload: DragOutSessionComplete): void {
  const { sessionKey } = payload
  const { message, level } = composeDragOutCompleteToast(payload)
  log.debug('Drag-out session {sessionKey} complete: {message}', { sessionKey, message })

  // Replace the in-progress toast in place (same id). Transient so it
  // auto-dismisses; the user already has Finder's own result.
  addToast(message, {
    id: toastIdFor(sessionKey),
    level,
    dismissal: 'transient',
    toastGroup: TOAST_GROUP,
  })
}
