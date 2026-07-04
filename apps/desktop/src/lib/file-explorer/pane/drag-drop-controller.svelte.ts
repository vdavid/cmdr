import { onDragImageSize, onDragModifiers, setSelfDragResolvedOperation, type UnlistenFn } from '$lib/tauri-commands'
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
  getSelfDragIdentity,
  clearSelfDragIdentity,
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
import { addToast } from '$lib/ui/toast'
import { statPathsKinds, resolvePathVolume } from '$lib/tauri-commands'
import { buildTransferPropsFromDroppedPaths } from './transfer-operations'
import { checkTransferDestinationGuard, resolveSourceVolumeId } from './transfer-entry'
import type { SelfDragIdentity } from '../drag/drag-drop'
import type { PathVolumeResolution } from '$lib/tauri-commands'
import type { TransferOperationType } from '../types'
import type { PaneAccess } from './pane-access'
import type { createDialogState } from './dialog-state.svelte'
import type { DragAutoScrollFrameResult } from '../drag/drag-auto-scroll'

type DialogState = ReturnType<typeof createDialogState>
type ResolvedDropTarget = ReturnType<typeof resolveDropTarget>

export interface DragDropControllerDeps {
  access: PaneAccess
  dialogs: DialogState
  /** Live reference to the pane-wrapper element record, used for hit-testing. */
  getPaneWrapperEls: () => Record<'left' | 'right', HTMLDivElement | undefined>
  /**
   * Resolves a path to its containing volume (backend `resolve_path_volume`),
   * the fallback when a dropped path matches no registered volume root. Injected
   * so the controller stays headless-testable; defaults to the real command.
   */
  resolvePathVolume?: (path: string) => Promise<PathVolumeResolution>
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
  const resolveVolumeForPath = deps.resolvePathVolume ?? resolvePathVolume

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
  // update via setSelfDragResolvedOperation even when the cursor is still), and
  // after auto-scroll reveals a different row under a stationary cursor.
  let lastDragPosition: { x: number; y: number } | null = null

  // Last resolved op pushed to the native swizzle. Dedupe for IPC traffic.
  let lastPushedSelfDragOp: 'move' | 'copy' | null = null

  let dragAutoScrollPane: 'left' | 'right' | null = null
  let dragAutoScrollPosition: { x: number; y: number } | null = null
  let dragAutoScrollFrame: number | null = null
  let lastAutoScrollTimestamp: number | null = null

  let unlistenDragDrop: UnlistenFn | undefined
  let unlistenDragImageSize: UnlistenFn | undefined
  let unlistenDragModifiers: UnlistenFn | undefined

  const inactiveAutoScrollResult: DragAutoScrollFrameResult = { active: false, scrolled: false }

  /**
   * Handles a file drop onto a target pane by opening the transfer confirmation
   * dialog.
   *
   * Runs the SHARED destination guard (`checkTransferDestinationGuard`) first —
   * the same chain F5/F6 and paste run — so dropping onto a read-only volume
   * shows the same "Read-only device" alert (a drop must hit the same read-only
   * guard F5 does) instead of silently opening a copy dialog that the backend
   * would later reject.
   *
   * Source identity (two paths):
   *
   * - **In-app self-drag** (`recordedIdentity` set): the source volume id AND the
   *   source paths come from app state recorded at drag start — NOT from the
   *   pasteboard-derived `paths`. This is the only correct source for a virtual
   *   volume (MTP, smb2-native SMB), whose volume-relative paths
   *   (`/photos/sunset.jpg`) round-trip through wry's drop event looking exactly
   *   like local absolute paths, defeating the resolver. The kind probe is
   *   skipped: a relative path can't be stat'd locally (it would either
   *   all-unknown or, worse, coincidentally stat a same-named local path), so we
   *   use the approximate count shape — honest beats half-right.
   * - **External drop** (`recordedIdentity` absent): the dropped `paths` are
   *   genuine local absolute paths from Finder et al. We resolve the REAL source
   *   volume via `resolveSourceVolumeId` (frontend longest-prefix, backend
   *   fallback, honest-unknown default) so an MTP→local / local→MTP drop stats
   *   the right volume and the dialog's byte/file/dir counters fill (a wrong
   *   source volume id makes the preview report zeros). The top-level kind probe
   *   (`statPathsKinds`) runs in one batched IPC so the dialog and completion
   *   toast report the real file/folder split; it degrades to all-unknown on a
   *   slow mount, so it never blocks the drop.
   */
  async function handleFileDrop(
    paths: string[],
    targetPane: 'left' | 'right',
    targetFolderPath?: string,
    operation: TransferOperationType = 'copy',
    recordedIdentity?: SelfDragIdentity,
  ) {
    // For a recorded self-drag, the source paths come from app state, never the
    // pasteboard. The dropped `paths` are only used for hit-testing upstream.
    const sourcePaths = recordedIdentity?.sourcePaths ?? paths
    if (sourcePaths.length === 0) return

    const destVolId = access.getPaneVolumeId(targetPane)
    // The real drop target: a hovered folder row (`targetFolderPath`, which can be
    // a `.zip` row or a folder inside an open archive) else the pane's own path.
    // It drives the archive kind-from-path check, so dropping ONTO a zip or INTO
    // an open archive is refused up front.
    const destPath = targetFolderPath ?? access.getPanePath(targetPane)

    const guard = checkTransferDestinationGuard(destVolId, access.getVolumes(), destPath)
    if (!guard.ok) {
      if (guard.toast) addToast(guard.toast.message, { level: guard.toast.level })
      else dialogs.showAlert(guard.alert.title, guard.alert.message)
      return
    }

    const { sortBy, sortOrder } = access.getPaneSort(targetPane)

    if (recordedIdentity) {
      // In-app self-drag: trust the recorded identity wholesale; skip the
      // resolver and the local kind probe (a volume-relative path can't be
      // stat'd here). The approximate count shape is the honest fallback.
      dialogs.showTransfer(
        buildTransferPropsFromDroppedPaths(
          operation,
          sourcePaths,
          destPath,
          targetPane,
          destVolId,
          recordedIdentity.sourceVolumeId,
          sortBy,
          sortOrder,
          undefined,
        ),
      )
      return
    }

    const sourceVolumeId = await resolveSourceVolumeId(sourcePaths, access.getVolumes(), resolveVolumeForPath)

    let isDirectoryFlags: (boolean | null)[] | undefined
    try {
      isDirectoryFlags = await statPathsKinds(sourcePaths)
    } catch {
      // Stat failed entirely — leave flags undefined so the builder uses the
      // approximate shape rather than blocking the drop on the error.
      isDirectoryFlags = undefined
    }

    dialogs.showTransfer(
      buildTransferPropsFromDroppedPaths(
        operation,
        sourcePaths,
        destPath,
        targetPane,
        destVolId,
        sourceVolumeId,
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

  function scheduleDragAutoScroll() {
    if (dragAutoScrollFrame !== null) return
    if (typeof requestAnimationFrame !== 'function') return
    dragAutoScrollFrame = requestAnimationFrame(runDragAutoScrollFrame)
  }

  function stopDragAutoScroll() {
    if (dragAutoScrollFrame !== null && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(dragAutoScrollFrame)
    }
    dragAutoScrollPane = null
    dragAutoScrollPosition = null
    dragAutoScrollFrame = null
    lastAutoScrollTimestamp = null
  }

  function updateDragAutoScrollTarget(paneId: 'left' | 'right' | null, position: { x: number; y: number }) {
    if (!paneId) {
      stopDragAutoScroll()
      return
    }
    if (dragAutoScrollPane !== paneId) {
      lastAutoScrollTimestamp = null
    }
    dragAutoScrollPane = paneId
    dragAutoScrollPosition = position
    scheduleDragAutoScroll()
  }

  function runDragAutoScrollFrame(timestamp: number) {
    dragAutoScrollFrame = null
    const paneId = dragAutoScrollPane
    const position = dragAutoScrollPosition
    if (!paneId || !position) {
      stopDragAutoScroll()
      return
    }

    const elapsedMs =
      lastAutoScrollTimestamp === null ? 16.67 : Math.min(50, Math.max(0, timestamp - lastAutoScrollTimestamp))
    lastAutoScrollTimestamp = timestamp

    const result = access.getPaneRef(paneId)?.autoScrollDuringDrag(position, elapsedMs) ?? inactiveAutoScrollResult
    if (result.scrolled && lastDragPosition) {
      handleDragOver(lastDragPosition, { updateAutoScroll: false })
    }
    if (result.active) {
      scheduleDragAutoScroll()
    } else {
      stopDragAutoScroll()
    }
  }

  function isSamePaneSelfDrop(resolved: ResolvedDropTarget): boolean {
    return resolved?.type === 'pane' && getIsDraggingFromSelf() && resolved.paneId === access.getFocusedPane()
  }

  function canDropOnResolvedTarget(resolved: ResolvedDropTarget, isInvalidSelfDrop: boolean): boolean {
    return resolved !== null && !isSamePaneSelfDrop(resolved) && !isInvalidSelfDrop
  }

  function updateDropTargetState(resolved: ResolvedDropTarget, isInvalidSelfDrop: boolean) {
    if (isInvalidSelfDrop) {
      clearDropTargets()
      return
    }
    if (resolved?.type === 'folder') {
      dropTargetPane = null
      dropTargetFolderPath = resolved.path
      dropTargetFolderEl = resolved.element
      return
    }
    if (resolved?.type === 'pane') {
      dropTargetPane = isSamePaneSelfDrop(resolved) ? null : resolved.paneId
      dropTargetFolderPath = null
      dropTargetFolderEl = null
      return
    }
    clearDropTargets()
  }

  /** Updates drop-target highlights and overlay as the cursor moves during a drag. */
  function handleDragOver(position: { x: number; y: number }, options: { updateAutoScroll?: boolean } = {}) {
    lastDragPosition = position
    const paneWrapperEls = getPaneWrapperEls()
    const resolved = resolveDropTarget(position.x, position.y, paneWrapperEls.left, paneWrapperEls.right)
    if (options.updateAutoScroll !== false) {
      updateDragAutoScrollTarget(resolved?.paneId ?? null, position)
    }

    // Block drops onto the source itself or into one of its descendants.
    const effectiveTarget = targetPathOf(resolved)
    const isInvalidSelfDrop =
      effectiveTarget !== null && isInvalidSelfDescendantDrop(effectiveTarget, currentDragSourcePaths)
    updateDropTargetState(resolved, isInvalidSelfDrop)

    const canDrop = canDropOnResolvedTarget(resolved, isInvalidSelfDrop)
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
    const effectiveTarget = targetPathOf(resolved)

    // For our own in-flight drag, the source identity comes from app state — the
    // volume the user dragged FROM and the paths as that volume knows them — not
    // the lossy pasteboard round-trip. Tied to the existing self-drag flag, NOT a
    // parallel detection: a genuine external drop has no in-flight self-drag, so
    // its (real, local) paths flow through the resolver path below.
    //
    // We only trust a recorded identity whose `sourceVolumeId` is a REGISTERED
    // backend-real volume (present in `getVolumes()`). That excludes the
    // search-results virtual volume — its drags carry real absolute paths that
    // span volumes, with no single dispatchable source id, so the resolver (which
    // matches each absolute path to its real volume) is the correct path. This is
    // a registry-membership check, not a virtual-id string compare.
    const recordedIdentity = getIsDraggingFromSelf() ? consumableSelfDragIdentity() : undefined

    // Read modifiers BEFORE stopping the tracker (which resets state).
    // Same source-of-truth as the overlay (`handleDragOver`) so the displayed
    // operation matches what we actually run.
    const operation = pickDropOperation({
      sourcePath: operationSourcePath(recordedIdentity, paths),
      targetPath: effectiveTarget,
      volumes: access.getVolumes(),
      modifiers: getModifierState(),
    })

    clearDropTargets()
    stopDragAutoScroll()
    hideOverlay()
    stopModifierTracking()

    if (!resolved) return
    const targetPane = resolved.paneId
    // For same-pane pane-level drops (not folder), suppress (no-op)
    if (resolved.type === 'pane' && getIsDraggingFromSelf() && targetPane === access.getFocusedPane()) return

    // Guard against drops onto the source itself or into its descendants. Uses
    // the recorded source paths for a self-drag (the pasteboard paths may be
    // volume-relative), else the dropped paths.
    const guardPaths = recordedIdentity?.sourcePaths ?? paths
    if (effectiveTarget !== null && isInvalidSelfDescendantDrop(effectiveTarget, guardPaths)) return

    void handleFileDrop(
      paths,
      targetPane,
      resolved.type === 'folder' ? resolved.path : undefined,
      operation,
      recordedIdentity,
    )
  }

  /**
   * The source path fed to `pickDropOperation` (move-vs-copy). For a recorded
   * self-drag the pasteboard path is volume-relative (can't be matched to a
   * volume root), so we use the recorded source volume's ROOT path instead —
   * that's what makes a same-volume MTP/SMB move resolve to Move, not Copy.
   * Otherwise the first dropped path.
   */
  function operationSourcePath(recordedIdentity: SelfDragIdentity | undefined, paths: string[]): string | null {
    const firstPath = paths.at(0) ?? null
    if (recordedIdentity) {
      return access.getVolumes().find((v) => v.id === recordedIdentity.sourceVolumeId)?.path ?? firstPath
    }
    return firstPath
  }

  /**
   * The recorded self-drag identity to consume, or undefined when there's none
   * to trust. We trust it only when its `sourceVolumeId` is a REGISTERED
   * backend-real volume: that's what makes the MTP/SMB self-drag correct (a real
   * volume + volume-relative paths) while letting a search-results self-drag
   * (virtual `'search-results'` id, real absolute paths spanning volumes) fall
   * through to the resolver. A registry-membership check, not a string compare.
   */
  function consumableSelfDragIdentity(): SelfDragIdentity | undefined {
    const identity = getSelfDragIdentity()
    if (!identity) return undefined
    const isRegisteredVolume = access.getVolumes().some((v) => v.id === identity.sourceVolumeId)
    return isRegisteredVolume ? identity : undefined
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
    unlistenDragImageSize = await onDragImageSize((payload) => {
      const { width, height } = payload
      externalDragHasLargeImage = width > smallDragImageThreshold || height > smallDragImageThreshold
    })

    // Listen for native modifier key state during drags (macOS).
    // [NSEvent modifierFlags] works even when the webview doesn't have keyboard focus.
    unlistenDragModifiers = await onDragModifiers((payload) => {
      setModifiers(payload)
      // Re-evaluate the resolved op (and overlay action line) on modifier change
      // without a mouse move. Otherwise the OS "+" badge wouldn't update until
      // the cursor moves, lagging the user's intent.
      if (lastDragPosition !== null) {
        handleDragOver(lastDragPosition)
      }
    })

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
        // `handleDrop` consumes the recorded self-drag identity (if any) while
        // the self-drag flag is still set, BEFORE the resets below clear it.
        handleDrop(event.payload.paths, toViewportPosition(event.payload.position))
        resetDraggingFromSelf()
        clearSelfDragFingerprint()
        // Terminal: a new drag re-records its own identity. Cleared here (not on
        // 'leave') so a self-drag that exits and re-enters the window keeps its
        // identity for the eventual in-window drop — same lifecycle as the
        // fingerprint. On 'leave' the self-drag flag is reset, so a lingering
        // record can't be consumed unless re-entry restores the flag.
        clearSelfDragIdentity()
        void endSelfDragSession()
        externalDragHasLargeImage = false
        currentDragSourcePaths = []
        lastDragPosition = null
        lastPushedSelfDragOp = null
      } else {
        // 'leave': cursor left the window or drag was cancelled
        clearDropTargets()
        stopDragAutoScroll()
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
    stopDragAutoScroll()
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
    runDragAutoScrollFrame,
    stopDragAutoScroll,
    pushSelfDragOpIfChanged,
    handleDrop,
    clearDropTargets,
  }
}
