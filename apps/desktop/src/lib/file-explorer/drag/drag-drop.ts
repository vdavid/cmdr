// Drag and drop utilities for file items
// Handles both single-file drag and selection-based multi-file drag
//
// ## macOS timing invariant
//
// `startDragPaths()` / `startSelectionDrag()` resolve BEFORE macOS delivers
// `draggingEntered:`/`draggingExited:` events to the webview. Any state that the native
// swizzle reads (SELF_DRAG_ACTIVE, rich image path) must NOT be cleared from JS code
// that runs after the start call resolves; it would race with the AppKit callbacks.
// Self-drag state is only cleared on drop (via endSelfDragSession from the drop handler).
//
// ## Pasteboard types
//
// Both backend commands route through `native_drag.rs`, which advertises both
// `public.file-url` AND `public.utf8-plain-text` so terminals like Warp receive
// shell-escaped paths as text. The crabnebula plugin only advertised file URLs,
// which terminals don't subscribe to.

import { tempDir, join } from '@tauri-apps/api/path'
import { getCachedIcon } from '$lib/icon-cache'
import {
  startSelectionDrag,
  startDragPaths,
  prepareSelfDragOverlay,
  clearSelfDragOverlay,
  setSelfDragResolvedOperation,
} from '$lib/tauri-commands'
import { getSetting } from '$lib/settings/settings-store'
import { cancelClickToRename } from '../rename/rename-activation'
import { renderDragImage } from './drag-image-renderer'

/** Gets the drag threshold from settings (minimum distance in pixels to trigger drag) */
export function getDragThreshold(): number {
  return getSetting('advanced.dragThreshold')
}

/** Name of the temp icon file */
const TEMP_ICON_FILENAME = 'drag-icon.png'

/** Name of the temp rendered drag image file */
const TEMP_DRAG_IMAGE_FILENAME = 'drag-image.png'

/** Info for a file being dragged, used to render the drag image and overlay icons. */
export interface DragFileInfo {
  name: string
  isDirectory: boolean
  iconId: string
}

/** Context for a single file drag (no prior selection) */
interface SingleFileDragContext {
  type: 'single'
  path: string
  iconId: string
  index: number
  /** File info for the drag image renderer */
  fileInfo?: DragFileInfo
}

/** Context for a selection-based drag */
interface SelectionDragContext {
  type: 'selection'
  listingId: string
  indices: number[]
  includeHidden: boolean
  hasParent: boolean
  /** Icon ID to use for the drag preview (first selected file) */
  iconId: string
  /** File info for the drag image renderer (first N files of selection) */
  fileInfos?: DragFileInfo[]
}

/**
 * Context for a multi-path drag where the frontend already has resolved
 * absolute paths. Used by the search-results pane (M8d), which has no backend
 * listing for `start_selection_drag` to resolve indices against. Routes
 * through `start_drag_paths` instead.
 */
interface PathsDragContext {
  type: 'paths'
  paths: string[]
  /** Icon ID to use for the drag preview (first file in `paths`). */
  iconId: string
  /** File info for the drag image renderer (first N files). */
  fileInfos?: DragFileInfo[]
}

/** Callbacks for drag lifecycle events */
interface DragCallbacks {
  /** Called when drag threshold is crossed (for single-file case, to trigger selection) */
  onDragStart?: () => void
  /** Called when drag is cancelled (ESC key or mouseup before threshold) */
  onDragCancel?: () => void
  /**
   * Called when the drag actually initiates (threshold crossed), for BOTH single-file and
   * selection contexts. Use this for side-effects that should run on every drag start,
   * regardless of whether the drag promoted a selection or used an existing one. Type-to-jump
   * uses this to clear its buffer; typing a query then dragging means the user moved on.
   */
  onDragInitiate?: () => void
}

/** Tracks whether the current native drag originated from this app (pane-to-pane or self-drop). */
let draggingFromSelf = false

/** Getter to read the flag reliably across modules (avoids ES module live-binding timing issues). */
export function getIsDraggingFromSelf(): boolean {
  return draggingFromSelf
}

/** Resets the self-drag flag. Call from the drop event handler after processing. */
export function resetDraggingFromSelf(): void {
  draggingFromSelf = false
}

/** Restores the draggingFromSelf flag (for re-entry detection). */
export function markAsSelfDrag(): void {
  draggingFromSelf = true
}

/** Fingerprint of the last self-initiated drag for re-entry detection. */
interface DragFingerprint {
  count: number
  samplePaths: string[]
}

let selfDragFingerprint: DragFingerprint | null = null
/** File info stored from self-drag for overlay icon rendering. */
let selfDragFileInfos: DragFileInfo[] | null = null

/** Stores a fingerprint from the current drag's paths for re-entry detection. */
export function storeSelfDragFingerprint(paths: string[], fileInfos?: DragFileInfo[]): void {
  selfDragFingerprint = {
    count: paths.length,
    samplePaths: paths.slice(0, 5),
  }
  if (fileInfos) {
    selfDragFileInfos = fileInfos
  }
}

/** Checks if incoming drag paths match a stored self-drag fingerprint. O(1) for 50k+ files. */
export function matchesSelfDragFingerprint(paths: string[]): boolean {
  if (!selfDragFingerprint) return false
  if (paths.length !== selfDragFingerprint.count) return false
  return selfDragFingerprint.samplePaths.every((p, i) => paths[i] === p)
}

/** Returns stored file infos from self-drag (for overlay icons), or null. */
export function getSelfDragFileInfos(): DragFileInfo[] | null {
  return selfDragFileInfos
}

/** Clears both the fingerprint and stored file infos. Call on drop completion. */
export function clearSelfDragFingerprint(): void {
  selfDragFingerprint = null
  selfDragFileInfos = null
}

/** Pending temp file cleanup: stored during drag, executed when session ends. */
let pendingImageCleanup: (() => Promise<void>) | null = null

/**
 * Ends the self-drag session: clears Rust state and deletes the temp drag image.
 * Idempotent; safe to call from both the drop handler and the startDrag finally block.
 */
export async function endSelfDragSession(): Promise<void> {
  const cleanup = pendingImageCleanup
  pendingImageCleanup = null
  await clearSelfDragOverlay()
  if (cleanup) await cleanup()
}

/** Global state for active drag operation */
let activeDrag: {
  startX: number
  startY: number
  context: SingleFileDragContext | SelectionDragContext | PathsDragContext
  callbacks: DragCallbacks
  cleanup: () => void
} | null = null

/** Decodes a base64 data URL and writes it to a temp file, returning the file path */
async function writeIconToTemp(dataUrl: string): Promise<string> {
  // Get temp directory and build path
  const tempPath = await tempDir()
  const iconPath = await join(tempPath, TEMP_ICON_FILENAME)

  // Extract base64 data from data URL (format: data:image/png;base64,...)
  const base64Match = dataUrl.match(/^data:image\/\w+;base64,(.+)$/)
  if (!base64Match) {
    throw new Error('Invalid data URL format')
  }
  const base64Data = base64Match[1]

  // Convert base64 to binary
  const binaryString = atob(base64Data)
  const bytes = new Uint8Array(binaryString.length)
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i)
  }

  // Write to file using the Tauri fs API
  const { writeFile } = await import('@tauri-apps/plugin-fs')
  await writeFile(iconPath, bytes)

  return iconPath
}

/** Cleans up the temp icon file */
async function cleanupTempIcon(): Promise<void> {
  try {
    const tempPath = await tempDir()
    const iconPath = await join(tempPath, TEMP_ICON_FILENAME)
    const { remove } = await import('@tauri-apps/plugin-fs')
    await remove(iconPath)
  } catch {
    // Ignore cleanup errors (file may not exist)
  }
}

/** Writes a canvas drag image to a temp PNG file and returns the path. */
async function writeDragImageToTemp(canvas: HTMLCanvasElement): Promise<string> {
  const tempPath = await tempDir()
  const imagePath = await join(tempPath, TEMP_DRAG_IMAGE_FILENAME)

  const blob = await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((result) => {
      if (result) resolve(result)
      else reject(new Error('Canvas toBlob failed'))
    }, 'image/png')
  })

  const buffer = await blob.arrayBuffer()
  const bytes = new Uint8Array(buffer)
  const { writeFile } = await import('@tauri-apps/plugin-fs')
  await writeFile(imagePath, bytes)
  return imagePath
}

/** Cleans up the temp drag image file */
async function cleanupTempDragImage(): Promise<void> {
  try {
    const tempPath = await tempDir()
    const imagePath = await join(tempPath, TEMP_DRAG_IMAGE_FILENAME)
    const { remove } = await import('@tauri-apps/plugin-fs')
    await remove(imagePath)
  } catch {
    // Ignore cleanup errors
  }
}

/**
 * Resolves the drag icon path: if file infos are available, renders a rich canvas image.
 * Falls back to the simple cached icon.
 */
async function resolveDragIconPath(
  iconId: string,
  fileInfos: DragFileInfo[] | undefined,
): Promise<{ path: string; usedCanvas: boolean } | null> {
  // Try rich canvas image first
  if (fileInfos && fileInfos.length > 0) {
    try {
      const canvas = await renderDragImage(fileInfos)
      const path = await writeDragImageToTemp(canvas)
      return { path, usedCanvas: true }
    } catch {
      // Fall through to simple icon
    }
  }

  // Fall back to simple icon
  const iconDataUrl = getCachedIcon(iconId)
  if (!iconDataUrl) return null

  try {
    const path = await writeIconToTemp(iconDataUrl)
    return { path, usedCanvas: false }
  } catch {
    return null
  }
}

/**
 * Starts tracking a potential drag operation with selection awareness.
 *
 * For single-file drags (no prior selection), the file is selected only when the
 * drag threshold is crossed. For selection drags, all selected files are dragged.
 *
 * @param event - The mousedown event
 * @param context - Either a single file or a selection to drag
 * @param callbacks - Optional callbacks for drag lifecycle events
 */
export function startSelectionDragTracking(
  event: MouseEvent,
  context: SingleFileDragContext | SelectionDragContext | PathsDragContext,
  callbacks: DragCallbacks = {},
): void {
  // Cancel any existing drag
  cancelDragTracking()

  const handleMouseMove = (moveEvent: MouseEvent) => {
    if (!activeDrag) return

    const dx = moveEvent.clientX - activeDrag.startX
    const dy = moveEvent.clientY - activeDrag.startY
    const distance = Math.sqrt(dx * dx + dy * dy)

    if (distance >= getDragThreshold()) {
      // Threshold crossed - trigger the drag
      const ctx = activeDrag.context
      const cbs = activeDrag.callbacks

      // Stop any pending click-to-rename timer so it doesn't fire mid-drag.
      cancelClickToRename()

      // Fire onDragInitiate for both contexts; anything that wants to react
      // to "a drag is starting" (type-to-jump buffer clear, etc.) hooks in here.
      cbs.onDragInitiate?.()

      // For single-file drag, call onDragStart to select the file first
      if (ctx.type === 'single') {
        cbs.onDragStart?.()
      }

      // The backend publishes a permissive op mask (Copy | Move | Generic | Link); macOS
      // arbitrates the actual operation via modifier keys live during the drag (Alt → Copy,
      // Cmd → Move, Ctrl-Alt → Link), so we no longer pass mode here.
      if (ctx.type === 'single') {
        void performSingleFileDrag(ctx.path, ctx.iconId, ctx.fileInfo)
      } else if (ctx.type === 'selection') {
        void performSelectionDrag(ctx)
      } else {
        // `paths` context: the FE already has resolved paths (search-results
        // pane via the snapshot store). Route through `start_drag_paths` so
        // the backend doesn't need a listing-cache lookup. M8d.
        void performPathsDrag(ctx)
      }

      cancelDragTracking()
    }
  }

  const handleMouseUp = () => {
    // Mouse released before threshold - cancel
    activeDrag?.callbacks.onDragCancel?.()
    cancelDragTracking()
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      // ESC pressed - cancel drag
      activeDrag?.callbacks.onDragCancel?.()
      cancelDragTracking()
    }
  }

  const cleanup = () => {
    document.removeEventListener('mousemove', handleMouseMove)
    document.removeEventListener('mouseup', handleMouseUp)
    document.removeEventListener('keydown', handleKeyDown)
  }

  activeDrag = {
    startX: event.clientX,
    startY: event.clientY,
    context,
    callbacks,
    cleanup,
  }

  document.addEventListener('mousemove', handleMouseMove)
  document.addEventListener('mouseup', handleMouseUp)
  document.addEventListener('keydown', handleKeyDown)
}

/**
 * Cancels any active drag tracking.
 */
export function cancelDragTracking(): void {
  if (activeDrag) {
    activeDrag.cleanup()
    activeDrag = null
  }
  draggingFromSelf = false
}

/**
 * Performs a single-file native drag operation.
 * Uses the rich PNG as the OS drag image (visible outside the window).
 * The native swizzle hides it over our window so the DOM overlay takes over.
 */
async function performSingleFileDrag(filePath: string, iconId: string, fileInfo?: DragFileInfo): Promise<void> {
  const fileInfos = fileInfo ? [fileInfo] : undefined
  const resolved = await resolveDragIconPath(iconId, fileInfos)
  if (!resolved) return

  // Store cleanup for later; the temp file must survive the entire drag session
  // because the native swizzle reads it from disk on every window exit.
  pendingImageCleanup = resolved.usedCanvas ? cleanupTempDragImage : cleanupTempIcon

  // Store rich image path so native swizzle can swap to it on window exit
  await prepareSelfDragOverlay(resolved.path)

  // Seed the swizzle with our best-guess op so the very first draggingEntered:
  // returns Move (no badge) instead of wry's hardcoded Copy ("+"). 'move' wins
  // over 'copy' as the default because it's the same-volume case (most common)
  // and because a "+" appearing later feels intentional, while a "+" disappearing
  // would feel like a glitch.
  await setSelfDragResolvedOperation('move')

  // Don't reset draggingFromSelf after the start call; it resolves before the
  // OS delivers drop/leave events. The flag is cleared by the drop handler.
  draggingFromSelf = true
  await startDragPaths([filePath], resolved.path)
}

/**
 * Performs a selection-based drag operation via the backend.
 * This avoids transferring file paths over IPC for large selections.
 * Uses the rich PNG as the OS drag image (visible outside, hidden inside by native swizzle).
 */
async function performSelectionDrag(context: SelectionDragContext): Promise<void> {
  const resolved = await resolveDragIconPath(context.iconId, context.fileInfos)
  if (!resolved) return

  // Store cleanup for later; the temp file must survive the entire drag session
  pendingImageCleanup = resolved.usedCanvas ? cleanupTempDragImage : cleanupTempIcon

  // Store rich image path so native swizzle can swap to it on window exit
  await prepareSelfDragOverlay(resolved.path)

  // Seed the swizzle with our best-guess op; see performSingleFileDrag comment.
  await setSelfDragResolvedOperation('move')

  // Don't reset draggingFromSelf after startDrag; see performSingleFileDrag comment.
  draggingFromSelf = true
  await startSelectionDrag(context.listingId, context.indices, context.includeHidden, context.hasParent, resolved.path)
}

/**
 * Performs a paths-by-value drag. The search-results pane uses this because
 * it has no backend listing for `start_selection_drag` to resolve indices
 * against. `start_drag_paths` is the same Tauri command used for single-file
 * drags; it just accepts >1 path. M8d.
 */
async function performPathsDrag(context: PathsDragContext): Promise<void> {
  if (context.paths.length === 0) return

  const resolved = await resolveDragIconPath(context.iconId, context.fileInfos)
  if (!resolved) return

  pendingImageCleanup = resolved.usedCanvas ? cleanupTempDragImage : cleanupTempIcon

  await prepareSelfDragOverlay(resolved.path)
  await setSelfDragResolvedOperation('move')

  draggingFromSelf = true
  await startDragPaths(context.paths, resolved.path)
}
