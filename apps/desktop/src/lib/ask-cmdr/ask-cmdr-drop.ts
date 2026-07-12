/**
 * Wires the composer as a drop target for files/folders dragged from a Cmdr pane (or from
 * Finder). Pane drags are NATIVE OS drags delivered through Tauri's webview
 * `onDragDropEvent`, NOT HTML5 `dragover`/`drop` with a `DataTransfer`, so a plain DOM
 * drop handler would never fire — we subscribe to the same webview event the pane
 * drag-drop controller uses and hit-test the composer's own rect.
 *
 * For an in-app drag the trustworthy source is the recorded self-drag identity, not the
 * pasteboard-round-tripped payload paths (virtual-volume paths mis-resolve to local). So
 * only LOCAL (`'root'`) self-drags are supported; a Finder drop uses the payload paths
 * (genuine local absolute paths). Kinds are resolved backend-side from known pane state.
 *
 * The subscription glue (dynamic Tauri import + `onDragDropEvent`) is deliberately thin;
 * the hit-test, event dispatch, and path resolution are pure/injectable functions so the
 * meaningful branches are unit-testable without a webview. Loading the Tauri API is lazy
 * and every failure is swallowed, so mounting the composer outside a Tauri webview (unit
 * tests, SSR) is a no-op.
 */

import { getIsDraggingFromSelf, getSelfDragIdentity } from '$lib/file-explorer/drag/drag-drop'
import { toViewportPosition } from '$lib/file-explorer/drag/drag-position'
import { resolveAskCmdrAttachments, type AttachmentRef } from '$lib/tauri-commands'

/** The local volume id; a pane on it round-trips genuine absolute paths. */
const LOCAL_VOLUME_ID = 'root'

/** A native drag-drop event payload, mirroring the Tauri webview shape (only the fields
 * this module reads). */
export type DragDropPayload =
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' }

/** The composer-side hooks a drag-drop event drives. */
export interface DropHandlers {
  /** The composer's current bounding rect, or `null` when unmounted. */
  getRect: () => DOMRect | null
  /** Toggle the drop overlay as a drag moves over / off the composer. */
  onDragActive: (active: boolean) => void
  /** Receive the resolved refs on a drop inside the composer. */
  onAttachments: (refs: AttachmentRef[]) => void
}

/** Is `position` (after the DevTools-docking correction) inside `rect`? `null` rect (the
 * composer is unmounted) is never a hit. Pure. */
export function isWithinRect(position: { x: number; y: number }, rect: DOMRect | null): boolean {
  if (!rect) return false
  const p = toViewportPosition(position)
  return p.x >= rect.left && p.x <= rect.right && p.y >= rect.top && p.y <= rect.bottom
}

/** Dispatch one native drag-drop event against the composer: enter/over toggle the
 * overlay by rect hit-test, leave clears it, and a drop inside resolves its paths into
 * attachment refs. The choke point the subscription forwards every event to. */
export async function handleDragDropEvent(payload: DragDropPayload, handlers: DropHandlers): Promise<void> {
  if (payload.type === 'enter' || payload.type === 'over') {
    handlers.onDragActive(isWithinRect(payload.position, handlers.getRect()))
  } else if (payload.type === 'leave') {
    handlers.onDragActive(false)
  } else {
    // 'drop'.
    handlers.onDragActive(false)
    if (!isWithinRect(payload.position, handlers.getRect())) return
    handlers.onAttachments(await resolveDroppedPaths(payload.paths))
  }
}

/**
 * Subscribe the composer to native drag-drop. Returns an unlisten function (a no-op when
 * Tauri isn't available). The callback just forwards to [`handleDragDropEvent`].
 */
export async function installComposerDrop(
  getRect: () => DOMRect | null,
  onDragActive: (active: boolean) => void,
  onAttachments: (refs: AttachmentRef[]) => void,
): Promise<() => void> {
  const handlers: DropHandlers = { getRect, onDragActive, onAttachments }
  try {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview')
    return await getCurrentWebview().onDragDropEvent((event) => {
      void handleDragDropEvent(event.payload as DragDropPayload, handlers)
    })
  } catch {
    return () => {}
  }
}

/** Resolve a drop's paths into typed attachment refs, honoring the self-drag identity.
 * Exported for unit testing the branch logic (in-app local vs. virtual vs. Finder). */
export async function resolveDroppedPaths(payloadPaths: string[]): Promise<AttachmentRef[]> {
  const identity = getSelfDragIdentity()
  if (getIsDraggingFromSelf() && identity) {
    // Only local self-drags are trustworthy; virtual-volume paths mis-resolve.
    if (identity.sourceVolumeId !== LOCAL_VOLUME_ID) return []
    return resolveAskCmdrAttachments(identity.sourcePaths)
  }
  // Finder (external) drop: the payload paths are genuine local absolute paths.
  return payloadPaths.length > 0 ? resolveAskCmdrAttachments(payloadPaths) : []
}
