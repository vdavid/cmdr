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
 * The Tauri APIs are loaded lazily and every failure is swallowed, so mounting the
 * composer outside a Tauri webview (unit tests, SSR) is a no-op.
 */

import { getIsDraggingFromSelf, getSelfDragIdentity } from '$lib/file-explorer/drag/drag-drop'
import { toViewportPosition } from '$lib/file-explorer/drag/drag-position'
import { resolveAskCmdrAttachments, type AttachmentRef } from '$lib/tauri-commands'

/** The local volume id; a pane on it round-trips genuine absolute paths. */
const LOCAL_VOLUME_ID = 'root'

/**
 * Subscribe the composer to native drag-drop. `getRect` returns the composer's current
 * bounding rect (or `null` when unmounted); `onDragActive` toggles the drop overlay;
 * `onAttachments` receives the resolved refs on a drop inside the composer. Returns an
 * unlisten function (a no-op when Tauri isn't available).
 */
export async function installComposerDrop(
  getRect: () => DOMRect | null,
  onDragActive: (active: boolean) => void,
  onAttachments: (refs: AttachmentRef[]) => void,
): Promise<() => void> {
  try {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview')
    const within = (position: { x: number; y: number }): boolean => {
      const rect = getRect()
      if (!rect) return false
      const p = toViewportPosition(position)
      return p.x >= rect.left && p.x <= rect.right && p.y >= rect.top && p.y <= rect.bottom
    }
    return await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      if (payload.type === 'enter' || payload.type === 'over') {
        onDragActive(within(payload.position))
      } else if (payload.type === 'leave') {
        onDragActive(false)
      } else {
        // The only remaining variant is 'drop'.
        onDragActive(false)
        if (!within(payload.position)) return
        void resolveDroppedPaths(payload.paths).then(onAttachments)
      }
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
