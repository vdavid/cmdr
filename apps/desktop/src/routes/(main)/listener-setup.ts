/**
 * Tauri listener setup for the main page, extracted from `+page.svelte` to keep
 * the component focused on reactive `$state` and rendering.
 *
 * This module owns the *logic* of wiring menu and MCP-dialog event listeners; the
 * component owns all `$state`. State crosses the boundary through `ListenerSetupContext`:
 * reads come in as getter functions and writes go out as setter callbacks, so the
 * captured closures always see live reactive values instead of a stale snapshot. The
 * shared `unlistenFns` array carries every registered unlisten back to the component's
 * `onDestroy` (important for HMR: stale listeners would otherwise stack on every reload).
 *
 * Runes can't live in a plain `.ts`, which is why the keydown handler, licensing init,
 * and onboarding gating stay in the component: they read/write `$state` directly.
 */

import {
  listen,
  type UnlistenFn,
  activateWindowMenu,
  onViewModeChanged,
  onMenuSort,
  onMediaIndexFolderExclusion,
  onMediaIndexFolderChoice,
  onExecuteCommand,
  onOpenSettings,
  onFocusAbout,
  onFocusFileViewer,
  onFocusConfirmation,
  onOpenFileViewer,
  onCloseFileViewer,
  onCloseAllFileViewers,
  onCloseAbout,
  onCloseConfirmation,
} from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { markDispatchSource } from './dispatch-dedup'
import { setFolderExcluded } from '$lib/media-index/excluded-folders'
import { setFolderChosen } from '$lib/media-index/always-index-folders'
import { isCommandId, type CommandId, type CommandDispatchArgs } from '$lib/commands'
import type { ViewMode } from '$lib/app-status-store'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { saveSettings } from '$lib/settings-store'
import { seedSettingForE2E } from '$lib/settings'
import { openFileViewer } from '$lib/file-viewer/open-viewer'
import { closeDialogById } from '$lib/ui/dialog-close-registry'
import type { SoftDialogId } from '$lib/ui/dialog-registry'
import { openGalleryDialog } from '$lib/dialog-gallery/gallery-state.svelte'
import { resolveDiskFixture, type FixtureDirPayload } from '$lib/dialog-gallery/disk-fixture'
import { openOnboardingPreview } from '$lib/dialog-gallery/onboarding-preview'
import { getAppMode } from '$lib/app-mode'
import type { ExplorerAPI } from './explorer-api'
import type { FriendlyError, TransferOperationType } from '$lib/file-explorer/types'

const log = getAppLogger('mainListeners')

/**
 * The seam between the main component (owns `$state`) and the extracted listener
 * logic. Reads are getters so each fired listener sees live reactive values; writes
 * are setter callbacks. `unlistenFns` is the component-owned cleanup array (every
 * registered listener's unlisten is pushed here for `onDestroy` / HMR teardown).
 */
export interface ListenerSetupContext {
  /** Live read of the explorer handle (`undefined` until `DualPaneExplorer` mounts; HMR can swap it). */
  getExplorer: () => ExplorerAPI | undefined
  /** Dispatch through the same typed command bus the keyboard / palette / MCP paths use. */
  dispatch: <K extends CommandId>(commandId: K, ...args: CommandDispatchArgs<K>) => Promise<void>
  /** Component-owned cleanup array; every registered unlisten is pushed here. */
  unlistenFns: UnlistenFn[]
  /** Write-only dialog setters (the component owns the `$state`). */
  dialogs: {
    setAboutWindow: (show: boolean) => void
  }
  /**
   * Re-runs the "What's new" startup trigger. Lives in the component because it
   * reads reactive startup-modal `$state`; the E2E rerun listener calls it.
   */
  maybeRunWhatsNew: (force: boolean) => Promise<void>
}

/** Safe wrapper for Tauri event listeners - handles non-Tauri environment. */
async function safeListenTauri(
  event: string,
  handler: (event: { payload: unknown }) => void,
): Promise<UnlistenFn | undefined> {
  try {
    return await listen(event, handler)
  } catch {
    return undefined
  }
}

/**
 * Builds a `listenTauri(event, handler)` bound to the given cleanup array: registers
 * the listener (no-op in a non-Tauri environment) and stores its unlisten for cleanup.
 * The component holds onto the returned function to also pass into `setupMcpListeners`,
 * so MCP and dialog listeners share one cleanup array.
 */
export function makeListenTauri(
  unlistenFns: UnlistenFn[],
): (event: string, handler: (event: { payload: unknown }) => void) => Promise<void> {
  return async (event, handler) => {
    const unlisten = await safeListenTauri(event, handler)
    if (unlisten) unlistenFns.push(unlisten)
  }
}

/**
 * Registers a typed `on*` event wrapper and stores its unlisten for cleanup.
 * Swallows the non-Tauri-environment rejection, matching `listenTauri`.
 */
async function pushTauri(unlistenFns: UnlistenFn[], register: () => Promise<UnlistenFn>): Promise<void> {
  try {
    unlistenFns.push(await register())
  } catch {
    // Not in a Tauri environment
  }
}

/** Get all file viewer windows (labels starting with 'viewer-'), sorted by creation time (most recent first). */
async function getFileViewerWindows() {
  try {
    const { getAllWindows } = await import('@tauri-apps/api/window')
    const windows = await getAllWindows()
    return windows
      .filter((w) => w.label.startsWith('viewer-'))
      .sort((a, b) => {
        const aTime = parseInt(a.label.replace('viewer-', ''), 10)
        const bTime = parseInt(b.label.replace('viewer-', ''), 10)
        return bTime - aTime // Most recent first
      })
  } catch {
    return []
  }
}

/** Emit an event to file viewer windows. Returns true if the event was emitted to at least one viewer. */
async function emitToFileViewers(event: string, payload?: { path?: string }): Promise<boolean> {
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit(event, payload)
    return true
  } catch {
    return false
  }
}

/** Close a file viewer window. If path is provided, closes the viewer with that path. Otherwise closes the most recent. */
async function closeFileViewer(path?: string) {
  const viewers = await getFileViewerWindows()
  if (viewers.length === 0) return

  if (path) {
    // Emit event with path - the viewer with that path will close itself
    await emitToFileViewers('mcp-viewer-close', { path })
  } else {
    // Close the most recent viewer directly
    try {
      await viewers[0].close()
    } catch {
      // Window may already be closed
    }
  }
}

/** Close all file viewer windows sequentially to avoid concurrent destruction races. */
async function closeAllFileViewers() {
  const viewers = await getFileViewerWindows()
  for (const viewer of viewers) {
    try {
      await viewer.close()
    } catch {
      // Window may already be closed
    }
  }
}

/** Focus a file viewer window. If path is provided, focuses the viewer with that path. Otherwise focuses the most recent. */
async function focusFileViewer(path?: string) {
  const viewers = await getFileViewerWindows()
  if (viewers.length === 0) return

  if (path) {
    // Emit event with path - the viewer with that path will focus itself
    await emitToFileViewers('mcp-viewer-focus', { path })
  } else {
    try {
      await viewers[0].setFocus()
    } catch {
      // Window may already be closed
    }
  }
}

/**
 * Brings the main window forward, for events fired from another window (the
 * confirmation-dialog focus request, the dialog gallery). Needs
 * `core:window:allow-set-focus` in `capabilities/default.json`: without it the
 * call rejects and the dialog opens BEHIND whatever window asked for it, which
 * reads as a dialog bug. Logged rather than swallowed for exactly that reason.
 */
async function focusMainWindow() {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    await getCurrentWindow().setFocus()
  } catch (error) {
    log.warn('Focusing the main window failed: {error}', { error: String(error) })
  }
}

/** Typed id for the per-pane view command (keeps dispatch off raw literals; A3). */
const viewSetModeCommand: CommandId = 'view.setMode'

/**
 * Maps a native `menu-sort` payload onto a focused-pane `sort.*` command id,
 * or `undefined` for an unrecognized payload. `sortBy` selects the column;
 * `sortOrder` selects ascending/descending (the menu never emits `toggle`).
 */
function menuSortToCommand(action: unknown, value: unknown): CommandId | undefined {
  if (action === 'sortBy') {
    const byColumn: Record<string, CommandId> = {
      name: 'sort.byName',
      extension: 'sort.byExtension',
      size: 'sort.bySize',
      modified: 'sort.byModified',
      created: 'sort.byCreated',
    }
    return typeof value === 'string' ? byColumn[value] : undefined
  }
  if (action === 'sortOrder') {
    const byOrder: Record<string, CommandId> = {
      asc: 'sort.ascending',
      desc: 'sort.descending',
      toggle: 'sort.toggleOrder',
    }
    return typeof value === 'string' ? byOrder[value] : undefined
  }
  return undefined
}

/** Set up menu-related event listeners. */
export async function setupMenuListeners(ctx: ListenerSetupContext): Promise<void> {
  const { dispatch, getExplorer, unlistenFns } = ctx

  // Single unified listener for all menu commands routed through "execute-command"
  try {
    unlistenFns.push(
      await onExecuteCommand((payload) => {
        const { commandId } = payload
        // The Rust menu emit (`menu_id_to_command`) and cross-window emits send a bare
        // string across IPC; the `CommandId` union can't reach over that boundary. Narrow
        // at the edge so a stale Rust id is dropped here rather than no-oping in the switch
        // `default`. The Rust↔registry drift test pins the two id sets together.
        if (isCommandId(commandId)) {
          // Tag the source so the dispatch core can swallow the spurious
          // second half of a macOS keyboard+menu double-fire (dispatch-dedup.ts).
          markDispatchSource('menu')
          void dispatch(commandId)
        }
      }),
    )
  } catch {
    // Not in a Tauri environment
  }

  // Per-pane view change from a native-menu click. Rust emits this directly
  // (not via `execute-command`) because the CheckMenuItem already toggled
  // its own state; the dispatch maps it onto the `view.setMode` command. The
  // payload is validated rather than `as`-cast: an unknown `mode` is dropped,
  // and an absent/unknown `pane` falls back to the focused pane (matching the
  // old in-component listener's `event.payload.pane ?? focusedPane`).
  unlistenFns.push(
    await onViewModeChanged((payload) => {
      const mode: ViewMode | undefined = payload.mode === 'full' || payload.mode === 'brief' ? payload.mode : undefined
      if (!mode) return
      const pane: 'left' | 'right' =
        payload.pane === 'left' || payload.pane === 'right' ? payload.pane : (getExplorer()?.getFocusedPane() ?? 'left')
      // `viewSetModeCommand` is a typed const (not an inline literal) so a
      // registry rename breaks compilation and `cmdr/no-raw-command-dispatch`
      // stays satisfied (A3). `fromMenu: true` → the handler skips
      // `pushViewMenuState` (the menu already toggled its CheckMenuItem).
      void dispatch(viewSetModeCommand, { pane, mode, fromMenu: true })
    }),
  )

  // Native sort-menu clicks. Rust emits this directly (not via
  // `execute-command`) with `{ action, value }`; the dispatch maps each
  // value onto the focused-pane `sort.*` command. Validated, not `as`-cast:
  // an unknown `action`/`value` pair is dropped.
  unlistenFns.push(
    await onMenuSort((payload) => {
      const command = menuSortToCommand(payload.action, payload.value)
      if (command) void dispatch(command)
    }),
  )

  // Folder "Don't index images in this folder" / "Index images here again" click.
  // Rust emits the right-clicked folder + target state directly; the FE persists
  // `mediaIndex.excludedFolders` and live-applies it (excluding retro-deletes the
  // folder's indexed rows backend-side). The helper rolls back + logs on failure, so a
  // rejection here needs no extra handling (a background privacy action, no toast).
  unlistenFns.push(
    await onMediaIndexFolderExclusion((payload) => {
      void setFolderExcluded(payload.folder, payload.excluded).catch(() => {})
    }),
  )

  // Folder "Add to indexed folders" / "Remove from indexed folders" click. Same shape as
  // the exclusion above, through the ONE chosen-folder path (`setFolderChosen`), so the
  // menu and the Settings list write the same setting and stay in agreement. Adding kicks
  // an indexing pass backend-side. The helper rolls back + logs on failure.
  unlistenFns.push(
    await onMediaIndexFolderChoice((payload) => {
      void setFolderChosen(payload.folder, payload.chosen).catch(() => {})
    }),
  )
}

/** Set up MCP dialog event listeners (close/focus). */
export async function setupDialogListeners(ctx: ListenerSetupContext): Promise<void> {
  const { getExplorer, unlistenFns, dialogs, maybeRunWhatsNew } = ctx
  const listenTauri = makeListenTauri(unlistenFns)

  // Settings with section (MCP-specific: "dialog open settings --section shortcuts").
  // The Rust MCP executor (`mcp/executor/dialogs.rs`) emits `{ section: <string> }`
  // — a BARE string, no anchor (the `dialog` tool has no anchor param, so MCP can't
  // deep-link to a row today; that's future work). Parse defensively (no `as` cast,
  // same discipline as `mcp-listeners.ts`) and wrap the bare string in an array.
  await pushTauri(unlistenFns, () =>
    onOpenSettings((payload) => {
      const section = payload.section || undefined
      void openSettingsWindow(section ? [section] : undefined)
    }),
  )

  // Generic soft-dialog close (MCP `dialog` tool, `action: close` for any registered
  // soft dialog beyond the special-cased about / confirmations). Routes the id to the
  // dialog's own close via the close registry (populated by ModalDialog / QueryDialog).
  // Fire-and-forget: the backend acks on `SoftDialogDisappeared(id)` (or times out
  // honestly if nothing closed), so no reply is needed here.
  await listenTauri('mcp-close-dialog', (event) => {
    const raw = event.payload
    const id =
      raw && typeof raw === 'object' && typeof (raw as { id?: unknown }).id === 'string'
        ? (raw as { id: string }).id
        : undefined
    if (id === undefined) return
    closeDialogById(id)
  })

  // About dialog
  await pushTauri(unlistenFns, () =>
    onCloseAbout(() => {
      dialogs.setAboutWindow(false)
    }),
  )
  await pushTauri(unlistenFns, () =>
    onFocusAbout(() => {
      // Already shown, just ensure it's visible
      dialogs.setAboutWindow(true)
    }),
  )

  // Volume picker
  await listenTauri('open-volume-picker', () => {
    getExplorer()?.openVolumeChooser()
  })
  await listenTauri('close-volume-picker', () => {
    getExplorer()?.closeVolumeChooser()
  })
  await listenTauri('focus-volume-picker', () => {
    // Volume picker is handled by DualPaneExplorer
  })

  // File viewer
  await pushTauri(unlistenFns, () =>
    onOpenFileViewer((payload) => {
      if (payload.path) {
        // Open viewer for specific path
        void openFileViewer(payload.path)
      } else {
        // Open viewer for cursor file
        void getExplorer()?.openViewerForCursor()
      }
    }),
  )
  await pushTauri(unlistenFns, () =>
    onCloseFileViewer((payload) => {
      void closeFileViewer(payload.path ?? undefined)
    }),
  )
  await pushTauri(unlistenFns, () =>
    onCloseAllFileViewers(() => {
      void closeAllFileViewers()
    }),
  )
  await pushTauri(unlistenFns, () =>
    onFocusFileViewer((payload) => {
      void focusFileViewer(payload.path ?? undefined)
    }),
  )

  // Confirmation dialog - handled by DualPaneExplorer
  await pushTauri(unlistenFns, () =>
    onCloseConfirmation(() => {
      getExplorer()?.closeConfirmationDialog()
    }),
  )
  await pushTauri(unlistenFns, () =>
    onFocusConfirmation(() => {
      // The confirmation dialog is a modal overlay in the main window.
      // If it's open, ensure the main window is focused so the dialog is visible.
      if (getExplorer()?.isConfirmationDialogOpen()) {
        void focusMainWindow()
      }
    }),
  )

  // Debug error injection (dev mode only)
  if (import.meta.env.DEV) {
    await listenTauri('debug-inject-error', (event) => {
      const { pane, friendly } = event.payload as { pane: 'left' | 'right'; friendly: FriendlyError }
      getExplorer()?.injectError(pane, friendly)
    })
    await listenTauri('debug-reset-error', (event) => {
      const { pane } = event.payload as { pane: 'left' | 'right' | 'both' }
      getExplorer()?.resetError(pane)
    })
    // Debug > Soft dialogs: open a gallery dialog over the main window. The main
    // window focuses ITSELF here rather than the Debug window pushing it, because
    // `debug.json` is a deliberately minimal capability and Tauri permissions fail
    // silently. Without the focus, the previewed dialog would sit behind the Debug
    // window and Escape would go to the wrong window.
    await listenTauri('debug-open-gallery-dialog', (event) => {
      const { dialogId, stateId, fixtures } = event.payload as {
        dialogId: SoftDialogId
        stateId: string
        /** Present for the disk-backed dialogs; the Debug window owns that IPC. */
        fixtures: FixtureDirPayload | null
      }
      void (async () => {
        // `onboarding` has no store and nothing the harness can render (its open
        // flag is a local `$state` in `+page.svelte`), so its preview dispatches
        // the app's own re-entry command instead. See `dialog-gallery/onboarding-preview.ts`.
        if (dialogId === 'onboarding') {
          await openOnboardingPreview(stateId, ctx.dispatch)
          await focusMainWindow()
          return
        }
        // A disk-backed dialog needs the focused pane pointed at the fixture
        // directory first: it's where its live listing id and its real entries
        // come from. No context resolved means open nothing, not a half-real dialog.
        const disk = fixtures ? await resolveDiskFixture(getExplorer(), fixtures) : null
        if (fixtures && !disk) return
        openGalleryDialog(dialogId, stateId, disk ?? undefined)
        await focusMainWindow()
      })()
    })
  }

  // E2E only: drive the native drag-and-drop drop entry programmatically.
  // Real OS drag can't be synthesized in Playwright, so the harness emits
  // this event to exercise OUR drop handling (the shared destination guard,
  // source-volume resolution, and transfer dialog) through the SAME
  // `dragDrop.handleFileDrop` the live drop branch runs. Gated on
  // `getAppMode() === 'e2e'` (set by CMDR_E2E_MODE=1, never true in prod),
  // so production never reacts even if the event were somehow emitted.
  await listenTauri('e2e-trigger-file-drop', (event) => {
    if (getAppMode() !== 'e2e') return
    const { paths, targetPane, targetFolderPath, operation, recordedIdentity } = event.payload as {
      paths: string[]
      targetPane: 'left' | 'right'
      targetFolderPath?: string
      operation?: TransferOperationType
      recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] }
    }
    getExplorer()?.triggerFileDrop(paths, targetPane, targetFolderPath, operation, recordedIdentity)
  })

  // E2E only: seed settings, then re-run the "What's new" check with `force`.
  // The boot auto-check is suppressed under E2E mode (see `maybeRunWhatsNew`),
  // so no popup leaks into other specs. To exercise the real auto-show path,
  // the whats-new spec emits this event with `isOnboarded: true` and an old
  // `lastSeenVersion`; the handler seeds them and runs `maybeRunWhatsNew(true)`,
  // the SAME trigger the boot path uses. Gated on `getAppMode() === 'e2e'`,
  // never true in prod.
  //
  // The whats-new keys are seeded via `seedSettingForE2E` (cache + save, NO
  // cross-window emit), NOT `setSetting`: the trigger then stamps
  // `lastSeenVersion` to the current version, and a `setSetting` seed's
  // self-echo (`settings:changed` loops back to this same window) could land
  // AFTER the stamp and revert it. The non-emitting seed sidesteps that race,
  // matching production where the seed comes from disk at boot, never a live
  // emit.
  await listenTauri('e2e-rerun-whats-new', (event) => {
    if (getAppMode() !== 'e2e') return
    const { isOnboarded, lastSeenVersion, showOnUpdate } = event.payload as {
      isOnboarded: boolean
      lastSeenVersion: string
      showOnUpdate: boolean
    }
    void (async () => {
      await saveSettings({ isOnboarded })
      seedSettingForE2E('whatsNew.lastSeenVersion', lastSeenVersion)
      seedSettingForE2E('whatsNew.showOnUpdate', showOnUpdate)
      await maybeRunWhatsNew(true)
    })()
  })
}

/** Sync file-scoped menu items with main window focus state. */
export async function setupWindowFocusListener(ctx: ListenerSetupContext): Promise<void> {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      void activateWindowMenu(focused ? 'main' : 'other')
    })
    ctx.unlistenFns.push(unlisten)
  } catch {
    // Not in Tauri environment
  }
}
