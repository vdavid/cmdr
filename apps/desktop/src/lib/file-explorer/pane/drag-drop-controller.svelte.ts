import { listen, setSelfDragResolvedOperation, type UnlistenFn } from '$lib/tauri-commands'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { toViewportPosition } from '../drag/drag-position'
import {
  getIsDraggingFromSelf,
  resetDraggingFromSelf,
  matchesSelfDragFingerprint,
  markAsSelfDrag,
  storeSelfDragFingerprint,
  clearSelfDragFingerprint,
  getSelfDragFileInfos,
  endSelfDragSession,
} from '../drag/drag-drop'
import { resolveDropTarget } from '../drag/drop-target-hit-testing'
import { isInvalidSelfDescendantDrop } from '../drag/drop-target-validation'
import { pickDropOperation } from '../drag/drop-operation'
import { showOverlay, updateOverlay, hideOverlay, type OverlayFileInfo } from '../drag/drag-overlay.svelte.js'
import { getCachedIcon } from '$lib/icon-cache'
import {
  startModifierTracking,
  stopModifierTracking,
  getModifierState,
  setModifiers,
} from '../modifier-key-tracker.svelte'
import { statPathsKinds } from '$lib/tauri-commands'
import { buildTransferPropsFromDroppedPaths } from './transfer-operations'
import type { TransferOperationType } from '../types'
import type { PaneAccess } from './pane-access'
import type { createDialogState } from './dialog-state.svelte'

type DialogState = ReturnType<typeof createDialogState>

export interface DragDropControllerDeps {
  access: PaneAccess
  dialogs: DialogState
  /** Live reference to the pane-wrapper element record, used for hit-testing. */
  getPaneWrapperEls: () => Record<'left' | 'right', HTMLDivElement | undefined>
}

/**
 * Owns the native drag-and-drop band lifted out of `DualPaneExplorer`: the
 * drop-target highlight state, the ten drag handlers, the three Tauri drag
 * listeners (`onDragDropEvent`, `drag-image-size`, `drag-modifiers`), and the
 * folder-highlight `$effect`.
 *
 * The `$effect` is created synchronously in this factory body (the
 * `initListingDiffSync` pattern, landmine L3) so it lands in the component's
 * effect-tracking context; it must NOT be moved into `init()` or `onMount`.
 * `init()` registers the three async listeners and is called from the
 * component's `onMount`; `cleanup()` unsubscribes them and stops modifier
 * tracking, called from `onDestroy`.
 *
 * The self-drag fingerprint dance, the leave-vs-drop cleanup asymmetry, the
 * `pushSelfDragOpIfChanged` dedupe, and the `externalDragHasLargeImage`
 * suppression all moved verbatim — they encode hard-won native-drag behavior.
 */
export function createDragDropController(deps: DragDropControllerDeps) {
  const { access, dialogs, getPaneWrapperEls } = deps

  // Drag image size from the source app (macOS only, via swizzle).
  // If the source provides a large preview (like Finder), we suppress our overlay.
  const smallDragImageThreshold = 32
  let externalDragHasLargeImage = false

  // Drop target highlight state: which pane (if any) is the active drop target
  let dropTargetPane = $state<'left' | 'right' | null>(null)

  // Folder-level drop target state: when hovering over a directory row
  let dropTargetFolderPath = $state<string | null>(null)
  let dropTargetFolderEl = $state<HTMLElement | null>(null)

  // Paths being dragged for the current drag session. Captured on drag-enter,
  // cleared on drag-leave/drop. Used to block dropping onto the source itself
  // or into one of its descendants.
  let currentDragSourcePaths: string[] = []

  // Last cursor position seen during the current drag. Used to re-run handleDragOver
  // when the modifier state changes without a mouse move (so the OS "+" badge can
  // update via setSelfDragResolvedOperation even when the cursor is still).
  let lastDragPosition: { x: number; y: number } | null = null

  // Last resolved op pushed to the native swizzle. Dedupe for IPC traffic.
  let lastPushedSelfDragOp: 'move' | 'copy' | null = null

  let unlistenDragDrop: UnlistenFn | undefined
  let unlistenDragImageSize: UnlistenFn | undefined
  let unlistenDragModifiers: UnlistenFn | undefined

  /**
   * Handles a file drop onto a target pane by opening the transfer confirmation
   * dialog. Fetches each dropped path's top-level kind (file vs. folder) in one
   * batched IPC so the dialog and completion toast report the real split. The
   * stat runs under the backend read timeout and degrades to all-unknown on a
   * slow mount, so this never blocks the drop; on any unknown flag the builder
   * falls back to the approximate count shape.
   */
  async function handleFileDrop(
    paths: string[],
    targetPane: 'left' | 'right',
    targetFolderPath?: string,
    operation: TransferOperationType = 'copy',
  ) {
    if (paths.length === 0) return

    const { sortBy, sortOrder } = access.getPaneSort(targetPane)
    const destPath = targetFolderPath ?? access.getPanePath(targetPane)
    const destVolId = access.getPaneVolumeId(targetPane)

    let isDirectoryFlags: (boolean | null)[] | undefined
    try {
      isDirectoryFlags = await statPathsKinds(paths)
    } catch {
      // Stat failed entirely — leave flags undefined so the builder uses the
      // approximate shape rather than blocking the drop on the error.
      isDirectoryFlags = undefined
    }

    dialogs.showTransfer(
      buildTransferPropsFromDroppedPaths(
        operation,
        paths,
        destPath,
        targetPane,
        destVolId,
        sortBy,
        sortOrder,
        isDirectoryFlags,
      ),
    )
  }

  /** Extracts the last path component as a display name. */
  function extractFolderName(path: string): string {
    const segments = path.split('/')
    return segments[segments.length - 1] || path
  }

  /** Builds overlay file infos from drag paths, using self-drag data when available for proper icons. */
  function buildOverlayFileInfos(paths: string[]): OverlayFileInfo[] {
    // For self-drags, use stored file infos with proper icon IDs
    const selfInfos = getIsDraggingFromSelf() ? getSelfDragFileInfos() : null
    if (selfInfos && selfInfos.length > 0) {
      return selfInfos.map((info) => ({
        name: info.name,
        iconUrl: getCachedIcon(info.iconId),
        isDirectory: info.isDirectory,
      }))
    }

    // For external drags, extract names and try extension-based icon lookup
    return paths.slice(0, 20).map((p) => {
      const name = p.split('/').pop() || p
      const ext = name.includes('.') ? name.split('.').pop() || '' : ''
      const iconUrl = ext ? getCachedIcon(`ext:${ext}`) : undefined
      return { name, iconUrl, isDirectory: false }
    })
  }

  /** Resolves the target display name for the overlay action line. */
  function resolveTargetDisplayName(
    resolved: ReturnType<typeof resolveDropTarget>,
    folderPath: string | null,
  ): string | null {
    if (!resolved) return null
    if (resolved.type === 'folder' && folderPath) {
      return extractFolderName(folderPath)
    }
    if (resolved.type === 'pane') {
      return extractFolderName(access.getPanePath(resolved.paneId))
    }
    return null
  }

  /** Called on drag enter to initialize the overlay with file infos. */
  function handleDragEnter(paths: string[], position: { x: number; y: number }) {
    // Skip the overlay when an external drag has a large source image (like Finder's preview).
    // Self-drags always show the overlay (the OS drag image is transparent inside the window).
    const suppressOverlay = externalDragHasLargeImage && !getIsDraggingFromSelf()
    if (!suppressOverlay) {
      const overlayInfos = buildOverlayFileInfos(paths)
      showOverlay(overlayInfos, paths.length)
    }
    startModifierTracking()
    handleDragOver(position)
  }

  /** Resolves the effective target path for a drop target (folder path or pane's current path). */
  function targetPathOf(resolved: ReturnType<typeof resolveDropTarget>): string | null {
    if (!resolved) return null
    if (resolved.type === 'folder') return resolved.path
    return access.getPanePath(resolved.paneId)
  }

  /** Updates drop-target highlights and overlay as the cursor moves during a drag. */
  function handleDragOver(position: { x: number; y: number }) {
    lastDragPosition = position
    const paneWrapperEls = getPaneWrapperEls()
    const resolved = resolveDropTarget(position.x, position.y, paneWrapperEls.left, paneWrapperEls.right)

    // Block drops onto the source itself or into one of its descendants.
    const effectiveTarget = targetPathOf(resolved)
    const isInvalidSelfDrop =
      effectiveTarget !== null && isInvalidSelfDescendantDrop(effectiveTarget, currentDragSourcePaths)

    if (isInvalidSelfDrop) {
      clearDropTargets()
    } else if (resolved?.type === 'folder') {
      dropTargetPane = null
      dropTargetFolderPath = resolved.path
      dropTargetFolderEl = resolved.element
    } else if (resolved?.type === 'pane') {
      // Suppress highlight when self-drag targets the source pane (no-op)
      const suppress = getIsDraggingFromSelf() && resolved.paneId === access.getFocusedPane()
      dropTargetPane = suppress ? null : resolved.paneId
      dropTargetFolderPath = null
      dropTargetFolderEl = null
    } else {
      clearDropTargets()
    }

    // Determine if dropping is allowed
    const isSelfPaneNoOp =
      resolved?.type === 'pane' && getIsDraggingFromSelf() && resolved.paneId === access.getFocusedPane()
    const canDrop = resolved !== null && !isSelfPaneNoOp && !isInvalidSelfDrop
    const targetName = resolveTargetDisplayName(resolved, dropTargetFolderPath)
    const operation = pickDropOperation({
      sourcePath: currentDragSourcePaths[0] ?? null,
      targetPath: effectiveTarget,
      volumes: access.getVolumes(),
      modifiers: getModifierState(),
    })

    updateOverlay(position.x, position.y, targetName, canDrop, operation)

    pushSelfDragOpIfChanged(operation)
  }

  /**
   * Pushes the resolved op to the native swizzle so the OS-rendered "+" copy badge
   * tracks reality (Copy → +, Move → no badge). Deduped via `lastPushedSelfDragOp`
   * to keep IPC traffic to op transitions only.
   */
  function pushSelfDragOpIfChanged(operation: 'move' | 'copy') {
    if (!getIsDraggingFromSelf()) return
    if (operation === lastPushedSelfDragOp) return
    lastPushedSelfDragOp = operation
    void setSelfDragResolvedOperation(operation)
  }

  /** Handles the drop event: resolves the target and opens the transfer dialog. */
  function handleDrop(paths: string[], position: { x: number; y: number }) {
    const paneWrapperEls = getPaneWrapperEls()
    const resolved = resolveDropTarget(position.x, position.y, paneWrapperEls.left, paneWrapperEls.right)
    const folderPath = dropTargetFolderPath
    const effectiveTarget = targetPathOf(resolved)

    // Read modifiers BEFORE stopping the tracker (which resets state).
    // Same source-of-truth as the overlay (`handleDragOver`) so the displayed
    // operation matches what we actually run.
    const operation = pickDropOperation({
      sourcePath: paths[0] ?? null,
      targetPath: effectiveTarget,
      volumes: access.getVolumes(),
      modifiers: getModifierState(),
    })

    clearDropTargets()
    hideOverlay()
    stopModifierTracking()

    if (!resolved) return
    const targetPane = resolved.paneId
    // For same-pane pane-level drops (not folder), suppress (no-op)
    if (resolved.type === 'pane' && getIsDraggingFromSelf() && targetPane === access.getFocusedPane()) return

    // Guard against drops onto the source itself or into its descendants
    if (effectiveTarget !== null && isInvalidSelfDescendantDrop(effectiveTarget, paths)) return

    void handleFileDrop(
      paths,
      targetPane,
      resolved.type === 'folder' ? (folderPath ?? undefined) : undefined,
      operation,
    )
  }

  /** Clears all drop target highlight state and hides overlay. */
  function clearDropTargets() {
    dropTargetPane = null
    dropTargetFolderPath = null
    dropTargetFolderEl = null
  }

  // Manage folder drop-target highlight class imperatively (elements live in child components).
  // Created synchronously in the factory body (L3) so it lands in the component's
  // effect-tracking context — NOT in init() / onMount.
  $effect(() => {
    const el = dropTargetFolderEl
    if (el) {
      el.classList.add('folder-drop-target')
      return () => {
        el.classList.remove('folder-drop-target')
      }
    }
  })

  /** Registers the three native-drag Tauri listeners. Called from the component's `onMount`. */
  async function init(): Promise<void> {
    // Listen for drag image size from native swizzle (macOS).
    // Fires before the Tauri drag enter event, so the flag is ready when handleDragEnter runs.
    unlistenDragImageSize = await listen<{ width: number; height: number }>('drag-image-size', (event) => {
      const { width, height } = event.payload
      externalDragHasLargeImage = width > smallDragImageThreshold || height > smallDragImageThreshold
    })

    // Listen for native modifier key state during drags (macOS).
    // [NSEvent modifierFlags] works even when the webview doesn't have keyboard focus.
    unlistenDragModifiers = await listen<{ altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean }>(
      'drag-modifiers',
      (event) => {
        setModifiers(event.payload)
        // Re-evaluate the resolved op (and overlay action line) on modifier change
        // without a mouse move. Otherwise the OS "+" badge wouldn't update until
        // the cursor moves, lagging the user's intent.
        if (lastDragPosition !== null) {
          handleDragOver(lastDragPosition)
        }
      },
    )

    // Register drag-and-drop target handler for external and pane-to-pane drops
    unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const { type } = event.payload
      if (type === 'enter') {
        const paths = event.payload.paths
        // Re-entry detection: if not currently flagged as self-drag but
        // fingerprint matches, restore the flag before any highlight logic
        if (!getIsDraggingFromSelf() && matchesSelfDragFingerprint(paths)) {
          markAsSelfDrag()
        }
        // On first entry of a self-drag, store fingerprint for re-entry detection
        if (getIsDraggingFromSelf() && !matchesSelfDragFingerprint(paths)) {
          storeSelfDragFingerprint(paths)
        }
        currentDragSourcePaths = paths
        handleDragEnter(paths, toViewportPosition(event.payload.position))
      } else if (type === 'over') {
        handleDragOver(toViewportPosition(event.payload.position))
      } else if (type === 'drop') {
        handleDrop(event.payload.paths, toViewportPosition(event.payload.position))
        resetDraggingFromSelf()
        clearSelfDragFingerprint()
        void endSelfDragSession()
        externalDragHasLargeImage = false
        currentDragSourcePaths = []
        lastDragPosition = null
        lastPushedSelfDragOp = null
      } else {
        // 'leave': cursor left the window or drag was cancelled
        clearDropTargets()
        hideOverlay()
        stopModifierTracking()
        resetDraggingFromSelf()
        // Do NOT call endSelfDragSession() here; the native swizzle needs
        // SELF_DRAG_ACTIVE + rich image path to swap images on window exit.
        // State is cleaned up when startDrag resolves (finally block) or on drop.
        externalDragHasLargeImage = false
        currentDragSourcePaths = []
        lastDragPosition = null
        // Keep lastPushedSelfDragOp set so re-entry doesn't redundantly re-push the
        // same op. clear_self_drag_state on the native side resets the AtomicU8 too.
        // Do NOT clear the fingerprint here; that's the key to re-entry detection
      }
    })
  }

  /** Unsubscribes the three listeners and stops modifier tracking. Called from `onDestroy`. */
  function cleanup(): void {
    unlistenDragImageSize?.()
    unlistenDragModifiers?.()
    unlistenDragDrop?.()
    stopModifierTracking()
  }

  return {
    init,
    cleanup,
    /** Live getter for the active drop-target pane (drives the `drop-target-active` class). */
    getDropTargetPane: () => dropTargetPane,
    // Handlers exposed for characterization tests.
    handleFileDrop,
    extractFolderName,
    buildOverlayFileInfos,
    resolveTargetDisplayName,
    handleDragEnter,
    targetPathOf,
    handleDragOver,
    pushSelfDragOpIfChanged,
    handleDrop,
    clearDropTargets,
  }
}
